use crate::indexer::entry::entry_kind::EntryKind;
use crate::server::RubyLanguageServer;
use log::info;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LocationInfo {
    pub uri: String,
    pub line: u32,
    pub character: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NamespaceTreeResponse {
    /// Root namespace nodes
    pub namespaces: Vec<NamespaceNode>,
}

pub async fn handle_namespace_tree(
    lang_server: &RubyLanguageServer,
    _params: NamespaceTreeParams,
) -> NamespaceTreeResponse {
    info!("[NAMESPACE_TREE] Request received");

    let index = lang_server.index.lock();
    info!(
        "[NAMESPACE_TREE] Index has {} definition entries",
        index.definitions.len()
    );
    let mut namespace_map: HashMap<String, NamespaceNode> = HashMap::new();

    // Collect all class and module entries from the index
    for (fqn, entries) in &index.definitions {
        for entry in entries {
            match &entry.kind {
                EntryKind::Class { .. } | EntryKind::Module { .. } => {
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

                    let node = NamespaceNode {
                        name,
                        fqn: fqn_string.clone(),
                        kind,
                        children: Vec::new(),
                        location,
                    };

                    namespace_map.insert(fqn_string, node);
                }
                _ => {}
            }
        }
    }

    info!(
        "[NAMESPACE_TREE] Found {} namespace entries",
        namespace_map.len()
    );

    // Log some sample namespace entries
    let sample_count = std::cmp::min(5, namespace_map.len());
    for (i, (fqn, node)) in namespace_map.iter().enumerate() {
        if i < sample_count {
            info!(
                "[NAMESPACE_TREE] Sample namespace: {} (kind: {}, children: {})",
                fqn,
                node.kind,
                node.children.len()
            );
        }
    }

    // Build the tree structure
    info!(
        "[NAMESPACE_TREE] Starting tree building with {} total namespaces",
        namespace_map.len()
    );
    let namespaces = build_namespace_tree(namespace_map);
    info!(
        "[NAMESPACE_TREE] Built tree with {} root namespaces",
        namespaces.len()
    );

    // Log sample root namespaces
    for (i, ns) in namespaces.iter().enumerate() {
        if i < 3 {
            info!(
                "[NAMESPACE_TREE] Root namespace {}: {} (kind: {}, children: {})",
                i,
                ns.name,
                ns.kind,
                ns.children.len()
            );
        }
    }

    NamespaceTreeResponse { namespaces }
}

fn build_namespace_tree(namespace_map: HashMap<String, NamespaceNode>) -> Vec<NamespaceNode> {
    let mut root_nodes = Vec::new();
    let mut all_nodes: Vec<NamespaceNode> = namespace_map.into_values().collect();

    // Sort by FQN to ensure consistent ordering
    all_nodes.sort_by(|a, b| a.fqn.cmp(&b.fqn));

    // Build parent-child relationships
    let mut nodes_by_fqn: HashMap<String, NamespaceNode> = HashMap::new();

    for node in all_nodes {
        nodes_by_fqn.insert(node.fqn.clone(), node);
    }

    // Build the tree structure by collecting root nodes first
    let mut processed = std::collections::HashSet::new();
    let root_fqns: Vec<String> = nodes_by_fqn
        .keys()
        .filter(|fqn| {
            // Consider nodes as root if they have no :: or are direct children of Object
            let parts: Vec<&str> = fqn.split("::").collect();
            parts.len() == 1 || (parts.len() == 2 && parts[0] == "Object")
        })
        .cloned()
        .collect();

    info!(
        "[NAMESPACE_TREE] Found {} root nodes out of {} total nodes",
        root_fqns.len(),
        nodes_by_fqn.len()
    );

    // Log sample root FQNs
    for (i, fqn) in root_fqns.iter().enumerate() {
        if i < 5 {
            info!("[NAMESPACE_TREE] Root FQN {}: {}", i, fqn);
        }
    }

    for fqn in root_fqns {
        if let Some(node) = nodes_by_fqn.get(&fqn) {
            let mut root_node = node.clone();
            root_node.children = find_children(&fqn, &mut nodes_by_fqn, &mut processed);
            root_nodes.push(root_node);
            processed.insert(fqn);
        }
    }

    root_nodes
}

fn find_children(
    parent_fqn: &str,
    nodes_by_fqn: &HashMap<String, NamespaceNode>,
    processed: &mut std::collections::HashSet<String>,
) -> Vec<NamespaceNode> {
    let mut children = Vec::new();
    let child_fqns: Vec<String> = nodes_by_fqn
        .keys()
        .filter(|fqn| !processed.contains(*fqn) && is_direct_child(parent_fqn, fqn))
        .cloned()
        .collect();

    for fqn in child_fqns {
        if let Some(node) = nodes_by_fqn.get(&fqn) {
            let mut child_node = node.clone();
            child_node.children = find_children(&fqn, nodes_by_fqn, processed);
            children.push(child_node);
            processed.insert(fqn);
        }
    }

    children.sort_by(|a, b| a.name.cmp(&b.name));
    children
}

fn is_direct_child(parent_fqn: &str, child_fqn: &str) -> bool {
    if !child_fqn.starts_with(parent_fqn) {
        return false;
    }

    let parent_parts: Vec<&str> = parent_fqn.split("::").collect();
    let child_parts: Vec<&str> = child_fqn.split("::").collect();

    // Direct child should have exactly one more part than parent
    child_parts.len() == parent_parts.len() + 1 && child_parts[..parent_parts.len()] == parent_parts
}
