use crate::invariant::ExpectInvariant;
use std::collections::HashMap;

use crate::core::names::fqn_id::ConstLookupId;
use crate::core::names::fqn_id::FqnId;
use crate::core::storage::memory_estimate::{map_table_bytes, vec_payload_bytes};
use crate::core::{FullyQualifiedName, SourceFileId, TextRange};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GraphNodeKind {
    Class,
    Module,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GraphEdgeKind {
    Superclass,
    Include,
    Prepend,
    Extend,
    /// A reusable execution template evaluated independently against one or
    /// more runtime owners. This is not Ruby ancestry and must never enter the
    /// ordinary MRO.
    ExecutionContextApplication,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GraphEdgeProvenance {
    Explicit,
    ImplicitObject,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoredSuperclassResolution {
    Missing,
    Unique(StoredGraphEdgeFact),
    Ambiguous,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphNodeFact {
    pub fqn: FullyQualifiedName,
    pub kind: GraphNodeKind,
    pub range: TextRange,
}

impl GraphNodeFact {
    pub fn new(fqn: FullyQualifiedName, kind: GraphNodeKind, range: TextRange) -> Self {
        Self { fqn, kind, range }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphEdgeFact {
    pub source: FullyQualifiedName,
    pub target: FullyQualifiedName,
    pub kind: GraphEdgeKind,
    pub provenance: GraphEdgeProvenance,
    pub range: TextRange,
}

impl GraphEdgeFact {
    pub fn new(
        source: FullyQualifiedName,
        target: FullyQualifiedName,
        kind: GraphEdgeKind,
        range: TextRange,
    ) -> Self {
        Self {
            source,
            target,
            kind,
            provenance: GraphEdgeProvenance::Explicit,
            range,
        }
    }

    pub fn with_provenance(mut self, provenance: GraphEdgeProvenance) -> Self {
        self.provenance = provenance;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedGraphEdgeFact {
    pub source: FullyQualifiedName,
    pub target_parts: Vec<crate::core::RubyConstant>,
    pub absolute: bool,
    pub context: FullyQualifiedName,
    pub kind: GraphEdgeKind,
    pub provenance: GraphEdgeProvenance,
    pub range: TextRange,
}

impl UnresolvedGraphEdgeFact {
    pub fn new(
        source: FullyQualifiedName,
        target_parts: Vec<crate::core::RubyConstant>,
        absolute: bool,
        context: FullyQualifiedName,
        kind: GraphEdgeKind,
        range: TextRange,
    ) -> Self {
        Self {
            source,
            target_parts,
            absolute,
            context,
            kind,
            provenance: GraphEdgeProvenance::Explicit,
            range,
        }
    }

    pub fn with_provenance(mut self, provenance: GraphEdgeProvenance) -> Self {
        self.provenance = provenance;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoredGraphNodeFact {
    pub fqn: FqnId,
    pub kind: GraphNodeKind,
    pub range: TextRange,
}

impl StoredGraphNodeFact {
    pub fn new(fqn: FqnId, kind: GraphNodeKind, range: TextRange) -> Self {
        Self { fqn, kind, range }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoredGraphEdgeFact {
    pub source: FqnId,
    pub target: FqnId,
    pub kind: GraphEdgeKind,
    pub provenance: GraphEdgeProvenance,
    pub range: TextRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoredUnresolvedGraphEdgeFact {
    pub source: FqnId,
    pub target: ConstLookupId,
    pub kind: GraphEdgeKind,
    pub provenance: GraphEdgeProvenance,
    pub range: TextRange,
}

impl StoredUnresolvedGraphEdgeFact {
    pub fn new(
        source: FqnId,
        target: ConstLookupId,
        kind: GraphEdgeKind,
        range: TextRange,
    ) -> Self {
        Self {
            source,
            target,
            kind,
            provenance: GraphEdgeProvenance::Explicit,
            range,
        }
    }

    pub fn with_provenance(mut self, provenance: GraphEdgeProvenance) -> Self {
        self.provenance = provenance;
        self
    }
}

impl StoredGraphEdgeFact {
    pub fn new(source: FqnId, target: FqnId, kind: GraphEdgeKind, range: TextRange) -> Self {
        Self {
            source,
            target,
            kind,
            provenance: GraphEdgeProvenance::Explicit,
            range,
        }
    }

    pub fn with_provenance(mut self, provenance: GraphEdgeProvenance) -> Self {
        self.provenance = provenance;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GraphEdgeId(u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GraphNodeDefinition {
    kind: GraphNodeKind,
    range: TextRange,
}

/// One edge endpoint on a node: the edge, its kind, and whether the node is
/// the edge's source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GraphLink {
    edge: GraphEdgeId,
    kind: GraphEdgeKind,
    outgoing: bool,
}

#[derive(Debug, Clone, Default)]
pub struct GraphNode {
    definitions: Vec<GraphNodeDefinition>,
    /// Incoming links first, then outgoing links, so hub nodes with many
    /// incoming edges answer outgoing queries without scanning them.
    links: Vec<GraphLink>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GraphEdge {
    pub source: FqnId,
    pub target: FqnId,
    pub kind: GraphEdgeKind,
    pub provenance: GraphEdgeProvenance,
    pub range: TextRange,
}

impl From<StoredGraphEdgeFact> for GraphEdge {
    fn from(fact: StoredGraphEdgeFact) -> Self {
        Self {
            source: fact.source,
            target: fact.target,
            kind: fact.kind,
            provenance: fact.provenance,
            range: fact.range,
        }
    }
}

impl From<GraphEdge> for StoredGraphEdgeFact {
    fn from(edge: GraphEdge) -> Self {
        StoredGraphEdgeFact::new(edge.source, edge.target, edge.kind, edge.range)
            .with_provenance(edge.provenance)
    }
}

#[derive(Debug, Clone, Default)]
pub struct SemanticGraph {
    nodes: HashMap<FqnId, GraphNode>,
    edges: Vec<Option<GraphEdge>>,
    free_edges: Vec<GraphEdgeId>,
    /// Nodes with at least one definition in each file.
    nodes_by_file: HashMap<SourceFileId, Vec<FqnId>>,
    edges_by_file: HashMap<SourceFileId, Vec<GraphEdgeId>>,
    unresolved_by_file: HashMap<SourceFileId, Vec<StoredUnresolvedGraphEdgeFact>>,
    unresolved_explicit_superclasses_by_source: HashMap<FqnId, usize>,
    unresolved_explicit_sources_by_source: HashMap<FqnId, usize>,
}

impl SemanticGraph {
    pub fn add_node(&mut self, fact: StoredGraphNodeFact) {
        let file_id = fact.range.file_id;
        let node = self.nodes.entry(fact.fqn).or_default();
        let first_in_file = !node
            .definitions
            .iter()
            .any(|definition| definition.range.file_id == file_id);
        node.definitions.push(GraphNodeDefinition {
            kind: fact.kind,
            range: fact.range,
        });
        sort_node_definitions(&mut node.definitions);
        if first_in_file {
            self.nodes_by_file
                .entry(file_id)
                .or_default()
                .push(fact.fqn);
        }
    }

    pub fn add_edge(&mut self, fact: StoredGraphEdgeFact) {
        self.insert_edge(fact.into());
    }

    /// Remove one resolved edge equal to `fact`. Returns false when no such
    /// edge is live, for example because its owning file was replaced.
    pub fn remove_edge_fact(&mut self, fact: &StoredGraphEdgeFact) -> bool {
        let file_id = fact.range.file_id;
        let Some(ids) = self.edges_by_file.get(&file_id) else {
            return false;
        };
        let Some(position) = ids.iter().position(|id| {
            self.edge(*id)
                .is_some_and(|edge| StoredGraphEdgeFact::from(edge) == *fact)
        }) else {
            return false;
        };
        let ids = self.edges_by_file.get_mut(&file_id).expect_invariant(
            "graph edge file index vanished during edge removal",
            "the index was read immediately before this mutable lookup",
            "keep edge removal free of intervening file index mutation",
        );
        let id = ids.swap_remove(position);
        if ids.is_empty() {
            self.edges_by_file.remove(&file_id);
        }
        self.remove_edge(id);
        true
    }

    pub fn nodes_for(&self, fqn: FqnId) -> Vec<StoredGraphNodeFact> {
        self.nodes
            .get(&fqn)
            .map(|node| {
                node.definitions
                    .iter()
                    .map(|definition| {
                        StoredGraphNodeFact::new(fqn, definition.kind, definition.range)
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn has_node_definition(&self, fqn: FqnId) -> bool {
        self.nodes
            .get(&fqn)
            .is_some_and(|node| !node.definitions.is_empty())
    }

    pub fn first_node_kind(&self, fqn: FqnId) -> Option<GraphNodeKind> {
        self.nodes
            .get(&fqn)
            .and_then(|node| node.definitions.first().map(|definition| definition.kind))
    }

    pub fn latest_node_kind(&self, fqn: FqnId) -> Option<GraphNodeKind> {
        self.nodes
            .get(&fqn)
            .and_then(|node| node.definitions.last().map(|definition| definition.kind))
    }

    pub fn has_node_kind(&self, fqn: FqnId, kind: GraphNodeKind) -> bool {
        self.nodes.get(&fqn).is_some_and(|node| {
            node.definitions
                .iter()
                .any(|definition| definition.kind == kind)
        })
    }

    pub fn first_node_definition(&self, fqn: FqnId) -> Option<(GraphNodeKind, TextRange)> {
        self.nodes.get(&fqn).and_then(|node| {
            node.definitions
                .first()
                .map(|definition| (definition.kind, definition.range))
        })
    }

    pub fn edges_from(&self, source: FqnId) -> Vec<StoredGraphEdgeFact> {
        self.linked_edges(source, true, |_| true)
    }

    /// Outgoing edges of one kind, sorted by the same source-range key as
    /// [`Self::edges_from`]. Callers that only need include/prepend/extend
    /// must not clone every other outgoing kind.
    pub fn edges_from_kind(&self, source: FqnId, kind: GraphEdgeKind) -> Vec<StoredGraphEdgeFact> {
        self.linked_edges(source, true, |link_kind| link_kind == kind)
    }

    /// Superclass, include, prepend, and extend edges only. Execution-context
    /// applications are not Ruby ancestry and must not enter this list.
    pub fn ancestry_edges_from(&self, source: FqnId) -> Vec<StoredGraphEdgeFact> {
        self.linked_edges(source, true, |kind| {
            kind != GraphEdgeKind::ExecutionContextApplication
        })
    }

    pub fn superclass_resolution(&self, source: FqnId) -> StoredSuperclassResolution {
        let Some(node) = self.nodes.get(&source) else {
            return StoredSuperclassResolution::Missing;
        };
        let superclasses = || {
            node.outgoing()
                .iter()
                .filter(|link| link.kind == GraphEdgeKind::Superclass)
                .filter_map(|link| self.edge(link.edge))
        };
        let has_explicit =
            superclasses().any(|edge| edge.provenance == GraphEdgeProvenance::Explicit);
        let mut chosen: Option<StoredGraphEdgeFact> = None;
        for edge in superclasses() {
            if has_explicit && edge.provenance != GraphEdgeProvenance::Explicit {
                continue;
            }
            let fact = StoredGraphEdgeFact::from(edge);
            let Some(previous) = chosen else {
                chosen = Some(fact);
                continue;
            };
            if previous.target != fact.target {
                return StoredSuperclassResolution::Ambiguous;
            }
            if graph_edge_order_key(fact) < graph_edge_order_key(previous) {
                chosen = Some(fact);
            }
        }
        chosen.map_or(
            StoredSuperclassResolution::Missing,
            StoredSuperclassResolution::Unique,
        )
    }

    pub fn has_unresolved_explicit_superclass(&self, source: FqnId) -> bool {
        self.unresolved_explicit_superclasses_by_source
            .contains_key(&source)
    }

    pub fn has_explicit_unresolved_edge_from(&self, source: FqnId) -> bool {
        self.unresolved_explicit_sources_by_source
            .contains_key(&source)
    }

    pub fn edges_to(&self, target: FqnId) -> Vec<StoredGraphEdgeFact> {
        self.linked_edges(target, false, |_| true)
    }

    pub fn nodes_in_file(&self, file_id: SourceFileId) -> Vec<StoredGraphNodeFact> {
        let mut facts = Vec::new();
        for fqn in self.nodes_by_file.get(&file_id).into_iter().flatten() {
            for definition in &self.indexed_node(*fqn).definitions {
                if definition.range.file_id == file_id {
                    facts.push(StoredGraphNodeFact::new(
                        *fqn,
                        definition.kind,
                        definition.range,
                    ));
                }
            }
        }
        sort_graph_nodes(&mut facts);
        facts
    }

    pub fn edges_in_file(&self, file_id: SourceFileId) -> Vec<StoredGraphEdgeFact> {
        self.edges_by_file
            .get(&file_id)
            .map(|ids| self.edges_by_ids(ids.iter().copied()))
            .unwrap_or_default()
    }

    pub fn all_nodes(&self) -> Vec<StoredGraphNodeFact> {
        let mut facts = Vec::new();
        for (fqn, node) in &self.nodes {
            for definition in &node.definitions {
                facts.push(StoredGraphNodeFact::new(
                    *fqn,
                    definition.kind,
                    definition.range,
                ));
            }
        }
        sort_graph_nodes(&mut facts);
        facts
    }

    pub fn all_edges(&self) -> Vec<StoredGraphEdgeFact> {
        let mut facts: Vec<_> = self.edges().collect();
        sort_graph_edges(&mut facts);
        facts
    }

    /// Every live resolved edge in arena order, without sorting or copying
    /// the edge list.
    pub fn edges(&self) -> impl Iterator<Item = StoredGraphEdgeFact> + '_ {
        self.edges
            .iter()
            .filter_map(|edge| edge.map(StoredGraphEdgeFact::from))
    }

    pub fn node_count(&self) -> usize {
        self.nodes.values().map(|node| node.definitions.len()).sum()
    }

    pub fn edge_count(&self) -> usize {
        self.edges.iter().filter(|edge| edge.is_some()).count()
    }

    /// Whether any node definition currently belongs to `file_id`.
    pub fn defines_nodes_in(&self, file_id: SourceFileId) -> bool {
        self.nodes_by_file.contains_key(&file_id)
    }

    pub fn remove_file(&mut self, file_id: SourceFileId) {
        for fqn in self.nodes_by_file.remove(&file_id).unwrap_or_default() {
            let node = self.nodes.get_mut(&fqn).expect_invariant(
                "graph file node index points to a missing node",
                "a node keeps its entry while any file still defines it",
                "remove a node only after its last definition leaves nodes_by_file",
            );
            node.definitions
                .retain(|definition| definition.range.file_id != file_id);
            if node.is_empty() {
                self.nodes.remove(&fqn);
            }
        }

        if let Some(unresolved) = self.unresolved_by_file.remove(&file_id) {
            for edge in unresolved {
                self.remove_unresolved_edge_indexes(edge);
            }
        }

        let Some(stale_edges) = self.edges_by_file.remove(&file_id) else {
            return;
        };
        for edge_id in stale_edges {
            self.remove_edge(edge_id);
        }
    }

    pub fn replace_file(
        &mut self,
        file_id: SourceFileId,
        nodes: impl IntoIterator<Item = StoredGraphNodeFact>,
        edges: impl IntoIterator<Item = StoredGraphEdgeFact>,
        unresolved: impl IntoIterator<Item = StoredUnresolvedGraphEdgeFact>,
    ) {
        self.remove_file(file_id);
        for node in nodes {
            invariant!(
                node.range.file_id == file_id,
                what = "replacement graph node belongs to a different file id",
                why = "SemanticGraph::replace_file must only receive facts for the target file",
                fix = "partition graph facts by SourceFileId before replacing",
            );
            self.add_node(node);
        }
        for edge in edges {
            invariant!(
                edge.range.file_id == file_id,
                what = "replacement graph edge belongs to a different file id",
                why = "SemanticGraph::replace_file must only receive facts for the target file",
                fix = "partition graph facts by SourceFileId before replacing",
            );
            self.add_edge(edge);
        }
        for edge in unresolved {
            invariant!(
                edge.range.file_id == file_id,
                what = "replacement unresolved graph edge belongs to a different file id",
                why = "SemanticGraph::replace_file must only receive facts for the target file",
                fix = "partition unresolved graph edges by SourceFileId before replacing",
            );
            self.add_unresolved_edge(edge);
        }
    }

    pub fn unresolved_edges(&self) -> impl Iterator<Item = StoredUnresolvedGraphEdgeFact> + '_ {
        self.unresolved_by_file
            .values()
            .flat_map(|edges| edges.iter().copied())
    }

    pub fn unresolved_edge_count(&self) -> usize {
        self.unresolved_by_file.values().map(Vec::len).sum()
    }

    pub fn has_unresolved_edges(&self) -> bool {
        self.unresolved_by_file
            .values()
            .any(|edges| !edges.is_empty())
    }

    pub fn take_unresolved_edges(&mut self) -> Vec<StoredUnresolvedGraphEdgeFact> {
        let pending = std::mem::take(&mut self.unresolved_by_file);
        self.unresolved_explicit_superclasses_by_source.clear();
        self.unresolved_explicit_sources_by_source.clear();
        pending
            .into_values()
            .flat_map(|edges| edges.into_iter())
            .collect()
    }

    pub fn add_unresolved_edge(&mut self, edge: StoredUnresolvedGraphEdgeFact) {
        if edge.provenance == GraphEdgeProvenance::Explicit {
            bump_unresolved_source_count(
                &mut self.unresolved_explicit_sources_by_source,
                edge.source,
                "explicit unresolved edge source",
            );
            if edge.kind == GraphEdgeKind::Superclass {
                bump_unresolved_source_count(
                    &mut self.unresolved_explicit_superclasses_by_source,
                    edge.source,
                    "unresolved explicit superclass source",
                );
            }
        }
        self.unresolved_by_file
            .entry(edge.range.file_id)
            .or_default()
            .push(edge);
    }

    fn remove_unresolved_edge_indexes(&mut self, edge: StoredUnresolvedGraphEdgeFact) {
        if edge.provenance == GraphEdgeProvenance::Explicit {
            release_unresolved_source_count(
                &mut self.unresolved_explicit_sources_by_source,
                edge.source,
                "explicit unresolved edge source",
            );
            if edge.kind == GraphEdgeKind::Superclass {
                release_unresolved_source_count(
                    &mut self.unresolved_explicit_superclasses_by_source,
                    edge.source,
                    "unresolved explicit superclass source",
                );
            }
        }
    }

    pub fn estimated_heap_bytes(&self) -> usize {
        map_table_bytes(&self.nodes)
            + vec_payload_bytes(&self.edges)
            + vec_payload_bytes(&self.free_edges)
            + map_table_bytes(&self.nodes_by_file)
            + map_table_bytes(&self.edges_by_file)
            + map_table_bytes(&self.unresolved_explicit_superclasses_by_source)
            + map_table_bytes(&self.unresolved_explicit_sources_by_source)
            + self
                .nodes
                .values()
                .map(|node| vec_payload_bytes(&node.definitions) + vec_payload_bytes(&node.links))
                .sum::<usize>()
            + self
                .nodes_by_file
                .values()
                .map(vec_payload_bytes)
                .sum::<usize>()
            + self
                .edges_by_file
                .values()
                .map(vec_payload_bytes)
                .sum::<usize>()
    }

    pub fn estimated_unresolved_heap_bytes(&self) -> usize {
        map_table_bytes(&self.unresolved_by_file)
            + self
                .unresolved_by_file
                .values()
                .map(vec_payload_bytes)
                .sum::<usize>()
    }

    pub fn shrink_to_fit(&mut self) {
        self.nodes.shrink_to_fit();
        self.edges.shrink_to_fit();
        self.free_edges.shrink_to_fit();
        self.nodes_by_file.shrink_to_fit();
        self.edges_by_file.shrink_to_fit();
        self.unresolved_by_file.shrink_to_fit();
        self.unresolved_explicit_superclasses_by_source
            .shrink_to_fit();
        self.unresolved_explicit_sources_by_source.shrink_to_fit();
        for node in self.nodes.values_mut() {
            node.definitions.shrink_to_fit();
            node.links.shrink_to_fit();
        }
        for fqns in self.nodes_by_file.values_mut() {
            fqns.shrink_to_fit();
        }
        for edges in self.edges_by_file.values_mut() {
            edges.shrink_to_fit();
        }
        for edges in self.unresolved_by_file.values_mut() {
            edges.shrink_to_fit();
        }
    }

    fn insert_edge(&mut self, edge: GraphEdge) -> GraphEdgeId {
        let file_id = edge.range.file_id;
        let source = edge.source;
        let target = edge.target;
        let kind = edge.kind;
        let id = if let Some(id) = self.free_edges.pop() {
            let slot = self.edges.get_mut(id.0 as usize).expect_invariant(
                "graph edge free list points outside edge arena",
                "free ids must come from previous arena slots",
                "only push ids returned by SemanticGraph::remove_edge",
            );
            invariant!(
                slot.is_none(),
                what = "graph edge free list points to occupied edge slot",
                why = "free ids must only reference removed graph edges",
                fix = "push each removed graph edge id at most once",
            );
            *slot = Some(edge);
            id
        } else {
            let id = GraphEdgeId(u32::try_from(self.edges.len()).expect_invariant(
                "graph edge arena exceeded u32 ids",
                "GraphEdgeId stores u32",
                "widen GraphEdgeId before storing more than u32::MAX edges",
            ));
            self.edges.push(Some(edge));
            id
        };

        for (fqn, outgoing) in [(source, true), (target, false)] {
            self.nodes.entry(fqn).or_default().link(GraphLink {
                edge: id,
                kind,
                outgoing,
            });
        }
        self.edges_by_file.entry(file_id).or_default().push(id);
        id
    }

    fn remove_edge(&mut self, id: GraphEdgeId) {
        let edge = self
            .edges
            .get_mut(id.0 as usize)
            .and_then(Option::take)
            .expect_invariant(
                "graph edge file index points to missing edge",
                "edge ids in edges_by_file must reference live edges",
                "remove stale edge ids from edges_by_file when deleting edges",
            );
        for (fqn, outgoing) in [(edge.source, true), (edge.target, false)] {
            let node = self.nodes.get_mut(&fqn).expect_invariant(
                "graph edge endpoint has no node",
                "insert_edge creates both endpoint nodes and a node outlives its links",
                "prune a node only after its last link and definition are gone",
            );
            node.unlink(id, outgoing);
            if node.is_empty() {
                self.nodes.remove(&fqn);
            }
        }
        self.free_edges.push(id);
    }

    fn edge(&self, id: GraphEdgeId) -> Option<GraphEdge> {
        self.edges.get(id.0 as usize).and_then(|edge| *edge)
    }

    fn indexed_node(&self, fqn: FqnId) -> &GraphNode {
        self.nodes.get(&fqn).expect_invariant(
            "graph file node index points to a missing node",
            "a node keeps its entry while any file still defines it",
            "remove a node only after its last definition leaves nodes_by_file",
        )
    }

    /// The edges in one direction of `fqn` whose kind `selected` keeps, in
    /// source order.
    fn linked_edges(
        &self,
        fqn: FqnId,
        outgoing: bool,
        selected: impl Fn(GraphEdgeKind) -> bool,
    ) -> Vec<StoredGraphEdgeFact> {
        let Some(node) = self.nodes.get(&fqn) else {
            return Vec::new();
        };
        let links = if outgoing {
            node.outgoing()
        } else {
            node.incoming()
        };
        self.edges_by_ids(
            links
                .iter()
                .filter(|link| selected(link.kind))
                .map(|link| link.edge),
        )
    }

    fn edges_by_ids(&self, ids: impl Iterator<Item = GraphEdgeId>) -> Vec<StoredGraphEdgeFact> {
        let mut facts: Vec<_> = ids
            .filter_map(|id| self.edge(id).map(StoredGraphEdgeFact::from))
            .collect();
        sort_graph_edges(&mut facts);
        facts
    }
}

impl GraphNode {
    fn is_empty(&self) -> bool {
        self.definitions.is_empty() && self.links.is_empty()
    }

    fn outgoing_start(&self) -> usize {
        self.links.partition_point(|link| !link.outgoing)
    }

    fn incoming(&self) -> &[GraphLink] {
        &self.links[..self.outgoing_start()]
    }

    fn outgoing(&self) -> &[GraphLink] {
        &self.links[self.outgoing_start()..]
    }

    /// Add one endpoint, keeping incoming links before outgoing ones.
    fn link(&mut self, link: GraphLink) {
        let outgoing_start = self.outgoing_start();
        self.links.push(link);
        if !link.outgoing {
            let last = self.links.len() - 1;
            self.links.swap(outgoing_start, last);
        }
    }

    /// Drop the endpoint of `edge` on this side. A self-loop keeps its other
    /// endpoint.
    fn unlink(&mut self, edge: GraphEdgeId, outgoing: bool) {
        let outgoing_start = self.outgoing_start();
        let side = if outgoing {
            outgoing_start..self.links.len()
        } else {
            0..outgoing_start
        };
        let Some(offset) = self.links[side.clone()]
            .iter()
            .position(|link| link.edge == edge)
        else {
            return;
        };
        let position = side.start + offset;
        if !outgoing {
            // Move the hole to the end of the incoming run so the swap below
            // only moves an outgoing link across the boundary.
            self.links.swap(position, outgoing_start - 1);
            self.links.swap_remove(outgoing_start - 1);
        } else {
            self.links.swap_remove(position);
        }
    }
}

fn sort_node_definitions(definitions: &mut [GraphNodeDefinition]) {
    definitions.sort_by_key(|definition| {
        (
            definition.range.file_id,
            definition.range.start_byte,
            definition.range.end_byte,
        )
    });
}

fn sort_graph_nodes(facts: &mut [StoredGraphNodeFact]) {
    facts.sort_by_key(|fact| {
        (
            fact.fqn,
            fact.range.file_id,
            fact.range.start_byte,
            fact.range.end_byte,
        )
    });
}

fn sort_graph_edges(facts: &mut [StoredGraphEdgeFact]) {
    facts.sort_by_key(|fact| graph_edge_order_key(*fact));
}

fn graph_edge_order_key(
    fact: StoredGraphEdgeFact,
) -> (
    FqnId,
    SourceFileId,
    u32,
    u32,
    GraphEdgeKind,
    GraphEdgeProvenance,
    FqnId,
) {
    (
        fact.source,
        fact.range.file_id,
        fact.range.start_byte,
        fact.range.end_byte,
        fact.kind,
        fact.provenance,
        fact.target,
    )
}

fn bump_unresolved_source_count(
    counts: &mut HashMap<FqnId, usize>,
    source: FqnId,
    what: &'static str,
) {
    let count = counts.entry(source).or_default();
    *count = count.checked_add(1).unwrap_or_else(|| {
        unreachable_invariant!(
            what = "{what} count overflowed usize",
            why = "one graph cannot contain more edges than addressable memory",
            fix = "inspect duplicate unresolved edge insertion",
            what = what,
        )
    });
}

fn release_unresolved_source_count(
    counts: &mut HashMap<FqnId, usize>,
    source: FqnId,
    what: &'static str,
) {
    let count = counts.get_mut(&source).unwrap_or_else(|| {
        unreachable_invariant!(
            what = "{what} has no source count",
            why = "the source index and file-owned edge were inserted atomically",
            fix = "update both indexes on every unresolved edge lifecycle operation",
            what = what,
        )
    });
    *count = count.checked_sub(1).unwrap_or_else(|| {
        unreachable_invariant!(
            what = "{what} count underflowed",
            why = "an edge was removed more than once",
            fix = "remove each file-owned unresolved edge exactly once",
            what = what,
        )
    });
    if *count == 0 {
        counts.remove(&source);
    }
}

#[cfg(test)]
mod tests;
