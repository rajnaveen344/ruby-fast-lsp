use super::index::RubyIndex;
use crate::indexer::entry::entry_kind::EntryKind;
use crate::indexer::entry::MixinRef;
use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::types::ruby_namespace::RubyConstant;
use std::collections::HashSet;

// Resolve a MixinRef to a FullyQualifiedName by searching the index
// according to Ruby's constant lookup rules.
fn resolve_mixin_ref(
    index: &RubyIndex,
    mixin_ref: &MixinRef,
    current_fqn: &FullyQualifiedName,
) -> Option<FullyQualifiedName> {
    let mut search_paths: Vec<Vec<RubyConstant>> = vec![];

    if mixin_ref.absolute {
        // For `::Foo::Bar`, we check `Foo::Bar`, then `Bar`
        let mut parts = mixin_ref.parts.clone();
        while !parts.is_empty() {
            search_paths.push(parts.clone());
            parts.remove(0);
        }
    } else {
        // For relative paths like `C` inside `module A; module B;`,
        // search order is `A::B::C`, `A::C`, `C`.
        let mut lexical_scope = current_fqn.namespace_parts().to_vec();
        loop {
            let mut candidate_parts = lexical_scope.clone();
            candidate_parts.extend(mixin_ref.parts.clone());
            search_paths.push(candidate_parts);

            if lexical_scope.is_empty() {
                break;
            }
            lexical_scope.pop();
        }
    }

    for parts in search_paths {
        let candidate_fqn = FullyQualifiedName::Constant(parts);
        if index.definitions.contains_key(&candidate_fqn) {
            return Some(candidate_fqn);
        }
    }

    None
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

                    if let Some(superclass) = superclass {
                        build_chain_recursive(index, superclass, chain, visited);
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
