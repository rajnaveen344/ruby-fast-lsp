use crate::core::{FullyQualifiedName, RubyMethod, RubyType, TypeInferenceOutcome, UnknownReason};
use crate::indexer::fact_collector::FactCollector;
use crate::indexer::utf8_str;
use crate::inference::r#type::literal::project_immediate_hash_receiver_type;
use crate::inference::r#type::shape as shape_reads;
use crate::inference::type_tracker::TypeTracker;
use ruby_prism::*;
use std::collections::{HashMap, HashSet};

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

impl FactCollector {
    pub(crate) fn invalidate_escaped_callables_in_value(&mut self, value: &Node<'_>) {
        if self.flow.local_callables.is_empty() {
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

    pub(in crate::indexer::fact_collector) fn invalidate_callable_names(
        &mut self,
        names: HashSet<String>,
    ) {
        for name in names {
            let Some(identity) = self
                .flow
                .local_callables
                .get(&name)
                .map(|callable| callable.identity)
            else {
                continue;
            };
            for callable in self.flow.local_callables.values_mut() {
                if callable.identity == identity {
                    callable.summary = Err(UnknownReason::EscapedCallableValue);
                }
            }
        }
    }

    pub(in crate::indexer::fact_collector) fn invalidate_escaped_callables_in_call(
        &mut self,
        node: &CallNode<'_>,
    ) {
        if self.flow.local_callables.is_empty() {
            return;
        }
        let mut escaped = EscapedCallableReadCollector::default();
        if let Some(receiver) = node.receiver() {
            let direct_invoke = node.name().as_slice() == b"call"
                && receiver.as_local_variable_read_node().is_some();
            if !direct_invoke {
                escaped.visit(&receiver);
            }
        }
        if let Some(arguments) = node.arguments() {
            escaped.visit_arguments_node(&arguments);
        }
        if node
            .block()
            .is_some_and(|block| block.as_block_node().is_some())
        {
            escaped.visit(node.block().as_ref().expect("checked block presence"));
        }
        self.invalidate_callable_names(escaped.names);
    }

    pub(crate) fn bind_local_callable(
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
            .flow
            .local_callables
            .iter()
            .filter(|(existing_name, existing)| {
                existing_name.as_str() != name && existing.identity == callable.identity
            })
            .count();
        if alias_count >= crate::core::callable_body::MAX_CALLABLE_BODY_ALIASES {
            callable.summary = Err(UnknownReason::CallableBodyBoundExceeded);
            for existing in self.flow.local_callables.values_mut() {
                if existing.identity == callable.identity {
                    existing.summary = Err(UnknownReason::CallableBodyBoundExceeded);
                }
            }
        }
        self.flow.local_callables.insert(name, callable);
    }

    pub(in crate::indexer::fact_collector) fn merge_local_callables(
        left: HashMap<String, crate::inference::higher_order::KnownProcType>,
        right: HashMap<String, crate::inference::higher_order::KnownProcType>,
    ) -> HashMap<String, crate::inference::higher_order::KnownProcType> {
        let names = left
            .keys()
            .chain(right.keys())
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();
        let mut merged = HashMap::with_capacity(names.len());
        for name in names {
            let callable = match (left.get(&name), right.get(&name)) {
                (Some(left), Some(right)) if left == right => left.clone(),
                (Some(left), Some(right)) => crate::inference::higher_order::KnownProcType {
                    identity: left.identity.min(right.identity),
                    summary: Err(UnknownReason::AmbiguousCallableValue),
                },
                (Some(callable), None) | (None, Some(callable)) => {
                    crate::inference::higher_order::KnownProcType {
                        identity: callable.identity,
                        summary: Err(UnknownReason::AmbiguousCallableValue),
                    }
                }
                (None, None) => panic!(
                    "INVARIANT VIOLATED: callable merge key `{name}` is absent from both branches. This is a bug because keys are derived from those exact maps. Fix: keep key collection and lookup atomic."
                ),
            };
            merged.insert(name, callable);
        }
        merged
    }

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
        if receiver_type.as_ref().is_some_and(|receiver| {
            receiver == &RubyType::Unknown || RubyType::contains_unknown(receiver)
        }) {
            return Err(UnknownReason::IncompleteBlockInput);
        }
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
        let engine = self.semantics.engine.read();
        let query = crate::engine::AnalysisQuery::new(&engine);
        let namespace = FullyQualifiedName::namespace(self.scope_tracker.get_ns_stack());
        crate::inference::rbs::prepare_higher_order_call_with_fallbacks(
            Some(&query),
            Some(self.semantics.query_cache.as_ref()),
            receiver_type.as_ref(),
            Some(&namespace),
            &method_name,
            &argument_types,
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
            let parameter_names = block_parameter_names(&block);
            let mut block_locals = local_types.clone();
            for (index, name) in parameter_names.iter().enumerate() {
                let parameter_type = parameter_types
                    .get(index)
                    .cloned()
                    .unwrap_or_else(RubyType::nil_class);
                block_locals.insert(name.clone(), parameter_type);
            }
            let tracked_parameters = parameter_names
                .iter()
                .enumerate()
                .map(|(index, name)| {
                    (
                        name.clone(),
                        parameter_types
                            .get(index)
                            .cloned()
                            .unwrap_or_else(RubyType::nil_class),
                    )
                })
                .collect::<Vec<_>>();
            let mut tracker = TypeTracker::new()
                .with_analysis_engine(self.semantics.engine.clone())
                .with_analysis_query_cache(self.semantics.query_cache.clone());
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
        let Some(expression) = block_argument.expression() else {
            return Some(TypeInferenceOutcome::unknown(
                UnknownReason::UnsupportedCallable,
            ));
        };
        if let Some(symbol) = expression.as_symbol_node() {
            let target = std::str::from_utf8(symbol.unescaped()).ok()?;
            return Some(
                prepared.finish_static_method(target, |receiver_type, target| {
                    self.resolve_method_return_type_outcome_with_private(
                        receiver_type,
                        target,
                        false,
                    )
                }),
            );
        }
        let callable = if let Some(local) = expression.as_local_variable_read_node() {
            let name = String::from_utf8_lossy(local.name().as_slice()).to_string();
            self.flow.local_callables.get(&name).cloned().map(Ok)
        } else {
            self.constant_callable_body_for_node(&expression)
                .map(|result| {
                    result.map(|summary| crate::inference::higher_order::KnownProcType {
                        identity: u32::MAX,
                        summary: Ok(summary),
                    })
                })
        };
        Some(callable.map_or_else(
            || TypeInferenceOutcome::unknown(UnknownReason::UnsupportedCallable),
            |callable| match callable {
                Err(reason) => TypeInferenceOutcome::unknown(reason),
                Ok(callable) => {
                    let mut stack = vec![callable.identity];
                    prepared.finish_known_proc(
                        &callable,
                        |capture| {
                            local_types.get(capture).cloned().or_else(|| {
                                self.get_local_var_type(capture, &expression.location())
                            })
                        },
                        |capture, arguments| {
                            let nested = self.flow.local_callables.get(capture)?.clone();
                            Some(self.instantiate_known_proc_with_stack(
                                &nested,
                                arguments,
                                local_types,
                                &expression.location(),
                                &mut stack,
                            ))
                        },
                        |receiver, method, _arguments| {
                            self.resolve_method_return_type_outcome_with_private(
                                receiver,
                                method.as_str(),
                                false,
                            )
                        },
                    )
                }
            },
        ))
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

    pub(crate) fn infer_known_proc_type(
        &self,
        value_node: &Node,
    ) -> Option<crate::inference::higher_order::KnownProcType> {
        crate::indexer::is_static_callable_literal(value_node).then(|| {
            let scope_id = self.document.variable_scopes().current_scope().expect(
                "INVARIANT VIOLATED: callable lowering ran without an active lexical scope. This is a bug because FactCollector and VariableScopes must enter and exit scopes together. Fix: keep callable lowering inside the ordinary collector traversal.",
            );
            let outer_locals = self
                .document
                .variable_scopes()
                .get_visible_variables(scope_id)
                .into_iter()
                .map(|variable| variable.name.to_string());
            crate::inference::higher_order::KnownProcType {
                identity: u32::try_from(value_node.location().start_offset()).expect(
                    "INVARIANT VIOLATED: callable literal offset exceeded u32. This is a bug because analysis ranges already require u32 offsets. Fix: reject oversized source before callable lowering.",
                ),
                summary: crate::indexer::lower_callable_literal_with_outer_locals(
                    value_node,
                    outer_locals,
                ),
            }
        })
    }

    pub(in crate::indexer::fact_collector) fn instantiate_known_proc_with_stack(
        &self,
        callable: &crate::inference::higher_order::KnownProcType,
        arguments: &[RubyType],
        local_types: &HashMap<String, RubyType>,
        location: &Location<'_>,
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
            |capture| {
                local_types
                    .get(capture)
                    .cloned()
                    .or_else(|| self.get_local_var_type(capture, location))
            },
            |capture, nested_arguments| {
                let nested = self.flow.local_callables.get(capture)?.clone();
                Some(self.instantiate_known_proc_with_stack(
                    &nested,
                    nested_arguments,
                    local_types,
                    location,
                    stack,
                ))
            },
            |receiver, method, _arguments| {
                self.resolve_method_return_type_outcome_with_private(
                    receiver,
                    method.as_str(),
                    false,
                )
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
        let callable = if let Some(local) = receiver.as_local_variable_read_node() {
            let name = String::from_utf8_lossy(local.name().as_slice()).to_string();
            self.flow.local_callables.get(&name)?.clone()
        } else {
            match self.constant_callable_body_for_node(&receiver)? {
                Ok(summary) => crate::inference::higher_order::KnownProcType {
                    identity: u32::MAX,
                    summary: Ok(summary),
                },
                Err(reason) => return Some(TypeInferenceOutcome::unknown(reason)),
            }
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
        Some(self.instantiate_known_proc_with_stack(
            &callable,
            &argument_types,
            local_types,
            &call_node.location(),
            &mut Vec::new(),
        ))
    }

    pub(in crate::indexer::fact_collector) fn constant_callable_body_for_node(
        &self,
        node: &Node<'_>,
    ) -> Option<Result<crate::core::CallableBodySummary, UnknownReason>> {
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
        let engine = self.semantics.engine.read();
        crate::engine::AnalysisQuery::new(&engine).constant_callable_body(&constant)
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
                Ok(ruby_type) => panic!(
                    "INVARIANT VIOLATED: shape keys projection returned `{ruby_type}` instead of Array. This is a bug because Hash#keys always returns an Array. Fix: keep shape_reads::keys canonical."
                ),
                Err(_) => return (Vec::new(), Some(Err(preparation_reason))),
            };
            let value_type = match shape_reads::values(&receiver_type) {
                Ok(RubyType::Array(types)) => RubyType::union(types),
                Ok(ruby_type) => panic!(
                    "INVARIANT VIOLATED: shape values projection returned `{ruby_type}` instead of Array. This is a bug because Hash#values always returns an Array. Fix: keep shape_reads::values canonical."
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
            panic!(
                "INVARIANT VIOLATED: non-Hash iterator reached shape block parameter projection. This is a bug because the method-name guard accepts only each variants. Fix: keep the guard and exhaustive projection branches aligned."
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

pub(in crate::indexer::fact_collector) fn block_parameter_names(
    block: &BlockNode<'_>,
) -> Vec<String> {
    let Some(parameters_node) = block.parameters() else {
        return Vec::new();
    };
    if let Some(numbered) = parameters_node.as_numbered_parameters_node() {
        return numbered_parameter_names(numbered);
    }
    let Some(params_node) = parameters_node
        .as_block_parameters_node()
        .and_then(|node| node.parameters())
    else {
        return Vec::new();
    };

    let mut names = Vec::new();
    for required in params_node.requireds().iter() {
        if let Some(param) = required.as_required_parameter_node() {
            names.push(String::from_utf8_lossy(param.name().as_slice()).to_string());
        }
    }
    for optional in params_node.optionals().iter() {
        if let Some(param) = optional.as_optional_parameter_node() {
            names.push(String::from_utf8_lossy(param.name().as_slice()).to_string());
        }
    }
    if let Some(rest) = params_node.rest() {
        if let Some(param) = rest.as_rest_parameter_node() {
            if let Some(name) = param.name() {
                names.push(String::from_utf8_lossy(name.as_slice()).to_string());
            }
        }
    }
    for post in params_node.posts().iter() {
        if let Some(param) = post.as_required_parameter_node() {
            names.push(String::from_utf8_lossy(param.name().as_slice()).to_string());
        }
    }
    names
}

pub(in crate::indexer::fact_collector) fn numbered_parameter_names(
    params: NumberedParametersNode<'_>,
) -> Vec<String> {
    (1..=usize::from(params.maximum()))
        .map(|index| format!("_{index}"))
        .collect()
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
