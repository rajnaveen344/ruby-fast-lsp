//! Method call return type resolution.

use crate::core::{FullyQualifiedName, RubyMethod, RubyType};
use crate::engine::AnalysisQuery;

/// Resolve a method call return type for a receiver type.
pub fn method_call_return_type(
    query: Option<&AnalysisQuery<'_>>,
    receiver_type: &RubyType,
    method_name: &str,
) -> Option<RubyType> {
    if method_name == "new" {
        if let RubyType::ClassReference(fqn) = receiver_type {
            return Some(RubyType::Class(fqn.clone()));
        }
    }

    if let Some(return_type) = generic_rbs_method_return_type(receiver_type, method_name) {
        return Some(return_type);
    }

    let method = RubyMethod::new(method_name).ok()?;
    if let Some(query) = query {
        for namespace in query.receiver_type_to_method_namespaces(receiver_type) {
            if let Some(return_type) = query.method_return_type_for_receiver(&namespace, &method) {
                return Some(return_type);
            }
        }
    }

    rbs_method_return_type(receiver_type, method_name)
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
        RubyType::Class(_)
        | RubyType::Module(_)
        | RubyType::ClassReference(_)
        | RubyType::ModuleReference(_)
        | RubyType::Union(_)
        | RubyType::Unknown => None,
    }
}

fn rbs_method_return_type(receiver_type: &RubyType, method_name: &str) -> Option<RubyType> {
    match receiver_type {
        RubyType::Class(fqn) | RubyType::Module(fqn) => {
            rbs_method_return_for_fqn(fqn, method_name, false)
        }
        RubyType::ClassReference(fqn) | RubyType::ModuleReference(fqn) => {
            rbs_method_return_for_fqn(fqn, method_name, true)
        }
        RubyType::Array(_) | RubyType::Hash(_, _) => {
            generic_rbs_method_return_type(receiver_type, method_name)
        }
        RubyType::Union(types) => {
            let mut return_types = types
                .iter()
                .filter_map(|ty| {
                    generic_rbs_method_return_type(ty, method_name)
                        .or_else(|| rbs_method_return_type(ty, method_name))
                })
                .collect::<Vec<_>>();
            return_types.sort_by_key(|ty| ty.to_string());
            return_types.dedup();
            match return_types.len() {
                0 => None,
                1 => return_types.pop(),
                _ => Some(RubyType::union(return_types)),
            }
        }
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
