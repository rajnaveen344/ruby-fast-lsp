use crate::indexer::ancestor_chain::resolve_mixin_ref;
use crate::indexer::entry::entry_kind::EntryKind;
use crate::indexer::entry::MixinRef;
use crate::indexer::index::RubyIndex;
use crate::indexer::utils;
use crate::server::RubyLanguageServer;
use crate::types::fully_qualified_name::FullyQualifiedName;
use log::debug;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::collections::hash_map::DefaultHasher;
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
    /// Type: "class" or "module"
    pub kind: String,
    /// Child namespaces
    pub children: Vec<NamespaceNode>,
    /// Location information
    pub location: Option<LocationInfo>,
    /// Superclass (only for classes)
    pub superclass: Option<String>,
    /// Included modules
    pub includes: Vec<String>,
    /// Prepended modules
    pub prepends: Vec<String>,
    /// Extended modules
    pub extends: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LocationInfo {
    pub uri: String,
    pub line: u32,
    pub character: u32,
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
                debug!("[NAMESPACE_TREE] Cache hit, returning cached result in {:?}", start_time.elapsed());
                return cached_response.clone();
            }
        }
    }

    debug!("[NAMESPACE_TREE] Cache miss, computing namespace tree");
    
    // Reacquire lock for computation
    let index = lang_server.index.lock();
    debug!(
        "[NAMESPACE_TREE] Index has {} definition entries",
        index.definitions.len()
    );

    // Early filtering: collect only project class/module entries
    let mut project_entries = Vec::new();
    for (fqn, entries) in &index.definitions {
        for entry in entries {
            if !utils::is_project_file(&entry.location.uri) {
                continue;
            }
            match &entry.kind {
                EntryKind::Class { .. } | EntryKind::Module { .. } => {
                    project_entries.push((fqn, entry));
                }
                _ => {}
            }
        }
    }

    debug!(
        "[NAMESPACE_TREE] Found {} project namespace entries (filtered from {} total definitions)",
        project_entries.len(),
        index.definitions.len()
    );

    // Batch mixin resolution to avoid repeated lookups
    let mut mixin_cache = HashMap::new();
    let mut namespace_map = HashMap::new();

    for (fqn, entry) in project_entries {
        let kind = match &entry.kind {
            EntryKind::Class { .. } => "Class".to_string(),
            EntryKind::Module { .. } => "Module".to_string(),
            _ => continue,
        };

        let fqn_string = fqn.to_string();
        let name = fqn.name().to_string();

        let location = Some(LocationInfo {
            uri: entry.location.uri.to_string(),
            line: entry.location.range.start.line,
            character: entry.location.range.start.character,
        });

        let current_fqn = FullyQualifiedName::namespace(fqn.namespace_parts().clone());
        let (superclass, includes, prepends, extends) = match &entry.kind {
            EntryKind::Class {
                superclass,
                includes,
                prepends,
                extends,
            } => (
                superclass.as_ref().map(|s| resolve_mixin_cached(s, &index, &current_fqn, &mut mixin_cache)),
                resolve_mixins_cached(includes, &index, &current_fqn, &mut mixin_cache),
                resolve_mixins_cached(prepends, &index, &current_fqn, &mut mixin_cache),
                resolve_mixins_cached(extends, &index, &current_fqn, &mut mixin_cache),
            ),
            EntryKind::Module {
                includes,
                prepends,
                extends,
            } => (
                None,
                resolve_mixins_cached(includes, &index, &current_fqn, &mut mixin_cache),
                resolve_mixins_cached(prepends, &index, &current_fqn, &mut mixin_cache),
                resolve_mixins_cached(extends, &index, &current_fqn, &mut mixin_cache),
            ),
            _ => (None, Vec::new(), Vec::new(), Vec::new()),
        };

        let node = NamespaceNode {
            name,
            fqn: fqn_string.clone(),
            kind,
            children: Vec::new(),
            location,
            superclass,
            includes,
            prepends,
            extends,
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
    index.definitions.len().hash(&mut hasher);
    // Hash a sample of FQN strings to detect changes
    let mut fqn_strings: Vec<String> = index.definitions.keys().map(|fqn| fqn.to_string()).collect();
    fqn_strings.sort();
    for fqn_str in fqn_strings.iter().take(100) {
        fqn_str.hash(&mut hasher);
    }
    hasher.finish()
}

// Cached mixin resolution to avoid repeated lookups
fn resolve_mixin_cached(
    mixin_ref: &MixinRef,
    index: &RubyIndex,
    current_fqn: &FullyQualifiedName,
    cache: &mut HashMap<String, String>,
) -> String {
    let mixin_str = format_mixin_ref(mixin_ref);
    let key = format!("{}-{}", mixin_str, current_fqn.to_string());
    if let Some(cached) = cache.get(&key) {
        return cached.clone();
    }
    
    let result = if let Some(resolved_fqn) = resolve_mixin_ref(index, mixin_ref, current_fqn) {
        resolved_fqn.to_string()
    } else {
        format!("{} (not found)", mixin_str)
    };
    
    cache.insert(key, result.clone());
    result
}

// Helper function to format MixinRef as string
fn format_mixin_ref(mixin_ref: &MixinRef) -> String {
    let prefix = if mixin_ref.absolute { "::" } else { "" };
    let parts: Vec<String> = mixin_ref.parts.iter().map(|part| part.to_string()).collect();
    format!("{}{}", prefix, parts.join("::"))
}

// Batch resolve mixins with caching
fn resolve_mixins_cached(
    mixins: &[MixinRef],
    index: &RubyIndex,
    current_fqn: &FullyQualifiedName,
    cache: &mut HashMap<String, String>,
) -> Vec<String> {
    mixins.iter()
        .map(|m| resolve_mixin_cached(m, index, current_fqn, cache))
        .collect()
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
            children_map.entry(parent_fqn).or_insert_with(Vec::new).push(fqn.clone());
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
                build_children_iterative(&fqn, &mut node, &children_map, &mut node_lookup, &mut processed);
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
                    build_children_iterative(child_fqn, &mut child_node, children_map, node_map, processed);
                }
                children.push(child_node);
            }
        }
        
        // Sort children by name
        children.sort_by(|a, b| a.name.cmp(&b.name));
        parent_node.children = children;
    }
}
