use crate::analyzer_prism::utils::resolve_constant_fqn_from_parts;
use crate::indexer::entry::entry_kind::EntryKind;
use crate::indexer::entry::{Entry, MixinRef, NamespaceKind};
use crate::indexer::graph::{Graph, NodeKind};
use crate::indexer::index::{FqnId, RubyIndex};
use crate::server::RubyLanguageServer;
use crate::types::fully_qualified_name::FullyQualifiedName;
use log::debug;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};

#[derive(Debug, Serialize, Deserialize)]
pub struct NamespaceTreeParams {
    /// Optional workspace URI to filter results
    pub workspace_uri: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NamespaceNode {
    /// The name of this namespace (class/module)
    pub name: String,
    /// The fully qualified name
    pub fqn: String,
    /// Type: "Class", "Module", or "Singleton"
    pub kind: String,
    /// Child namespaces
    pub children: Vec<NamespaceNode>,
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

/// Information about a class that includes a module (directly or transitively)
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct IncluderInfo {
    /// The fully qualified name of the class
    pub name: String,
    /// Definition locations (may have multiple if reopened)
    pub locations: Vec<LocationInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamespaceTreeResponse {
    /// Root namespace nodes
    pub namespaces: Vec<NamespaceNode>,
}

pub async fn handle_namespace_tree(
    lang_server: &RubyLanguageServer,
    _params: NamespaceTreeParams,
) -> NamespaceTreeResponse {
    debug!("[NAMESPACE_TREE] Request received");
    let start_time = std::time::Instant::now();

    // Check cache first
    let index = lang_server.index.lock();
    let index_hash = compute_index_hash(&index);
    drop(index); // Release lock early

    {
        let cache = lang_server.namespace_tree_cache.lock();
        if let Some((cached_hash, cached_response)) = cache.as_ref() {
            if *cached_hash == index_hash {
                debug!(
                    "[NAMESPACE_TREE] Cache hit, returning cached result in {:?}",
                    start_time.elapsed()
                );
                return cached_response.clone();
            }
        }
    }

    debug!("[NAMESPACE_TREE] Cache miss, computing namespace tree");

    // Reacquire lock for computation
    let index = lang_server.index.lock();
    debug!(
        "[NAMESPACE_TREE] Index has {} definition entries",
        index.definitions_len()
    );

    // Early filtering and deduplication: collect only project class/module entries
    // Group entries by FQN to avoid duplicate processing
    // Skip singleton namespaces (#<Class:Foo>) - they represent the same class/module
    let mut fqn_to_entries: HashMap<&FullyQualifiedName, Vec<&Entry>> = HashMap::new();
    for (fqn, entries) in index.definitions() {
        // Skip singleton namespaces - we only want instance namespaces in the tree
        if fqn.namespace_kind() == Some(NamespaceKind::Singleton) {
            continue;
        }

        let mut project_entries_for_fqn = Vec::new();
        for entry in entries {
            // Get URL from file_id for project file check
            let Some(uri) = index.get_file_url(entry.location.file_id) else {
                continue;
            };
            if !crate::utils::is_project_file(uri) {
                continue;
            }
            match &entry.kind {
                EntryKind::Class(_) | EntryKind::Module(_) => {
                    project_entries_for_fqn.push(entry);
                }
                _ => {}
            }
        }
        if !project_entries_for_fqn.is_empty() {
            fqn_to_entries.insert(fqn, project_entries_for_fqn);
        }
    }

    debug!(
        "[NAMESPACE_TREE] Found {} unique project namespaces (filtered from {} total definitions)",
        fqn_to_entries.len(),
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

        let resolved_superclass = superclass
            .as_ref()
            .map(|s| resolve_single_mixin(s, &index, &current_fqn, &mut mixin_cache));
        let resolved_includes =
            resolve_mixins_cached(&all_includes, &index, &current_fqn, &mut mixin_cache);
        let resolved_prepends =
            resolve_mixins_cached(&all_prepends, &index, &current_fqn, &mut mixin_cache);
        let resolved_extends =
            resolve_mixins_cached(&all_extends, &index, &current_fqn, &mut mixin_cache);

        // Create singleton class node if there are extends (extends become includes on singleton)
        let singleton_class = if !resolved_extends.is_empty() {
            let singleton_fqn = format!("#<Class:{}>", fqn_string);
            Some(Box::new(NamespaceNode {
                name: singleton_fqn.clone(),
                fqn: singleton_fqn,
                kind: "Singleton".to_string(),
                children: Vec::new(),
                locations: Vec::new(), // Singleton doesn't have its own location
                superclass: None,
                includes: resolved_extends, // extends become includes on singleton
                prepends: Vec::new(),
                singleton_class: None,
                included_by: Vec::new(),
            }))
        } else {
            None
        };

        // For modules, find all classes/modules that include this module
        let included_by = if kind == "Module" {
            if let Some(fqn_id) = index.get_fqn_id(fqn) {
                find_includers(fqn_id, index.get_graph(), &index)
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        let node = NamespaceNode {
            name,
            fqn: fqn_string.clone(),
            kind,
            children: Vec::new(),
            locations,
            superclass: resolved_superclass,
            includes: resolved_includes,
            prepends: resolved_prepends,
            singleton_class,
            included_by,
        };

        namespace_map.insert(fqn_string, node);
    }

    drop(index); // Release lock before tree building

    // Build the tree structure
    debug!(
        "[NAMESPACE_TREE] Starting tree building with {} total namespaces",
        namespace_map.len()
    );
    let namespaces = build_namespace_tree(namespace_map);
    debug!(
        "[NAMESPACE_TREE] Built tree with {} root namespaces",
        namespaces.len()
    );

    let response = NamespaceTreeResponse { namespaces };

    // Cache the result
    {
        let mut cache = lang_server.namespace_tree_cache.lock();
        *cache = Some((index_hash, response.clone()));
    }

    debug!("[NAMESPACE_TREE] Completed in {:?}", start_time.elapsed());
    response
}

// Compute a hash of the index for caching
fn compute_index_hash(index: &RubyIndex) -> u64 {
    let mut hasher = DefaultHasher::new();
    index.definitions_len().hash(&mut hasher);
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
    cache: &mut HashMap<String, String>,
) -> String {
    let mixin_str = format_mixin_ref(mixin_ref);
    let key = format!("{}-{}", mixin_str, current_fqn);

    if let Some(cached) = cache.get(&key) {
        cached.clone()
    } else {
        let result = if let Some(resolved_fqn) =
            resolve_constant_fqn_from_parts(index, &mixin_ref.parts, mixin_ref.absolute, current_fqn)
        {
            resolved_fqn.to_string()
        } else {
            format!("{} (not found)", mixin_str)
        };
        cache.insert(key, result.clone());
        result
    }
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

// Batch resolve mixins with caching, grouping by resolved name
fn resolve_mixins_cached(
    mixins: &[MixinRef],
    index: &RubyIndex,
    current_fqn: &FullyQualifiedName,
    cache: &mut HashMap<String, String>,
) -> Vec<MixinInfo> {
    // Group mixins by their resolved name, collecting all locations
    let mut grouped: HashMap<String, Vec<LocationInfo>> = HashMap::new();

    for mixin_ref in mixins {
        let name = resolve_mixin_name_cached(mixin_ref, index, current_fqn, cache);

        // Get call site location from the MixinRef
        if let Some(uri) = index.get_file_url(mixin_ref.location.file_id) {
            let location = LocationInfo {
                uri: uri.to_string(),
                line: mixin_ref.location.range.start.line,
                character: mixin_ref.location.range.start.character,
            };
            grouped.entry(name).or_default().push(location);
        } else {
            // Ensure the name is in the map even if we can't get the location
            grouped.entry(name).or_default();
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
fn resolve_single_mixin(
    mixin_ref: &MixinRef,
    index: &RubyIndex,
    current_fqn: &FullyQualifiedName,
    cache: &mut HashMap<String, String>,
) -> MixinInfo {
    let name = resolve_mixin_name_cached(mixin_ref, index, current_fqn, cache);

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

    MixinInfo { name, locations }
}

// Tree building using iterative approach
fn build_namespace_tree(namespace_map: HashMap<String, NamespaceNode>) -> Vec<NamespaceNode> {
    if namespace_map.is_empty() {
        return Vec::new();
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
    let mut roots = Vec::new();
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
                roots.push(node);
            }
        }
    }

    // Sort roots by name
    roots.sort_by(|a, b| a.name.cmp(&b.name));
    roots
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
        let mut children = Vec::new();

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
                children.push(child_node);
            }
        }

        // Sort children by name
        children.sort_by(|a, b| a.name.cmp(&b.name));
        parent_node.children = children;
    }
}

/// Find all classes that include this module by traversing included_by/prepended_by edges.
/// Uses BFS to traverse through intermediate modules until finding classes.
/// Returns only classes (not modules) with their definition locations.
fn find_includers(start_id: FqnId, graph: &Graph, index: &RubyIndex) -> Vec<IncluderInfo> {
    let mut classes = Vec::new();
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();

    // Start from the module's direct includers
    if let Some(node) = graph.get_node(start_id) {
        for &id in node.included_by.iter().chain(node.prepended_by.iter()) {
            if visited.insert(id) {
                queue.push_back(id);
            }
        }
    }

    // BFS through the graph
    while let Some(current_id) = queue.pop_front() {
        let Some(current_node) = graph.get_node(current_id) else {
            continue;
        };

        match current_node.kind {
            NodeKind::Class => {
                // Found a class - add it to results with its locations
                if let Some(fqn) = index.get_fqn(current_id) {
                    let locations = index
                        .get(fqn)
                        .map(|entries| {
                            entries
                                .iter()
                                .filter_map(|entry| {
                                    index.get_file_url(entry.location.file_id).map(|uri| {
                                        LocationInfo {
                                            uri: uri.to_string(),
                                            line: entry.location.range.start.line,
                                            character: entry.location.range.start.character,
                                        }
                                    })
                                })
                                .collect()
                        })
                        .unwrap_or_default();

                    classes.push(IncluderInfo {
                        name: fqn.to_string(),
                        locations,
                    });
                }
            }
            NodeKind::Module => {
                // It's a module - continue traversing through its includers
                for &id in current_node
                    .included_by
                    .iter()
                    .chain(current_node.prepended_by.iter())
                {
                    if visited.insert(id) {
                        queue.push_back(id);
                    }
                }
            }
        }
    }

    classes.sort_by(|a, b| a.name.cmp(&b.name));
    classes
}
