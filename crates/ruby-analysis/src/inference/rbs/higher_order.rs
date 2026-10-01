//! Higher-order call preparation from project signature facts with embedded RBS fallback.

use super::embedded_callable::{prepare_rbs_higher_order_call, rbs_receiver_identity};
use crate::core::{
    CallableBlockTemplate, CallableSignature, CallableTypeTemplate, FullyQualifiedName, MethodFact,
    MethodParamKind, RubyMethod, RubyType,
};
use crate::engine::{AnalysisQuery, AnalysisQueryCache};
use crate::inference::higher_order::{prepare_callable_set, PreparedCallableSet};

fn receiver_uses_embedded_rbs_without_engine(receiver_type: &RubyType) -> bool {
    matches!(
        receiver_type,
        RubyType::Array(_) | RubyType::Hash(_, _) | RubyType::Shape(_)
    )
}

/// Prepare a higher-order call from ordinary file-owned method facts, falling
/// back to the embedded language signatures only when no project signature
/// fact supplies callable evidence.
pub(crate) fn prepare_higher_order_call(
    query: Option<&AnalysisQuery<'_>>,
    cache: Option<&AnalysisQueryCache>,
    receiver_type: &RubyType,
    method_name: &str,
    argument_types: &[RubyType],
) -> Result<PreparedCallableSet, crate::core::UnknownReason> {
    if receiver_uses_embedded_rbs_without_engine(receiver_type) {
        return prepare_rbs_higher_order_call(receiver_type, method_name, argument_types);
    }
    if let Some(query) = query {
        let method = RubyMethod::new(method_name)
            .map_err(|_| crate::core::UnknownReason::UnsupportedCallable)?;
        let signature_facts =
            resolve_signature_facts_for_type(query, cache, receiver_type, &method);
        return fallback_unsupported_callable(
            prepare_from_callable_signatures(
                Some(receiver_type),
                signature_facts.as_slice(),
                argument_types,
            ),
            || prepare_rbs_higher_order_call(receiver_type, method_name, argument_types),
        );
    }
    prepare_rbs_higher_order_call(receiver_type, method_name, argument_types)
}

pub(super) fn prepare_higher_order_call_with_fallbacks_uncached(
    query: Option<&AnalysisQuery<'_>>,
    cache: Option<&AnalysisQueryCache>,
    receiver_type: Option<&RubyType>,
    implicit_namespace: Option<&FullyQualifiedName>,
    method_name: &str,
    argument_types: &[RubyType],
) -> Result<PreparedCallableSet, crate::core::UnknownReason> {
    if let Some(receiver_type) = receiver_type
        .filter(|receiver_type| receiver_uses_embedded_rbs_without_engine(receiver_type))
    {
        return prepare_rbs_higher_order_call(receiver_type, method_name, argument_types);
    }

    let Some(query) = query else {
        return receiver_type.map_or(
            Err(crate::core::UnknownReason::UnsupportedCallable),
            |receiver_type| {
                prepare_rbs_higher_order_call(receiver_type, method_name, argument_types)
            },
        );
    };
    let method = RubyMethod::new(method_name)
        .map_err(|_| crate::core::UnknownReason::UnsupportedCallable)?;
    let facts = match (receiver_type, implicit_namespace) {
        (Some(receiver_type), _) => {
            resolve_signature_facts_for_type(query, cache, receiver_type, &method)
        }
        (None, Some(namespace)) => resolve_signature_facts(query, cache, namespace, &method),
        (None, None) => return Err(crate::core::UnknownReason::IncompleteBlockInput),
    };
    fallback_unsupported_callable(
        prepare_from_callable_signatures(receiver_type, &facts, argument_types),
        || {
            fallback_unsupported_callable(
                prepare_forwarded_from_facts(query, cache, &facts, argument_types),
                || {
                    fallback_unsupported_callable(
                        prepare_direct_yield_from_facts(&facts, argument_types),
                        || {
                            receiver_type.map_or(
                                Err(crate::core::UnknownReason::UnsupportedCallable),
                                |receiver_type| {
                                    prepare_rbs_higher_order_call(
                                        receiver_type,
                                        method_name,
                                        argument_types,
                                    )
                                },
                            )
                        },
                    )
                },
            )
        },
    )
}

fn resolve_signature_facts_for_type(
    query: &AnalysisQuery<'_>,
    cache: Option<&AnalysisQueryCache>,
    receiver_type: &RubyType,
    method: &RubyMethod,
) -> std::sync::Arc<Vec<MethodFact>> {
    cache.map_or_else(
        || {
            std::sync::Arc::new(
                query.resolve_method_signature_facts_for_type(receiver_type, method),
            )
        },
        |cache| {
            std::sync::Arc::new(query.resolve_method_signature_facts_for_type_cached(
                receiver_type,
                method,
                cache,
            ))
        },
    )
}

fn resolve_signature_facts(
    query: &AnalysisQuery<'_>,
    cache: Option<&AnalysisQueryCache>,
    namespace: &FullyQualifiedName,
    method: &RubyMethod,
) -> std::sync::Arc<Vec<MethodFact>> {
    cache.map_or_else(
        || std::sync::Arc::new(query.resolve_method_signature_facts(namespace, method)),
        |cache| query.resolve_method_signature_facts_cached_arc(namespace, method, cache),
    )
}

fn fallback_unsupported_callable(
    result: Result<PreparedCallableSet, crate::core::UnknownReason>,
    next: impl FnOnce() -> Result<PreparedCallableSet, crate::core::UnknownReason>,
) -> Result<PreparedCallableSet, crate::core::UnknownReason> {
    match result {
        Err(crate::core::UnknownReason::UnsupportedCallable) => next(),
        other => other,
    }
}

fn prepare_from_callable_signatures(
    receiver_type: Option<&RubyType>,
    signature_facts: &[MethodFact],
    argument_types: &[RubyType],
) -> Result<PreparedCallableSet, crate::core::UnknownReason> {
    let signatures = signature_facts
        .iter()
        .flat_map(|fact| fact.callable_signatures().iter().cloned())
        .collect::<Vec<_>>();
    if signatures.is_empty() {
        return Err(crate::core::UnknownReason::UnsupportedCallable);
    }
    let receiver_parameter_names = signatures[0].receiver_type_parameters.clone();
    if signatures
        .iter()
        .skip(1)
        .any(|signature| signature.receiver_type_parameters != receiver_parameter_names)
    {
        return Err(crate::core::UnknownReason::AmbiguousCallableOverload);
    }
    let receiver_bindings = if receiver_parameter_names.is_empty() {
        Vec::new()
    } else {
        let receiver_type =
            receiver_type.ok_or(crate::core::UnknownReason::IncompleteBlockInput)?;
        let (_receiver_name, type_arguments) = rbs_receiver_identity(receiver_type)
            .ok_or(crate::core::UnknownReason::IncompleteBlockInput)?;
        if receiver_parameter_names.len() != type_arguments.len() {
            return Err(crate::core::UnknownReason::IncompleteBlockInput);
        }
        receiver_parameter_names
            .into_iter()
            .zip(type_arguments)
            .collect()
    };
    prepare_callable_set(
        receiver_type,
        &signatures,
        &receiver_bindings,
        argument_types,
    )
}

fn prepare_forwarded_from_facts(
    query: &AnalysisQuery<'_>,
    cache: Option<&AnalysisQueryCache>,
    facts: &[MethodFact],
    argument_types: &[RubyType],
) -> Result<PreparedCallableSet, crate::core::UnknownReason> {
    let mut forwarded_targets = Vec::new();
    for fact in facts {
        let Some(forwarded) = fact.forwarded_block_call() else {
            continue;
        };
        let Some(index) = fact
            .param_facts
            .iter()
            .filter(|parameter| {
                matches!(
                    parameter.kind,
                    MethodParamKind::Required | MethodParamKind::Optional
                )
            })
            .position(|parameter| parameter.name == forwarded.receiver_parameter)
        else {
            return Err(crate::core::UnknownReason::UnsupportedCallable);
        };
        let receiver = argument_types
            .get(index)
            .cloned()
            .ok_or(crate::core::UnknownReason::IncompleteBlockInput)?;
        if RubyType::contains_unknown(&receiver) {
            return Err(crate::core::UnknownReason::IncompleteBlockInput);
        }
        forwarded_targets.push((forwarded.method, receiver));
    }
    forwarded_targets
        .sort_by(|left, right| (left.0.as_str(), &left.1).cmp(&(right.0.as_str(), &right.1)));
    forwarded_targets.dedup();
    let [(target_method, target_receiver)] = forwarded_targets.as_slice() else {
        return Err(if forwarded_targets.is_empty() {
            crate::core::UnknownReason::UnsupportedCallable
        } else {
            crate::core::UnknownReason::AmbiguousCallableOverload
        });
    };
    prepare_higher_order_call(
        Some(query),
        cache,
        target_receiver,
        target_method.as_str(),
        &[],
    )
}

fn prepare_direct_yield_from_facts(
    facts: &[MethodFact],
    argument_types: &[RubyType],
) -> Result<PreparedCallableSet, crate::core::UnknownReason> {
    let mut block_parameter_sets = Vec::new();
    for fact in facts {
        let Some(direct_yield) = fact.direct_yield_call() else {
            continue;
        };
        let positional = fact
            .param_facts
            .iter()
            .filter(|parameter| {
                matches!(
                    parameter.kind,
                    MethodParamKind::Required | MethodParamKind::Optional
                )
            })
            .collect::<Vec<_>>();
        let mut block_parameters = Vec::new();
        for parameter_name in &direct_yield.parameter_names {
            let Some(index) = positional
                .iter()
                .position(|parameter| parameter.name == *parameter_name)
            else {
                return Err(crate::core::UnknownReason::UnsupportedCallable);
            };
            let ruby_type = argument_types
                .get(index)
                .cloned()
                .ok_or(crate::core::UnknownReason::IncompleteBlockInput)?;
            if RubyType::contains_unknown(&ruby_type) {
                return Err(crate::core::UnknownReason::IncompleteBlockInput);
            }
            block_parameters.push(ruby_type);
        }
        block_parameter_sets.push(block_parameters);
    }
    block_parameter_sets.sort();
    block_parameter_sets.dedup();
    let [block_parameters] = block_parameter_sets.as_slice() else {
        return Err(if block_parameter_sets.is_empty() {
            crate::core::UnknownReason::UnsupportedCallable
        } else {
            crate::core::UnknownReason::AmbiguousCallableOverload
        });
    };
    let output = "BlockResult".to_string();
    let signature = CallableSignature {
        receiver_type_parameters: Vec::new(),
        type_parameters: vec![output.clone()],
        parameters: Vec::new(),
        block: CallableBlockTemplate {
            parameters: block_parameters
                .iter()
                .cloned()
                .map(CallableTypeTemplate::Concrete)
                .collect(),
            return_type: CallableTypeTemplate::Variable(output.clone()),
            required: true,
        },
        return_type: CallableTypeTemplate::Variable(output),
    };
    prepare_callable_set(None, &[signature], &[], &[])
}
