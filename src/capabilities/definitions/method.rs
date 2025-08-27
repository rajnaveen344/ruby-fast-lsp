//! # Method Definition Search Module
//!
//! This module implements Ruby method definition lookup following Ruby's method resolution order.
//! It handles both class and module contexts with different search strategies.
//!
//! ## Search Strategy Overview
//!
//! ### Class Context (when receiver is a class):
//! 1. Search current class for the method
//! 2. Search included modules recursively (in reverse order of inclusion)
//! 3. Search parent class and repeat the process
//! 4. Search parent class's modules recursively
//!
//! ### Module Context (when receiver is a module):
//! 1. Search the module itself
//! 2. Find all classes that include/prepend/extend this module
//! 3. For each including class, search its complete hierarchy
//!
//! ## Key Functions:
//! - `find_method_definitions`: Main entry point for method definition search
//! - `search_method_in_class_hierarchy`: Handles class context search
//! - `search_method_in_including_classes`: Handles module context search
//! - `get_ancestor_chain`: Gets the complete ancestor chain from ancestor_chain.rs

use log::debug;
use std::collections::HashSet;
use tower_lsp::lsp_types::Location;

use crate::analyzer_prism::utils::resolve_constant_fqn_from_parts;
use crate::analyzer_prism::ReceiverKind;
use crate::indexer::ancestor_chain::get_ancestor_chain;
use crate::indexer::entry::entry_kind::EntryKind;
use crate::indexer::entry::MethodKind;
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
            handle_constant_receiver(receiver, method, index, ancestors)
        }
        ReceiverKind::None | ReceiverKind::SelfReceiver => {
            find_method_without_receiver(method, index, ancestors)
        }
        ReceiverKind::Expr => {
            search_by_name(method, index)
        }
    }
}

fn handle_constant_receiver(
    receiver: &Option<Vec<RubyConstant>>,
    method: &RubyMethod,
    index: &RubyIndex,
    ancestors: &[RubyConstant],
) -> Option<Vec<Location>> {
    if let Some(receiver_ns) = receiver {
        let current_fqn = FullyQualifiedName::Constant(ancestors.to_vec());
        if let Some(resolved_fqn) =
            resolve_constant_fqn_from_parts(index, receiver_ns, false, &current_fqn)
        {
            if let FullyQualifiedName::Constant(resolved_ns) = resolved_fqn {
                find_method_with_receiver(&resolved_ns, method, index)
            } else {
                None
            }
        } else {
            // Fallback: if resolution fails, try with the receiver namespace as-is
            find_method_with_receiver(receiver_ns, method, index)
        }
    } else {
        find_method_without_receiver(method, index, ancestors)
    }
}

// ============================================================================
// METHOD SEARCH FUNCTIONS
// ============================================================================

/// Find method definitions when a receiver is present
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

fn find_method_without_receiver(
    method: &RubyMethod,
    index: &RubyIndex,
    ancestors: &[RubyConstant],
) -> Option<Vec<Location>> {
    let receiver_fqn = FullyQualifiedName::Constant(ancestors.to_vec());
    let mut visited = HashSet::new(); // Global visited set for this entire search

    // Search current module and its mixins/inheritance chain
    // For methods without receivers, only search for the method kind that matches the method
    let method_kind = method.get_kind();

    if let Some(locations) = search_in_ancestor_chain_with_visited(&receiver_fqn, method, index, method_kind, &mut visited) {
        return Some(locations);
    }

    // If we're in a module and didn't find the method, search in all classes/modules that include this module
    // This handles the case where a method in ModuleA calls a method from ModuleB, and both are included in a class
    if let Some(including_classes) =
        search_in_sibling_modules_with_visited(&receiver_fqn, method, index, method_kind, &mut visited)
    {
        return Some(including_classes);
    }
    
    None
}

// ============================================================================
// UTILITY FUNCTIONS
// ============================================================================

/// Check if a method name represents a constant (starts with uppercase)
fn is_constant_receiver(method: &RubyMethod) -> bool {
    // For constant receivers, we search direct references
    // For non-constant receivers, we fall back to name search
    method.get_kind() == MethodKind::Class || method.get_kind() == MethodKind::Unknown
}

/// Search for direct method references in the index
fn search_direct_references(
    receiver_fqn: &FullyQualifiedName,
    method: &RubyMethod,
    index: &RubyIndex,
) -> Option<Vec<Location>> {
    let mut found_locations = Vec::new();
    let mut visited = HashSet::new(); // Global visited set for this search

    let kinds_to_check = if method.get_kind() == MethodKind::Unknown {
        vec![MethodKind::Instance, MethodKind::Class]
    } else {
        vec![method.get_kind()]
    };

    for kind in kinds_to_check {
        if let Some(locations) = search_in_ancestor_chain_with_visited(receiver_fqn, method, index, kind, &mut visited) {
            found_locations.extend(locations);
        }
    }

    if found_locations.is_empty() {
        None
    } else {
        Some(deduplicate_locations(found_locations))
    }
}

// ============================================================================
// MODULE INCLUSION HANDLING
// ============================================================================

/// Get all modules included/extended/prepended by a class or module
fn get_included_modules(
    index: &RubyIndex,
    class_fqn: &FullyQualifiedName,
) -> Vec<FullyQualifiedName> {
    debug!("Starting get_included_modules for {:?}", class_fqn);

    let mut included_modules = Vec::new();
    let mut seen_modules = HashSet::<FullyQualifiedName>::new();

    // Get the ancestor chain to check superclasses too
    let ancestor_chain = get_ancestor_chain(index, class_fqn, false);

    // Check each class/module in the ancestor chain for included modules
    for ancestor_fqn in &ancestor_chain {
        if let Some(entries) = index.definitions.get(ancestor_fqn) {
            for entry in entries.iter() {
                process_entry_mixins(
                    index,
                    &entry.kind,
                    ancestor_fqn,
                    &mut included_modules,
                    &mut seen_modules,
                );
            }
        }
    }

    included_modules
}

/// Process mixins for a specific entry (class/module)
fn process_entry_mixins(
    index: &RubyIndex,
    entry_kind: &crate::indexer::entry::entry_kind::EntryKind,
    ancestor_fqn: &FullyQualifiedName,
    included_modules: &mut Vec<FullyQualifiedName>,
    seen_modules: &mut HashSet<FullyQualifiedName>,
) {
    use crate::indexer::entry::entry_kind::EntryKind;

    match entry_kind {
        EntryKind::Class {
            includes,
            extends,
            prepends,
            ..
        }
        | EntryKind::Module {
            includes,
            extends,
            prepends,
            ..
        } => {
            process_mixins(
                index,
                prepends,
                ancestor_fqn,
                included_modules,
                seen_modules,
                true,
            );
            process_mixins(
                index,
                includes,
                ancestor_fqn,
                included_modules,
                seen_modules,
                false,
            );
            process_mixins(
                index,
                extends,
                ancestor_fqn,
                included_modules,
                seen_modules,
                false,
            );
        }
        _ => {}
    }
}

/// Process a list of mixins and add them to the included modules
fn process_mixins(
    index: &RubyIndex,
    mixins: &[crate::indexer::entry::MixinRef],
    ancestor_fqn: &FullyQualifiedName,
    included_modules: &mut Vec<FullyQualifiedName>,
    seen_modules: &mut HashSet<FullyQualifiedName>,
    reverse_order: bool,
) {
    use crate::indexer::ancestor_chain::resolve_mixin_ref;

    let iter: Box<dyn Iterator<Item = _>> = if reverse_order {
        Box::new(mixins.iter().rev())
    } else {
        Box::new(mixins.iter())
    };

    for mixin_ref in iter {
        if let Some(resolved_fqn) = resolve_mixin_ref(index, mixin_ref, ancestor_fqn) {
            if seen_modules.insert(resolved_fqn.clone()) {
                included_modules.push(resolved_fqn);
            }
        }
    }
}

/// Search for methods by name across the entire index
fn search_by_name(method: &RubyMethod, index: &RubyIndex) -> Option<Vec<Location>> {
    if let Some(entries) = index.methods_by_name.get(method) {
        let locations: Vec<Location> = entries.iter().map(|entry| entry.location.clone()).collect();

        if locations.is_empty() {
            None
        } else {
            Some(locations)
        }
    } else {
        None
    }
}

// ============================================================================
// CONTEXT DETERMINATION
// ============================================================================

/// Check if the given FQN represents a class context
fn is_class_context(index: &RubyIndex, fqn: &FullyQualifiedName) -> bool {
    if let Some(entries) = index.definitions.get(fqn) {
        for entry in entries {
            match &entry.kind {
                EntryKind::Class { .. } => return true,
                EntryKind::Module { .. } => return false,
                _ => continue,
            }
        }
    }
    // Default to class if we can't determine
    true
}

// ============================================================================
// ANCESTOR CHAIN SEARCH
// ============================================================================



/// Search for methods in the ancestor chain with visited tracking
fn search_in_ancestor_chain_with_visited(
    receiver_fqn: &FullyQualifiedName,
    method: &RubyMethod,
    index: &RubyIndex,
    kind: MethodKind,
    visited: &mut HashSet<FullyQualifiedName>,
) -> Option<Vec<Location>> {
    // Prevent infinite recursion
    if visited.contains(receiver_fqn) {
        return None;
    }

    visited.insert(receiver_fqn.clone());

    let found_locations = if is_class_context(index, receiver_fqn) {
        search_method_in_class_hierarchy(receiver_fqn, method, index, kind, visited)
    } else {
        search_method_in_including_classes(receiver_fqn, method, index, kind, visited)
    };

    if found_locations.is_empty() {
        None
    } else {
        Some(found_locations)
    }
}

/// Searches for method definitions in a class hierarchy context.
/// 
/// Follows Ruby's method lookup order:
/// 1. Current class - search for the method in the class itself
/// 2. Included modules - search modules included by the class (in reverse order)
/// 3. Parent class - search the superclass and repeat the process recursively
/// 4. Parent's modules - search modules included by parent classes
/// 
/// For class methods, this also processes 'extend' mixins which add class methods.
fn search_method_in_class_hierarchy(
    receiver_fqn: &FullyQualifiedName,
    method: &RubyMethod,
    index: &RubyIndex,
    kind: MethodKind,
    _visited: &mut HashSet<FullyQualifiedName>,
) -> Vec<Location> {
    debug!("Searching method '{}' in class hierarchy starting from {:?}", method.get_name(), receiver_fqn);
    let mut found_locations = Vec::new();
    let is_class_method = kind == MethodKind::Class;

    // Build complete set of all modules/classes to search (single-pass collection)
    let mut modules_to_search = HashSet::new();
    
    // Step 1: Add the current class to search
    modules_to_search.insert(receiver_fqn.clone());
    
    // Step 2: Get the complete ancestor chain (includes parent classes and their modules)
    let ancestor_chain = get_ancestor_chain(index, receiver_fqn, is_class_method);
    debug!("Found {} ancestors in chain for {:?}", ancestor_chain.len(), receiver_fqn);
    
    // Step 3: Add all ancestors to the search set
    for ancestor_fqn in &ancestor_chain {
        modules_to_search.insert(ancestor_fqn.clone());
        
        // Add all modules included by this ancestor
        let included_modules = get_included_modules(index, ancestor_fqn);
        for module_fqn in included_modules {
            collect_all_searchable_modules(index, &module_fqn, &mut modules_to_search);
        }
    }
    
    // 3. Search each module exactly once
    for module_fqn in &modules_to_search {
        let method_fqn = FullyQualifiedName::method(module_fqn.namespace_parts(), method.clone());
        if let Some(entries) = index.definitions.get(&method_fqn) {
            found_locations.extend(entries.iter().map(|e| e.location.clone()));
        }
    }

    deduplicate_locations(found_locations)
}

/// Remove duplicate locations by comparing URI and range
fn deduplicate_locations(locations: Vec<Location>) -> Vec<Location> {
    let mut unique_locations = Vec::new();
    
    for location in locations {
        if !unique_locations.iter().any(|existing: &Location| {
            existing.uri == location.uri && existing.range == location.range
        }) {
            unique_locations.push(location);
        }
    }
    
    unique_locations
}

/// Searches for method definitions in a module context.
/// 
/// When searching in a module, we need to:
/// 1. Search the module itself for the method definition
/// 2. Find all classes that include/prepend/extend this module
/// 3. For each including class, search its complete hierarchy
/// 
/// This handles cases where a method is called on a module but the actual
/// implementation might be in a class that includes the module.
fn search_method_in_including_classes(
    receiver_fqn: &FullyQualifiedName,
    method: &RubyMethod,
    index: &RubyIndex,
    _kind: MethodKind,
    _visited: &mut HashSet<FullyQualifiedName>,
) -> Vec<Location> {
    debug!("Searching method '{}' in module context for {:?}", method.get_name(), receiver_fqn);
    let mut found_locations = Vec::new();

    // Build complete set of all modules/classes to search (single-pass collection)
    let mut modules_to_search = HashSet::new();
    
    // Step 1: Add the module itself to search
    modules_to_search.insert(receiver_fqn.clone());
    
    // Step 2: Find all classes that include this module and add their dependencies
    let including_classes = index.get_including_classes(receiver_fqn);
    debug!("Found {} classes that include module {:?}", including_classes.len(), receiver_fqn);
    
    // Step 3: For each including class, add it and its complete hierarchy
    for class_fqn in including_classes {
        // Add the class itself and its ancestor chain
        collect_all_searchable_modules(index, &class_fqn, &mut modules_to_search);
        
        // Add all modules included by this class
        let included_modules = get_included_modules(index, &class_fqn);
        for module_fqn in included_modules {
            collect_all_searchable_modules(index, &module_fqn, &mut modules_to_search);
        }
    }
    
    // 3. Search each module exactly once
    for module_fqn in &modules_to_search {
        let method_fqn = FullyQualifiedName::method(module_fqn.namespace_parts(), method.clone());
        if let Some(entries) = index.definitions.get(&method_fqn) {
            found_locations.extend(entries.iter().map(|e| e.location.clone()));
        }
    }

    deduplicate_locations(found_locations)
}

/// Recursively collect all modules that should be searched for a given module/class,
/// including its ancestor chain and included modules, without exponential traversal
fn collect_all_searchable_modules(
    index: &RubyIndex,
    fqn: &FullyQualifiedName,
    modules_to_search: &mut HashSet<FullyQualifiedName>,
) {
    // Avoid infinite recursion
    if modules_to_search.contains(fqn) {
        return;
    }
    
    modules_to_search.insert(fqn.clone());
    
    // Add ancestor chain (for classes)
    let ancestor_chain = get_ancestor_chain(index, fqn, false); // Use instance method chain
    for ancestor_fqn in &ancestor_chain {
        if !modules_to_search.contains(ancestor_fqn) {
            modules_to_search.insert(ancestor_fqn.clone());
        }
    }
    
    // Add included modules recursively
    let included_modules = get_included_modules(index, fqn);
    for module_fqn in included_modules {
        collect_all_searchable_modules(index, &module_fqn, modules_to_search);
    }
}

// ============================================================================
// SIBLING MODULE SEARCH
// ============================================================================

/// Search for methods in sibling modules (included/extended/prepended)


/// Search for methods in sibling modules with visited tracking
fn search_in_sibling_modules_with_visited(
    class_fqn: &FullyQualifiedName,
    method: &RubyMethod,
    index: &RubyIndex,
    kind: MethodKind,
    visited: &mut HashSet<FullyQualifiedName>,
) -> Option<Vec<Location>> {
    let mut found_locations = Vec::new();

    // Get the modules that this class/module includes, extends, or prepends
    let included_modules = get_included_modules(index, class_fqn);

    // Search in each included module and its ancestor chain
    for module_fqn in included_modules {
        if let Some(locations) =
            search_in_ancestor_chain_with_visited(&module_fqn, method, index, kind, visited)
        {
            found_locations.extend(locations);
        }
    }

    if found_locations.is_empty() {
        None
    } else {
        Some(found_locations)
    }
}
