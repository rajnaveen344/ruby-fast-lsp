//! Method-call return types for chained completion receivers.

use crate::core::{FullyQualifiedName, NamespaceKind, RubyMethod, RubyType};
use crate::indexer::Identifier;
use crate::inference::r#type::shape as shape_reads;

use super::method_matches::class_names_for_fqn;
use super::CompletionSemanticQuery;

pub(super) fn infer_method_call_return_type(
    query: &impl CompletionSemanticQuery,
    receiver_type: &RubyType,
    method_name: &str,
) -> Option<RubyType> {
    if let RubyType::Union(types) = receiver_type {
        return crate::inference::method::return_type::resolve_proven_union(types, |member| {
            infer_method_call_return_type(query, member, method_name)
        });
    }

    if method_name == "new" {
        if let RubyType::ClassReference(fqn) = receiver_type {
            return Some(RubyType::Class(fqn.clone()));
        }
    }

    if shape_reads::is_shape_only(receiver_type) {
        if let Some(outcome) = shape_reads::argument_free_method_return(receiver_type, method_name)
        {
            return outcome.ok();
        }
        if shape_reads::operation_requires_call_arguments(method_name) {
            return None;
        }
    }

    if let Some(return_type) = infer_generic_rbs_method_return_type(receiver_type, method_name) {
        return Some(return_type);
    }

    let method = RubyMethod::new(method_name).ok()?;
    for namespace in receiver_type_to_analysis_namespaces(receiver_type) {
        if let Some(return_type) = query.method_return_type_for_receiver(&namespace, &method) {
            return Some(return_type);
        }
    }

    infer_rbs_method_return_type(receiver_type, method_name)
}

fn infer_generic_rbs_method_return_type(
    receiver_type: &RubyType,
    method_name: &str,
) -> Option<RubyType> {
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
            infer_generic_rbs_method_return_type(&shape.generic_hash_type(), method_name)
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

fn infer_rbs_method_return_type(receiver_type: &RubyType, method_name: &str) -> Option<RubyType> {
    match receiver_type {
        RubyType::Class(fqn) | RubyType::Module(fqn) => {
            rbs_method_return_for_fqn(fqn, method_name, false)
        }
        RubyType::ClassReference(fqn) | RubyType::ModuleReference(fqn) => {
            rbs_method_return_for_fqn(fqn, method_name, true)
        }
        RubyType::Array(_) | RubyType::Hash(_, _) => {
            infer_generic_rbs_method_return_type(receiver_type, method_name)
        }
        RubyType::Shape(shape) => {
            infer_rbs_method_return_type(&shape.generic_hash_type(), method_name)
        }
        RubyType::Literal(value) => {
            infer_rbs_method_return_type(&value.widened_type(), method_name)
        }
        RubyType::Union(types) => {
            crate::inference::method::return_type::resolve_proven_union(types, |ty| {
                infer_method_call_return_type_fallback(ty, method_name)
            })
        }
        RubyType::Unknown => None,
    }
}

fn infer_method_call_return_type_fallback(
    receiver_type: &RubyType,
    method_name: &str,
) -> Option<RubyType> {
    infer_generic_rbs_method_return_type(receiver_type, method_name)
        .or_else(|| infer_rbs_method_return_type(receiver_type, method_name))
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

fn receiver_type_to_analysis_namespaces(receiver_type: &RubyType) -> Vec<FullyQualifiedName> {
    match receiver_type {
        RubyType::Class(fqn) | RubyType::Module(fqn) => {
            vec![FullyQualifiedName::namespace_with_kind(
                fqn.namespace_parts(),
                NamespaceKind::Instance,
            )]
        }
        RubyType::ClassReference(fqn) | RubyType::ModuleReference(fqn) => {
            vec![FullyQualifiedName::namespace_with_kind(
                fqn.namespace_parts(),
                NamespaceKind::Singleton,
            )]
        }
        RubyType::Union(types) => types
            .iter()
            .flat_map(receiver_type_to_analysis_namespaces)
            .collect(),
        RubyType::Literal(value) => receiver_type_to_analysis_namespaces(&value.widened_type()),
        RubyType::Array(_) | RubyType::Hash(_, _) | RubyType::Shape(_) | RubyType::Unknown => {
            Vec::new()
        }
    }
}

pub(super) fn infer_bare_method_return_type(
    query: &impl CompletionSemanticQuery,
    method_name: &str,
    identifier: &Option<Identifier>,
) -> Option<RubyType> {
    let method = RubyMethod::new(method_name).ok()?;
    let mut namespaces = Vec::new();
    if let Some(Identifier::RubyMethod { namespace, .. }) = identifier {
        namespaces.push(FullyQualifiedName::namespace_with_kind(
            namespace.clone(),
            NamespaceKind::Instance,
        ));
    }
    namespaces.push(FullyQualifiedName::namespace_with_kind(
        Vec::new(),
        NamespaceKind::Instance,
    ));

    for namespace in namespaces {
        if let Some(return_type) = query.method_return_type_for_receiver(&namespace, &method) {
            return Some(return_type);
        }
    }
    None
}
