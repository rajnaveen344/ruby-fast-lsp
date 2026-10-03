use crate::core::{FullyQualifiedName, RubyMethod, RubyType};
use crate::inference::control_flow;
use crate::inference::higher_order::{block_parameter_names, KnownProcType};
use crate::inference::r#type::literal::project_immediate_hash_receiver_type;
use crate::inference::type_tracker::flow::shapes::values::type_is_shape_only;
use crate::inference::type_tracker::TypeTracker;
use ruby_prism::*;
use std::collections::{BTreeSet, HashMap};

impl TypeTracker {
    /// Infer one block body from an explicit environment and return the
    /// post-body value of each tracked parameter only when the same bounded
    /// mutable identity remains proven. This is the shared bridge used by the
    /// indexer and ordinary flow tracker; local rebinding is not mistaken for
    /// mutation of the yielded object.
    pub(crate) fn track_isolated_block_body(
        &mut self,
        body: Option<Node<'_>>,
        bindings: &HashMap<String, RubyType>,
        tracked_parameters: &[(String, RubyType)],
    ) -> (RubyType, Vec<RubyType>) {
        self.environment.clear();
        self.next_shape_identity = 0;
        for (name, ruby_type) in bindings {
            if type_is_shape_only(ruby_type) {
                let identity = self.allocate_shape_identity(ruby_type.clone());
                self.environment.bind_shape_identities(
                    name.clone(),
                    ruby_type.clone(),
                    BTreeSet::from([identity]),
                );
            } else {
                self.environment.insert(name.clone(), ruby_type.clone());
            }
        }
        let result = body
            .map(|body| self.track_node(&body))
            .unwrap_or_else(RubyType::nil_class);
        let post_parameters = tracked_parameters
            .iter()
            .map(|(name, original)| {
                let identities = self.environment.shape_identities(name);
                if identities.is_empty() {
                    original.clone()
                } else {
                    self.shape_identity_type(&identities)
                        .unwrap_or(RubyType::Unknown)
                }
            })
            .collect();
        (result, post_parameters)
    }

    pub(in crate::inference::type_tracker) fn infer_rbs_higher_order_call(
        &mut self,
        call: &CallNode<'_>,
        method_name: &str,
    ) -> Option<RubyType> {
        let block_expression = call.block()?;
        if method_name.ends_with('!') {
            return None;
        }
        let receiver_type = call.receiver().map(|receiver| {
            project_immediate_hash_receiver_type(&receiver, self.infer_expression(&receiver))
        });
        if receiver_type.as_ref().is_some_and(|receiver| {
            receiver == &RubyType::Unknown || RubyType::contains_unknown(receiver)
        }) {
            return None;
        }
        let argument_types = call
            .arguments()
            .map(|arguments| {
                arguments
                    .arguments()
                    .iter()
                    .map(|argument| self.track_node(&argument))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let prepared_result = if let Some(project) = &self.analysis.project {
            let namespace = FullyQualifiedName::namespace(
                self.context
                    .class
                    .as_ref()
                    .map(FullyQualifiedName::namespace_parts)
                    .unwrap_or_default(),
            );
            project.prepare_higher_order_call(
                receiver_type.as_ref(),
                &namespace,
                method_name,
                &argument_types,
            )
        } else {
            crate::inference::rbs::prepare_higher_order_call_with_fallbacks(
                None::<&dyn crate::inference::semantics::Semantics>,
                receiver_type.as_ref(),
                None,
                method_name,
                &argument_types,
            )
        };
        let prepared = prepared_result.ok()?;
        if let Some(block) = block_expression.as_block_node() {
            if block
                .body()
                .as_ref()
                .is_some_and(|body| control_flow::has_unsupported_higher_order_exit(body))
            {
                return None;
            }
            let parameter_types = prepared.block_parameter_types().to_vec();
            let parameter_names = block_parameter_names(&block);
            let environment_before = self.environment.clone();
            let explicit_return_count = self.returns.explicit_values.len();
            for (index, name) in parameter_names.iter().enumerate() {
                let parameter_type = parameter_types
                    .get(index)
                    .cloned()
                    .unwrap_or_else(RubyType::nil_class);
                if type_is_shape_only(&parameter_type) {
                    let identity = self.allocate_shape_identity(parameter_type.clone());
                    self.environment.bind_shape_identities(
                        name.clone(),
                        parameter_type,
                        BTreeSet::from([identity]),
                    );
                } else {
                    self.environment.insert(name.clone(), parameter_type);
                }
            }
            let block_return_type = block
                .body()
                .map(|body| self.track_node(&body))
                .unwrap_or_else(RubyType::nil_class);
            let post_block_parameter_types = parameter_names
                .iter()
                .zip(&parameter_types)
                .map(|(name, original)| {
                    let identities = self.environment.shape_identities(name);
                    if identities.is_empty() {
                        original.clone()
                    } else {
                        self.shape_identity_type(&identities)
                            .unwrap_or(RubyType::Unknown)
                    }
                })
                .collect::<Vec<_>>();
            self.environment = environment_before;
            self.returns.explicit_values.truncate(explicit_return_count);
            return prepared
                .finish_with_proven_block_state(&block_return_type, &post_block_parameter_types)
                .into_proven_type();
        }

        let block_argument = block_expression.as_block_argument_node()?;
        let expression = block_argument.expression()?;
        if let Some(symbol) = expression.as_symbol_node() {
            let target = std::str::from_utf8(symbol.unescaped()).ok()?;
            return prepared
                .finish_static_method(target, |receiver_type, target| {
                    self.resolve_static_method_return_outcome(receiver_type, target)
                })
                .into_proven_type();
        }
        let callable = if let Some(local) = expression.as_local_variable_read_node() {
            let name = String::from_utf8_lossy(local.name().as_slice()).to_string();
            self.environment.callables.get(&name)?.clone()
        } else {
            match self.constant_callable_body_for_node(&expression)? {
                Ok(summary) => KnownProcType::constant(summary),
                Err(_) => return None,
            }
        };
        self.environment
            .callables
            .finish_block_argument(
                prepared,
                &callable,
                &|capture| self.environment.types.get(capture).cloned(),
                &|receiver, method| {
                    self.resolve_static_method_return_outcome(receiver, method.as_str())
                },
            )
            .into_proven_type()
    }

    pub(in crate::inference::type_tracker) fn infer_proc_call_return_type(
        &mut self,
        call: &CallNode,
        method_name: &str,
    ) -> Option<RubyType> {
        if method_name != "call" {
            return None;
        }
        let receiver = call.receiver()?;
        let callable = if let Some(local) = receiver.as_local_variable_read_node() {
            let name = String::from_utf8_lossy(local.name().as_slice()).to_string();
            self.environment.callables.get(&name)?.clone()
        } else {
            match self.constant_callable_body_for_node(&receiver)? {
                Ok(summary) => KnownProcType::constant(summary),
                Err(_) => return None,
            }
        };
        let argument_types = call
            .arguments()
            .map(|arguments| {
                arguments
                    .arguments()
                    .iter()
                    .map(|argument| self.track_node(&argument))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        self.environment
            .callables
            .instantiate(
                &callable,
                &argument_types,
                &|capture| self.environment.types.get(capture).cloned(),
                &|receiver, method| {
                    self.resolve_static_method_return_outcome(receiver, method.as_str())
                },
                &mut Vec::new(),
            )
            .into_proven_type()
    }

    pub(in crate::inference::type_tracker) fn infer_yielding_block_return_type(
        &mut self,
        call: &CallNode,
        method_name: &str,
    ) -> Option<RubyType> {
        let block = call.block()?.as_block_node()?;
        let method = RubyMethod::new(method_name).ok()?;
        let method_fqn = match call.receiver() {
            None => FullyQualifiedName::method(
                self.context
                    .class
                    .as_ref()
                    .map(|fqn| fqn.namespace_parts())
                    .unwrap_or_default(),
                method,
            ),
            Some(receiver) if receiver.as_self_node().is_some() => FullyQualifiedName::method(
                self.context
                    .class
                    .as_ref()
                    .map(|fqn| fqn.namespace_parts())
                    .unwrap_or_default(),
                method,
            ),
            Some(receiver) => {
                let receiver_type = self.infer_expression(&receiver);
                let parts = match receiver_type {
                    RubyType::Class(fqn)
                    | RubyType::Module(fqn)
                    | RubyType::ClassReference(fqn)
                    | RubyType::ModuleReference(fqn) => fqn.namespace_parts(),
                    RubyType::Literal(_) | RubyType::Shape(_) => {
                        return None;
                    }
                    RubyType::Array(_)
                    | RubyType::Hash(_, _)
                    | RubyType::Union(_)
                    | RubyType::Unknown => {
                        return None;
                    }
                };
                FullyQualifiedName::method(parts, method)
            }
        };
        let param_types = self.analysis.method_yields.get(&method_fqn)?.clone();
        if param_types.iter().all(|ty| *ty == RubyType::Unknown) {
            return None;
        }
        let param_names = block_parameter_names(&block);

        let env_before = self.environment.clone();
        for (index, name) in param_names.iter().enumerate() {
            if let Some(param_type) = param_types.get(index) {
                if *param_type != RubyType::Unknown {
                    self.environment.insert(name.clone(), param_type.clone());
                }
            }
        }
        let return_type = block
            .body()
            .map(|body| self.track_node(&body))
            .unwrap_or_else(RubyType::nil_class);
        self.environment = env_before;

        (return_type != RubyType::Unknown).then_some(return_type)
    }
}
