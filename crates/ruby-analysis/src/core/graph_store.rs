use std::collections::HashMap;

use super::memory_estimate::{map_table_bytes, vec_payload_bytes};
use crate::{FqnId, FullyQualifiedName, SourceFileId, TextRange};

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

#[derive(Debug, Clone, Default)]
pub struct GraphStore {
    nodes: Vec<Option<StoredGraphNodeFact>>,
    edges: Vec<Option<StoredGraphEdgeFact>>,
    free_nodes: Vec<GraphNodeFactId>,
    free_edges: Vec<GraphEdgeFactId>,
    nodes_by_fqn: HashMap<FqnId, Vec<GraphNodeFactId>>,
    edges_by_source: HashMap<FqnId, Vec<GraphEdgeFactId>>,
    nodes_by_file: HashMap<SourceFileId, Vec<GraphNodeFactId>>,
    edges_by_file: HashMap<SourceFileId, Vec<GraphEdgeFactId>>,
    edges_by_target: HashMap<FqnId, Vec<GraphEdgeFactId>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct GraphNodeFactId(usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct GraphEdgeFactId(usize);

impl GraphStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_node(&mut self, fact: StoredGraphNodeFact) {
        let file_id = fact.range.file_id;
        let fqn = fact.fqn;
        let id = self.insert_node(fact);
        let facts = self.nodes_by_fqn.entry(fqn).or_default();
        facts.push(id);
        sort_graph_node_ids(&self.nodes, facts);
        let file_facts = self.nodes_by_file.entry(file_id).or_default();
        file_facts.push(id);
        sort_graph_node_ids_in_file(&self.nodes, file_facts);
    }

    pub fn add_edge(&mut self, fact: StoredGraphEdgeFact) {
        let file_id = fact.range.file_id;
        let source = fact.source;
        let target = fact.target;
        let id = self.insert_edge(fact);
        let facts = self.edges_by_source.entry(source).or_default();
        facts.push(id);
        sort_graph_edge_ids_by_source(&self.edges, facts);
        let file_facts = self.edges_by_file.entry(file_id).or_default();
        file_facts.push(id);
        sort_graph_edge_ids_in_file(&self.edges, file_facts);
        let target_facts = self.edges_by_target.entry(target).or_default();
        target_facts.push(id);
        sort_graph_edge_ids_by_target(&self.edges, target_facts);
    }

    pub fn nodes_for(&self, fqn: FqnId) -> Vec<StoredGraphNodeFact> {
        self.nodes_by_fqn
            .get(&fqn)
            .map(|ids| self.clone_nodes(ids))
            .unwrap_or_default()
    }

    pub fn edges_from(&self, source: FqnId) -> Vec<StoredGraphEdgeFact> {
        self.edges_by_source
            .get(&source)
            .map(|ids| self.clone_edges(ids))
            .unwrap_or_default()
    }

    pub fn edges_to(&self, target: FqnId) -> Vec<StoredGraphEdgeFact> {
        self.edges_by_target
            .get(&target)
            .map(|ids| self.clone_edges(ids))
            .unwrap_or_default()
    }

    pub fn nodes_in_file(&self, file_id: SourceFileId) -> Vec<StoredGraphNodeFact> {
        self.nodes_by_file
            .get(&file_id)
            .map(|ids| self.clone_nodes(ids))
            .unwrap_or_default()
    }

    pub fn edges_in_file(&self, file_id: SourceFileId) -> Vec<StoredGraphEdgeFact> {
        self.edges_by_file
            .get(&file_id)
            .map(|ids| self.clone_edges(ids))
            .unwrap_or_default()
    }

    pub fn all_nodes(&self) -> Vec<StoredGraphNodeFact> {
        self.nodes.iter().filter_map(|fact| *fact).collect()
    }

    pub fn all_edges(&self) -> Vec<StoredGraphEdgeFact> {
        self.edges.iter().filter_map(|fact| *fact).collect()
    }

    pub fn node_count(&self) -> usize {
        self.nodes.iter().filter(|fact| fact.is_some()).count()
    }

    pub fn edge_count(&self) -> usize {
        self.edges.iter().filter(|fact| fact.is_some()).count()
    }

    pub fn remove_file(&mut self, file_id: SourceFileId) {
        if let Some(stale_nodes) = self.nodes_by_file.remove(&file_id) {
            for stale_id in stale_nodes {
                let Some(stale) = self.take_node(stale_id) else {
                    continue;
                };
                self.free_nodes.push(stale_id);
                if let Some(facts) = self.nodes_by_fqn.get_mut(&stale.fqn) {
                    facts.retain(|id| *id != stale_id);
                    if facts.is_empty() {
                        self.nodes_by_fqn.remove(&stale.fqn);
                    }
                }
            }
        }
        if let Some(stale_edges) = self.edges_by_file.remove(&file_id) {
            for stale_id in stale_edges {
                let Some(stale) = self.take_edge(stale_id) else {
                    continue;
                };
                self.free_edges.push(stale_id);
                if let Some(facts) = self.edges_by_source.get_mut(&stale.source) {
                    facts.retain(|id| *id != stale_id);
                    if facts.is_empty() {
                        self.edges_by_source.remove(&stale.source);
                    }
                }
                if let Some(facts) = self.edges_by_target.get_mut(&stale.target) {
                    facts.retain(|id| *id != stale_id);
                    if facts.is_empty() {
                        self.edges_by_target.remove(&stale.target);
                    }
                }
            }
        }
    }

    pub fn replace_file(
        &mut self,
        file_id: SourceFileId,
        nodes: impl IntoIterator<Item = StoredGraphNodeFact>,
        edges: impl IntoIterator<Item = StoredGraphEdgeFact>,
    ) {
        self.remove_file(file_id);
        let mut touched_nodes = Vec::new();
        for node in nodes {
            assert!(
                node.range.file_id == file_id,
                "INVARIANT VIOLATED: replacement graph node belongs to a different file id. \
                 This is a bug because GraphStore::replace_file must only receive facts for the target file. \
                 Fix: partition graph facts by SourceFileId before replacing."
            );
            let key = node.fqn;
            if !touched_nodes.contains(&key) {
                touched_nodes.push(key);
            }
            let id = self.insert_node(node);
            self.nodes_by_fqn.entry(key).or_default().push(id);
            self.nodes_by_file.entry(file_id).or_default().push(id);
        }
        let mut touched_sources = Vec::new();
        let mut touched_targets = Vec::new();
        for edge in edges {
            assert!(
                edge.range.file_id == file_id,
                "INVARIANT VIOLATED: replacement graph edge belongs to a different file id. \
                 This is a bug because GraphStore::replace_file must only receive facts for the target file. \
                 Fix: partition graph facts by SourceFileId before replacing."
            );
            let source = edge.source;
            let target = edge.target;
            if !touched_sources.contains(&source) {
                touched_sources.push(source);
            }
            if !touched_targets.contains(&target) {
                touched_targets.push(target);
            }
            let id = self.insert_edge(edge);
            self.edges_by_source.entry(source).or_default().push(id);
            self.edges_by_target.entry(target).or_default().push(id);
            self.edges_by_file.entry(file_id).or_default().push(id);
        }
        for fqn in touched_nodes {
            if let Some(nodes) = self.nodes_by_fqn.get_mut(&fqn) {
                sort_graph_node_ids(&self.nodes, nodes);
            }
        }
        if let Some(nodes) = self.nodes_by_file.get_mut(&file_id) {
            sort_graph_node_ids_in_file(&self.nodes, nodes);
        }
        for source in touched_sources {
            if let Some(edges) = self.edges_by_source.get_mut(&source) {
                sort_graph_edge_ids_by_source(&self.edges, edges);
            }
        }
        for target in touched_targets {
            if let Some(edges) = self.edges_by_target.get_mut(&target) {
                sort_graph_edge_ids_by_target(&self.edges, edges);
            }
        }
        if let Some(edges) = self.edges_by_file.get_mut(&file_id) {
            sort_graph_edge_ids_in_file(&self.edges, edges);
        }
    }

    pub fn estimated_heap_bytes(&self) -> usize {
        vec_payload_bytes(&self.nodes)
            + vec_payload_bytes(&self.edges)
            + vec_payload_bytes(&self.free_nodes)
            + vec_payload_bytes(&self.free_edges)
            + map_table_bytes(&self.nodes_by_fqn)
            + map_table_bytes(&self.edges_by_source)
            + map_table_bytes(&self.nodes_by_file)
            + map_table_bytes(&self.edges_by_file)
            + map_table_bytes(&self.edges_by_target)
            + self
                .nodes_by_fqn
                .values()
                .map(vec_payload_bytes)
                .sum::<usize>()
            + self
                .nodes_by_file
                .values()
                .map(vec_payload_bytes)
                .sum::<usize>()
            + self
                .edges_by_source
                .values()
                .map(vec_payload_bytes)
                .sum::<usize>()
            + self
                .edges_by_target
                .values()
                .map(vec_payload_bytes)
                .sum::<usize>()
            + self
                .edges_by_file
                .values()
                .map(vec_payload_bytes)
                .sum::<usize>()
    }

    pub fn shrink_to_fit(&mut self) {
        self.nodes.shrink_to_fit();
        self.edges.shrink_to_fit();
        self.free_nodes.shrink_to_fit();
        self.free_edges.shrink_to_fit();
        self.nodes_by_fqn.shrink_to_fit();
        self.edges_by_source.shrink_to_fit();
        self.nodes_by_file.shrink_to_fit();
        self.edges_by_file.shrink_to_fit();
        self.edges_by_target.shrink_to_fit();
        for nodes in self.nodes_by_fqn.values_mut() {
            nodes.shrink_to_fit();
        }
        for nodes in self.nodes_by_file.values_mut() {
            nodes.shrink_to_fit();
        }
        for edges in self.edges_by_source.values_mut() {
            edges.shrink_to_fit();
        }
        for edges in self.edges_by_file.values_mut() {
            edges.shrink_to_fit();
        }
        for edges in self.edges_by_target.values_mut() {
            edges.shrink_to_fit();
        }
    }

    fn insert_node(&mut self, fact: StoredGraphNodeFact) -> GraphNodeFactId {
        if let Some(id) = self.free_nodes.pop() {
            let slot = self.nodes.get_mut(id.0).expect(
                "INVARIANT VIOLATED: graph node free list points outside node arena. \
                 This is a bug because free ids must come from previous arena slots. \
                 Fix: only push ids returned by GraphStore::take_node.",
            );
            assert!(
                slot.is_none(),
                "INVARIANT VIOLATED: graph node free list points to occupied node slot. \
                 This is a bug because free ids must only reference removed graph nodes. \
                 Fix: push each removed graph node id at most once."
            );
            *slot = Some(fact);
            return id;
        }
        let id = GraphNodeFactId(self.nodes.len());
        self.nodes.push(Some(fact));
        id
    }

    fn insert_edge(&mut self, fact: StoredGraphEdgeFact) -> GraphEdgeFactId {
        if let Some(id) = self.free_edges.pop() {
            let slot = self.edges.get_mut(id.0).expect(
                "INVARIANT VIOLATED: graph edge free list points outside edge arena. \
                 This is a bug because free ids must come from previous arena slots. \
                 Fix: only push ids returned by GraphStore::take_edge.",
            );
            assert!(
                slot.is_none(),
                "INVARIANT VIOLATED: graph edge free list points to occupied edge slot. \
                 This is a bug because free ids must only reference removed graph edges. \
                 Fix: push each removed graph edge id at most once."
            );
            *slot = Some(fact);
            return id;
        }
        let id = GraphEdgeFactId(self.edges.len());
        self.edges.push(Some(fact));
        id
    }

    fn node(&self, id: GraphNodeFactId) -> Option<&StoredGraphNodeFact> {
        self.nodes.get(id.0).and_then(Option::as_ref)
    }

    fn edge(&self, id: GraphEdgeFactId) -> Option<&StoredGraphEdgeFact> {
        self.edges.get(id.0).and_then(Option::as_ref)
    }

    fn take_node(&mut self, id: GraphNodeFactId) -> Option<StoredGraphNodeFact> {
        self.nodes.get_mut(id.0).and_then(Option::take)
    }

    fn take_edge(&mut self, id: GraphEdgeFactId) -> Option<StoredGraphEdgeFact> {
        self.edges.get_mut(id.0).and_then(Option::take)
    }

    fn clone_nodes(&self, ids: &[GraphNodeFactId]) -> Vec<StoredGraphNodeFact> {
        ids.iter()
            .filter_map(|id| self.node(*id).copied())
            .collect()
    }

    fn clone_edges(&self, ids: &[GraphEdgeFactId]) -> Vec<StoredGraphEdgeFact> {
        ids.iter()
            .filter_map(|id| self.edge(*id).copied())
            .collect()
    }
}

fn sort_graph_node_ids(facts: &[Option<StoredGraphNodeFact>], ids: &mut [GraphNodeFactId]) {
    ids.sort_by_key(|id| {
        let fact = facts[id.0].as_ref().expect(
            "INVARIANT VIOLATED: graph node index points to missing fact. \
             This is a bug because indexes must be removed before arena facts. \
             Fix: remove stale ids from every GraphStore node index.",
        );
        (
            fact.range.file_id,
            fact.range.start_byte,
            fact.range.end_byte,
        )
    });
}

fn sort_graph_node_ids_in_file(facts: &[Option<StoredGraphNodeFact>], ids: &mut [GraphNodeFactId]) {
    ids.sort_by_key(|id| {
        let fact = facts[id.0].as_ref().expect(
            "INVARIANT VIOLATED: graph node file index points to missing fact. \
             This is a bug because indexes must be removed before arena facts. \
             Fix: remove stale ids from every GraphStore node index.",
        );
        (fact.range.start_byte, fact.range.end_byte)
    });
}

fn sort_graph_edge_ids_by_source(
    facts: &[Option<StoredGraphEdgeFact>],
    ids: &mut [GraphEdgeFactId],
) {
    ids.sort_by_key(|id| {
        let fact = facts[id.0].as_ref().expect(
            "INVARIANT VIOLATED: graph edge source index points to missing fact. \
             This is a bug because indexes must be removed before arena facts. \
             Fix: remove stale ids from every GraphStore edge index.",
        );
        (
            fact.range.file_id,
            fact.range.start_byte,
            fact.range.end_byte,
            fact.kind,
            fact.target,
        )
    });
}

fn sort_graph_edge_ids_by_target(
    facts: &[Option<StoredGraphEdgeFact>],
    ids: &mut [GraphEdgeFactId],
) {
    ids.sort_by_key(|id| {
        let fact = facts[id.0].as_ref().expect(
            "INVARIANT VIOLATED: graph edge target index points to missing fact. \
             This is a bug because indexes must be removed before arena facts. \
             Fix: remove stale ids from every GraphStore edge index.",
        );
        (
            fact.range.file_id,
            fact.range.start_byte,
            fact.range.end_byte,
            fact.kind,
            fact.source,
        )
    });
}

fn sort_graph_edge_ids_in_file(facts: &[Option<StoredGraphEdgeFact>], ids: &mut [GraphEdgeFactId]) {
    ids.sort_by_key(|id| {
        let fact = facts[id.0].as_ref().expect(
            "INVARIANT VIOLATED: graph edge file index points to missing fact. \
             This is a bug because indexes must be removed before arena facts. \
             Fix: remove stale ids from every GraphStore edge index.",
        );
        (
            fact.range.start_byte,
            fact.range.end_byte,
            fact.kind,
            fact.target,
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
        let mut store = GraphStore::new();
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
        );

        assert!(store.nodes_for(source).is_empty());
        assert!(store.edges_from(source).is_empty());
        assert_eq!(store.nodes_for(target).len(), 1);
    }
}
