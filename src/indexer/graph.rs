//! Inheritance Graph
//!
//! A graph structure for representing Ruby's class inheritance and module mixin relationships.
//! This provides efficient traversal for method resolution order (MRO) computation and
//! supports incremental updates when files change.
//!
//! ## Design
//!
//! - **Nodes**: Two per class (Instance + Singleton namespace), one per module
//! - **Forward edges**: superclass, includes, prepends (resolved FqnIds)
//! - **Reverse edges**: children, included_by, prepended_by
//! - **Edge provenance**: `edges_by_file` tracks which file added each edge for O(E) removal
//! - **Note**: "extend Foo" is modeled as Singleton node including Foo's Instance namespace

use std::collections::{HashMap, HashSet};

use super::index::{FileId, FqnId};

// ============================================================================
// Types
// ============================================================================

/// Whether the node represents a class or module
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NodeKind {
    #[default]
    Class,
    Module,
}

/// Type of edge in the inheritance graph
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeType {
    Superclass,
    Include,
    Prepend,
    // Note: "Extend" is modeled as Include on the Singleton namespace
    // extend Foo → Singleton node includes Foo's Instance namespace
}

/// Record of an edge for provenance tracking
/// Used to efficiently remove edges when a file changes
#[derive(Debug, Clone)]
pub struct EdgeRecord {
    pub source: FqnId,
    pub target: FqnId,
    pub edge_type: EdgeType,
}

// ============================================================================
// GraphNode
// ============================================================================

/// A node in the inheritance graph representing a class or module
///
/// With the "two nodes per class" model:
/// - Instance namespace has includes/prepends/superclass for instance methods
/// - Singleton namespace has includes/prepends/superclass for class methods
/// - "extend Foo" is modeled as: Singleton node includes Foo's Instance namespace
#[derive(Debug, Clone, Default)]
pub struct GraphNode {
    pub kind: NodeKind,

    // Forward edges (resolved FqnIds - no locations, those are in MixinRef)
    pub superclass: Option<FqnId>,
    pub includes: Vec<FqnId>,
    pub prepends: Vec<FqnId>,

    // Reverse edges
    pub children: Vec<FqnId>,
    pub included_by: Vec<FqnId>,
    pub prepended_by: Vec<FqnId>,
}

impl GraphNode {
    pub fn new(kind: NodeKind) -> Self {
        Self {
            kind,
            ..Default::default()
        }
    }
}

// ============================================================================
// InheritanceGraph
// ============================================================================

/// The inheritance graph for all classes and modules in the index
#[derive(Debug, Default)]
pub struct Graph {
    /// Nodes indexed by FqnId
    nodes: HashMap<FqnId, GraphNode>,

    /// Edge provenance: which file added each edge
    /// Used for O(E) removal when a file changes
    edges_by_file: HashMap<FileId, Vec<EdgeRecord>>,
}

impl Graph {
    pub fn new() -> Self {
        Self::default()
    }

    // ========================================================================
    // Node Access
    // ========================================================================

    /// Get a node by FqnId
    pub fn get_node(&self, fqn_id: FqnId) -> Option<&GraphNode> {
        self.nodes.get(&fqn_id)
    }

    /// Get a mutable node by FqnId
    pub fn get_node_mut(&mut self, fqn_id: FqnId) -> Option<&mut GraphNode> {
        self.nodes.get_mut(&fqn_id)
    }

    /// Ensure a node exists, creating it if necessary
    pub fn ensure_node(&mut self, fqn_id: FqnId, kind: NodeKind) {
        self.nodes
            .entry(fqn_id)
            .or_insert_with(|| GraphNode::new(kind));
    }

    /// Check if a node exists
    pub fn contains(&self, fqn_id: FqnId) -> bool {
        self.nodes.contains_key(&fqn_id)
    }

    // ========================================================================
    // Edge Building (all take file_id for provenance)
    // ========================================================================

    /// Set the superclass for a class
    pub fn set_superclass(&mut self, child: FqnId, parent: FqnId, file_id: FileId) {
        // Ensure both nodes exist
        self.nodes.entry(child).or_default();
        self.nodes.entry(parent).or_default();

        // Set forward edge
        if let Some(node) = self.nodes.get_mut(&child) {
            node.superclass = Some(parent);
        }

        // Set reverse edge
        if let Some(node) = self.nodes.get_mut(&parent) {
            if !node.children.contains(&child) {
                node.children.push(child);
            }
        }

        // Record provenance
        self.edges_by_file
            .entry(file_id)
            .or_default()
            .push(EdgeRecord {
                source: child,
                target: parent,
                edge_type: EdgeType::Superclass,
            });
    }

    /// Add an include relationship
    pub fn add_include(&mut self, includer: FqnId, module: FqnId, file_id: FileId) {
        // Ensure both nodes exist
        self.nodes.entry(includer).or_default();
        self.nodes.entry(module).or_default();

        // Set forward edge
        if let Some(node) = self.nodes.get_mut(&includer) {
            if !node.includes.contains(&module) {
                node.includes.push(module);
            }
        }

        // Set reverse edge
        if let Some(node) = self.nodes.get_mut(&module) {
            if !node.included_by.contains(&includer) {
                node.included_by.push(includer);
            }
        }

        // Record provenance
        self.edges_by_file
            .entry(file_id)
            .or_default()
            .push(EdgeRecord {
                source: includer,
                target: module,
                edge_type: EdgeType::Include,
            });
    }

    /// Add a prepend relationship
    pub fn add_prepend(&mut self, prepender: FqnId, module: FqnId, file_id: FileId) {
        // Ensure both nodes exist
        self.nodes.entry(prepender).or_default();
        self.nodes.entry(module).or_default();

        // Set forward edge
        if let Some(node) = self.nodes.get_mut(&prepender) {
            if !node.prepends.contains(&module) {
                node.prepends.push(module);
            }
        }

        // Set reverse edge
        if let Some(node) = self.nodes.get_mut(&module) {
            if !node.prepended_by.contains(&prepender) {
                node.prepended_by.push(prepender);
            }
        }

        // Record provenance
        self.edges_by_file
            .entry(file_id)
            .or_default()
            .push(EdgeRecord {
                source: prepender,
                target: module,
                edge_type: EdgeType::Prepend,
            });
    }

    // Note: "extend Foo" is now modeled as Singleton node including Foo's Instance namespace.
    // Callers should use add_include(singleton_fqn_id, module_instance_fqn_id, file_id).

    // ========================================================================
    // Incremental Updates
    // ========================================================================

    /// Remove all edges that came from a specific file
    /// This is O(E) where E = edges from that file, not O(N) for all nodes
    pub fn remove_edges_from_file(&mut self, file_id: FileId) {
        let Some(edges) = self.edges_by_file.remove(&file_id) else {
            return;
        };

        for edge in edges {
            match edge.edge_type {
                EdgeType::Include => {
                    if let Some(node) = self.nodes.get_mut(&edge.source) {
                        node.includes.retain(|id| *id != edge.target);
                    }
                    if let Some(node) = self.nodes.get_mut(&edge.target) {
                        node.included_by.retain(|id| *id != edge.source);
                    }
                }
                EdgeType::Prepend => {
                    if let Some(node) = self.nodes.get_mut(&edge.source) {
                        node.prepends.retain(|id| *id != edge.target);
                    }
                    if let Some(node) = self.nodes.get_mut(&edge.target) {
                        node.prepended_by.retain(|id| *id != edge.source);
                    }
                }
                EdgeType::Superclass => {
                    if let Some(node) = self.nodes.get_mut(&edge.source) {
                        if node.superclass == Some(edge.target) {
                            node.superclass = None;
                        }
                    }
                    if let Some(node) = self.nodes.get_mut(&edge.target) {
                        node.children.retain(|id| *id != edge.source);
                    }
                }
            }
        }
    }

    /// Remove a node entirely (call only when FQN has no more definitions)
    pub fn remove_node(&mut self, fqn_id: FqnId) {
        if let Some(node) = self.nodes.remove(&fqn_id) {
            // Clean up reverse edges on nodes we pointed to
            if let Some(parent_id) = node.superclass {
                if let Some(parent) = self.nodes.get_mut(&parent_id) {
                    parent.children.retain(|id| *id != fqn_id);
                }
            }

            for module_id in &node.includes {
                if let Some(module_node) = self.nodes.get_mut(module_id) {
                    module_node.included_by.retain(|id| *id != fqn_id);
                }
            }

            for module_id in &node.prepends {
                if let Some(module_node) = self.nodes.get_mut(module_id) {
                    module_node.prepended_by.retain(|id| *id != fqn_id);
                }
            }

            // Clean up forward edges on nodes that pointed to us
            for child_id in &node.children {
                if let Some(child) = self.nodes.get_mut(child_id) {
                    if child.superclass == Some(fqn_id) {
                        child.superclass = None;
                    }
                }
            }

            for includer_id in &node.included_by {
                if let Some(includer) = self.nodes.get_mut(includer_id) {
                    includer.includes.retain(|id| *id != fqn_id);
                }
            }

            for prepender_id in &node.prepended_by {
                if let Some(prepender) = self.nodes.get_mut(prepender_id) {
                    prepender.prepends.retain(|id| *id != fqn_id);
                }
            }
        }
    }

    // ========================================================================
    // Traversal - Method Resolution Order
    // ========================================================================

    /// Build the Method Resolution Order (MRO) for a namespace
    ///
    /// With the "two nodes per class" model, this works identically for both:
    /// - Instance methods: FQN is Instance namespace → traverses instance mixins/superclass
    /// - Class methods: FQN is Singleton namespace → traverses singleton mixins/superclass
    ///
    /// Ruby's MRO traversal order:
    /// 1. Prepended modules (in reverse order of prepending)
    /// 2. The class/module itself
    /// 3. Included modules (in reverse order of inclusion)
    /// 4. Superclass chain (recursively)
    ///
    /// Note: "extend Foo" is modeled as Singleton node includes Foo's Instance namespace,
    /// so it appears in the Singleton node's includes (step 3).
    pub fn method_lookup_chain(&self, fqn_id: FqnId) -> Vec<FqnId> {
        let mut chain = Vec::new();
        let mut visited = HashSet::new();
        self.build_instance_mro(fqn_id, &mut chain, &mut visited);
        chain
    }

    /// Get all classes/modules that include or prepend this module
    pub fn mixers(&self, fqn_id: FqnId) -> Vec<FqnId> {
        let Some(node) = self.nodes.get(&fqn_id) else {
            return Vec::new();
        };

        let mut result = Vec::new();
        result.extend(node.included_by.iter().copied());
        result.extend(node.prepended_by.iter().copied());
        result
    }

    /// Get all transitive subclasses of a class
    pub fn descendants(&self, fqn_id: FqnId) -> Vec<FqnId> {
        let mut result = Vec::new();
        let mut visited = HashSet::new();
        self.collect_descendants(fqn_id, &mut result, &mut visited);
        result
    }

    // ========================================================================
    // Internal Helpers
    // ========================================================================

    /// Recursively build instance method MRO
    fn build_instance_mro(
        &self,
        fqn_id: FqnId,
        chain: &mut Vec<FqnId>,
        visited: &mut HashSet<FqnId>,
    ) {
        if !visited.insert(fqn_id) {
            return; // Already visited, prevent cycles
        }

        let Some(node) = self.nodes.get(&fqn_id) else {
            // Node doesn't exist in graph, still add to chain
            chain.push(fqn_id);
            return;
        };

        // 1. Prepended modules (in reverse order - last prepend is searched first)
        for module_id in node.prepends.iter().rev() {
            self.build_instance_mro(*module_id, chain, visited);
        }

        // 2. The class/module itself
        chain.push(fqn_id);

        // 3. Included modules (in reverse order - last include is searched first)
        for module_id in node.includes.iter().rev() {
            self.build_instance_mro(*module_id, chain, visited);
        }

        // 4. Superclass (if any)
        if let Some(superclass_id) = node.superclass {
            self.build_instance_mro(superclass_id, chain, visited);
        }
    }

    /// Recursively collect all descendants
    fn collect_descendants(
        &self,
        fqn_id: FqnId,
        result: &mut Vec<FqnId>,
        visited: &mut HashSet<FqnId>,
    ) {
        if !visited.insert(fqn_id) {
            return;
        }

        let Some(node) = self.nodes.get(&fqn_id) else {
            return;
        };

        for child_id in &node.children {
            result.push(*child_id);
            self.collect_descendants(*child_id, result, visited);
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use slotmap::SlotMap;

    fn create_fqn_ids(count: usize) -> (SlotMap<FqnId, ()>, Vec<FqnId>) {
        let mut map = SlotMap::with_key();
        let ids: Vec<FqnId> = (0..count).map(|_| map.insert(())).collect();
        (map, ids)
    }

    #[test]
    fn test_simple_inheritance() {
        let (_map, ids) = create_fqn_ids(3);
        let file_id = FileId::default();

        let mut graph = Graph::new();

        // BasicObject <- Object <- User
        graph.set_superclass(ids[1], ids[0], file_id); // Object < BasicObject
        graph.set_superclass(ids[2], ids[1], file_id); // User < Object

        let chain = graph.method_lookup_chain(ids[2]);
        assert_eq!(chain, vec![ids[2], ids[1], ids[0]]);
    }

    #[test]
    fn test_include_ordering() {
        let (_map, ids) = create_fqn_ids(4);
        let file_id = FileId::default();

        let mut graph = Graph::new();

        // User includes M1, then M2
        // MRO should be: User, M2, M1 (reverse order)
        graph.add_include(ids[0], ids[1], file_id); // include M1
        graph.add_include(ids[0], ids[2], file_id); // include M2

        let chain = graph.method_lookup_chain(ids[0]);
        assert_eq!(chain, vec![ids[0], ids[2], ids[1]]);
    }

    #[test]
    fn test_prepend_before_class() {
        let (_map, ids) = create_fqn_ids(3);
        let file_id = FileId::default();

        let mut graph = Graph::new();

        // User prepends Logging
        // MRO should be: Logging, User
        graph.add_prepend(ids[0], ids[1], file_id);

        let chain = graph.method_lookup_chain(ids[0]);
        assert_eq!(chain, vec![ids[1], ids[0]]);
    }

    #[test]
    fn test_complex_hierarchy() {
        let (_map, ids) = create_fqn_ids(6);
        let file_id = FileId::default();

        let mut graph = Graph::new();

        // C1 includes M2
        graph.add_include(ids[1], ids[4], file_id);

        // C2 < C1, includes M3 (which includes M1)
        graph.set_superclass(ids[2], ids[1], file_id);
        graph.add_include(ids[2], ids[5], file_id);
        graph.add_include(ids[5], ids[3], file_id); // M3 includes M1

        // MRO for C2: C2 -> M3 -> M1 -> C1 -> M2
        let chain = graph.method_lookup_chain(ids[2]);
        assert_eq!(chain, vec![ids[2], ids[5], ids[3], ids[1], ids[4]]);
    }

    #[test]
    fn test_remove_edges_from_file() {
        let (_map, ids) = create_fqn_ids(3);
        let file_id_1 = FileId::default();

        let mut graph = Graph::new();

        // Add edges from file 1
        graph.add_include(ids[0], ids[1], file_id_1);
        graph.add_include(ids[0], ids[2], file_id_1);

        // Verify edges exist
        assert_eq!(graph.method_lookup_chain(ids[0]).len(), 3);

        // Remove edges from file 1
        graph.remove_edges_from_file(file_id_1);

        // Only the node itself should remain in chain
        assert_eq!(graph.method_lookup_chain(ids[0]), vec![ids[0]]);
    }

    #[test]
    fn test_mixers() {
        let (_map, ids) = create_fqn_ids(4);
        let file_id = FileId::default();

        let mut graph = Graph::new();

        // M is included by C1, prepended by C2
        // Note: "extend" is now modeled as include on singleton node, not a separate edge type
        graph.add_include(ids[1], ids[0], file_id);
        graph.add_prepend(ids[2], ids[0], file_id);

        let mixers = graph.mixers(ids[0]);
        assert_eq!(mixers.len(), 2);
        assert!(mixers.contains(&ids[1]));
        assert!(mixers.contains(&ids[2]));
    }
}
