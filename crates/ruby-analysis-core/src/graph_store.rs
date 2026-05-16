use std::collections::HashMap;

use crate::{FullyQualifiedName, SourceFileId, TextRange};

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

#[derive(Debug, Clone, Default)]
pub struct GraphStore {
    nodes_by_fqn: HashMap<FullyQualifiedName, Vec<GraphNodeFact>>,
    edges_by_source: HashMap<FullyQualifiedName, Vec<GraphEdgeFact>>,
}

impl GraphStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_node(&mut self, fact: GraphNodeFact) {
        let facts = self.nodes_by_fqn.entry(fact.fqn.clone()).or_default();
        facts.push(fact);
        facts.sort_by_key(|fact| {
            (
                fact.range.file_id,
                fact.range.start_byte,
                fact.range.end_byte,
            )
        });
    }

    pub fn add_edge(&mut self, fact: GraphEdgeFact) {
        let facts = self.edges_by_source.entry(fact.source.clone()).or_default();
        facts.push(fact);
        facts.sort_by_key(|fact| {
            (
                fact.range.file_id,
                fact.range.start_byte,
                fact.range.end_byte,
                fact.kind,
                fact.target.to_string(),
            )
        });
    }

    pub fn nodes_for(&self, fqn: &FullyQualifiedName) -> &[GraphNodeFact] {
        self.nodes_by_fqn.get(fqn).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn edges_from(&self, source: &FullyQualifiedName) -> &[GraphEdgeFact] {
        self.edges_by_source
            .get(source)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn nodes_in_file(&self, file_id: SourceFileId) -> Vec<GraphNodeFact> {
        self.nodes_by_fqn
            .values()
            .flat_map(|facts| facts.iter())
            .filter(|fact| fact.range.file_id == file_id)
            .cloned()
            .collect()
    }

    pub fn edges_in_file(&self, file_id: SourceFileId) -> Vec<GraphEdgeFact> {
        self.edges_by_source
            .values()
            .flat_map(|facts| facts.iter())
            .filter(|fact| fact.range.file_id == file_id)
            .cloned()
            .collect()
    }

    pub fn all_nodes(&self) -> Vec<GraphNodeFact> {
        self.nodes_by_fqn
            .values()
            .flat_map(|facts| facts.iter().cloned())
            .collect()
    }

    pub fn all_edges(&self) -> Vec<GraphEdgeFact> {
        self.edges_by_source
            .values()
            .flat_map(|facts| facts.iter().cloned())
            .collect()
    }

    pub fn remove_file(&mut self, file_id: SourceFileId) {
        self.nodes_by_fqn.retain(|_, facts| {
            facts.retain(|fact| fact.range.file_id != file_id);
            !facts.is_empty()
        });
        self.edges_by_source.retain(|_, facts| {
            facts.retain(|fact| fact.range.file_id != file_id);
            !facts.is_empty()
        });
    }

    pub fn replace_file(
        &mut self,
        file_id: SourceFileId,
        nodes: impl IntoIterator<Item = GraphNodeFact>,
        edges: impl IntoIterator<Item = GraphEdgeFact>,
    ) {
        self.remove_file(file_id);
        for node in nodes {
            assert!(
                node.range.file_id == file_id,
                "INVARIANT VIOLATED: replacement graph node belongs to a different file id. \
                 This is a bug because GraphStore::replace_file must only receive facts for the target file. \
                 Fix: partition graph facts by SourceFileId before replacing."
            );
            self.add_node(node);
        }
        for edge in edges {
            assert!(
                edge.range.file_id == file_id,
                "INVARIANT VIOLATED: replacement graph edge belongs to a different file id. \
                 This is a bug because GraphStore::replace_file must only receive facts for the target file. \
                 Fix: partition graph facts by SourceFileId before replacing."
            );
            self.add_edge(edge);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{FullyQualifiedName, RubyConstant, SourceFileId, TextRange};

    use super::*;

    fn file() -> SourceFileId {
        SourceFileId(1)
    }

    fn ns(name: &str) -> FullyQualifiedName {
        FullyQualifiedName::namespace(vec![RubyConstant::new(name).unwrap()])
    }

    #[test]
    fn replace_file_removes_stale_graph_facts_for_same_file_only() {
        let source = ns("User");
        let target = ns("ApplicationRecord");
        let mut store = GraphStore::new();
        store.add_node(GraphNodeFact::new(
            source.clone(),
            GraphNodeKind::Class,
            TextRange::new(file(), 0, 10),
        ));
        store.add_edge(GraphEdgeFact::new(
            source.clone(),
            target.clone(),
            GraphEdgeKind::Superclass,
            TextRange::new(file(), 0, 10),
        ));

        store.replace_file(
            file(),
            [GraphNodeFact::new(
                target.clone(),
                GraphNodeKind::Class,
                TextRange::new(file(), 20, 30),
            )],
            [],
        );

        assert!(store.nodes_for(&source).is_empty());
        assert!(store.edges_from(&source).is_empty());
        assert_eq!(store.nodes_for(&target).len(), 1);
    }
}
