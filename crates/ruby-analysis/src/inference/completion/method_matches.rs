//! RBS method candidates for a completion receiver type.

use crate::core::{FullyQualifiedName, NamespaceKind, RubyType};

use super::CompletionMethodMatch;

pub fn rbs_method_matches_for_type(
    receiver_type: &RubyType,
    partial_method: &str,
    kind: NamespaceKind,
) -> Vec<CompletionMethodMatch> {
    if let RubyType::Union(types) = receiver_type {
        let Some((first, rest)) = types.split_first() else {
            return Vec::new();
        };
        let mut common = rbs_method_matches_for_type(first, partial_method, kind)
            .into_iter()
            .map(|candidate| (candidate.name.clone(), candidate))
            .collect::<std::collections::BTreeMap<_, _>>();

        for member in rest {
            let member_matches = rbs_method_matches_for_type(member, partial_method, kind)
                .into_iter()
                .map(|candidate| (candidate.name.clone(), candidate))
                .collect::<std::collections::BTreeMap<_, _>>();
            common.retain(|name, candidate| {
                let Some(member_candidate) = member_matches.get(name) else {
                    return false;
                };
                if candidate.params != member_candidate.params {
                    return false;
                }
                candidate.return_type = match (
                    candidate.return_type.take(),
                    member_candidate.return_type.clone(),
                ) {
                    (Some(left), Some(right)) => RubyType::union_from_proven([left, right], Some),
                    (Some(_), None) | (None, Some(_)) | (None, None) => None,
                };
                true
            });
        }

        return common.into_values().collect();
    }

    let mut matches = Vec::new();
    let mut seen_methods = std::collections::HashSet::new();
    let is_singleton = kind == NamespaceKind::Singleton;

    for class_name in class_names_for_type(receiver_type) {
        for method_info in crate::inference::rbs::get_rbs_class_methods(&class_name, is_singleton) {
            if !method_info.name.starts_with(partial_method) {
                continue;
            }
            if !seen_methods.insert(method_info.name.clone()) {
                continue;
            }
            matches.push(CompletionMethodMatch {
                name: method_info.name,
                params: method_info.params,
                return_type: method_info.return_type,
            });
        }
    }

    if is_singleton {
        for rbs_class in ["Class", "Module"] {
            for method_info in crate::inference::rbs::get_rbs_class_methods(rbs_class, false) {
                if !method_info.name.starts_with(partial_method) {
                    continue;
                }
                if !seen_methods.insert(method_info.name.clone()) {
                    continue;
                }
                matches.push(CompletionMethodMatch {
                    name: method_info.name,
                    params: method_info.params,
                    return_type: method_info.return_type,
                });
            }
        }
    }

    matches.sort_by(|left, right| left.name.cmp(&right.name));
    matches
}

pub(super) fn class_names_for_fqn(fqn: &FullyQualifiedName) -> Vec<String> {
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

fn class_names_for_type(ruby_type: &RubyType) -> Vec<String> {
    match ruby_type {
        RubyType::Class(fqn) | RubyType::ClassReference(fqn) => class_names_for_fqn(fqn),
        RubyType::Module(fqn) | RubyType::ModuleReference(fqn) => fqn
            .namespace_parts()
            .last()
            .map(|constant| vec![constant.to_string()])
            .unwrap_or_default(),
        RubyType::Array(_) => vec!["Array".to_string()],
        RubyType::Hash(_, _) | RubyType::Shape(_) => vec!["Hash".to_string()],
        RubyType::Literal(value) => class_names_for_type(&value.widened_type()),
        RubyType::Union(types) => {
            let mut all_names = Vec::new();
            for ty in types {
                for name in class_names_for_type(ty) {
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
