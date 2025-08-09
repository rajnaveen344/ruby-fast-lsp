use log::debug;
use tower_lsp::lsp_types::Location;

use crate::analyzer_prism::ReceiverKind;
use crate::indexer::ancestor_chain::get_ancestor_chain;
use crate::indexer::entry::entry_kind::EntryKind;
use crate::indexer::entry::{MethodKind, MethodOrigin};
use crate::indexer::index::RubyIndex;
use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::types::ruby_method::RubyMethod;
use crate::types::ruby_namespace::RubyConstant;

/// Find definitions for a Ruby method
pub fn find_method_definitions(
    _ns: &[RubyConstant],
    receiver_kind: &ReceiverKind,
    receiver: &Option<Vec<RubyConstant>>,
    method: &RubyMethod,
    index: &RubyIndex,
    ancestors: &[RubyConstant],
) -> Option<Vec<Location>> {
    match receiver_kind {
        ReceiverKind::Constant => {
            if let Some(receiver_ns) = receiver {
                // For constant receivers, we need to resolve the receiver in the current namespace context
                // Check if the first part of the receiver matches any part in the ancestors
                let full_receiver_ns = if let Some(first_receiver_part) = receiver_ns.first() {
                    if let Some(pos) = ancestors.iter().position(|ancestor| ancestor == first_receiver_part) {
                        // Found the receiver's first part in ancestors, resolve from that position
                        let mut resolved_ns = ancestors[..=pos].to_vec();
                        resolved_ns.extend(receiver_ns[1..].iter().cloned());
                        resolved_ns
                    } else {
                        // Receiver not found in ancestors, extend the current ancestors
                        let mut full_ns = ancestors.to_vec();
                        full_ns.extend(receiver_ns.clone());
                        full_ns
                    }
                } else {
                    // Empty receiver namespace, use ancestors as-is
                    ancestors.to_vec()
                };
                find_method_with_receiver(&full_receiver_ns, method, index)
            } else {
                find_method_without_receiver(method, index, ancestors)
            }
        }
        ReceiverKind::None => find_method_without_receiver(method, index, ancestors),
        ReceiverKind::SelfReceiver => find_method_without_receiver(method, index, ancestors),
        ReceiverKind::Expr => search_by_name(method, index),
    }
}

/// Find method definitions when called with a receiver
/// e.g. A.method, A::B.method, a.method
fn find_method_with_receiver(
    ns: &[RubyConstant],
    method: &RubyMethod,
    index: &RubyIndex,
) -> Option<Vec<Location>> {
    let receiver_fqn = FullyQualifiedName::Constant(ns.to_vec());

    if is_constant_receiver(method) {
        search_direct_references(&receiver_fqn, method, index)
    } else {
        search_by_name(method, index)
    }
}

/// Find method definitions when called without a receiver (e.g., method)
fn find_method_without_receiver(
    method: &RubyMethod,
    index: &RubyIndex,
    ancestors: &[RubyConstant],
) -> Option<Vec<Location>> {
    let receiver_fqn = FullyQualifiedName::Constant(ancestors.to_vec());

    // Search current module and its mixins/inheritance chain
    // For methods without receivers, only search for the method kind that matches the method
    let method_kind = method.get_kind();

    if let Some(locations) = search_in_ancestor_chain(&receiver_fqn, method, index, method_kind) {
        return Some(locations);
    }

    // If we're in a module and didn't find the method, search in all classes/modules that include this module
    // This handles the case where a method in ModuleA calls a method from ModuleB, and both are included in a class
    if let Some(including_classes) = search_in_sibling_modules(&receiver_fqn, method, index, method_kind) {
        return Some(including_classes);
    }

    None
}

/// Check if the receiver is a constant path/read node
fn is_constant_receiver(method: &RubyMethod) -> bool {
    // For constant receivers, we search direct references
    // For non-constant receivers, we fall back to name search
    method.get_kind() == MethodKind::Class || method.get_kind() == MethodKind::Unknown
}

/// Search for direct references in the receiver's ancestor chain
fn search_direct_references(
    receiver_fqn: &FullyQualifiedName,
    method: &RubyMethod,
    index: &RubyIndex,
) -> Option<Vec<Location>> {
    let mut found_locations = Vec::new();

    let kinds_to_check = if method.get_kind() == MethodKind::Unknown {
        vec![MethodKind::Instance, MethodKind::Class]
    } else {
        vec![method.get_kind()]
    };

    for kind in kinds_to_check {
        if let Some(locations) = search_in_ancestor_chain(receiver_fqn, method, index, kind) {
            found_locations.extend(locations);
        }
    }

    if found_locations.is_empty() {
        None
    } else {
        Some(found_locations)
    }
}

/// Search for method definitions by name (fallback for non-constant receivers)
fn search_by_name(method: &RubyMethod, index: &RubyIndex) -> Option<Vec<Location>> {
    let mut found_locations = Vec::new();

    if let Some(entries) = index.methods_by_name.get(method) {
        for entry in entries {
            if let EntryKind::Method { origin, .. } = &entry.kind {
                debug!("Checking method entry: origin={:?}", origin);
                if matches!(origin, MethodOrigin::Direct) {
                    debug!("Adding location: {:?}", entry.location);
                    found_locations.push(entry.location.clone());
                }
            }
        }
    }

    if found_locations.is_empty() {
        None
    } else {
        Some(found_locations)
    }
}

/// Search for method definitions in the ancestor chain
fn search_in_ancestor_chain(
    receiver_fqn: &FullyQualifiedName,
    method: &RubyMethod,
    index: &RubyIndex,
    kind: MethodKind,
) -> Option<Vec<Location>> {
    let mut found_locations = Vec::new();
    let is_class_method = kind == MethodKind::Class;
    let ancestor_chain = get_ancestor_chain(index, receiver_fqn, is_class_method);

    debug!(
        "Searching for {} method {:?} in ancestor chain: {:?}",
        if is_class_method { "class" } else { "instance" },
        method.get_name(),
        ancestor_chain
            .iter()
            .map(|fqn| fqn.to_string())
            .collect::<Vec<_>>(),
    );

    for ancestor_fqn in ancestor_chain {
        let method_fqn = FullyQualifiedName::method(ancestor_fqn.namespace_parts(), method.clone());

        if let Some(entries) = index.definitions.get(&method_fqn.into()) {
            found_locations.extend(entries.iter().map(|e| e.location.clone()));
        }
    }

    if found_locations.is_empty() {
        None
    } else {
        Some(found_locations)
    }
}

/// Search for method definitions in sibling modules
/// When we're in a module and looking for a method, we should also search in all classes/modules
/// that include this module, and then search in their sibling modules (other included modules)
fn search_in_sibling_modules(
    module_fqn: &FullyQualifiedName,
    method: &RubyMethod,
    index: &RubyIndex,
    kind: MethodKind,
) -> Option<Vec<Location>> {
    let mut found_locations = Vec::new();
    
    debug!(
        "Searching for {} method {:?} in sibling modules of {:?}",
        if kind == MethodKind::Class { "class" } else { "instance" },
        method.get_name(),
        module_fqn.to_string(),
    );

    // Get all classes/modules that include this module
    let including_classes = index.get_including_classes(module_fqn);
    
    debug!(
        "Found including classes: {:?}",
        including_classes.iter().map(|fqn| fqn.to_string()).collect::<Vec<_>>()
    );

    // For each including class, search in its complete ancestor chain
    for including_class_fqn in including_classes {
        if let Some(locations) = search_in_ancestor_chain(&including_class_fqn, method, index, kind) {
            found_locations.extend(locations);
        }
    }

    if found_locations.is_empty() {
        None
    } else {
        Some(found_locations)
    }
}
