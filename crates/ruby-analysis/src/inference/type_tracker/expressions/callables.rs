use crate::core::{FullyQualifiedName, RubyMethod, RubyType, TypeInferenceOutcome, UnknownReason};
use crate::engine::AnalysisQuery;
use crate::inference::control_flow;
use crate::inference::r#type::literal::project_immediate_hash_receiver_type;
use crate::inference::type_tracker::flow::shapes::values::type_is_shape_only;
use crate::inference::type_tracker::TypeTracker;
use ruby_prism::*;
use std::collections::{BTreeSet, HashMap, HashSet};

#[derive(Default)]
struct EscapedCallableReadCollector {
    names: HashSet<String>,
}

impl<'pr> Visit<'pr> for EscapedCallableReadCollector {
    fn visit_local_variable_read_node(&mut self, node: &LocalVariableReadNode<'pr>) {
        self.names
            .insert(String::from_utf8_lossy(node.name().as_slice()).to_string());
    }

    fn visit_call_node(&mut self, node: &CallNode<'pr>) {
        if let Some(receiver) = node.receiver() {
            let direct_invoke = node.name().as_slice() == b"call"
                && receiver.as_local_variable_read_node().is_some();
            if !direct_invoke {
                self.visit(&receiver);
            }
        }
        if let Some(arguments) = node.arguments() {
            self.visit_arguments_node(&arguments);
        }
        if node
            .block()
            .is_some_and(|block| block.as_block_node().is_some())
        {
            self.visit(node.block().as_ref().expect("checked block presence"));
        }
    }
}

impl TypeTracker {
    pub(in crate::inference::type_tracker) fn invalidate_escaped_callables_in_value(
        &mut self,
        value: &Node<'_>,
    ) {
        if self.environment.callables.is_empty() {
            return;
        }
        if crate::indexer::is_static_callable_literal(value)
            || value.as_local_variable_read_node().is_some()
        {
            return;
        }
        let mut escaped = EscapedCallableReadCollector::default();
        escaped.visit(value);
        self.invalidate_callable_names(escaped.names);
    }

    pub(in crate::inference::type_tracker) fn invalidate_callable_names(
        &mut self,
        names: HashSet<String>,
    ) {
        for name in names {
            let Some(identity) = self
                .environment
                .callables
                .get(&name)
                .map(|callable| callable.identity)
            else {
                continue;
            };
            for callable in self.environment.callables.values_mut() {
                if callable.identity == identity {
                    callable.summary = Err(UnknownReason::EscapedCallableValue);
                }
            }
        }
    }

    pub(in crate::inference::type_tracker) fn invalidate_escaped_callables_in_call(
        &mut self,
        call: &CallNode<'_>,
    ) {
        if self.environment.callables.is_empty() {
            return;
        }
        let mut escaped = EscapedCallableReadCollector::default();
        if let Some(receiver) = call.receiver() {
            let direct_invoke = call.name().as_slice() == b"call"
                && receiver.as_local_variable_read_node().is_some();
            if !direct_invoke {
                escaped.visit(&receiver);
            }
        }
        if let Some(arguments) = call.arguments() {
            escaped.visit_arguments_node(&arguments);
        }
        if call
            .block()
            .is_some_and(|block| block.as_block_node().is_some())
        {
            escaped.visit(call.block().as_ref().expect("checked block presence"));
        }
        self.invalidate_callable_names(escaped.names);
    }

    pub(in crate::inference::type_tracker) fn bind_local_callable(
        &mut self,
        name: String,
        mut callable: crate::inference::higher_order::KnownProcType,
    ) {
        if callable
            .summary
            .as_ref()
            .is_ok_and(|summary| summary.captures.binary_search(&name).is_ok())
        {
            callable.summary = Err(UnknownReason::CallableRecursionUnsupported);
        }
        let alias_count = self
            .environment
            .callables
            .iter()
            .filter(|(existing_name, existing)| {
                existing_name.as_str() != name && existing.identity == callable.identity
            })
            .count();
        if alias_count >= crate::core::callable_body::MAX_CALLABLE_BODY_ALIASES {
            callable.summary = Err(UnknownReason::CallableBodyBoundExceeded);
            for existing in self.environment.callables.values_mut() {
                if existing.identity == callable.identity {
                    existing.summary = Err(UnknownReason::CallableBodyBoundExceeded);
                }
            }
        }
        self.environment.callables.insert(name, callable);
    }

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
        let prepared_result = if let Some(analysis_engine) = &self.analysis.engine {
            let engine = analysis_engine.read();
            let query = AnalysisQuery::new(&engine);
            let namespace = FullyQualifiedName::namespace(
                self.context
                    .class
                    .as_ref()
                    .map(FullyQualifiedName::namespace_parts)
                    .unwrap_or_default(),
            );
            crate::inference::rbs::prepare_higher_order_call_with_fallbacks(
                Some(&query),
                self.analysis.query_cache.as_deref(),
                receiver_type.as_ref(),
                Some(&namespace),
                method_name,
                &argument_types,
            )
        } else {
            crate::inference::rbs::prepare_higher_order_call_with_fallbacks(
                None,
                None,
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
                Ok(summary) => crate::inference::higher_order::KnownProcType {
                    identity: u32::MAX,
                    summary: Ok(summary),
                },
                Err(_) => return None,
            }
        };
        let mut stack = vec![callable.identity];
        prepared
            .finish_known_proc(
                &callable,
                |capture| self.environment.types.get(capture).cloned(),
                |capture, arguments| {
                    let nested = self.environment.callables.get(capture)?.clone();
                    Some(self.instantiate_known_proc_with_stack(&nested, arguments, &mut stack))
                },
                |receiver, method, _arguments| {
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
                Ok(summary) => crate::inference::higher_order::KnownProcType {
                    identity: u32::MAX,
                    summary: Ok(summary),
                },
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
        self.instantiate_known_proc_with_stack(&callable, &argument_types, &mut Vec::new())
            .into_proven_type()
    }

    pub(in crate::inference::type_tracker) fn instantiate_known_proc_with_stack(
        &self,
        callable: &crate::inference::higher_order::KnownProcType,
        arguments: &[RubyType],
        stack: &mut Vec<u32>,
    ) -> TypeInferenceOutcome {
        if stack.contains(&callable.identity) {
            return TypeInferenceOutcome::unknown(UnknownReason::CallableRecursionUnsupported);
        }
        if stack.len() >= crate::core::callable_body::MAX_CALLABLE_BODY_INSTANTIATIONS {
            return TypeInferenceOutcome::unknown(UnknownReason::CallableBodyBoundExceeded);
        }
        let summary = match &callable.summary {
            Ok(summary) => summary,
            Err(reason) => return TypeInferenceOutcome::unknown(*reason),
        };
        stack.push(callable.identity);
        let result = crate::inference::callable_body::instantiate_callable_body(
            summary,
            arguments,
            |capture| self.environment.types.get(capture).cloned(),
            |capture, nested_arguments| {
                let nested = self.environment.callables.get(capture)?.clone();
                Some(self.instantiate_known_proc_with_stack(&nested, nested_arguments, stack))
            },
            |receiver, method, _arguments| {
                self.resolve_static_method_return_outcome(receiver, method.as_str())
            },
        );
        let popped = stack.pop().expect(
            "INVARIANT VIOLATED: callable instantiation stack underflowed. This is a bug because every accepted callable pushes exactly one identity. Fix: keep push/evaluate/pop in one function.",
        );
        assert_eq!(
            popped, callable.identity,
            "INVARIANT VIOLATED: callable instantiation stack order changed during evaluation. This is a bug because nested evaluation must be strictly LIFO. Fix: do not retain or reorder stack entries."
        );
        result
    }

    pub(in crate::inference::type_tracker) fn infer_proc_literal_return_type(
        &mut self,
        value: &Node,
    ) -> Option<crate::inference::higher_order::KnownProcType> {
        crate::indexer::is_static_callable_literal(value).then(|| {
            let outer_locals = self.environment.types.keys().cloned();
            crate::inference::higher_order::KnownProcType {
                identity: u32::try_from(value.location().start_offset()).expect(
                    "INVARIANT VIOLATED: callable literal offset exceeded u32. This is a bug because analysis ranges already require u32 offsets. Fix: reject oversized source before callable lowering.",
                ),
                summary: crate::indexer::lower_callable_literal_with_outer_locals(
                    value,
                    outer_locals,
                ),
            }
        })
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
pub(in crate::inference::type_tracker) fn block_parameter_names(
    block: &BlockNode<'_>,
) -> Vec<String> {
    let Some(parameters_node) = block.parameters() else {
        return Vec::new();
    };
    if let Some(numbered) = parameters_node.as_numbered_parameters_node() {
        return numbered_parameter_names(numbered);
    }
    let Some(parameters) = parameters_node
        .as_block_parameters_node()
        .and_then(|node| node.parameters())
    else {
        return Vec::new();
    };

    let mut names = Vec::new();
    for required in parameters.requireds().iter() {
        if let Some(param) = required.as_required_parameter_node() {
            names.push(String::from_utf8_lossy(param.name().as_slice()).to_string());
        }
    }
    for optional in parameters.optionals().iter() {
        if let Some(param) = optional.as_optional_parameter_node() {
            names.push(String::from_utf8_lossy(param.name().as_slice()).to_string());
        }
    }
    if let Some(rest) = parameters.rest() {
        if let Some(param) = rest.as_rest_parameter_node() {
            if let Some(name) = param.name() {
                names.push(String::from_utf8_lossy(name.as_slice()).to_string());
            }
        }
    }
    for post in parameters.posts().iter() {
        if let Some(param) = post.as_required_parameter_node() {
            names.push(String::from_utf8_lossy(param.name().as_slice()).to_string());
        }
    }
    names
}

pub(in crate::inference::type_tracker) fn numbered_parameter_names(
    params: NumberedParametersNode<'_>,
) -> Vec<String> {
    (1..=usize::from(params.maximum()))
        .map(|index| format!("_{index}"))
        .collect()
}
