use log::{debug, info};
use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;
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

// Global cache for included modules to prevent recomputation
static INCLUDED_MODULES_CACHE: OnceLock<Mutex<HashMap<FullyQualifiedName, Vec<FullyQualifiedName>>>> = OnceLock::new();

/// Find definitions for a Ruby method
pub fn find_method_definitions(
    _ns: &[RubyConstant],
    receiver_kind: &ReceiverKind,
    receiver: &Option<Vec<RubyConstant>>,
    method: &RubyMethod,
    index: &RubyIndex,
    ancestors: &[RubyConstant],
) -> Option<Vec<Location>> {
    let start_time = Instant::now();
    info!("[PERF] Starting method definition search for method: {:?}, receiver_kind: {:?}", method.get_name(), receiver_kind);
    
    let result = match receiver_kind {
        ReceiverKind::Constant => {
            let const_start = Instant::now();
            let result = handle_constant_receiver(receiver, method, index, ancestors);
            info!("[PERF] Constant receiver search took: {:?}", const_start.elapsed());
            result
        }
        ReceiverKind::None | ReceiverKind::SelfReceiver => {
            let no_receiver_start = Instant::now();
            let result = find_method_without_receiver(method, index, ancestors);
            info!("[PERF] No receiver search took: {:?}", no_receiver_start.elapsed());
            result
        }
        ReceiverKind::Expr => {
            let expr_start = Instant::now();
            let result = search_by_name(method, index);
            info!("[PERF] Expression receiver search took: {:?}", expr_start.elapsed());
            result
        }
    };
    
    info!("[PERF] Total method definition search took: {:?}, found {} locations", 
          start_time.elapsed(), 
          result.as_ref().map_or(0, |v| v.len()));
    result
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
    let start_time = Instant::now();
    info!("[PERF] Starting find_method_without_receiver for method: {:?}", method.get_name());
    
    let receiver_fqn = FullyQualifiedName::Constant(ancestors.to_vec());
    let mut visited = HashSet::new(); // Global visited set for this entire search

    // Search current module and its mixins/inheritance chain
    // For methods without receivers, only search for the method kind that matches the method
    let method_kind = method.get_kind();

    let ancestor_start = Instant::now();
    if let Some(locations) = search_in_ancestor_chain_with_visited(&receiver_fqn, method, index, method_kind, &mut visited) {
        info!("[PERF] Ancestor chain search took: {:?}, found {} locations", 
              ancestor_start.elapsed(), locations.len());
        info!("[PERF] find_method_without_receiver total time: {:?}", start_time.elapsed());
        return Some(locations);
    }
    info!("[PERF] Ancestor chain search took: {:?}, found 0 locations", ancestor_start.elapsed());

    // If we're in a module and didn't find the method, search in all classes/modules that include this module
    // This handles the case where a method in ModuleA calls a method from ModuleB, and both are included in a class
    let sibling_start = Instant::now();
    if let Some(including_classes) =
        search_in_sibling_modules_with_visited(&receiver_fqn, method, index, method_kind, &mut visited)
    {
        info!("[PERF] Sibling modules search took: {:?}, found {} locations", 
              sibling_start.elapsed(), including_classes.len());
        info!("[PERF] find_method_without_receiver total time: {:?}", start_time.elapsed());
        return Some(including_classes);
    }
    info!("[PERF] Sibling modules search took: {:?}, found 0 locations", sibling_start.elapsed());
    
    info!("[PERF] find_method_without_receiver total time: {:?}", start_time.elapsed());
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
    let start_time = Instant::now();
    debug!("Starting get_included_modules for {:?}", class_fqn);

    // Check cache first
    let cache = INCLUDED_MODULES_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(cache_guard) = cache.lock() {
        if let Some(cached_modules) = cache_guard.get(class_fqn) {
            info!("[PERF] get_included_modules for {:?} found in cache in {:?}, {} modules",
                class_fqn, start_time.elapsed(), cached_modules.len());
            return cached_modules.clone();
        }
    }

    let mut included_modules = Vec::new();
    let mut seen_modules = HashSet::<FullyQualifiedName>::new();

    // Get the ancestor chain to check superclasses too
    let ancestor_start = Instant::now();
    let ancestor_chain = get_ancestor_chain(index, class_fqn, false);
    info!("[PERF] get_ancestor_chain took: {:?}, found {} ancestors", 
          ancestor_start.elapsed(), ancestor_chain.len());

    // Check each class/module in the ancestor chain for included modules
    let mixin_start = Instant::now();
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
    info!("[PERF] Processing mixins took: {:?}", mixin_start.elapsed());

    // Store in cache
    if let Ok(mut cache_guard) = cache.lock() {
        cache_guard.insert(class_fqn.clone(), included_modules.clone());
    }

    info!("[PERF] get_included_modules for {:?} completed in {:?}, found {} modules",
        class_fqn, start_time.elapsed(), included_modules.len());

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
    let start_time = Instant::now();
    info!("[PERF] Starting search_by_name for method: {:?}", method.get_name());
    
    let result = if let Some(entries) = index.methods_by_name.get(method) {
        let locations: Vec<Location> = entries.iter().map(|entry| entry.location.clone()).collect();

        if locations.is_empty() {
            None
        } else {
            Some(locations)
        }
    } else {
        None
    };
    
    info!("[PERF] search_by_name took: {:?}, found {} locations", 
          start_time.elapsed(), 
          result.as_ref().map_or(0, |v| v.len()));
    result
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

/// Search for methods in the ancestor chain without caching
fn search_in_ancestor_chain(
    receiver_fqn: &FullyQualifiedName,
    method: &RubyMethod,
    index: &RubyIndex,
    kind: MethodKind,
) -> Option<Vec<Location>> {
    let mut visited = HashSet::new();
    search_in_ancestor_chain_with_visited(receiver_fqn, method, index, kind, &mut visited)
}

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
        search_in_class_context(receiver_fqn, method, index, kind, visited)
    } else {
        search_in_module_context(receiver_fqn, method, index, kind, visited)
    };

    if found_locations.is_empty() {
        None
    } else {
        Some(found_locations)
    }
}

/// Search for methods specifically in class context
fn search_in_class_context(
    receiver_fqn: &FullyQualifiedName,
    method: &RubyMethod,
    index: &RubyIndex,
    kind: MethodKind,
    visited: &mut HashSet<FullyQualifiedName>,
) -> Vec<Location> {
    let start_time = Instant::now();
    debug!("Searching in class context for {:?}", receiver_fqn);
    let mut found_locations = Vec::new();
    let is_class_method = kind == MethodKind::Class;

    // Build complete set of all modules/classes to search (single-pass collection)
    let collection_start = Instant::now();
    let mut modules_to_search = HashSet::new();
    
    // 1. Add current class
    modules_to_search.insert(receiver_fqn.clone());
    
    // 2. Add ancestor chain
    let ancestor_chain = get_ancestor_chain(index, receiver_fqn, is_class_method);
    for ancestor_fqn in &ancestor_chain {
        modules_to_search.insert(ancestor_fqn.clone());
        
        // Add all modules included by this ancestor
        let included_modules = get_included_modules(index, ancestor_fqn);
        for module_fqn in included_modules {
            collect_all_searchable_modules(index, &module_fqn, &mut modules_to_search);
        }
    }
    info!("[PERF] Module collection took: {:?}, found {} total modules to search", 
          collection_start.elapsed(), modules_to_search.len());
    
    // 3. Search each module exactly once
    let search_start = Instant::now();
    for module_fqn in &modules_to_search {
        let method_fqn = FullyQualifiedName::method(module_fqn.namespace_parts(), method.clone());
        if let Some(entries) = index.definitions.get(&method_fqn) {
            found_locations.extend(entries.iter().map(|e| e.location.clone()));
        }
    }
    info!("[PERF] Module search took: {:?}", search_start.elapsed());

    let dedup_start = Instant::now();
    let result = deduplicate_locations(found_locations);
    info!("[PERF] Deduplication took: {:?}", dedup_start.elapsed());
    
    info!("[PERF] search_in_class_context total time: {:?}, found {} locations", 
          start_time.elapsed(), result.len());
    result
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

/// Search for methods specifically in module context
fn search_in_module_context(
    receiver_fqn: &FullyQualifiedName,
    method: &RubyMethod,
    index: &RubyIndex,
    kind: MethodKind,
    visited: &mut HashSet<FullyQualifiedName>,
) -> Vec<Location> {
    let start_time = Instant::now();
    debug!("Searching in module context for {:?}", receiver_fqn);
    let mut found_locations = Vec::new();

    // Build complete set of all modules/classes to search (single-pass collection)
    let collection_start = Instant::now();
    let mut modules_to_search = HashSet::new();
    
    // 1. Add current module
    modules_to_search.insert(receiver_fqn.clone());
    
    // 2. Add all classes that include this module and their dependencies
     let including_classes = index.get_including_classes(receiver_fqn);
    for class_fqn in including_classes {
        // Add the class itself and its ancestor chain
        collect_all_searchable_modules(index, &class_fqn, &mut modules_to_search);
        
        // Add all modules included by this class
        let included_modules = get_included_modules(index, &class_fqn);
        for module_fqn in included_modules {
            collect_all_searchable_modules(index, &module_fqn, &mut modules_to_search);
        }
    }
    info!("[PERF] Module collection took: {:?}, found {} total modules to search", 
          collection_start.elapsed(), modules_to_search.len());
    
    // 3. Search each module exactly once
    let search_start = Instant::now();
    for module_fqn in &modules_to_search {
        let method_fqn = FullyQualifiedName::method(module_fqn.namespace_parts(), method.clone());
        if let Some(entries) = index.definitions.get(&method_fqn) {
            found_locations.extend(entries.iter().map(|e| e.location.clone()));
        }
    }
    info!("[PERF] Module search took: {:?}", search_start.elapsed());

    let dedup_start = Instant::now();
    let result = deduplicate_locations(found_locations);
    info!("[PERF] Deduplication took: {:?}", dedup_start.elapsed());
    
    info!("[PERF] search_in_module_context total time: {:?}, found {} locations", 
          start_time.elapsed(), result.len());
    result
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
fn search_in_sibling_modules(
    class_fqn: &FullyQualifiedName,
    method: &RubyMethod,
    index: &RubyIndex,
    kind: MethodKind,
) -> Option<Vec<Location>> {
    let mut visited = HashSet::new();
    search_in_sibling_modules_with_visited(class_fqn, method, index, kind, &mut visited)
}

/// Search for methods in sibling modules with visited tracking
fn search_in_sibling_modules_with_visited(
    class_fqn: &FullyQualifiedName,
    method: &RubyMethod,
    index: &RubyIndex,
    kind: MethodKind,
    visited: &mut HashSet<FullyQualifiedName>,
) -> Option<Vec<Location>> {
    let start_time = Instant::now();
    info!("[PERF] Starting search_in_sibling_modules for: {:?}", class_fqn);
    
    let mut found_locations = Vec::new();

    // Get the modules that this class/module includes, extends, or prepends
    let included_modules = get_included_modules(index, class_fqn);

    // Search in each included module and its ancestor chain
    let search_start = Instant::now();
    for module_fqn in included_modules {
        if let Some(locations) =
            search_in_ancestor_chain_with_visited(&module_fqn, method, index, kind, visited)
        {
            found_locations.extend(locations);
        }
    }
    info!("[PERF] Sibling module searches took: {:?}", search_start.elapsed());

    let result = if found_locations.is_empty() {
        None
    } else {
        Some(found_locations)
    };
    
    info!("[PERF] search_in_sibling_modules total time: {:?}, found {} locations", 
          start_time.elapsed(), 
          result.as_ref().map_or(0, |v| v.len()));
    result
}
