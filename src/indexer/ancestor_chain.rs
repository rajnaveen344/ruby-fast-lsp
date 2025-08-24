use super::index::RubyIndex;
use crate::analyzer_prism::utils;
use crate::indexer::entry::entry_kind::EntryKind;
use crate::indexer::entry::MixinRef;
use crate::types::fully_qualified_name::FullyQualifiedName;
use std::collections::HashSet;

/// Resolve a MixinRef to a FullyQualifiedName by using the centralized
/// constant resolution utility that follows Ruby's constant lookup rules.
pub fn resolve_mixin_ref(
    index: &RubyIndex,
    mixin_ref: &MixinRef,
    current_fqn: &FullyQualifiedName,
) -> Option<FullyQualifiedName> {
    utils::resolve_constant_fqn_from_parts(
        index,
        &mixin_ref.parts,
        mixin_ref.absolute,
        current_fqn,
    )
}

pub fn get_ancestor_chain(
    index: &RubyIndex,
    fqn: &FullyQualifiedName,
    is_class_method: bool,
) -> Vec<FullyQualifiedName> {
    let mut chain = Vec::new();
    let mut visited = HashSet::new();

    if is_class_method {
        if let Some(entries) = index.definitions.get(fqn) {
            if let Some(entry) = entries.first() {
                if let EntryKind::Class { extends, .. } | EntryKind::Module { extends, .. } =
                    &entry.kind
                {
                    for mixin_ref in extends.iter().rev() {
                        if let Some(resolved_fqn) = resolve_mixin_ref(index, mixin_ref, fqn) {
                            build_chain_recursive(index, &resolved_fqn, &mut chain, &mut visited);
                        }
                    }
                }
            }
        }
    }

    build_chain_recursive(index, fqn, &mut chain, &mut visited);
    chain
}

fn build_chain_recursive(
    index: &RubyIndex,
    fqn: &FullyQualifiedName,
    chain: &mut Vec<FullyQualifiedName>,
    visited: &mut HashSet<FullyQualifiedName>,
) {
    if !visited.insert(fqn.clone()) {
        return;
    }

    if let Some(entries) = index.definitions.get(fqn) {
        if let Some(entry) = entries.first() {
            match &entry.kind {
                EntryKind::Class {
                    superclass,
                    includes,
                    prepends,
                    ..
                } => {
                    for mixin_ref in prepends.iter().rev() {
                        if let Some(resolved_fqn) = resolve_mixin_ref(index, mixin_ref, fqn) {
                            build_chain_recursive(index, &resolved_fqn, chain, visited);
                        }
                    }

                    chain.push(fqn.clone());

                    for mixin_ref in includes {
                        if let Some(resolved_fqn) = resolve_mixin_ref(index, mixin_ref, fqn) {
                            build_chain_recursive(index, &resolved_fqn, chain, visited);
                        }
                    }

                    if let Some(superclass_ref) = superclass {
                        if let Some(resolved_superclass) = resolve_mixin_ref(index, superclass_ref, fqn) {
                            build_chain_recursive(index, &resolved_superclass, chain, visited);
                        }
                    }
                }
                EntryKind::Module {
                    includes, prepends, ..
                } => {
                    for mixin_ref in prepends.iter().rev() {
                        if let Some(resolved_fqn) = resolve_mixin_ref(index, mixin_ref, fqn) {
                            build_chain_recursive(index, &resolved_fqn, chain, visited);
                        }
                    }
                    chain.push(fqn.clone());
                    for mixin_ref in includes {
                        if let Some(resolved_fqn) = resolve_mixin_ref(index, mixin_ref, fqn) {
                            build_chain_recursive(index, &resolved_fqn, chain, visited);
                        }
                    }
                }
                _ => {
                    chain.push(fqn.clone());
                }
            }
        } else {
            chain.push(fqn.clone());
        }
    } else {
        chain.push(fqn.clone());
    }
}
