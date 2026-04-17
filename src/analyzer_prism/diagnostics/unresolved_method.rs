use tower_lsp::lsp_types::Location;

use crate::{
    analyzer_prism::diagnostics::ReceiverInfo,
    indexer::{
        entry::{entry_kind::EntryKind, NamespaceKind},
        index::UnresolvedEntry,
        symbol_table::SymbolTable,
    },
    inferrer::r#type::ruby::RubyType,
    types::{fully_qualified_name::FullyQualifiedName, ruby_method::RubyMethod, ruby_namespace::RubyConstant},
};

/// Check whether a method call is unresolved and return an `UnresolvedEntry`
/// when so. Returns `None` when the method is found or the check is inconclusive.
pub fn check(
    receiver_info: &ReceiverInfo,
    inferred_expr_type: Option<&RubyType>,
    method_name: &str,
    target_namespace: &[RubyConstant],
    namespace_kind: NamespaceKind,
    message_location: &Location,
    symbols: &dyn SymbolTable,
) -> Option<UnresolvedEntry> {
    match receiver_info {
        ReceiverInfo::NoReceiver => {
            if !method_exists(symbols, method_name, target_namespace, namespace_kind) {
                let suggestion =
                    find_suggestion(symbols, method_name, target_namespace, namespace_kind);
                Some(UnresolvedEntry::method_with_suggestion(
                    method_name.to_string(),
                    None,
                    message_location.clone(),
                    suggestion,
                ))
            } else {
                None
            }
        }
        ReceiverInfo::ConstantReceiver(receiver_name) => {
            if !method_exists(symbols, method_name, target_namespace, namespace_kind) {
                let suggestion =
                    find_suggestion(symbols, method_name, target_namespace, namespace_kind);
                Some(UnresolvedEntry::method_with_suggestion(
                    method_name.to_string(),
                    Some(RubyType::class(receiver_name)),
                    message_location.clone(),
                    suggestion,
                ))
            } else {
                None
            }
        }
        ReceiverInfo::ExpressionReceiver | ReceiverInfo::InvalidConstantPath => {
            if let Some(class_type @ (RubyType::Class(fqn) | RubyType::Module(fqn))) =
                inferred_expr_type
            {
                let receiver_class_known_in_user_index = fqn
                    .to_instance_namespace()
                    .as_ref()
                    .map(|ns_fqn| symbols.contains_fqn(ns_fqn))
                    .unwrap_or(false);
                if receiver_class_known_in_user_index {
                    let ns_parts = fqn.namespace_parts();
                    if !method_exists(symbols, method_name, &ns_parts, NamespaceKind::Instance) {
                        let suggestion = find_suggestion(
                            symbols,
                            method_name,
                            &ns_parts,
                            NamespaceKind::Instance,
                        );
                        return Some(UnresolvedEntry::method_with_suggestion(
                            method_name.to_string(),
                            Some(class_type.clone()),
                            message_location.clone(),
                            suggestion,
                        ));
                    }
                }
            }
            None
        }
        ReceiverInfo::SelfReceiver => None,
    }
}

/// True when the method exists in the index for `target_namespace` or any ancestor.
fn method_exists(
    symbols: &dyn SymbolTable,
    method_name: &str,
    target_namespace: &[RubyConstant],
    _namespace_kind: NamespaceKind,
) -> bool {
    let method = match RubyMethod::new(method_name) {
        Ok(m) => m,
        Err(_) => return true,
    };

    if symbols.contains_method(&method) {
        return true;
    }

    let method_fqn = FullyQualifiedName::method(target_namespace.to_vec(), method.clone());
    if symbols.contains_fqn(&method_fqn) {
        return true;
    }

    if target_namespace.is_empty() {
        return false;
    }

    let mut ancestors = target_namespace.to_vec();
    while !ancestors.is_empty() {
        if let Ok(m) = RubyMethod::new(method_name) {
            let fqn = FullyQualifiedName::method(ancestors.clone(), m);
            if symbols.contains_fqn(&fqn) {
                return true;
            }
        }
        ancestors.pop();
    }

    false
}

/// Find the closest matching method name within the suggestion threshold.
fn find_suggestion(
    symbols: &dyn SymbolTable,
    target: &str,
    owner: &[RubyConstant],
    kind: NamespaceKind,
) -> Option<String> {
    let threshold = suggestion_threshold(target.len());
    if threshold == 0 {
        return None;
    }

    let mut search: Vec<FullyQualifiedName> = Vec::new();
    let owner_with_kind = FullyQualifiedName::namespace_with_kind(owner.to_vec(), kind);
    search.push(owner_with_kind.clone());
    for ancestor in symbols.get_ancestor_chain(&owner_with_kind) {
        let with_kind =
            FullyQualifiedName::namespace_with_kind(ancestor.namespace_parts(), kind);
        if !search.contains(&with_kind) {
            search.push(with_kind);
        }
    }

    let target_len = target.len();
    let mut best: Option<(String, usize)> = None;

    for owner_fqn in &search {
        for entry in symbols.methods_on_owner(owner_fqn) {
            let EntryKind::Method(data) = &entry.kind else {
                continue;
            };
            let candidate = data.name.get_name();
            if candidate == target {
                continue;
            }
            if candidate.len().abs_diff(target_len) > threshold {
                continue;
            }
            let dist = crate::analyzer_prism::utils::levenshtein(&candidate, target);
            if dist > threshold {
                continue;
            }
            match &best {
                Some((_, d)) if *d <= dist => {}
                _ => best = Some((candidate.to_string(), dist)),
            }
        }
    }
    best.map(|(name, _)| name)
}

/// Suggestion threshold scales with name length.
fn suggestion_threshold(name_len: usize) -> usize {
    match name_len {
        0..=2 => 0,
        3..=8 => 2,
        _ => 3,
    }
}
