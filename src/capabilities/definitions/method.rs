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
//! - `find_method_definitions`: Main entry point for method definition search (type-aware)
//! - `search_method_in_class_hierarchy`: Handles class context search
//! - `search_method_in_including_classes`: Handles module context search
//! - `get_ancestor_chain`: Gets the complete ancestor chain from ancestor_chain.rs

use log::debug;
use std::collections::HashSet;
use tower_lsp::lsp_types::{Location, Position, Url};

use crate::analyzer_prism::utils::resolve_constant_fqn_from_parts;
use crate::analyzer_prism::MethodReceiver;
use crate::indexer::ancestor_chain::get_ancestor_chain;
use crate::indexer::entry::entry_kind::EntryKind;
use crate::indexer::entry::MethodKind;
use crate::indexer::index::RubyIndex;
use crate::type_inference::ruby_type::RubyType;
use crate::type_inference::TypeNarrowingEngine;
use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::types::ruby_method::RubyMethod;
use crate::types::ruby_namespace::RubyConstant;
use crate::utils::position_to_offset;

/// Find definitions for a Ruby method with type-aware filtering.
///
/// Uses the type narrowing engine to filter results based on receiver type when available.
/// This provides precise go-to-definition for method calls like `a.length` where `a` is a
/// typed local variable.
pub fn find_method_definitions(
    _ns: &[RubyConstant],
    receiver: &MethodReceiver,
    method: &RubyMethod,
    index: &RubyIndex,
    ancestors: &[RubyConstant],
    type_narrowing: &TypeNarrowingEngine,
    uri: &Url,
    position: Position,
    content: &str,
) -> Option<Vec<Location>> {
    match receiver {
        MethodReceiver::Constant(path) => {
            handle_constant_receiver(&Some(path.clone()), method, index, ancestors)
        }
        MethodReceiver::None | MethodReceiver::SelfReceiver => {
            find_method_without_receiver(method, index, ancestors)
        }
        MethodReceiver::LocalVariable(name)
        | MethodReceiver::InstanceVariable(name)
        | MethodReceiver::ClassVariable(name)
        | MethodReceiver::GlobalVariable(name) => {
            // Try to get the receiver's type using type narrowing
            let offset = position_to_offset(content, position);
            if let Some(receiver_type) = type_narrowing.get_narrowed_type(uri, name, offset) {
                debug!("Found receiver type for '{}': {:?}", name, receiver_type);
                return search_by_name_filtered(method, index, &receiver_type);
            }
            // Fallback to unfiltered search
            search_by_name(method, index)
        }
        MethodReceiver::MethodCall {
            inner_receiver,
            method_name,
        } => {
            // Method call receiver - try to resolve the inner receiver's type first,
            // then look up the method's return type
            let receiver_type = resolve_method_call_type(
                inner_receiver,
                method_name,
                index,
                type_narrowing,
                uri,
                position,
                content,
            );
            if let Some(ty) = receiver_type {
                debug!(
                    "Found method call receiver type for '{}.{}': {:?}",
                    inner_receiver_to_string(inner_receiver),
                    method_name,
                    ty
                );
                return search_by_name_filtered(method, index, &ty);
            }
            // Fallback to unfiltered search
            search_by_name(method, index)
        }
        MethodReceiver::Expression => {
            // Complex expression - can't determine type, search by name
            search_by_name(method, index)
        }
    }
}

/// Resolve the type of a method call receiver by looking up the method's return type
fn resolve_method_call_type(
    inner_receiver: &MethodReceiver,
    method_name: &str,
    index: &RubyIndex,
    type_narrowing: &TypeNarrowingEngine,
    uri: &Url,
    position: Position,
    content: &str,
) -> Option<RubyType> {
    use crate::type_inference::method_resolver::MethodResolver;

    // First, resolve the inner receiver's type
    let inner_type = match inner_receiver {
        MethodReceiver::None | MethodReceiver::SelfReceiver => {
            // TODO: For self, we'd need to know the current class context
            return None;
        }
        MethodReceiver::Constant(path) => {
            // Class method call - the type is the class itself
            RubyType::ClassReference(FullyQualifiedName::Constant(path.clone()))
        }
        MethodReceiver::LocalVariable(name)
        | MethodReceiver::InstanceVariable(name)
        | MethodReceiver::ClassVariable(name)
        | MethodReceiver::GlobalVariable(name) => {
            let offset = position_to_offset(content, position);
            type_narrowing.get_narrowed_type(uri, name, offset)?
        }
        MethodReceiver::MethodCall {
            inner_receiver: nested_receiver,
            method_name: nested_method,
        } => {
            // Recursively resolve nested method calls
            resolve_method_call_type(
                nested_receiver,
                nested_method,
                index,
                type_narrowing,
                uri,
                position,
                content,
            )?
        }
        MethodReceiver::Expression => {
            return None;
        }
    };

    // Now look up the method's return type on the inner type
    MethodResolver::resolve_method_return_type(index, &inner_type, method_name)
}

/// Helper to convert inner receiver to string for debug logging
fn inner_receiver_to_string(receiver: &MethodReceiver) -> String {
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
        } => format!(
            "{}.{}",
            inner_receiver_to_string(inner_receiver),
            method_name
        ),
        MethodReceiver::Expression => "<expr>".to_string(),
    }
}

/// Search for methods by name, filtered by receiver type
fn search_by_name_filtered(
    method: &RubyMethod,
    index: &RubyIndex,
    receiver_type: &RubyType,
) -> Option<Vec<Location>> {
    let type_names = get_type_names(receiver_type);

    if type_names.is_empty() {
        // Unknown type, fall back to unfiltered search
        return search_by_name(method, index);
    }

    let mut filtered_locations = Vec::new();

    if let Some(entries) = index.get_methods_by_name(method) {
        for entry in entries.iter() {
            // Check if this method belongs to one of the receiver's types
            let fqn = match index.get_fqn(entry.fqn_id) {
                Some(f) => f,
                None => continue,
            };
            let method_class = fqn.namespace_parts();
            if !method_class.is_empty() {
                let class_name = method_class
                    .iter()
                    .map(|c| c.to_string())
                    .collect::<Vec<_>>()
                    .join("::");

                if type_names.iter().any(|t| *t == class_name) {
                    if let Some(loc) = index.to_lsp_location(&entry.location) {
                        filtered_locations.push(loc);
                    }
                }
            }
        }
    }

    if filtered_locations.is_empty() {
        // No matches for the specific type, fall back to unfiltered
        search_by_name(method, index)
    } else {
        Some(filtered_locations)
    }
}

/// Extract type names from a RubyType (handles unions)
fn get_type_names(ty: &RubyType) -> Vec<String> {
    match ty {
        RubyType::Class(fqn) => vec![fqn.to_string()],
        RubyType::Union(types) => types.iter().flat_map(get_type_names).collect(),
        _ => vec![],
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
    let mut visited = HashSet::new();

    // Search current module and its mixins/inheritance chain
    let method_kind = method.get_kind();

    if let Some(locations) = search_in_ancestor_chain_with_visited(
        &receiver_fqn,
        method,
        index,
        method_kind,
        &mut visited,
    ) {
        return Some(locations);
    }

    // If we're in a module and didn't find the method, search in sibling modules
    if let Some(including_classes) = search_in_sibling_modules_with_visited(
        &receiver_fqn,
        method,
        index,
        method_kind,
        &mut visited,
    ) {
        return Some(including_classes);
    }

    None
}

// ============================================================================
// UTILITY FUNCTIONS
// ============================================================================

/// Check if a method should be searched via direct references (class/unknown methods)
fn is_constant_receiver(method: &RubyMethod) -> bool {
    method.get_kind() == MethodKind::Class || method.get_kind() == MethodKind::Unknown
}

/// Search for direct method references in the index
fn search_direct_references(
    receiver_fqn: &FullyQualifiedName,
    method: &RubyMethod,
    index: &RubyIndex,
) -> Option<Vec<Location>> {
    let mut found_locations = Vec::new();
    let mut visited = HashSet::new();

    let kinds_to_check = if method.get_kind() == MethodKind::Unknown {
        vec![MethodKind::Instance, MethodKind::Class]
    } else {
        vec![method.get_kind()]
    };

    for kind in kinds_to_check {
        if let Some(locations) =
            search_in_ancestor_chain_with_visited(receiver_fqn, method, index, kind, &mut visited)
        {
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
    let mut included_modules = Vec::new();
    let mut seen_modules = HashSet::<FullyQualifiedName>::new();

    let ancestor_chain = get_ancestor_chain(index, class_fqn, false);

    for ancestor_fqn in &ancestor_chain {
        if let Some(entries) = index.get(ancestor_fqn) {
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
    entry_kind: &EntryKind,
    ancestor_fqn: &FullyQualifiedName,
    included_modules: &mut Vec<FullyQualifiedName>,
    seen_modules: &mut HashSet<FullyQualifiedName>,
) {
    let (includes, extends, prepends) = match entry_kind {
        EntryKind::Class(data) => (&data.includes, &data.extends, &data.prepends),
        EntryKind::Module(data) => (&data.includes, &data.extends, &data.prepends),
        _ => return,
    };

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
    index.get_methods_by_name(method).and_then(|entries| {
        let locations: Vec<Location> = entries
            .iter()
            .filter_map(|entry| index.to_lsp_location(&entry.location))
            .collect();
        if locations.is_empty() {
            None
        } else {
            Some(locations)
        }
    })
}

// ============================================================================
// CONTEXT DETERMINATION
// ============================================================================

/// Check if the given FQN represents a class context
fn is_class_context(index: &RubyIndex, fqn: &FullyQualifiedName) -> bool {
    if let Some(entries) = index.get(fqn) {
        for entry in entries {
            match &entry.kind {
                EntryKind::Class(_) => return true,
                EntryKind::Module(_) => return false,
                _ => continue,
            }
        }
    }
    true // Default to class if we can't determine
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
    if visited.contains(receiver_fqn) {
        return None;
    }

    visited.insert(receiver_fqn.clone());

    let found_locations = if is_class_context(index, receiver_fqn) {
        search_method_in_class_hierarchy(receiver_fqn, method, index, kind)
    } else {
        search_method_in_including_classes(receiver_fqn, method, index)
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
fn search_method_in_class_hierarchy(
    receiver_fqn: &FullyQualifiedName,
    method: &RubyMethod,
    index: &RubyIndex,
    kind: MethodKind,
) -> Vec<Location> {
    let mut found_locations = Vec::new();
    let is_class_method = kind == MethodKind::Class;

    let mut modules_to_search = HashSet::new();
    modules_to_search.insert(receiver_fqn.clone());

    let ancestor_chain = get_ancestor_chain(index, receiver_fqn, is_class_method);

    for ancestor_fqn in &ancestor_chain {
        modules_to_search.insert(ancestor_fqn.clone());

        let included_modules = get_included_modules(index, ancestor_fqn);
        for module_fqn in included_modules {
            collect_all_searchable_modules(index, &module_fqn, &mut modules_to_search);
        }
    }

    for module_fqn in &modules_to_search {
        let method_fqn = FullyQualifiedName::method(module_fqn.namespace_parts(), method.clone());
        if let Some(entries) = index.get(&method_fqn) {
            found_locations.extend(
                entries
                    .iter()
                    .filter_map(|e| index.to_lsp_location(&e.location)),
            );
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
fn search_method_in_including_classes(
    receiver_fqn: &FullyQualifiedName,
    method: &RubyMethod,
    index: &RubyIndex,
) -> Vec<Location> {
    let mut found_locations = Vec::new();
    let mut modules_to_search = HashSet::new();

    modules_to_search.insert(receiver_fqn.clone());

    let including_classes = index.get_including_classes(receiver_fqn);

    for class_fqn in including_classes {
        collect_all_searchable_modules(index, &class_fqn, &mut modules_to_search);

        let included_modules = get_included_modules(index, &class_fqn);
        for module_fqn in included_modules {
            collect_all_searchable_modules(index, &module_fqn, &mut modules_to_search);
        }
    }

    for module_fqn in &modules_to_search {
        let method_fqn = FullyQualifiedName::method(module_fqn.namespace_parts(), method.clone());
        if let Some(entries) = index.get(&method_fqn) {
            found_locations.extend(
                entries
                    .iter()
                    .filter_map(|e| index.to_lsp_location(&e.location)),
            );
        }
    }

    deduplicate_locations(found_locations)
}

/// Recursively collect all modules that should be searched for a given module/class
fn collect_all_searchable_modules(
    index: &RubyIndex,
    fqn: &FullyQualifiedName,
    modules_to_search: &mut HashSet<FullyQualifiedName>,
) {
    if modules_to_search.contains(fqn) {
        return;
    }

    modules_to_search.insert(fqn.clone());

    let ancestor_chain = get_ancestor_chain(index, fqn, false);
    for ancestor_fqn in &ancestor_chain {
        if !modules_to_search.contains(ancestor_fqn) {
            modules_to_search.insert(ancestor_fqn.clone());
        }
    }

    let included_modules = get_included_modules(index, fqn);
    for module_fqn in included_modules {
        collect_all_searchable_modules(index, &module_fqn, modules_to_search);
    }
}

// ============================================================================
// SIBLING MODULE SEARCH
// ============================================================================

/// Search for methods in sibling modules with visited tracking
fn search_in_sibling_modules_with_visited(
    class_fqn: &FullyQualifiedName,
    method: &RubyMethod,
    index: &RubyIndex,
    kind: MethodKind,
    visited: &mut HashSet<FullyQualifiedName>,
) -> Option<Vec<Location>> {
    let mut found_locations = Vec::new();

    let included_modules = get_included_modules(index, class_fqn);

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
