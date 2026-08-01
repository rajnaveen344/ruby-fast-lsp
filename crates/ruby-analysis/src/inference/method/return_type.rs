//! Method call return type resolution.

use crate::core::{FullyQualifiedName, NamespaceKind, RubyMethod, RubyType};
use crate::engine::AnalysisQuery;

/// Resolve a method call return type for a receiver type.
pub fn method_call_return_type(
    query: Option<&AnalysisQuery<'_>>,
    receiver_type: &RubyType,
    method_name: &str,
) -> Option<RubyType> {
    method_call_return_type_with_private(query, receiver_type, method_name, true)
}

pub fn method_call_return_type_with_private(
    query: Option<&AnalysisQuery<'_>>,
    receiver_type: &RubyType,
    method_name: &str,
    allow_private: bool,
) -> Option<RubyType> {
    method_call_return_type_with_visibility(query, receiver_type, method_name, allow_private, None)
}

pub fn method_call_return_type_with_visibility(
    query: Option<&AnalysisQuery<'_>>,
    receiver_type: &RubyType,
    method_name: &str,
    allow_private: bool,
    protected_caller: Option<&FullyQualifiedName>,
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
        for namespace in AnalysisQuery::receiver_type_to_method_namespaces(receiver_type) {
            let return_type = if allow_private {
                query.method_return_type_for_receiver(&namespace, &method)
            } else if let Some(caller) = protected_caller {
                query.method_return_type_for_protected_receiver(&namespace, &method, caller)
            } else {
                query.method_return_type_for_public_receiver(&namespace, &method)
            };
            if let Some(return_type) = return_type {
                return Some(return_type);
            }
        }
    }

    rbs_method_return_type(receiver_type, method_name)
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
        RubyType::Hash(_, _) => vec!["Hash".to_string()],
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
