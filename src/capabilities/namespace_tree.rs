use crate::analyzer_prism::utils::resolve_constant_fqn_from_parts;
use crate::indexer::entry::entry_kind::EntryKind;
use crate::indexer::entry::{Entry, MixinRef};
use crate::indexer::index::RubyIndex;
use crate::server::RubyLanguageServer;
use crate::types::fully_qualified_name::FullyQualifiedName;
use log::debug;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
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
    let mut fqn_to_entries: HashMap<&FullyQualifiedName, Vec<&Entry>> = HashMap::new();
    for (fqn, entries) in index.definitions() {
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

        let location = index
            .get_file_url(first_entry.location.file_id)
            .map(|uri| LocationInfo {
                uri: uri.to_string(),
                line: first_entry.location.range.start.line,
                character: first_entry.location.range.start.character,
            });

        let current_fqn = FullyQualifiedName::namespace(fqn.namespace_parts().clone());

        // Merge all mixins from all entries for this FQN
        let mut all_includes = Vec::new();
        let mut all_prepends = Vec::new();
        let mut all_extends = Vec::new();
        let mut superclass = None;

        for entry in &entries {
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

        let (resolved_superclass, resolved_includes, resolved_prepends, resolved_extends) = (
            superclass
                .as_ref()
                .map(|s| resolve_mixin_cached(s, &index, &current_fqn, &mut mixin_cache)),
            resolve_mixins_cached(&all_includes, &index, &current_fqn, &mut mixin_cache),
            resolve_mixins_cached(&all_prepends, &index, &current_fqn, &mut mixin_cache),
            resolve_mixins_cached(&all_extends, &index, &current_fqn, &mut mixin_cache),
        );

        let node = NamespaceNode {
            name,
            fqn: fqn_string.clone(),
            kind,
            children: Vec::new(),
            location,
            superclass: resolved_superclass,
            includes: resolved_includes,
            prepends: resolved_prepends,
            extends: resolved_extends,
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

// Cached mixin resolution to avoid repeated lookups
fn resolve_mixin_cached(
    mixin_ref: &MixinRef,
    index: &RubyIndex,
    current_fqn: &FullyQualifiedName,
    cache: &mut HashMap<String, String>,
) -> String {
    let mixin_str = format_mixin_ref(mixin_ref);
    let key = format!("{}-{}", mixin_str, current_fqn);
    if let Some(cached) = cache.get(&key) {
        return cached.clone();
    }

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

// Batch resolve mixins with caching
fn resolve_mixins_cached(
    mixins: &[MixinRef],
    index: &RubyIndex,
    current_fqn: &FullyQualifiedName,
    cache: &mut HashMap<String, String>,
) -> Vec<String> {
    mixins
        .iter()
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
