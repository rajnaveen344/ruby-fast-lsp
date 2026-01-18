//! Helper utilities for method resolution

use crate::analyzer_prism::MethodReceiver;
use crate::indexer::entry::entry_kind::EntryKind;
use crate::indexer::entry::NamespaceKind;
use crate::indexer::index::RubyIndex;
use crate::types::fully_qualified_name::FullyQualifiedName;

/// Check if FQN is a module instance namespace.
pub fn is_module_instance_namespace(index: &RubyIndex, fqn: &FullyQualifiedName) -> bool {
    // Must be instance namespace (include/prepend don't affect singletons)
    if fqn.namespace_kind() != Some(NamespaceKind::Instance) {
        return false;
    }

    // Must be a module (not a class)
    if let Some(entries) = index.get(fqn) {
        if let Some(entry) = entries.first() {
            return matches!(entry.kind, EntryKind::Module(_));
        }
    }

    false
}

/// Get all FQNs of classes/modules that include this module.
///
/// INVARIANT: Must only be called with module instance namespaces.
pub fn get_module_includers(
    index: &RubyIndex,
    module_fqn: &FullyQualifiedName,
) -> Vec<FullyQualifiedName> {
    // INVARIANT: Must be module instance namespace
    assert!(
        is_module_instance_namespace(index, module_fqn),
        "INVARIANT VIOLATED: get_module_includers called with non-module-instance FQN: {:?}. \
         This is a bug - only module instance namespaces have includers. \
         Fix: Call is_module_instance_namespace() before calling this function.",
        module_fqn
    );

    let Some(module_id) = index.get_fqn_id(module_fqn) else {
        return Vec::new(); // Module not indexed yet
    };

    // Get includers from graph (reverse edges: who includes me?)
    let mut includer_ids = index.get_graph().mixers(module_id);

    // Fallback: scan all classes if graph incomplete
    if includer_ids.is_empty() {
        includer_ids = index
            .get_transitive_mixin_classes(module_fqn)
            .into_iter()
            .filter_map(|fqn| index.get_fqn_id(&fqn))
            .collect();
    }

    // Convert IDs to FQNs
    includer_ids
        .into_iter()
        .filter_map(|id| index.get_fqn(id).cloned())
        .collect()
}

/// Check if entry's owner is in the ancestor chain.
pub fn matches_ancestor(
    entry: &crate::indexer::entry::Entry,
    chain: &[FullyQualifiedName],
) -> bool {
    let EntryKind::Method(data) = &entry.kind else {
        return true;
    };

    chain.iter().any(|ancestor| {
        ancestor.namespace_parts() == data.owner.namespace_parts()
            && ancestor.namespace_kind() == data.owner.namespace_kind()
    })
}

/// Convert receiver to string for debugging.
pub fn receiver_to_string(receiver: &MethodReceiver) -> String {
    match receiver {
        MethodReceiver::None => "".to_string(),
        MethodReceiver::SelfReceiver => "self".to_string(),
        MethodReceiver::Constant(path) => path
            .iter()
            .map(|c| c.to_string())
            .collect::<Vec<_>>()
            .join("::"),
        MethodReceiver::LocalVariable(name)
        | MethodReceiver::InstanceVariable(name)
        | MethodReceiver::ClassVariable(name)
        | MethodReceiver::GlobalVariable(name) => name.clone(),
        MethodReceiver::MethodCall {
            inner_receiver,
            method_name,
        } => format!("{}.{}", receiver_to_string(inner_receiver), method_name),
        MethodReceiver::Expression => "<expr>".to_string(),
    }
}
