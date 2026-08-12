//! Method call return type resolution.

use crate::core::{
    FullyQualifiedName, NamespaceKind, RubyMethod, RubyType, TypeInferenceOutcome, UnknownReason,
};
use crate::engine::AnalysisQuery;
use crate::inference::r#type::shape as shape_reads;

/// Resolve every reachable member of a union before publishing a call result.
///
/// Returning a type for only the members that happen to resolve is unsound: a
/// later chained call would treat that partial result as proof for all runtime
/// paths. Unknown results are unresolved evidence and therefore fail closed.
pub(crate) fn resolve_proven_union(
    types: &[RubyType],
    mut resolve: impl FnMut(&RubyType) -> Option<RubyType>,
) -> Option<RubyType> {
    RubyType::union_from_proven(types, |receiver_type| resolve(receiver_type))
}

/// Resolve a method call return type for a receiver type.
pub fn method_call_return_type(
    query: Option<&AnalysisQuery<'_>>,
    receiver_type: &RubyType,
    method_name: &str,
) -> Option<RubyType> {
    method_call_type_outcome(query, receiver_type, method_name).into_proven_type()
}

/// Resolve a method call while retaining why a concrete result was withheld.
pub fn method_call_type_outcome(
    query: Option<&AnalysisQuery<'_>>,
    receiver_type: &RubyType,
    method_name: &str,
) -> TypeInferenceOutcome {
    method_call_type_outcome_with_private(query, receiver_type, method_name, true)
}

pub fn method_call_return_type_with_private(
    query: Option<&AnalysisQuery<'_>>,
    receiver_type: &RubyType,
    method_name: &str,
    allow_private: bool,
) -> Option<RubyType> {
    method_call_type_outcome_with_private(query, receiver_type, method_name, allow_private)
        .into_proven_type()
}

pub fn method_call_type_outcome_with_private(
    query: Option<&AnalysisQuery<'_>>,
    receiver_type: &RubyType,
    method_name: &str,
    allow_private: bool,
) -> TypeInferenceOutcome {
    method_call_type_outcome_with_visibility(query, receiver_type, method_name, allow_private, None)
}

pub fn method_call_return_type_with_visibility(
    query: Option<&AnalysisQuery<'_>>,
    receiver_type: &RubyType,
    method_name: &str,
    allow_private: bool,
    protected_caller: Option<&FullyQualifiedName>,
) -> Option<RubyType> {
    method_call_type_outcome_with_visibility(
        query,
        receiver_type,
        method_name,
        allow_private,
        protected_caller,
    )
    .into_proven_type()
}

pub fn method_call_type_outcome_with_visibility(
    query: Option<&AnalysisQuery<'_>>,
    receiver_type: &RubyType,
    method_name: &str,
    allow_private: bool,
    protected_caller: Option<&FullyQualifiedName>,
) -> TypeInferenceOutcome {
    if let RubyType::Union(types) = receiver_type {
        let mut return_types = Vec::with_capacity(types.len());
        for member in types {
            let outcome = method_call_type_outcome_with_visibility(
                query,
                member,
                method_name,
                allow_private,
                protected_caller,
            );
            let Some(return_type) = outcome.into_proven_type() else {
                return TypeInferenceOutcome::unknown(UnknownReason::IncompleteUnionMember);
            };
            return_types.push(return_type);
        }
        return TypeInferenceOutcome::from_optional(
            (!return_types.is_empty()).then(|| RubyType::union(return_types)),
            UnknownReason::IncompleteUnionMember,
        );
    }

    if receiver_type == &RubyType::Unknown {
        return TypeInferenceOutcome::unknown(UnknownReason::UnknownReceiver);
    }

    if shape_reads::is_shape_only(receiver_type) {
        if let Some(outcome) = shape_reads::argument_free_method_return(receiver_type, method_name)
        {
            return match outcome {
                Ok(ruby_type) => TypeInferenceOutcome::proven(ruby_type),
                Err(reason) => TypeInferenceOutcome::unknown(reason),
            };
        }
        if shape_reads::operation_requires_call_arguments(method_name) {
            return TypeInferenceOutcome::unknown(UnknownReason::UnresolvedMethodReturn);
        }
    }

    if method_name == "new" {
        if let RubyType::ClassReference(fqn) = receiver_type {
            return TypeInferenceOutcome::proven(RubyType::Class(fqn.clone()));
        }
    }

    if let Some(return_type) = generic_rbs_method_return_type(receiver_type, method_name) {
        return TypeInferenceOutcome::from_optional(
            Some(return_type),
            UnknownReason::UnresolvedMethodReturn,
        );
    }

    let Ok(method) = RubyMethod::new(method_name) else {
        return TypeInferenceOutcome::unknown(UnknownReason::InvalidMethodName);
    };
    if let Some(query) = query {
        for namespace in AnalysisQuery::receiver_type_to_method_namespaces(receiver_type) {
            let return_type = if allow_private {
                query.method_return_type_for_receiver(&namespace, &method)
            } else if let Some(caller) = protected_caller {
                query.method_return_type_for_protected_receiver(&namespace, &method, caller)
            } else {
                query.method_return_type_for_public_receiver(&namespace, &method)
            };
            if let Some(return_type) = return_type {
                return TypeInferenceOutcome::from_optional(
                    Some(return_type),
                    UnknownReason::UnresolvedMethodReturn,
                );
            }
        }
    }

    TypeInferenceOutcome::from_optional(
        rbs_method_return_type(receiver_type, method_name),
        UnknownReason::UnresolvedMethodReturn,
    )
}

pub fn rbs_method_exists_for_type(
    receiver_type: &RubyType,
    method: &RubyMethod,
    kind: NamespaceKind,
) -> bool {
    let is_singleton = kind == NamespaceKind::Singleton;
    let method_name = method.as_str();

    for class_name in rbs_class_names_for_type(receiver_type) {
        if crate::inference::rbs::rbs_class_method_exists(&class_name, method_name, is_singleton) {
            return true;
        }
    }

    if is_singleton {
        for rbs_class in ["Class", "Module"] {
            if crate::inference::rbs::rbs_class_method_exists(rbs_class, method_name, false) {
                return true;
            }
        }
    }

    false
}

pub fn rbs_class_exists_for_type(receiver_type: &RubyType) -> bool {
    rbs_class_names_for_type(receiver_type)
        .iter()
        .any(|class_name| crate::inference::rbs::has_rbs_class(class_name))
}

fn generic_rbs_method_return_type(receiver_type: &RubyType, method_name: &str) -> Option<RubyType> {
    match receiver_type {
        RubyType::Array(element_types) => {
            crate::inference::rbs::get_rbs_method_return_type_with_type_args(
                "Array",
                method_name,
                false,
                element_types,
            )
        }
        RubyType::Hash(key_types, value_types) => {
            let type_args = vec![
                RubyType::union(key_types.clone()),
                RubyType::union(value_types.clone()),
            ];
            crate::inference::rbs::get_rbs_method_return_type_with_type_args(
                "Hash",
                method_name,
                false,
                &type_args,
            )
        }
        RubyType::Shape(shape) => {
            generic_rbs_method_return_type(&shape.generic_hash_type(), method_name)
        }
        RubyType::Literal(_) => None,
        RubyType::Class(_)
        | RubyType::Module(_)
        | RubyType::ClassReference(_)
        | RubyType::ModuleReference(_)
        | RubyType::Union(_)
        | RubyType::Unknown => None,
    }
}

fn rbs_class_names_for_type(ruby_type: &RubyType) -> Vec<String> {
    match ruby_type {
        RubyType::Class(fqn) | RubyType::ClassReference(fqn) => {
            let parts = fqn.namespace_parts();
            let fqn_name = parts
                .iter()
                .map(|part| part.to_string())
                .collect::<Vec<_>>()
                .join("::");
            let simple_name = parts.last().map(|part| part.to_string());

            let mut names = Vec::new();
            if !fqn_name.is_empty() {
                names.push(fqn_name);
            }
            if let Some(simple_name) = simple_name {
                if !names.contains(&simple_name) {
                    names.push(simple_name);
                }
            }
            names
        }
        RubyType::Module(fqn) | RubyType::ModuleReference(fqn) => fqn
            .namespace_parts()
            .last()
            .map(|constant| vec![constant.to_string()])
            .unwrap_or_default(),
        RubyType::Array(_) => vec!["Array".to_string()],
        RubyType::Hash(_, _) | RubyType::Shape(_) => vec!["Hash".to_string()],
        RubyType::Literal(value) => rbs_class_names_for_type(&value.widened_type()),
        RubyType::Union(types) => {
            let mut all_names = Vec::new();
            for ty in types {
                for name in rbs_class_names_for_type(ty) {
                    if !all_names.contains(&name) {
                        all_names.push(name);
                    }
                }
            }
            all_names
        }
        RubyType::Unknown => Vec::new(),
    }
}

pub fn rbs_method_signatures_for_type(
    receiver_type: &RubyType,
    method_name: &str,
) -> Vec<crate::inference::rbs::RbsMethodSignature> {
    let is_singleton = matches!(
        receiver_type,
        RubyType::ClassReference(_) | RubyType::ModuleReference(_)
    );
    let mut signatures = Vec::new();
    for class_name in rbs_class_names_for_type(receiver_type) {
        signatures.extend(crate::inference::rbs::get_rbs_method_signatures(
            &class_name,
            method_name,
            is_singleton,
        ));
        if !signatures.is_empty() {
            break;
        }
    }
    signatures
}

fn rbs_method_return_type(receiver_type: &RubyType, method_name: &str) -> Option<RubyType> {
    match receiver_type {
        RubyType::Class(fqn) | RubyType::Module(fqn) => {
            rbs_method_return_for_fqn(fqn, method_name, false)
        }
        RubyType::ClassReference(fqn) | RubyType::ModuleReference(fqn) => {
            rbs_method_return_for_fqn(fqn, method_name, true)
        }
        RubyType::Array(_) | RubyType::Hash(_, _) | RubyType::Shape(_) => {
            generic_rbs_method_return_type(receiver_type, method_name)
        }
        RubyType::Literal(value) => rbs_method_return_type(&value.widened_type(), method_name),
        RubyType::Union(types) => resolve_proven_union(types, |ty| {
            generic_rbs_method_return_type(ty, method_name)
                .or_else(|| rbs_method_return_type(ty, method_name))
        }),
        RubyType::Unknown => None,
    }
}

fn rbs_method_return_for_fqn(
    fqn: &FullyQualifiedName,
    method_name: &str,
    is_singleton: bool,
) -> Option<RubyType> {
    for class_name in class_names_for_fqn(fqn) {
        if let Some(return_type) = crate::inference::rbs::get_rbs_method_return_type_as_ruby_type(
            &class_name,
            method_name,
            is_singleton,
        ) {
            return Some(return_type);
        }
    }
    None
}

fn class_names_for_fqn(fqn: &FullyQualifiedName) -> Vec<String> {
    let parts = fqn.namespace_parts();
    let fqn_name = parts
        .iter()
        .map(|part| part.to_string())
        .collect::<Vec<_>>()
        .join("::");
    let simple_name = parts.last().map(|part| part.to_string());

    let mut names = Vec::new();
    if !fqn_name.is_empty() {
        names.push(fqn_name);
    }
    if let Some(simple_name) = simple_name {
        if !names.contains(&simple_name) {
            names.push(simple_name);
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn union_receiver_reports_machine_readable_unknown_reason() {
        let receiver = RubyType::Union(vec![RubyType::string(), RubyType::integer()]);

        let outcome = method_call_type_outcome(None, &receiver, "length");

        assert_eq!(
            outcome.unknown_reason(),
            Some(UnknownReason::IncompleteUnionMember),
            "INVARIANT VIOLATED: an incomplete union call did not retain its Unknown reason. \
             This is a bug because CLI and LSP consumers need one shared, deterministic \
             explanation for withheld concrete types. Fix: return a TypeOutcome carrying \
             IncompleteUnionMember when any reachable receiver member cannot resolve."
        );
        assert_eq!(
            outcome
                .unknown_reason()
                .expect("the incomplete union must have an Unknown reason")
                .code(),
            "incomplete_union_member"
        );
    }

    #[test]
    fn union_receiver_requires_a_return_type_for_every_member() {
        let receiver = RubyType::Union(vec![RubyType::string(), RubyType::integer()]);

        assert_eq!(
            method_call_return_type(None, &receiver, "length"),
            None,
            "INVARIANT VIOLATED: a union call discarded the unresolved Integer#length branch. \
             This is a bug because a concrete chained-call type requires proof for every \
             reachable receiver member. Fix: return Unknown/None when any union member cannot \
             resolve the method return type."
        );
    }

    #[test]
    fn union_receiver_combines_returns_when_every_member_is_proven() {
        let receiver = RubyType::Union(vec![RubyType::string(), RubyType::integer()]);

        assert_eq!(
            method_call_return_type(None, &receiver, "to_s"),
            Some(RubyType::string()),
            "INVARIANT VIOLATED: a union call with two proven String returns did not resolve. \
             This is a bug because proof-first inference must retain complete evidence rather \
             than degrading all union receivers to Unknown. Fix: combine the return type from \
             every union member after all members resolve."
        );
    }
}
