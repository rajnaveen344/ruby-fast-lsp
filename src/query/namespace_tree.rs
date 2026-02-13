//! Namespace Tree Query — Builds a hierarchical tree of all namespaces in the index.
//!
//! Computes namespace nodes with mixin resolution, includer tracking,
//! and singleton class handling. Supports filtering external types.

use crate::analyzer_prism::utils::resolve_constant_fqn_from_parts;
use crate::indexer::entry::entry_kind::EntryKind;
use crate::indexer::entry::MixinRef;
use crate::indexer::index::RubyIndex;
use crate::types::fully_qualified_name::FullyQualifiedName;
use log::debug;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

use super::IndexQuery;

// ============================================================================
// Public types
// ============================================================================

#[derive(Debug, Serialize, Deserialize)]
pub struct NamespaceTreeParams {
    /// Optional workspace URI to filter results
    pub workspace_uri: Option<String>,
    /// Whether to show external types (core Ruby, stdlib, gems) in mixins.
    /// When false (default), only project-defined types are shown.
    #[serde(default)]
    pub show_external_types: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NamespaceNode {
    /// The name of this namespace (class/module)
    pub name: String,
    /// The fully qualified name
    pub fqn: String,
    /// Type: "Class", "Module", or "Singleton"
    pub kind: String,
    /// Location information (multiple if class/module is reopened in different files)
    pub locations: Vec<LocationInfo>,
    /// Superclass (only for classes)
    pub superclass: Option<MixinInfo>,
    /// Included modules
    pub includes: Vec<MixinInfo>,
    /// Prepended modules
    pub prepends: Vec<MixinInfo>,
    /// Singleton class node (for class methods, contains extends as includes)
    pub singleton_class: Option<Box<NamespaceNode>>,
    /// Classes/modules that ultimately include this module (for modules only)
    pub included_by: Vec<IncluderInfo>,
    /// Child modules (nested modules within this namespace)
    pub modules: Vec<NamespaceNode>,
    /// Child classes (nested classes within this namespace)
    pub classes: Vec<NamespaceNode>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LocationInfo {
    pub uri: String,
    pub line: u32,
    pub character: u32,
}

/// A mixin reference with its name and call site locations
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MixinInfo {
    /// The resolved name of the mixin (e.g., "ActiveSupport::Concern")
    pub name: String,
    /// Locations of the include/prepend/extend call sites (may have multiple if class is reopened)
    pub locations: Vec<LocationInfo>,
}

/// Information about an intermediate module in the include chain
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ViaModuleInfo {
    /// The fully qualified name of the intermediate module
    pub name: String,
    /// Location of the include/prepend call that includes the previous module in the chain
    /// (i.e., where this module includes its target)
    pub call_location: Option<LocationInfo>,
}

/// Information about a class that includes a module (directly or transitively)
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct IncluderInfo {
    /// The fully qualified name of the class
    pub name: String,
    /// Definition locations (may have multiple if reopened)
    pub locations: Vec<LocationInfo>,
    /// Intermediate modules in the include chain (empty if direct include)
    /// e.g., if Module A is included by Module B, and B is included by Class C,
    /// then via_modules = [ViaModuleInfo { name: "B", call_location: <where B includes A> }]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub via_modules: Vec<ViaModuleInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamespaceTreeResponse {
    /// Root modules (top-level modules)
    pub modules: Vec<NamespaceNode>,
    /// Root classes (top-level classes)
    pub classes: Vec<NamespaceNode>,
}

// ============================================================================
// IndexQuery entry points
// ============================================================================

impl IndexQuery {
    /// Compute a hash of the index state for cache invalidation.
    pub fn compute_namespace_tree_hash(&self, show_external_types: bool) -> u64 {
        let index = self.index.lock();
        compute_index_hash(&index, show_external_types)
    }

    /// Compute the full namespace tree from the index.
    ///
    /// Locks the index once, does all filtering, mixin resolution, and tree building,
    /// then returns the response.
    pub fn compute_namespace_tree(&self, show_external_types: bool) -> NamespaceTreeResponse {
        let index = self.index.lock();

        debug!(
            "[NAMESPACE_TREE] Index has {} definition entries",
            index.definitions_len()
        );

        // Early filtering and deduplication: collect class/module entries
        // Group entries by FQN to avoid duplicate processing
        // Skip singleton namespaces (#<Class:Foo>) - they represent the same class/module
        // When show_external_types is false, only include project files
        let mut fqn_to_entries: HashMap<&FullyQualifiedName, Vec<&crate::indexer::entry::Entry>> =
            HashMap::new();
        for (fqn, entries) in index.definitions() {
            // Skip singleton namespaces - we only want instance namespaces in the tree
            if fqn.namespace_kind() == Some(crate::indexer::entry::NamespaceKind::Singleton) {
                continue;
            }

            let mut filtered_entries_for_fqn = Vec::new();
            for entry in entries {
                // Filter by file source: skip external types unless show_external_types is true
                let is_project = index.is_project_file(entry.location.file_id);
                if !show_external_types && !is_project {
                    continue;
                }
                match &entry.kind {
                    EntryKind::Class(_) | EntryKind::Module(_) => {
                        filtered_entries_for_fqn.push(entry);
                    }
                    _ => {}
                }
            }
            if !filtered_entries_for_fqn.is_empty() {
                fqn_to_entries.insert(fqn, filtered_entries_for_fqn);
            }
        }

        debug!(
            "[NAMESPACE_TREE] Found {} namespaces (show_external_types={}, filtered from {} total definitions)",
            fqn_to_entries.len(),
            show_external_types,
            index.definitions_len()
        );

        // Batch mixin resolution to avoid repeated lookups
        let mut mixin_cache = HashMap::new();
        let mut namespace_map = HashMap::new();

        for (fqn, entries) in fqn_to_entries {
            let fqn_string = fqn.to_string();

            // Use the first entry for basic info (they should all have the same FQN and kind)
            let first_entry = entries[0];

            if fqn_string.contains("GoshPosh::Platform::API") {
                debug!(
                    "[NAMESPACE_TREE] Processing namespace: {} with {} entries",
                    fqn_string,
                    entries.len()
                );
            }

            let kind = match &first_entry.kind {
                EntryKind::Class(_) => "Class".to_string(),
                EntryKind::Module(_) => "Module".to_string(),
                _ => continue,
            };

            let name = fqn.name().to_string();

            let current_fqn = FullyQualifiedName::namespace(fqn.namespace_parts().clone());

            // Collect all locations and mixins from all entries for this FQN
            let mut locations = Vec::new();
            let mut all_includes = Vec::new();
            let mut all_prepends = Vec::new();
            let mut all_extends = Vec::new();
            let mut superclass = None;

            for entry in &entries {
                // Collect location from each entry (class/module reopened in multiple files)
                if let Some(uri) = index.get_file_url(entry.location.file_id) {
                    locations.push(LocationInfo {
                        uri: uri.to_string(),
                        line: entry.location.range.start.line,
                        character: entry.location.range.start.character,
                    });
                }

                match &entry.kind {
                    EntryKind::Class(data) => {
                        if superclass.is_none() {
                            superclass = data.superclass.clone();
                        }
                        all_includes.extend(data.includes.clone());
                        all_prepends.extend(data.prepends.clone());
                        all_extends.extend(data.extends.clone());
                    }
                    EntryKind::Module(data) => {
                        all_includes.extend(data.includes.clone());
                        all_prepends.extend(data.prepends.clone());
                        all_extends.extend(data.extends.clone());
                    }
                    _ => {}
                }
            }

            let resolved_superclass = superclass.as_ref().and_then(|s| {
                resolve_single_mixin(
                    s,
                    &index,
                    &current_fqn,
                    &mut mixin_cache,
                    show_external_types,
                )
            });
            let resolved_includes = resolve_mixins_cached(
                &all_includes,
                &index,
                &current_fqn,
                &mut mixin_cache,
                show_external_types,
            );
            let resolved_prepends = resolve_mixins_cached(
                &all_prepends,
                &index,
                &current_fqn,
                &mut mixin_cache,
                show_external_types,
            );
            let resolved_extends = resolve_mixins_cached(
                &all_extends,
                &index,
                &current_fqn,
                &mut mixin_cache,
                show_external_types,
            );

            // Create singleton class node if there are extends (extends become includes on singleton)
            let singleton_class = if !resolved_extends.is_empty() {
                let singleton_fqn = format!("#<Class:{}>", fqn_string);
                Some(Box::new(NamespaceNode {
                    name: singleton_fqn.clone(),
                    fqn: singleton_fqn,
                    kind: "Singleton".to_string(),
                    locations: Vec::new(), // Singleton doesn't have its own location
                    superclass: None,
                    includes: resolved_extends, // extends become includes on singleton
                    prepends: Vec::new(),
                    singleton_class: None,
                    included_by: Vec::new(),
                    modules: Vec::new(),
                    classes: Vec::new(),
                }))
            } else {
                None
            };

            // For modules, find all classes/modules that include this module
            let included_by = if kind == "Module" {
                find_includers(fqn, &index, show_external_types)
            } else {
                Vec::new()
            };

            let node = NamespaceNode {
                name,
                fqn: fqn_string.clone(),
                kind,
                locations,
                superclass: resolved_superclass,
                includes: resolved_includes,
                prepends: resolved_prepends,
                singleton_class,
                included_by,
                modules: Vec::new(),
                classes: Vec::new(),
            };

            namespace_map.insert(fqn_string, node);
        }

        // Build the tree structure (no longer needs the index lock)
        debug!(
            "[NAMESPACE_TREE] Starting tree building with {} total namespaces",
            namespace_map.len()
        );
        let tree_result = build_namespace_tree(namespace_map);
        debug!(
            "[NAMESPACE_TREE] Built tree with {} root modules and {} root classes",
            tree_result.modules.len(),
            tree_result.classes.len()
        );

        NamespaceTreeResponse {
            modules: tree_result.modules,
            classes: tree_result.classes,
        }
    }
}

// ============================================================================
// Internal types
// ============================================================================

/// Result of resolving a mixin reference
#[derive(Clone)]
struct ResolvedMixin {
    /// The resolved name (FQN or formatted name with "(not found)")
    name: String,
    /// Whether this mixin is from a project file (not core/stdlib/gem)
    is_project_type: bool,
}

/// Result of building the namespace tree, with modules and classes separated
struct NamespaceTreeResult {
    modules: Vec<NamespaceNode>,
    classes: Vec<NamespaceNode>,
}

// ============================================================================
// Private helpers
// ============================================================================

// Compute a hash of the index for caching (includes show_external_types setting)
fn compute_index_hash(index: &RubyIndex, show_external_types: bool) -> u64 {
    let mut hasher = DefaultHasher::new();
    index.definitions_len().hash(&mut hasher);
    show_external_types.hash(&mut hasher);
    // Hash a sample of FQN strings to detect changes
    let mut fqn_strings: Vec<String> = index
        .definitions()
        .map(|(fqn, _)| fqn.to_string())
        .collect();
    fqn_strings.sort();
    for fqn_str in fqn_strings.iter().take(100) {
        fqn_str.hash(&mut hasher);
    }
    hasher.finish()
}

// Cached mixin name resolution to avoid repeated lookups
fn resolve_mixin_name_cached(
    mixin_ref: &MixinRef,
    index: &RubyIndex,
    current_fqn: &FullyQualifiedName,
    cache: &mut HashMap<String, ResolvedMixin>,
) -> ResolvedMixin {
    let mixin_str = format_mixin_ref(mixin_ref);
    let key = format!("{}-{}", mixin_str, current_fqn);

    if let Some(cached) = cache.get(&key) {
        return cached.clone();
    }

    let result = if let Some(resolved_fqn) =
        resolve_constant_fqn_from_parts(index, &mixin_ref.parts, mixin_ref.absolute, current_fqn)
    {
        // Check if this resolved FQN is from a project file using the index's file source tracking
        let is_project_type = index.get(&resolved_fqn).is_some_and(|entries| {
            entries
                .iter()
                .any(|entry| index.is_project_file(entry.location.file_id))
        });
        ResolvedMixin {
            name: resolved_fqn.to_string(),
            is_project_type,
        }
    } else {
        ResolvedMixin {
            name: format!("{} (not found)", mixin_str),
            is_project_type: false,
        }
    };

    cache.insert(key, result.clone());
    result
}

// Batch resolve mixins with caching, grouping by resolved name
fn resolve_mixins_cached(
    mixins: &[MixinRef],
    index: &RubyIndex,
    current_fqn: &FullyQualifiedName,
    cache: &mut HashMap<String, ResolvedMixin>,
    show_external_types: bool,
) -> Vec<MixinInfo> {
    // Group mixins by their resolved name, collecting all locations
    let mut grouped: HashMap<String, Vec<LocationInfo>> = HashMap::new();

    for mixin_ref in mixins {
        let resolved = resolve_mixin_name_cached(mixin_ref, index, current_fqn, cache);

        // Skip external types if not showing them
        if !show_external_types && !resolved.is_project_type {
            continue;
        }

        // Get call site location from the MixinRef
        if let Some(uri) = index.get_file_url(mixin_ref.location.file_id) {
            let location = LocationInfo {
                uri: uri.to_string(),
                line: mixin_ref.location.range.start.line,
                character: mixin_ref.location.range.start.character,
            };
            grouped.entry(resolved.name).or_default().push(location);
        } else {
            // Ensure the name is in the map even if we can't get the location
            grouped.entry(resolved.name).or_default();
        }
    }

    // Convert to MixinInfo vec, maintaining order by first appearance
    let mut result: Vec<MixinInfo> = grouped
        .into_iter()
        .map(|(name, locations)| MixinInfo { name, locations })
        .collect();

    // Sort by name for consistent ordering
    result.sort_by(|a, b| a.name.cmp(&b.name));
    result
}

// Resolve a single mixin (for superclass)
// Returns None if the mixin is external and show_external_types is false
fn resolve_single_mixin(
    mixin_ref: &MixinRef,
    index: &RubyIndex,
    current_fqn: &FullyQualifiedName,
    cache: &mut HashMap<String, ResolvedMixin>,
    show_external_types: bool,
) -> Option<MixinInfo> {
    let resolved = resolve_mixin_name_cached(mixin_ref, index, current_fqn, cache);

    // Skip external types if not showing them
    if !show_external_types && !resolved.is_project_type {
        return None;
    }

    let locations = index
        .get_file_url(mixin_ref.location.file_id)
        .map(|uri| {
            vec![LocationInfo {
                uri: uri.to_string(),
                line: mixin_ref.location.range.start.line,
                character: mixin_ref.location.range.start.character,
            }]
        })
        .unwrap_or_default();

    Some(MixinInfo {
        name: resolved.name,
        locations,
    })
}

/// Find all classes that include this module by traversing included_by/prepended_by edges.
/// Uses the index's `including_classes` method which does BFS through intermediate modules.
/// Returns only classes (not modules) with their definition locations and via_modules path.
/// When `show_external_types` is false, only returns classes defined in project files.
fn find_includers(
    module_fqn: &FullyQualifiedName,
    index: &RubyIndex,
    show_external_types: bool,
) -> Vec<IncluderInfo> {
    let mut classes: Vec<IncluderInfo> = index
        .including_classes(module_fqn)
        .into_iter()
        .filter_map(|(class_fqn, via_module_fqns)| {
            let entries = index.get(&class_fqn)?;

            // Check if this class is from a project file using the index's file source tracking
            let is_project_class = entries
                .iter()
                .any(|entry| index.is_project_file(entry.location.file_id));

            // Skip external types if not showing them
            if !show_external_types && !is_project_class {
                return None;
            }

            let locations = entries
                .iter()
                .filter_map(|entry| {
                    index
                        .get_file_url(entry.location.file_id)
                        .map(|uri| LocationInfo {
                            uri: uri.to_string(),
                            line: entry.location.range.start.line,
                            character: entry.location.range.start.character,
                        })
                })
                .collect();

            // Build via_modules with call site locations
            // via_modules[0] includes module_fqn (the target)
            // via_modules[i] includes via_modules[i-1] for i > 0
            let via_modules: Vec<ViaModuleInfo> = via_module_fqns
                .iter()
                .enumerate()
                .map(|(i, via_fqn)| {
                    // Determine what this via module includes
                    let included_fqn = if i == 0 {
                        module_fqn // First via module includes the target
                    } else {
                        &via_module_fqns[i - 1] // Subsequent via modules include the previous one
                    };

                    // Find the call site location where via_fqn includes included_fqn
                    let call_location = find_mixin_call_location(index, via_fqn, included_fqn);

                    ViaModuleInfo {
                        name: via_fqn.to_string(),
                        call_location,
                    }
                })
                .collect();

            Some(IncluderInfo {
                name: class_fqn.to_string(),
                locations,
                via_modules,
            })
        })
        .collect();

    classes.sort_by(|a, b| a.name.cmp(&b.name));
    classes
}

/// Find the call site location where `includer_fqn` includes/prepends `included_fqn`.
/// Returns the location of the include/prepend statement, or None if not found.
fn find_mixin_call_location(
    index: &RubyIndex,
    includer_fqn: &FullyQualifiedName,
    included_fqn: &FullyQualifiedName,
) -> Option<LocationInfo> {
    let entries = index.get(includer_fqn)?;

    for entry in entries {
        let mixins = match &entry.kind {
            EntryKind::Module(data) => data.includes.iter().chain(data.prepends.iter()),
            EntryKind::Class(data) => data.includes.iter().chain(data.prepends.iter()),
            _ => continue,
        };

        // Check each mixin to see if it resolves to included_fqn
        let current_fqn = FullyQualifiedName::namespace(includer_fqn.namespace_parts().clone());
        for mixin_ref in mixins {
            if let Some(resolved_fqn) = resolve_constant_fqn_from_parts(
                index,
                &mixin_ref.parts,
                mixin_ref.absolute,
                &current_fqn,
            ) {
                if &resolved_fqn == included_fqn {
                    // Found the mixin call - return its location
                    return index.get_file_url(mixin_ref.location.file_id).map(|uri| {
                        LocationInfo {
                            uri: uri.to_string(),
                            line: mixin_ref.location.range.start.line,
                            character: mixin_ref.location.range.start.character,
                        }
                    });
                }
            }
        }
    }

    None
}

// Helper function to format MixinRef as string
fn format_mixin_ref(mixin_ref: &MixinRef) -> String {
    let prefix = if mixin_ref.absolute { "::" } else { "" };
    let parts: Vec<String> = mixin_ref
        .parts
        .iter()
        .map(|part| part.to_string())
        .collect();
    format!("{}{}", prefix, parts.join("::"))
}

// Tree building using iterative approach
fn build_namespace_tree(namespace_map: HashMap<String, NamespaceNode>) -> NamespaceTreeResult {
    if namespace_map.is_empty() {
        return NamespaceTreeResult {
            modules: Vec::new(),
            classes: Vec::new(),
        };
    }

    // Convert to vector and sort by FQN for consistent ordering
    let mut all_nodes: Vec<NamespaceNode> = namespace_map.into_values().collect();
    all_nodes.sort_by(|a, b| a.fqn.cmp(&b.fqn));

    // Build parent-child relationships using iterative approach
    let mut children_map: HashMap<String, Vec<String>> = HashMap::new();
    let mut node_lookup: HashMap<String, NamespaceNode> = HashMap::new();

    // First pass: identify all nodes and their potential children
    for node in all_nodes {
        let fqn = node.fqn.clone();

        // Find parent FQN by removing the last component
        if let Some(last_sep) = fqn.rfind("::") {
            let parent_fqn = fqn[..last_sep].to_string();
            children_map
                .entry(parent_fqn)
                .or_default()
                .push(fqn.clone());
        }

        node_lookup.insert(fqn, node);
    }

    // Second pass: identify root nodes and build tree
    let mut processed = HashSet::new();
    let mut root_modules = Vec::new();
    let mut root_classes = Vec::new();
    let all_fqns: Vec<String> = node_lookup.keys().cloned().collect();

    for fqn in all_fqns {
        if processed.contains(&fqn) {
            continue;
        }

        // Check if this is a root node (no parent exists in our map)
        let is_root = if let Some(last_sep) = fqn.rfind("::") {
            let parent_fqn = fqn[..last_sep].to_string();
            !node_lookup.contains_key(&parent_fqn)
        } else {
            true
        };

        if is_root {
            if let Some(mut node) = node_lookup.remove(&fqn) {
                // Build children recursively for this root
                build_children_iterative(
                    &fqn,
                    &mut node,
                    &children_map,
                    &mut node_lookup,
                    &mut processed,
                );
                // Separate root modules and classes
                if node.kind == "Module" {
                    root_modules.push(node);
                } else {
                    root_classes.push(node);
                }
            }
        }
    }

    // Sort roots by name
    root_modules.sort_by(|a, b| a.name.cmp(&b.name));
    root_classes.sort_by(|a, b| a.name.cmp(&b.name));

    NamespaceTreeResult {
        modules: root_modules,
        classes: root_classes,
    }
}

// Helper function to build children iteratively
fn build_children_iterative(
    parent_fqn: &str,
    parent_node: &mut NamespaceNode,
    children_map: &HashMap<String, Vec<String>>,
    node_map: &mut HashMap<String, NamespaceNode>,
    processed: &mut HashSet<String>,
) {
    processed.insert(parent_fqn.to_string());

    if let Some(child_fqns) = children_map.get(parent_fqn) {
        let mut modules = Vec::new();
        let mut classes = Vec::new();

        for child_fqn in child_fqns {
            if let Some(mut child_node) = node_map.remove(child_fqn) {
                if !processed.contains(child_fqn) {
                    build_children_iterative(
                        child_fqn,
                        &mut child_node,
                        children_map,
                        node_map,
                        processed,
                    );
                }
                // Separate modules and classes
                if child_node.kind == "Module" {
                    modules.push(child_node);
                } else {
                    classes.push(child_node);
                }
            }
        }

        // Sort modules and classes by name
        modules.sort_by(|a, b| a.name.cmp(&b.name));
        classes.sort_by(|a, b| a.name.cmp(&b.name));
        parent_node.modules = modules;
        parent_node.classes = classes;
    }
}
