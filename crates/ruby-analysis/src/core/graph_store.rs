use std::collections::HashMap;

use super::memory_estimate::{map_table_bytes, vec_payload_bytes};
use crate::{ConstLookupId, FqnId, FullyQualifiedName, SourceFileId, TextRange};

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
            range,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedGraphEdgeFact {
    pub source: FullyQualifiedName,
    pub target_parts: Vec<crate::RubyConstant>,
    pub absolute: bool,
    pub context: FullyQualifiedName,
    pub kind: GraphEdgeKind,
    pub range: TextRange,
}

impl UnresolvedGraphEdgeFact {
    pub fn new(
        source: FullyQualifiedName,
        target_parts: Vec<crate::RubyConstant>,
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
            range,
        }
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
    pub range: TextRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoredUnresolvedGraphEdgeFact {
    pub source: FqnId,
    pub target: ConstLookupId,
    pub kind: GraphEdgeKind,
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
            range,
        }
    }
}

impl StoredGraphEdgeFact {
    pub fn new(source: FqnId, target: FqnId, kind: GraphEdgeKind, range: TextRange) -> Self {
        Self {
            source,
            target,
            kind,
            range,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GraphEdgeId(usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GraphNodeDefinition {
    kind: GraphNodeKind,
    range: TextRange,
}

#[derive(Debug, Clone, Default)]
pub struct GraphNode {
    definitions: Vec<GraphNodeDefinition>,
    superclass: Option<GraphEdgeId>,
    includes: Vec<GraphEdgeId>,
    prepends: Vec<GraphEdgeId>,
    extends: Vec<GraphEdgeId>,
    execution_context_applications: Vec<GraphEdgeId>,
    children: Vec<GraphEdgeId>,
    included_by: Vec<GraphEdgeId>,
    prepended_by: Vec<GraphEdgeId>,
    extended_by: Vec<GraphEdgeId>,
    execution_context_templates: Vec<GraphEdgeId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GraphEdge {
    pub source: FqnId,
    pub target: FqnId,
    pub kind: GraphEdgeKind,
    pub range: TextRange,
}

impl From<StoredGraphEdgeFact> for GraphEdge {
    fn from(fact: StoredGraphEdgeFact) -> Self {
        Self {
            source: fact.source,
            target: fact.target,
            kind: fact.kind,
            range: fact.range,
        }
    }
}

impl From<GraphEdge> for StoredGraphEdgeFact {
    fn from(edge: GraphEdge) -> Self {
        StoredGraphEdgeFact::new(edge.source, edge.target, edge.kind, edge.range)
    }
}

#[derive(Debug, Clone, Default)]
pub struct SemanticGraph {
    nodes: HashMap<FqnId, GraphNode>,
    edges: Vec<Option<GraphEdge>>,
    free_edges: Vec<GraphEdgeId>,
    edges_by_file: HashMap<SourceFileId, Vec<GraphEdgeId>>,
    unresolved_by_file: HashMap<SourceFileId, Vec<StoredUnresolvedGraphEdgeFact>>,
}

impl SemanticGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_node(&mut self, fact: StoredGraphNodeFact) {
        let node = self.nodes.entry(fact.fqn).or_default();
        node.definitions.push(GraphNodeDefinition {
            kind: fact.kind,
            range: fact.range,
        });
        sort_node_definitions(&mut node.definitions);
    }

    pub fn add_edge(&mut self, fact: StoredGraphEdgeFact) {
        self.insert_edge(fact.into());
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

    pub fn edges_from(&self, source: FqnId) -> Vec<StoredGraphEdgeFact> {
        let Some(node) = self.nodes.get(&source) else {
            return Vec::new();
        };
        let mut ids = Vec::new();
        ids.extend(node.superclass);
        ids.extend(node.includes.iter().copied());
        ids.extend(node.prepends.iter().copied());
        ids.extend(node.extends.iter().copied());
        ids.extend(node.execution_context_applications.iter().copied());
        self.edges_by_ids(&ids)
    }

    pub fn edges_to(&self, target: FqnId) -> Vec<StoredGraphEdgeFact> {
        let Some(node) = self.nodes.get(&target) else {
            return Vec::new();
        };
        let mut ids = Vec::new();
        ids.extend(node.children.iter().copied());
        ids.extend(node.included_by.iter().copied());
        ids.extend(node.prepended_by.iter().copied());
        ids.extend(node.extended_by.iter().copied());
        ids.extend(node.execution_context_templates.iter().copied());
        self.edges_by_ids(&ids)
    }

    pub fn nodes_in_file(&self, file_id: SourceFileId) -> Vec<StoredGraphNodeFact> {
        let mut facts = Vec::new();
        for (fqn, node) in &self.nodes {
            for definition in &node.definitions {
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
            .map(|ids| self.edges_by_ids(ids))
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
        let mut facts: Vec<_> = self
            .edges
            .iter()
            .filter_map(|edge| edge.map(StoredGraphEdgeFact::from))
            .collect();
        sort_graph_edges(&mut facts);
        facts
    }

    pub fn node_count(&self) -> usize {
        self.nodes.values().map(|node| node.definitions.len()).sum()
    }

    pub fn edge_count(&self) -> usize {
        self.edges.iter().filter(|edge| edge.is_some()).count()
    }

    pub fn remove_file(&mut self, file_id: SourceFileId) {
        let mut empty_nodes = Vec::new();
        for (fqn, node) in &mut self.nodes {
            node.definitions
                .retain(|definition| definition.range.file_id != file_id);
            if node.definitions.is_empty() && node_has_no_edges(node) {
                empty_nodes.push(*fqn);
            }
        }
        for fqn in empty_nodes {
            self.nodes.remove(&fqn);
        }

        let Some(stale_edges) = self.edges_by_file.remove(&file_id) else {
            self.unresolved_by_file.remove(&file_id);
            return;
        };
        for edge_id in stale_edges {
            self.remove_edge(edge_id);
        }
        self.unresolved_by_file.remove(&file_id);
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
            assert!(
                node.range.file_id == file_id,
                "INVARIANT VIOLATED: replacement graph node belongs to a different file id. \
                 This is a bug because SemanticGraph::replace_file must only receive facts for the target file. \
                 Fix: partition graph facts by SourceFileId before replacing."
            );
            self.add_node(node);
        }
        for edge in edges {
            assert!(
                edge.range.file_id == file_id,
                "INVARIANT VIOLATED: replacement graph edge belongs to a different file id. \
                 This is a bug because SemanticGraph::replace_file must only receive facts for the target file. \
                 Fix: partition graph facts by SourceFileId before replacing."
            );
            self.add_edge(edge);
        }
        for edge in unresolved {
            assert!(
                edge.range.file_id == file_id,
                "INVARIANT VIOLATED: replacement unresolved graph edge belongs to a different file id. \
                 This is a bug because SemanticGraph::replace_file must only receive facts for the target file. \
                 Fix: partition unresolved graph edges by SourceFileId before replacing."
            );
            self.unresolved_by_file
                .entry(file_id)
                .or_default()
                .push(edge);
        }
    }

    pub fn unresolved_edges(&self) -> Vec<StoredUnresolvedGraphEdgeFact> {
        self.unresolved_by_file
            .values()
            .flat_map(|edges| edges.iter().copied())
            .collect()
    }

    pub fn unresolved_edges_in_file(
        &self,
        file_id: SourceFileId,
    ) -> Vec<StoredUnresolvedGraphEdgeFact> {
        self.unresolved_by_file
            .get(&file_id)
            .cloned()
            .unwrap_or_default()
    }

    pub fn take_unresolved_edges(&mut self) -> Vec<StoredUnresolvedGraphEdgeFact> {
        let pending = std::mem::take(&mut self.unresolved_by_file);
        pending
            .into_values()
            .flat_map(|edges| edges.into_iter())
            .collect()
    }

    pub fn add_unresolved_edge(&mut self, edge: StoredUnresolvedGraphEdgeFact) {
        self.unresolved_by_file
            .entry(edge.range.file_id)
            .or_default()
            .push(edge);
    }

    pub fn estimated_heap_bytes(&self) -> usize {
        map_table_bytes(&self.nodes)
            + vec_payload_bytes(&self.edges)
            + vec_payload_bytes(&self.free_edges)
            + map_table_bytes(&self.edges_by_file)
            + self
                .nodes
                .values()
                .map(|node| {
                    vec_payload_bytes(&node.definitions)
                        + vec_payload_bytes(&node.includes)
                        + vec_payload_bytes(&node.prepends)
                        + vec_payload_bytes(&node.extends)
                        + vec_payload_bytes(&node.children)
                        + vec_payload_bytes(&node.included_by)
                        + vec_payload_bytes(&node.prepended_by)
                        + vec_payload_bytes(&node.extended_by)
                })
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
        self.edges_by_file.shrink_to_fit();
        self.unresolved_by_file.shrink_to_fit();
        for node in self.nodes.values_mut() {
            node.definitions.shrink_to_fit();
            node.includes.shrink_to_fit();
            node.prepends.shrink_to_fit();
            node.extends.shrink_to_fit();
            node.children.shrink_to_fit();
            node.included_by.shrink_to_fit();
            node.prepended_by.shrink_to_fit();
            node.extended_by.shrink_to_fit();
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
            let slot = self.edges.get_mut(id.0).expect(
                "INVARIANT VIOLATED: graph edge free list points outside edge arena. \
                 This is a bug because free ids must come from previous arena slots. \
                 Fix: only push ids returned by SemanticGraph::remove_edge.",
            );
            assert!(
                slot.is_none(),
                "INVARIANT VIOLATED: graph edge free list points to occupied edge slot. \
                 This is a bug because free ids must only reference removed graph edges. \
                 Fix: push each removed graph edge id at most once."
            );
            *slot = Some(edge);
            id
        } else {
            let id = GraphEdgeId(self.edges.len());
            self.edges.push(Some(edge));
            id
        };

        self.nodes
            .entry(source)
            .or_default()
            .push_outgoing(kind, id);
        self.nodes
            .entry(target)
            .or_default()
            .push_incoming(kind, id);
        self.edges_by_file.entry(file_id).or_default().push(id);
        self.sort_edge_id_lists(source, target, file_id);
        id
    }

    fn remove_edge(&mut self, id: GraphEdgeId) {
        let edge = self.edges.get_mut(id.0).and_then(Option::take).expect(
            "INVARIANT VIOLATED: graph edge file index points to missing edge. \
             This is a bug because edge ids in edges_by_file must reference live edges. \
             Fix: remove stale edge ids from edges_by_file when deleting edges.",
        );
        if let Some(source) = self.nodes.get_mut(&edge.source) {
            source.retain_outgoing(edge.kind, id);
        }
        if let Some(target) = self.nodes.get_mut(&edge.target) {
            target.retain_incoming(edge.kind, id);
        }
        self.free_edges.push(id);
    }

    fn edge(&self, id: GraphEdgeId) -> Option<GraphEdge> {
        self.edges.get(id.0).and_then(|edge| *edge)
    }

    fn edges_by_ids(&self, ids: &[GraphEdgeId]) -> Vec<StoredGraphEdgeFact> {
        let mut facts: Vec<_> = ids
            .iter()
            .filter_map(|id| self.edge(*id).map(StoredGraphEdgeFact::from))
            .collect();
        sort_graph_edges(&mut facts);
        facts
    }

    fn sort_edge_id_lists(&mut self, source: FqnId, target: FqnId, file_id: SourceFileId) {
        let edges = &self.edges;
        if let Some(node) = self.nodes.get_mut(&source) {
            node.sort_outgoing(edges);
        }
        if let Some(node) = self.nodes.get_mut(&target) {
            node.sort_incoming(edges);
        }
        if let Some(ids) = self.edges_by_file.get_mut(&file_id) {
            sort_edge_ids(edges, ids);
        }
    }
}

impl GraphNode {
    fn push_outgoing(&mut self, kind: GraphEdgeKind, id: GraphEdgeId) {
        match kind {
            GraphEdgeKind::Superclass => self.superclass = Some(id),
            GraphEdgeKind::Include => self.includes.push(id),
            GraphEdgeKind::Prepend => self.prepends.push(id),
            GraphEdgeKind::Extend => self.extends.push(id),
            GraphEdgeKind::ExecutionContextApplication => {
                self.execution_context_applications.push(id)
            }
        }
    }

    fn push_incoming(&mut self, kind: GraphEdgeKind, id: GraphEdgeId) {
        match kind {
            GraphEdgeKind::Superclass => self.children.push(id),
            GraphEdgeKind::Include => self.included_by.push(id),
            GraphEdgeKind::Prepend => self.prepended_by.push(id),
            GraphEdgeKind::Extend => self.extended_by.push(id),
            GraphEdgeKind::ExecutionContextApplication => self.execution_context_templates.push(id),
        }
    }

    fn retain_outgoing(&mut self, kind: GraphEdgeKind, stale: GraphEdgeId) {
        match kind {
            GraphEdgeKind::Superclass => {
                if self.superclass == Some(stale) {
                    self.superclass = None;
                }
            }
            GraphEdgeKind::Include => self.includes.retain(|id| *id != stale),
            GraphEdgeKind::Prepend => self.prepends.retain(|id| *id != stale),
            GraphEdgeKind::Extend => self.extends.retain(|id| *id != stale),
            GraphEdgeKind::ExecutionContextApplication => self
                .execution_context_applications
                .retain(|id| *id != stale),
        }
    }

    fn retain_incoming(&mut self, kind: GraphEdgeKind, stale: GraphEdgeId) {
        match kind {
            GraphEdgeKind::Superclass => self.children.retain(|id| *id != stale),
            GraphEdgeKind::Include => self.included_by.retain(|id| *id != stale),
            GraphEdgeKind::Prepend => self.prepended_by.retain(|id| *id != stale),
            GraphEdgeKind::Extend => self.extended_by.retain(|id| *id != stale),
            GraphEdgeKind::ExecutionContextApplication => {
                self.execution_context_templates.retain(|id| *id != stale)
            }
        }
    }

    fn sort_outgoing(&mut self, edges: &[Option<GraphEdge>]) {
        sort_edge_ids(edges, &mut self.includes);
        sort_edge_ids(edges, &mut self.prepends);
        sort_edge_ids(edges, &mut self.extends);
        sort_edge_ids(edges, &mut self.execution_context_applications);
    }

    fn sort_incoming(&mut self, edges: &[Option<GraphEdge>]) {
        sort_edge_ids(edges, &mut self.children);
        sort_edge_ids(edges, &mut self.included_by);
        sort_edge_ids(edges, &mut self.prepended_by);
        sort_edge_ids(edges, &mut self.extended_by);
        sort_edge_ids(edges, &mut self.execution_context_templates);
    }
}

fn node_has_no_edges(node: &GraphNode) -> bool {
    node.superclass.is_none()
        && node.includes.is_empty()
        && node.prepends.is_empty()
        && node.extends.is_empty()
        && node.execution_context_applications.is_empty()
        && node.children.is_empty()
        && node.included_by.is_empty()
        && node.prepended_by.is_empty()
        && node.extended_by.is_empty()
        && node.execution_context_templates.is_empty()
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
    facts.sort_by_key(|fact| {
        (
            fact.source,
            fact.range.file_id,
            fact.range.start_byte,
            fact.range.end_byte,
            fact.kind,
            fact.target,
        )
    });
}

fn sort_edge_ids(edges: &[Option<GraphEdge>], ids: &mut [GraphEdgeId]) {
    ids.sort_by_key(|id| {
        let edge = edges[id.0].as_ref().expect(
            "INVARIANT VIOLATED: graph adjacency points to missing edge. \
             This is a bug because edge ids must be removed from adjacency lists before edge deletion. \
             Fix: remove stale ids from all graph adjacency lists.",
        );
        (
            edge.range.file_id,
            edge.range.start_byte,
            edge.range.end_byte,
            edge.kind,
            edge.source,
            edge.target,
        )
    });
}

#[cfg(test)]
mod tests {
    use crate::{FqnId, SourceFileId, TextRange};

    use super::*;

    fn file() -> SourceFileId {
        SourceFileId(1)
    }

    #[test]
    fn replace_file_removes_stale_graph_facts_for_same_file_only() {
        let source = FqnId(1);
        let target = FqnId(2);
        let mut store = SemanticGraph::new();
        store.add_node(StoredGraphNodeFact::new(
            source,
            GraphNodeKind::Class,
            TextRange::new(file(), 0, 10),
        ));
        store.add_edge(StoredGraphEdgeFact::new(
            source,
            target,
            GraphEdgeKind::Superclass,
            TextRange::new(file(), 0, 10),
        ));

        store.replace_file(
            file(),
            [StoredGraphNodeFact::new(
                target,
                GraphNodeKind::Class,
                TextRange::new(file(), 20, 30),
            )],
            [],
            [],
        );

        assert!(store.nodes_for(source).is_empty());
        assert!(store.edges_from(source).is_empty());
        assert_eq!(store.nodes_for(target).len(), 1);
    }
}
