use crate::core::{FullyQualifiedName, RubyMethod, RubyType, TypeInferenceOutcome, UnknownReason};
use crate::indexer::fact_collector::FactCollector;
use crate::indexer::utf8_str;
use crate::inference::higher_order::{block_parameter_names, call_site, KnownProcType};
use crate::inference::r#type::literal::project_immediate_hash_receiver_type;
use crate::inference::r#type::shape as shape_reads;
use crate::inference::type_tracker::TypeTracker;
use crate::invariant::ExpectInvariant;
use ruby_prism::*;
use std::collections::HashMap;

impl FactCollector {
    pub(in crate::indexer::fact_collector) fn prepare_higher_order_call_for_node(
        &self,
        call_node: &CallNode<'_>,
        local_types: &HashMap<String, RubyType>,
    ) -> Result<crate::inference::higher_order::PreparedCallableSet, UnknownReason> {
        let method_name = String::from_utf8_lossy(call_node.name().as_slice());
        let receiver_type = call_node.receiver().map(|receiver| {
            project_immediate_hash_receiver_type(
                &receiver,
                self.infer_type_from_value_with_locals(&receiver, local_types),
            )
        });
        call_site::prepare_call(
            Some(self.semantics.project.as_ref()),
            receiver_type.as_ref(),
            &FullyQualifiedName::namespace(self.scope_tracker.get_ns_stack()),
            &method_name,
            || {
                call_node
                    .arguments()
                    .map(|arguments| {
                        arguments
                            .arguments()
                            .iter()
                            .map(|argument| {
                                self.infer_type_from_value_with_locals(&argument, local_types)
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default()
            },
        )
    }

    pub(in crate::indexer::fact_collector) fn infer_rbs_higher_order_call_outcome(
        &self,
        call_node: &CallNode<'_>,
        local_types: &HashMap<String, RubyType>,
        prepared: Option<
            Result<crate::inference::higher_order::PreparedCallableSet, UnknownReason>,
        >,
    ) -> Option<TypeInferenceOutcome> {
        let block_expression = call_node.block()?;
        let method_name = String::from_utf8_lossy(call_node.name().as_slice());
        if method_name.ends_with('!') {
            return None;
        }
        let prepared = prepared
            .unwrap_or_else(|| self.prepare_higher_order_call_for_node(call_node, local_types))
            .ok()?;
        if let Some(block) = block_expression.as_block_node() {
            if block.body().as_ref().is_some_and(|body| {
                crate::inference::control_flow::has_unsupported_higher_order_exit(body)
            }) {
                return Some(TypeInferenceOutcome::unknown(
                    UnknownReason::UnsupportedBlockFlow,
                ));
            }
            let parameter_types = prepared.block_parameter_types().to_vec();
            let tracked_parameters =
                prepared.block_parameter_bindings(&block_parameter_names(&block));
            let mut block_locals = local_types.clone();
            block_locals.extend(tracked_parameters.iter().cloned());
            let mut tracker = TypeTracker::new().with_semantics(self.semantics.project.clone());
            let namespace = self.scope_tracker.get_ns_stack();
            if !namespace.is_empty() {
                tracker.set_current_class(Some(FullyQualifiedName::namespace(namespace)));
            }
            let (block_return_type, tracked_post_types) =
                tracker.track_isolated_block_body(block.body(), &block_locals, &tracked_parameters);
            let mut post_parameter_types = parameter_types.clone();
            for (index, ruby_type) in tracked_post_types.into_iter().enumerate() {
                if let Some(slot) = post_parameter_types.get_mut(index) {
                    *slot = ruby_type;
                }
            }
            return Some(
                prepared.finish_with_proven_block_state(&block_return_type, &post_parameter_types),
            );
        }

        let Some(block_argument) = block_expression.as_block_argument_node() else {
            return Some(TypeInferenceOutcome::unknown(
                UnknownReason::UnsupportedCallable,
            ));
        };
        let location = block_argument.expression().map_or_else(
            || block_argument.location(),
            |expression| expression.location(),
        );
        self.flow.local_callables.finish_block_argument_node(
            prepared,
            &block_argument,
            |expression| self.constant_callable_body_for_node(expression),
            &|capture| self.local_capture_type(capture, local_types, &location),
            &|receiver, method| {
                self.resolve_method_return_type_outcome_with_private(receiver, method, false)
            },
        )
    }

    pub(in crate::indexer::fact_collector) fn infer_yielding_block_return_type_for_call(
        &self,
        call_node: &CallNode<'_>,
    ) -> Option<RubyType> {
        let block = call_node.block()?.as_block_node()?;
        let param_types = self.infer_block_param_types_from_yielding_method(call_node)?;
        if param_types.iter().all(|ty| *ty == RubyType::Unknown) {
            return None;
        }

        let param_names = block_parameter_names(&block);
        let mut local_types = HashMap::new();
        for (index, name) in param_names.iter().enumerate() {
            if let Some(param_type) = param_types.get(index) {
                if *param_type != RubyType::Unknown {
                    local_types.insert(name.clone(), param_type.clone());
                }
            }
        }

        let return_type = block
            .body()
            .map(|body| self.infer_type_from_value_with_locals(&body, &local_types))
            .unwrap_or_else(RubyType::nil_class);
        (return_type != RubyType::Unknown).then_some(return_type)
    }

    pub fn infer_proc_literal_return_type(&self, value_node: &Node) -> Option<RubyType> {
        let callable = self.infer_known_proc_type(value_node)?;
        let summary = callable.summary.ok()?;
        crate::inference::callable_body::instantiate_callable_body(
            &summary,
            &[],
            |capture| self.get_local_var_type(capture, &value_node.location()),
            |_, _| None,
            |receiver, method, _arguments| {
                self.resolve_method_return_type_outcome_with_private(
                    receiver,
                    method.as_str(),
                    false,
                )
            },
        )
        .into_proven_type()
    }

    pub(crate) fn infer_known_proc_type(&self, value_node: &Node) -> Option<KnownProcType> {
        KnownProcType::from_literal(value_node, || {
            let scope_id = self
                .document
                .variable_scopes()
                .current_scope()
                .expect_invariant(
                    "callable lowering ran without an active lexical scope",
                    "FactCollector and VariableScopes must enter and exit scopes together",
                    "keep callable lowering inside the ordinary collector traversal",
                );
            self.document
                .variable_scopes()
                .get_visible_variables(scope_id)
                .into_iter()
                .map(|variable| variable.name.to_string())
                .collect::<Vec<_>>()
        })
    }

    fn local_capture_type(
        &self,
        capture: &str,
        local_types: &HashMap<String, RubyType>,
        location: &Location<'_>,
    ) -> Option<RubyType> {
        local_types
            .get(capture)
            .cloned()
            .or_else(|| self.get_local_var_type(capture, location))
    }

    pub(in crate::indexer::fact_collector) fn infer_proc_body(
        &self,
        body: Option<Node<'_>>,
    ) -> Option<RubyType> {
        let return_type = body
            .map(|body| self.infer_type_from_value(&body))
            .unwrap_or_else(RubyType::nil_class);
        (return_type != RubyType::Unknown).then_some(return_type)
    }

    /// Infer the value returned by a call's block using the current lexical,
    /// local, and execution-context state. Extension adapters may request this
    /// framework-neutral operation for DSL-generated methods such as memoized
    /// helpers; the adapter remains responsible for declaring that relationship.
    pub fn infer_call_block_return_type(&self, call: &CallNode<'_>) -> Option<RubyType> {
        let block = call.block()?.as_block_node()?;
        self.infer_proc_body(block.body())
    }

    pub(in crate::indexer::fact_collector) fn infer_proc_call_return_type(
        &self,
        call_node: &CallNode<'_>,
        local_types: &HashMap<String, RubyType>,
    ) -> Option<TypeInferenceOutcome> {
        if call_node.name().as_slice() != b"call" {
            return None;
        }
        let receiver = call_node.receiver()?;
        let callable = match self
            .flow
            .local_callables
            .callable_for(&receiver, |node| self.constant_callable_body_for_node(node))?
        {
            Ok(callable) => callable,
            Err(reason) => return Some(TypeInferenceOutcome::unknown(reason)),
        };
        let argument_types = call_node
            .arguments()
            .map(|arguments| {
                arguments
                    .arguments()
                    .iter()
                    .map(|argument| self.infer_type_from_value_with_locals(&argument, local_types))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let location = call_node.location();
        Some(self.flow.local_callables.instantiate(
            &callable,
            &argument_types,
            &|capture| self.local_capture_type(capture, local_types, &location),
            &|receiver, method| {
                self.resolve_method_return_type_outcome_with_private(
                    receiver,
                    method.as_str(),
                    false,
                )
            },
            &mut Vec::new(),
        ))
    }

    pub(in crate::indexer::fact_collector) fn constant_callable_body_for_node(
        &self,
        node: &Node<'_>,
    ) -> Option<Result<crate::core::callables::callable_body::CallableBodySummary, UnknownReason>>
    {
        let reference = crate::indexer::mixin_ref_from_node(node)?;
        let lexical_context = self.scope_tracker.get_ns_stack();
        let (constant, _) = self.resolve_constant_value_type_from(
            &reference.parts,
            reference.absolute,
            &lexical_context,
        )?;
        let mut local = self
            .constants
            .callable_bodies
            .iter()
            .filter(|fact| fact.constant == constant)
            .map(|fact| &fact.summary);
        if let Some(first) = local.next() {
            if local.any(|summary| summary != first) {
                return Some(Err(UnknownReason::AmbiguousCallableValue));
            }
            return Some(Ok(first.clone()));
        }
        self.semantics.project.constant_callable_body(&constant)
    }

    pub(in crate::indexer::fact_collector) fn infer_block_param_types_for_call(
        &self,
        node: &CallNode<'_>,
    ) -> (
        Vec<RubyType>,
        Option<Result<crate::inference::higher_order::PreparedCallableSet, UnknownReason>>,
    ) {
        if node.block().is_none() {
            return (Vec::new(), None);
        }
        if let Some(yield_param_types) = self.infer_block_param_types_from_yielding_method(node) {
            return (yield_param_types, None);
        }
        let method_name = node.name().as_slice();
        let param_count = block_required_param_count(node);
        let receiver_type = node.receiver().map(|receiver| {
            project_immediate_hash_receiver_type(&receiver, self.infer_type_from_value(&receiver))
        });

        let prepared = self.prepare_higher_order_call_for_node(node, &HashMap::new());
        let preparation_reason = match prepared {
            Ok(prepared) => {
                let parameter_types = prepared.block_parameter_types().to_vec();
                return (parameter_types, Some(Ok(prepared)));
            }
            Err(reason) => reason,
        };
        let Some(receiver_type) = receiver_type else {
            return (Vec::new(), Some(Err(preparation_reason)));
        };

        if shape_reads::is_shape_only(&receiver_type)
            && matches!(
                method_name,
                b"each" | b"each_pair" | b"each_key" | b"each_value"
            )
        {
            let key_type = match shape_reads::keys(&receiver_type) {
                Ok(RubyType::Array(types)) => RubyType::union(types),
                Ok(ruby_type) => unreachable_invariant!(
                    what = "shape keys projection returned `{ruby_type}` instead of Array",
                    why = "hash#keys always returns an Array",
                    fix = "keep shape_reads::keys canonical",
                    ruby_type = ruby_type,
                ),
                Err(_) => return (Vec::new(), Some(Err(preparation_reason))),
            };
            let value_type = match shape_reads::values(&receiver_type) {
                Ok(RubyType::Array(types)) => RubyType::union(types),
                Ok(ruby_type) => unreachable_invariant!(
                    what = "shape values projection returned `{ruby_type}` instead of Array",
                    why = "hash#values always returns an Array",
                    fix = "keep shape_reads::values canonical",
                    ruby_type = ruby_type,
                ),
                Err(_) => return (Vec::new(), Some(Err(preparation_reason))),
            };
            if matches!(method_name, b"each" | b"each_pair") {
                return if param_count == 1 {
                    (
                        vec![RubyType::Array(vec![key_type, value_type])],
                        Some(Err(preparation_reason)),
                    )
                } else {
                    (vec![key_type, value_type], Some(Err(preparation_reason)))
                };
            }
            if method_name == b"each_key" {
                return (vec![key_type], Some(Err(preparation_reason)));
            }
            if method_name == b"each_value" {
                return (vec![value_type], Some(Err(preparation_reason)));
            }
            unreachable_invariant!(
                what = "non-Hash iterator reached shape block parameter projection",
                why = "the method-name guard accepts only each variants",
                fix = "keep the guard and exhaustive projection branches aligned",
            );
        }

        match receiver_type {
            // Enumerable#each_with_index currently exposes its pair through a
            // tuple signature, which is outside the first callable-template
            // release. Keep this pre-existing precision until tuple templates
            // are represented; collection transforms above are signature-only.
            RubyType::Array(element_types) if method_name == b"each_with_index" => (
                vec![RubyType::union(element_types), RubyType::integer()],
                Some(Err(preparation_reason)),
            ),
            RubyType::Hash(key_types, value_types) if method_name == b"each" => {
                let key_type = RubyType::union(key_types);
                let value_type = RubyType::union(value_types);
                if param_count == 1 {
                    (
                        vec![RubyType::Array(vec![key_type, value_type])],
                        Some(Err(preparation_reason)),
                    )
                } else {
                    (vec![key_type, value_type], Some(Err(preparation_reason)))
                }
            }
            RubyType::Class(_)
            | RubyType::Module(_)
            | RubyType::ClassReference(_)
            | RubyType::ModuleReference(_)
            | RubyType::Array(_)
            | RubyType::Hash(_, _)
            | RubyType::Literal(_)
            | RubyType::Shape(_)
            | RubyType::Union(_)
            | RubyType::Unknown => (Vec::new(), Some(Err(preparation_reason))),
        }
    }

    pub(in crate::indexer::fact_collector) fn infer_block_param_types_from_yielding_method(
        &self,
        node: &CallNode<'_>,
    ) -> Option<Vec<RubyType>> {
        let method = RubyMethod::new(utf8_str(node.name().as_slice())).ok()?;
        let method_fqn = match node.receiver() {
            None => FullyQualifiedName::method(self.scope_tracker.get_ns_stack(), method),
            Some(receiver) if receiver.as_self_node().is_some() => {
                FullyQualifiedName::method(self.scope_tracker.get_ns_stack(), method)
            }
            Some(receiver) => {
                let fqn = self.constant_reference_type(&receiver)?;
                FullyQualifiedName::method(fqn.namespace_parts(), method)
            }
        };

        self.flow
            .method_yields
            .get(&method_fqn)
            .filter(|types| types.iter().any(|ty| *ty != RubyType::Unknown))
            .cloned()
    }

    pub(in crate::indexer::fact_collector) fn record_current_method_yield_types(
        &mut self,
        node: &YieldNode<'_>,
    ) {
        let Some(method_fqn) = self.scope_tracker.current_method_fqn().cloned() else {
            return;
        };
        let Some(arguments) = node.arguments() else {
            return;
        };
        let yield_types = arguments
            .arguments()
            .iter()
            .map(|arg| self.infer_type_from_value(&arg))
            .collect::<Vec<_>>();
        if yield_types.iter().all(|ty| *ty == RubyType::Unknown) {
            return;
        }
        let existing = self.flow.method_yields.entry(method_fqn).or_default();
        merge_position_types(existing, yield_types);
    }

    pub(in crate::indexer::fact_collector) fn record_current_method_forwarded_yield_types(
        &mut self,
        node: &CallNode<'_>,
    ) {
        if !call_forwards_anonymous_block(node) {
            return;
        }
        let Some(method_fqn) = self.scope_tracker.current_method_fqn().cloned() else {
            return;
        };
        let Some(yield_types) = self.infer_block_param_types_from_yielding_method(node) else {
            return;
        };
        if yield_types.iter().all(|ty| *ty == RubyType::Unknown) {
            return;
        }
        let existing = self.flow.method_yields.entry(method_fqn).or_default();
        merge_position_types(existing, yield_types);
    }
}

pub(in crate::indexer::fact_collector) fn block_required_param_count(node: &CallNode<'_>) -> usize {
    let Some(block) = node.block() else {
        return 0;
    };
    let Some(parameters) = block.as_block_node().and_then(|block| block.parameters()) else {
        return 0;
    };
    if let Some(numbered) = parameters.as_numbered_parameters_node() {
        return usize::from(numbered.maximum());
    }
    let Some(params_node) = parameters
        .as_block_parameters_node()
        .and_then(|node| node.parameters())
    else {
        return 0;
    };
    params_node.requireds().iter().count()
}

pub(in crate::indexer::fact_collector) fn merge_position_types(
    existing: &mut Vec<RubyType>,
    incoming: Vec<RubyType>,
) {
    for (index, incoming_type) in incoming.into_iter().enumerate() {
        if incoming_type == RubyType::Unknown {
            continue;
        }
        if existing.len() <= index {
            existing.resize(index, RubyType::Unknown);
            existing.push(incoming_type);
            continue;
        }
        let merged = RubyType::union([existing[index].clone(), incoming_type]);
        existing[index] = merged;
    }
}

pub(in crate::indexer::fact_collector) fn call_forwards_anonymous_block(
    node: &CallNode<'_>,
) -> bool {
    if node
        .block()
        .and_then(|block| block.as_block_argument_node())
        .is_some_and(|block_argument| block_argument.expression().is_none())
    {
        return true;
    }

    node.arguments()
        .map(|arguments| {
            arguments.arguments().iter().any(|argument| {
                if argument.as_forwarding_arguments_node().is_some() {
                    return true;
                }

                argument
                    .as_block_argument_node()
                    .is_some_and(|block_argument| block_argument.expression().is_none())
            })
        })
        .unwrap_or(false)
}
