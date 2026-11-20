//! Method call return type resolution.

use crate::core::{
    FullyQualifiedName, NamespaceKind, RubyMethod, RubyType, TypeInferenceOutcome, UnknownReason,
};
use crate::inference::r#type::shape as shape_reads;
use crate::inference::rbs::CallArguments;
use crate::inference::semantics::{ReceiverAccess, Semantics};

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
pub fn method_call_return_type<S: Semantics + ?Sized>(
    query: Option<&S>,
    receiver_type: &RubyType,
    method_name: &str,
) -> Option<RubyType> {
    method_call_type_outcome(query, receiver_type, method_name).into_proven_type()
}

/// Resolve a method call while retaining why a concrete result was withheld.
pub fn method_call_type_outcome<S: Semantics + ?Sized>(
    query: Option<&S>,
    receiver_type: &RubyType,
    method_name: &str,
) -> TypeInferenceOutcome {
    method_call_type_outcome_with_private(query, receiver_type, method_name, true)
}

pub fn method_call_type_outcome_with_private<S: Semantics + ?Sized>(
    query: Option<&S>,
    receiver_type: &RubyType,
    method_name: &str,
    allow_private: bool,
) -> TypeInferenceOutcome {
    method_call_type_outcome_with_visibility(query, receiver_type, method_name, allow_private, None)
}

/// Resolve a call from bundled signatures alone, selecting the overload
/// from what the call site proves about its arguments.
pub(crate) fn signature_call_type_outcome(
    receiver_type: &RubyType,
    method_name: &str,
    arguments: Option<&CallArguments>,
) -> TypeInferenceOutcome {
    method_call_type_outcome_for_call(
        None::<&dyn Semantics>,
        receiver_type,
        method_name,
        true,
        None,
        arguments,
    )
}

pub fn method_call_return_type_with_visibility<S: Semantics + ?Sized>(
    query: Option<&S>,
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

pub fn method_call_type_outcome_with_visibility<S: Semantics + ?Sized>(
    query: Option<&S>,
    receiver_type: &RubyType,
    method_name: &str,
    allow_private: bool,
    protected_caller: Option<&FullyQualifiedName>,
) -> TypeInferenceOutcome {
    method_call_type_outcome_for_call(
        query,
        receiver_type,
        method_name,
        allow_private,
        protected_caller,
        None,
    )
}

/// `arguments` is what the call site proves, or `None` when no call site is
/// visible; a bundled signature the call does not decide is unknown.
fn method_call_type_outcome_for_call<S: Semantics + ?Sized>(
    query: Option<&S>,
    receiver_type: &RubyType,
    method_name: &str,
    allow_private: bool,
    protected_caller: Option<&FullyQualifiedName>,
    arguments: Option<&CallArguments>,
) -> TypeInferenceOutcome {
    if let RubyType::Union(types) = receiver_type {
        let mut return_types = Vec::with_capacity(types.len());
        for member in types {
            let outcome = method_call_type_outcome_for_call(
                query,
                member,
                method_name,
                allow_private,
                protected_caller,
                arguments,
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
            let instance = RubyType::Class(fqn.clone());
            return match query {
                Some(query) => query.constructor_result(fqn).into_type_outcome(instance),
                None => TypeInferenceOutcome::from_optional(
                    super::constructor::seed_constructor_type(fqn),
                    UnknownReason::UnresolvedMethodReturn,
                ),
            };
        }
    }

    if let Some(bundled) = generic_rbs_method_return(receiver_type, method_name, arguments) {
        return signature_outcome(Some(bundled));
    }

    let Ok(method) = RubyMethod::new(method_name) else {
        return TypeInferenceOutcome::unknown(UnknownReason::InvalidMethodName);
    };
    if let Some(query) = query {
        let access = if allow_private {
            ReceiverAccess::Any
        } else if let Some(caller) = protected_caller {
            ReceiverAccess::Protected { caller }
        } else {
            ReceiverAccess::Public
        };
        for namespace in crate::core::receiver_type_to_method_namespaces(receiver_type) {
            if let Some(return_type) =
                query.receiver_method_return_type(&namespace, &method, access)
            {
                return TypeInferenceOutcome::from_optional(
                    Some(return_type),
                    UnknownReason::UnresolvedMethodReturn,
                );
            }
        }
    }

    signature_outcome(rbs_method_return(receiver_type, method_name, arguments))
}

/// A bundled-signature answer as an outcome, keeping why it was withheld.
fn signature_outcome(bundled: Option<Result<RubyType, UnknownReason>>) -> TypeInferenceOutcome {
    match bundled {
        Some(Ok(return_type)) => TypeInferenceOutcome::from_optional(
            Some(return_type),
            UnknownReason::UnresolvedMethodReturn,
        ),
        Some(Err(reason)) => TypeInferenceOutcome::unknown(reason),
        None => TypeInferenceOutcome::unknown(UnknownReason::UnresolvedMethodReturn),
    }
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

/// Whether bundled signatures know every class this receiver can be. One
/// known member of a union says nothing about the others, so an unknown
/// member keeps the whole receiver outside signature knowledge.
pub fn rbs_class_exists_for_type(receiver_type: &RubyType) -> bool {
    match receiver_type {
        RubyType::Union(members) => {
            !members.is_empty() && members.iter().all(rbs_class_exists_for_type)
        }
        // A nested name is known only under its full path: a project class
        // sharing its last segment with a core class is a different class.
        RubyType::Class(fqn)
        | RubyType::ClassReference(fqn)
        | RubyType::Module(fqn)
        | RubyType::ModuleReference(fqn) => {
            let full_name = fqn
                .namespace_parts()
                .iter()
                .map(|part| part.to_string())
                .collect::<Vec<_>>()
                .join("::");
            !full_name.is_empty() && crate::inference::rbs::has_rbs_class(&full_name)
        }
        RubyType::Array(_) | RubyType::Hash(_, _) | RubyType::Shape(_) | RubyType::Literal(_) => {
            rbs_class_names_for_type(receiver_type)
                .iter()
                .any(|class_name| crate::inference::rbs::has_rbs_class(class_name))
        }
        RubyType::Unknown => false,
    }
}

fn generic_rbs_method_return(
    receiver_type: &RubyType,
    method_name: &str,
    arguments: Option<&CallArguments>,
) -> Option<Result<RubyType, UnknownReason>> {
    match receiver_type {
        RubyType::Array(_) | RubyType::Hash(_, _) => crate::inference::rbs::rbs_call_return(
            &receiver_rbs_class_name(receiver_type)?,
            method_name,
            false,
            &type_args_for_receiver(receiver_type),
            arguments,
        ),
        RubyType::Shape(shape) => {
            generic_rbs_method_return(&shape.generic_hash_type(), method_name, arguments)
        }
        RubyType::Literal(_)
        | RubyType::Class(_)
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

fn rbs_method_return(
    receiver_type: &RubyType,
    method_name: &str,
    arguments: Option<&CallArguments>,
) -> Option<Result<RubyType, UnknownReason>> {
    match receiver_type {
        RubyType::Class(fqn) | RubyType::Module(fqn) => {
            rbs_method_return_for_fqn(fqn, method_name, false, arguments)
        }
        RubyType::ClassReference(fqn) | RubyType::ModuleReference(fqn) => {
            rbs_method_return_for_fqn(fqn, method_name, true, arguments)
        }
        RubyType::Array(_) | RubyType::Hash(_, _) | RubyType::Shape(_) => {
            generic_rbs_method_return(receiver_type, method_name, arguments)
        }
        RubyType::Literal(value) => {
            rbs_method_return(&value.widened_type(), method_name, arguments)
        }
        RubyType::Union(types) => resolve_proven_union(types, |ty| {
            generic_rbs_method_return(ty, method_name, arguments)
                .or_else(|| rbs_method_return(ty, method_name, arguments))
                .and_then(Result::ok)
        })
        .map(Ok),
        RubyType::Unknown => None,
    }
}

fn rbs_method_return_for_fqn(
    fqn: &FullyQualifiedName,
    method_name: &str,
    is_singleton: bool,
    arguments: Option<&CallArguments>,
) -> Option<Result<RubyType, UnknownReason>> {
    class_names_for_fqn(fqn).iter().find_map(|class_name| {
        crate::inference::rbs::rbs_call_return(
            class_name,
            method_name,
            is_singleton,
            &[],
            arguments,
        )
    })
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

/// The RBS return type of `method_name` on one receiver, substituting the
/// receiver's element types for an Array or Hash. The class is looked up by
/// its last constant name. `arguments` is what the call site proves, or
/// `None` when no call site is visible; a signature the call does not
/// decide is `Unknown`.
pub(crate) fn receiver_rbs_return_type(
    receiver_type: &RubyType,
    method_name: &str,
    arguments: Option<&CallArguments>,
) -> Option<RubyType> {
    receiver_rbs_call_return(receiver_type, method_name, arguments)
        .map(|outcome| outcome.unwrap_or(RubyType::Unknown))
}

/// As `receiver_rbs_return_type`, keeping why a return was withheld.
pub(crate) fn receiver_rbs_call_return(
    receiver_type: &RubyType,
    method_name: &str,
    arguments: Option<&CallArguments>,
) -> Option<Result<RubyType, UnknownReason>> {
    let class_name = receiver_rbs_class_name(receiver_type)?;
    crate::inference::rbs::rbs_call_return(
        &class_name,
        method_name,
        receiver_is_singleton(receiver_type),
        &type_args_for_receiver(receiver_type),
        arguments,
    )
}

/// Whether the arguments of a call decide the bundled return of
/// `method_name` on this receiver or, for a union, on any member.
pub(crate) fn receiver_rbs_return_depends_on_arguments(
    receiver_type: &RubyType,
    method_name: &str,
) -> bool {
    if let RubyType::Union(members) = receiver_type {
        return members
            .iter()
            .any(|member| receiver_rbs_return_depends_on_arguments(member, method_name));
    }
    receiver_rbs_class_name(receiver_type).is_some_and(|class_name| {
        crate::inference::rbs::rbs_return_depends_on_arguments(
            &class_name,
            method_name,
            receiver_is_singleton(receiver_type),
        )
    })
}

fn receiver_is_singleton(receiver_type: &RubyType) -> bool {
    matches!(
        receiver_type,
        RubyType::ClassReference(_) | RubyType::ModuleReference(_)
    )
}

fn receiver_rbs_class_name(receiver_type: &RubyType) -> Option<String> {
    match receiver_type {
        RubyType::Class(fqn)
        | RubyType::ClassReference(fqn)
        | RubyType::Module(fqn)
        | RubyType::ModuleReference(fqn) => fqn.namespace_parts().last().map(ToString::to_string),
        RubyType::Array(_) => Some("Array".to_string()),
        RubyType::Hash(_, _) | RubyType::Shape(_) => Some("Hash".to_string()),
        RubyType::Literal(value) => receiver_rbs_class_name(&value.widened_type()),
        RubyType::Union(_) => None,
        RubyType::Unknown => None,
    }
}

fn type_args_for_receiver(receiver_type: &RubyType) -> Vec<RubyType> {
    match receiver_type {
        RubyType::Array(element_types) => match element_types.len() {
            0 => Vec::new(),
            1 => vec![element_types[0].clone()],
            2.. => vec![RubyType::union(element_types.clone())],
        },
        RubyType::Hash(key_types, value_types) => {
            let key = match key_types.len() {
                0 => RubyType::Unknown,
                1 => key_types[0].clone(),
                2.. => RubyType::union(key_types.clone()),
            };
            let value = match value_types.len() {
                0 => RubyType::Unknown,
                1 => value_types[0].clone(),
                2.. => RubyType::union(value_types.clone()),
            };
            vec![key, value]
        }
        RubyType::Shape(shape) => type_args_for_receiver(&shape.generic_hash_type()),
        RubyType::Literal(_) => Vec::new(),
        RubyType::Class(_)
        | RubyType::Module(_)
        | RubyType::ClassReference(_)
        | RubyType::ModuleReference(_)
        | RubyType::Union(_)
        | RubyType::Unknown => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn union_receiver_reports_machine_readable_unknown_reason() {
        let receiver = RubyType::Union(vec![RubyType::string(), RubyType::integer()]);

        let outcome = method_call_type_outcome(None::<&dyn Semantics>, &receiver, "length");

        invariant_eq!(
            outcome.unknown_reason(),
            Some(UnknownReason::IncompleteUnionMember),
            what = "an incomplete union call lost its Unknown reason",
            why = "CLI and LSP need one explanation for withheld types",
            fix = "return IncompleteUnionMember when any receiver member is unresolved",
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

        invariant_eq!(
            method_call_return_type(None::<&dyn Semantics>, &receiver, "length"),
            None,
            what = "a union call discarded the unresolved Integer#length branch",
            why = "a concrete chained-call type requires proof for every reachable receiver member",
            fix = "return Unknown/None when any union member cannot resolve the method return type",
        );
    }

    #[test]
    fn union_receiver_combines_returns_when_every_member_is_proven() {
        let receiver = RubyType::Union(vec![RubyType::string(), RubyType::integer()]);

        invariant_eq!(
            method_call_return_type(None::<&dyn Semantics>, &receiver, "to_s"),
            Some(RubyType::string()),
            what = "a union call with two proven String returns did not resolve",
            why = "complete union evidence must not degrade to Unknown",
            fix = "combine returns from every member once all resolve",
        );
    }
}
