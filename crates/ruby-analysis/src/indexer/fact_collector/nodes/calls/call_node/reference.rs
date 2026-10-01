//! Method reference candidates and immediate call outcomes for ordinary calls.

use crate::core::{
    FullyQualifiedName, MethodReferenceAccess, NamespaceKind, ReferenceCandidate, RubyMethod,
    TypeInferenceOutcome, UnknownReason,
};
use crate::indexer::utf8_str;
use crate::invariant::ExpectInvariant;
use log::trace;
use ruby_prism::CallNode;

use crate::core::RubyType;
use crate::inference::method::return_type::{
    rbs_class_exists_for_type, rbs_method_exists_for_type,
};
use crate::inference::r#type::literal::literal_key;
use crate::inference::r#type::shape as shape_reads;

use super::names::define_method_name_and_range;
use super::receivers::ReceiverInfo;
use crate::indexer::fact_collector::FactCollector;

impl FactCollector {
    pub(super) fn process_call_reference_candidate(&mut self, node: &CallNode) {
        let static_send_target = static_send_target_name_and_range(self, node);
        let method_name = static_send_target
            .as_ref()
            .map(|(name, _range)| name.as_str())
            .unwrap_or_else(|| utf8_str(node.name().as_slice()));
        let call_range =
            self.text_range_from_prism_location(&node.location(), "method reference candidate");
        if !RubyMethod::is_valid_ruby_method_name(method_name) {
            self.record_immediate_call_outcome(
                call_range,
                TypeInferenceOutcome::unknown(UnknownReason::InvalidMethodName),
            );
            trace!("Skipping method call with invalid name: {}", method_name);
            return;
        }

        let receiver_uses_implicit_self = node
            .receiver()
            .is_none_or(|receiver| receiver.as_self_node().is_some());
        if receiver_uses_implicit_self && !self.scope_tracker.implicit_receiver_context_is_proven()
        {
            self.record_immediate_call_outcome(
                call_range,
                TypeInferenceOutcome::unknown(UnknownReason::UnknownReceiver),
            );
            return;
        }

        self.push_const_lookup_reference_candidate(node);
        self.push_reflected_method_reference_candidate(node);

        let message_range = static_send_target
            .as_ref()
            .map(|(_name, range)| *range)
            .or_else(|| {
                crate::indexer::call_reference_location(node).map(|loc| {
                    self.text_range_from_prism_location(&loc, "method diagnostic candidate")
                })
            })
            .unwrap_or(call_range);
        let current_namespace = self.scope_tracker.get_ns_stack();
        let (target_namespace, namespace_kind, receiver_info, inferred_expr_type) =
            match node.receiver() {
                Some(receiver_node) => {
                    self.handle_receiver_node_with_info(&receiver_node, &current_namespace)
                }
                None => {
                    let (ns, kind) = self.handle_no_receiver(&current_namespace);
                    (ns, kind, ReceiverInfo::NoReceiver, None)
                }
            };

        // A constant receiver reports Unknown only when it names a value
        // whose type is unproven; an unresolved constant keeps its namespace.
        let inference_failed = match receiver_info {
            ReceiverInfo::ExpressionReceiver | ReceiverInfo::InvalidConstantPath => {
                inferred_expr_type
                    .as_ref()
                    .is_none_or(|ruby_type| *ruby_type == RubyType::Unknown)
                    && target_namespace == current_namespace
            }
            ReceiverInfo::ConstantReceiver(_) => inferred_expr_type == Some(RubyType::Unknown),
            ReceiverInfo::NoReceiver | ReceiverInfo::SelfReceiver => false,
        };

        let method = match RubyMethod::new(method_name) {
            Ok(method) => method,
            Err(err) => {
                trace!("Failed to create RubyMethod for '{}': {}", method_name, err);
                return;
            }
        };

        let receiver_type = inferred_expr_type
            .as_ref()
            .filter(|ruby_type| **ruby_type != RubyType::Unknown)
            .cloned()
            .or_else(|| match &receiver_info {
                ReceiverInfo::ConstantReceiver(_) if inference_failed => None,
                ReceiverInfo::ConstantReceiver(_) if !target_namespace.is_empty() => {
                    self.proven_namespace_receiver_type(&target_namespace)
                }
                ReceiverInfo::NoReceiver | ReceiverInfo::SelfReceiver
                    if !target_namespace.is_empty() =>
                {
                    let target = FullyQualifiedName::constant(target_namespace.clone());
                    Some(match namespace_kind {
                        NamespaceKind::Instance => RubyType::Class(target),
                        NamespaceKind::Singleton => RubyType::ClassReference(target),
                    })
                }
                ReceiverInfo::NoReceiver
                | ReceiverInfo::SelfReceiver
                | ReceiverInfo::ConstantReceiver(_)
                | ReceiverInfo::ExpressionReceiver
                | ReceiverInfo::InvalidConstantPath => None,
            })
            .map(|ruby_type| {
                node.receiver().map_or(ruby_type.clone(), |receiver| {
                    crate::inference::r#type::literal::project_immediate_hash_receiver_type(
                        &receiver, ruby_type,
                    )
                })
            });
        // `&.` sends no message to nil: only the non-nil receiver is
        // dispatched, and a nil branch contributes nil to the call result.
        let safe_navigation = node.is_safe_navigation();
        let (dispatch_receiver_type, nil_skips_dispatch) = match receiver_type.clone() {
            Some(ruby_type) if safe_navigation => match ruby_type.safe_navigation_dispatch() {
                Some((receiver, nil_skips_dispatch)) => (Some(receiver), nil_skips_dispatch),
                None => (None, true),
            },
            receiver_type => (receiver_type, false),
        };
        let receiver_is_only_nil = nil_skips_dispatch && dispatch_receiver_type.is_none();
        let receiver_expression_range = matches!(
            receiver_info,
            ReceiverInfo::ExpressionReceiver | ReceiverInfo::InvalidConstantPath
        )
        .then(|| node.receiver())
        .flatten()
        .map(|receiver| self.direct_range(&receiver.location()));
        // Every expression receiver is revalidated against the engine-owned
        // solved expression at finalization. This includes locals and nonlocal
        // reads, whose exhaustive flow result can invalidate a concrete type
        // observed by the syntax-local collector.
        let deferred_receiver_may_resolve = node.receiver().is_some_and(|receiver| {
            if receiver.as_call_node().is_some() {
                let range = self.direct_range(&receiver.location());
                self.expressions.deferred_calls.contains(&range)
            } else {
                receiver_type.is_some()
            }
        });
        let access = method_reference_access(node, &receiver_info, static_send_target.is_some());
        let immediate_outcome = if inference_failed && deferred_receiver_may_resolve {
            None
        } else if inference_failed {
            let receiver_reason = receiver_expression_range
                .and_then(|range| self.expressions.unknown_reasons.get(&range).copied());
            let reason = match receiver_reason {
                // These reasons describe why a previously complete shape is
                // no longer available. Preserve that boundary at the keyed
                // operation itself so consumers do not see an unhelpful
                // generic receiver failure.
                Some(
                    UnknownReason::ShapeBoundExceeded
                    | UnknownReason::MutableShapeInvalidated
                    | UnknownReason::UnsupportedCallable
                    | UnknownReason::IncompleteBlockInput
                    | UnknownReason::IncompleteBlockResult
                    | UnknownReason::IncompleteGenericSubstitution
                    | UnknownReason::AmbiguousCallableOverload
                    | UnknownReason::HigherOrderBoundExceeded
                    | UnknownReason::UnsupportedBlockFlow
                    | UnknownReason::UnsupportedCallableBody
                    | UnknownReason::IncompleteCallableInput
                    | UnknownReason::IncompleteCallableCapture
                    | UnknownReason::AmbiguousCallableValue
                    | UnknownReason::EscapedCallableValue
                    | UnknownReason::CallableBodyBoundExceeded
                    | UnknownReason::CallableRecursionUnsupported
                    | UnknownReason::UnsupportedCallableFlow,
                ) => receiver_reason.expect_invariant(
                    "checked shape receiver reason disappeared",
                    "receiver_reason is immutable",
                    "bind the matched reason directly",
                ),
                // Ordinary assignment/scope failures prove only that this
                // call has no receiver. The receiver expression retains its
                // own more specific reason at its exact range.
                Some(
                    UnknownReason::NoReachingAssignment
                    | UnknownReason::UnresolvedAssignmentValue
                    | UnknownReason::AmbiguousReachingAssignment
                    | UnknownReason::UnknownReceiver
                    | UnknownReason::InvalidMethodName
                    | UnknownReason::UnresolvedMethodReturn
                    | UnknownReason::IncompleteUnionMember
                    | UnknownReason::UnprovenRecursiveCycle,
                )
                | None => UnknownReason::UnknownReceiver,
            };
            Some(TypeInferenceOutcome::unknown(reason))
        } else if receiver_is_only_nil {
            Some(TypeInferenceOutcome::proven(RubyType::nil_class()))
        } else if let Some(receiver_type) = dispatch_receiver_type.as_ref() {
            if method_name == "freeze" {
                Some(TypeInferenceOutcome::proven(receiver_type.clone()))
            } else if let Some(outcome) = self.shape_call_outcome(node, receiver_type) {
                Some(outcome)
            } else {
                let syntax_outcome =
                    crate::inference::method::return_type::method_call_type_outcome(
                        None,
                        receiver_type,
                        method_name,
                    );
                let outcome = if matches!(receiver_type, RubyType::Union(_))
                    && syntax_outcome.unknown_reason() == Some(UnknownReason::IncompleteUnionMember)
                {
                    self.resolve_method_return_type_outcome_with_private(
                        receiver_type,
                        method_name,
                        !matches!(access, MethodReferenceAccess::ExplicitReceiver),
                    )
                } else {
                    syntax_outcome
                };
                match outcome.unknown_reason() {
                    None
                    | Some(
                        UnknownReason::IncompleteUnionMember
                        | UnknownReason::ShapeBoundExceeded
                        | UnknownReason::MutableShapeInvalidated
                        | UnknownReason::UnsupportedCallable
                        | UnknownReason::IncompleteBlockInput
                        | UnknownReason::IncompleteBlockResult
                        | UnknownReason::IncompleteGenericSubstitution
                        | UnknownReason::AmbiguousCallableOverload
                        | UnknownReason::HigherOrderBoundExceeded
                        | UnknownReason::UnsupportedBlockFlow
                        | UnknownReason::UnsupportedCallableBody
                        | UnknownReason::IncompleteCallableInput
                        | UnknownReason::IncompleteCallableCapture
                        | UnknownReason::AmbiguousCallableValue
                        | UnknownReason::EscapedCallableValue
                        | UnknownReason::CallableBodyBoundExceeded
                        | UnknownReason::CallableRecursionUnsupported
                        | UnknownReason::UnsupportedCallableFlow,
                    ) => Some(outcome),
                    Some(
                        UnknownReason::NoReachingAssignment
                        | UnknownReason::UnresolvedAssignmentValue
                        | UnknownReason::AmbiguousReachingAssignment
                        | UnknownReason::UnknownReceiver
                        | UnknownReason::InvalidMethodName
                        | UnknownReason::UnresolvedMethodReturn
                        | UnknownReason::UnprovenRecursiveCycle,
                    ) => None,
                }
            }
        } else if matches!(
            receiver_info,
            ReceiverInfo::NoReceiver
                | ReceiverInfo::SelfReceiver
                | ReceiverInfo::ConstantReceiver(_)
        ) {
            None
        } else {
            Some(TypeInferenceOutcome::unknown(
                UnknownReason::UnknownReceiver,
            ))
        };
        let immediate_outcome = if nil_skips_dispatch {
            immediate_outcome.map(TypeInferenceOutcome::with_nil_alternative)
        } else {
            immediate_outcome
        };
        let defer_call_outcome = immediate_outcome.is_none();
        if let Some(outcome) = immediate_outcome {
            self.record_immediate_call_outcome(call_range, outcome);
        }

        if !inference_failed || deferred_receiver_may_resolve {
            let receiver_label = match (&receiver_info, inferred_expr_type.as_ref()) {
                (ReceiverInfo::ConstantReceiver(name), _) => Some(name.clone()),
                (
                    ReceiverInfo::ExpressionReceiver | ReceiverInfo::InvalidConstantPath,
                    Some(ruby_type),
                ) => Some(ruby_type.to_string()),
                (ReceiverInfo::NoReceiver | ReceiverInfo::SelfReceiver, _)
                | (ReceiverInfo::ExpressionReceiver | ReceiverInfo::InvalidConstantPath, None) => {
                    None
                }
            };
            let signature = if self.options.diagnostics_enabled {
                self.method_call_signature_candidate(
                    node,
                    usize::from(static_send_target.is_some()),
                )
            } else {
                crate::core::MethodCallSignatureCandidate::default()
            };
            let rbs_resolves_method = inferred_expr_type.as_ref().is_some_and(|ruby_type| {
                rbs_method_exists_for_type(ruby_type, &method, namespace_kind)
            });
            let rbs_receiver_class_exists = inferred_expr_type
                .as_ref()
                .is_some_and(rbs_class_exists_for_type);
            self.facts.references.push(ReferenceCandidate::method(
                message_range,
                crate::core::MethodReferenceCandidate {
                    owner: target_namespace,
                    owner_kind: namespace_kind,
                    method,
                    is_super: false,
                    access,
                    caller: self.scope_tracker.current_method_fqn().cloned(),
                    call_expression_range: defer_call_outcome.then_some(call_range),
                    preferred_definition_range: None,
                    diagnostics: crate::core::MethodReferenceDiagnostics {
                        diagnostic_range: message_range,
                        receiver_label,
                        receiver_expression_range,
                        receiver_type: receiver_type.clone().map(Box::new),
                        diagnose_unresolved: self.options.diagnostics_enabled
                            && !rbs_resolves_method
                            && !matches!(receiver_info, ReceiverInfo::SelfReceiver),
                        allow_unindexed_owner: rbs_receiver_class_exists,
                        safe_navigation,
                        signature: Some(signature),
                    },
                },
            ));
            if defer_call_outcome {
                invariant!(
                    self.expressions.deferred_calls.insert(call_range),
                    what = "one call expression registered multiple deferred outcomes",
                    why = "one runtime call has exactly one method candidate",
                    fix = "classify each CallNode once before retaining its deferred range",
                );
            }
        }

        if self.options.diagnostics_enabled && method_name == "raise" && node.receiver().is_none() {
            if let Some(candidate) = self.raise_non_exception_candidate(node) {
                self.facts.diagnostic_candidates.push(candidate);
            }
        }

        if self.options.diagnostics_enabled {
            for entry in super::super::bad_splat::check(node, &self.document) {
                let candidate = self.bad_splat_candidate(entry);
                self.facts.diagnostic_candidates.push(candidate);
            }
        }
    }

    fn shape_call_outcome(
        &self,
        node: &CallNode<'_>,
        receiver_type: &RubyType,
    ) -> Option<TypeInferenceOutcome> {
        if !shape_reads::is_shape_only(receiver_type) {
            return None;
        }
        let method_name = String::from_utf8_lossy(node.name().as_slice());
        let arguments = node
            .arguments()
            .map(|arguments| arguments.arguments().iter().collect::<Vec<_>>())
            .unwrap_or_default();
        let argument_types = arguments
            .iter()
            .map(|argument| self.infer_assignment_type_from_value(argument))
            .collect::<Vec<_>>();
        let result = match method_name.as_ref() {
            "[]" if arguments.len() == 1 => Some(shape_reads::indexed_read(
                receiver_type,
                literal_key(&arguments[0]).as_ref(),
            )),
            "fetch" if matches!(arguments.len(), 1 | 2) => Some(shape_reads::fetch(
                receiver_type,
                literal_key(&arguments[0]).as_ref(),
                argument_types.get(1),
            )),
            "dig" if !arguments.is_empty() => {
                let keys = arguments.iter().map(literal_key).collect::<Vec<_>>();
                Some(shape_reads::dig(receiver_type, &keys))
            }
            "key?" | "has_key?" | "include?" | "member?" if arguments.len() == 1 => Some(
                shape_reads::key_presence(receiver_type, literal_key(&arguments[0]).as_ref()),
            ),
            "keys" if arguments.is_empty() => Some(shape_reads::keys(receiver_type)),
            "values" if arguments.is_empty() => Some(shape_reads::values(receiver_type)),
            "each" | "each_pair" | "each_key" | "each_value" if arguments.is_empty() => Some(
                shape_reads::each_return(receiver_type, node.block().is_some()),
            ),
            _ => None,
        }?;
        Some(match result {
            Ok(ruby_type) => TypeInferenceOutcome::proven(ruby_type),
            Err(reason) => TypeInferenceOutcome::unknown(reason),
        })
    }
}

fn static_send_target_name_and_range(
    visitor: &FactCollector,
    node: &CallNode<'_>,
) -> Option<(String, crate::core::TextRange)> {
    match node.name().as_slice() {
        b"send" | b"public_send" | b"__send__" => {}
        _ => return None,
    }

    let (name, range) = define_method_name_and_range(visitor, node, 0)?;
    if name == "define_method" {
        return None;
    }
    RubyMethod::is_valid_ruby_method_name(&name).then_some((name, range))
}

fn method_reference_access(
    node: &CallNode<'_>,
    receiver_info: &ReceiverInfo,
    static_send_target: bool,
) -> MethodReferenceAccess {
    if static_send_target {
        return match node.name().as_slice() {
            b"send" | b"__send__" => MethodReferenceAccess::VisibilityBypass,
            b"public_send" => MethodReferenceAccess::ExplicitReceiver,
            other => unreachable_invariant!(
                what = "static send target came from unsupported call `{}`",
                why = "only send/public_send/__send__ calls expose a reflected target",
                fix = "keep static_send_target_name_and_range and method_reference_access in sync",
                String::from_utf8_lossy(other),
            ),
        };
    }

    match receiver_info {
        ReceiverInfo::NoReceiver => MethodReferenceAccess::Normal,
        ReceiverInfo::SelfReceiver
        | ReceiverInfo::ConstantReceiver(_)
        | ReceiverInfo::ExpressionReceiver
        | ReceiverInfo::InvalidConstantPath => MethodReferenceAccess::ExplicitReceiver,
    }
}
