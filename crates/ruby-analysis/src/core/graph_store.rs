use std::collections::{HashMap, HashSet};

use super::memory_estimate::{map_table_bytes, set_table_bytes, vec_payload_bytes};
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
    pub target_parts: Vec<crate::RubyConstant>,
    pub absolute: bool,
    pub context: FullyQualifiedName,
    pub kind: GraphEdgeKind,
    pub provenance: GraphEdgeProvenance,
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
pub struct GraphEdgeId(usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GraphNodeDefinition {
    kind: GraphNodeKind,
    range: TextRange,
}

#[derive(Debug, Clone, Default)]
pub struct GraphNode {
    definitions: Vec<GraphNodeDefinition>,
    superclasses: Vec<GraphEdgeId>,
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
    node_definition_files: HashSet<SourceFileId>,
    edges_by_file: HashMap<SourceFileId, Vec<GraphEdgeId>>,
    unresolved_by_file: HashMap<SourceFileId, Vec<StoredUnresolvedGraphEdgeFact>>,
    unresolved_explicit_superclasses_by_source: HashMap<FqnId, usize>,
}

impl SemanticGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_node(&mut self, fact: StoredGraphNodeFact) {
        self.node_definition_files.insert(fact.range.file_id);
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
        ids.extend(node.superclasses.iter().copied());
        ids.extend(node.includes.iter().copied());
        ids.extend(node.prepends.iter().copied());
        ids.extend(node.extends.iter().copied());
        ids.extend(node.execution_context_applications.iter().copied());
        self.edges_by_ids(&ids)
    }

    pub fn superclass_resolution(&self, source: FqnId) -> StoredSuperclassResolution {
        let Some(node) = self.nodes.get(&source) else {
            return StoredSuperclassResolution::Missing;
        };
        let has_explicit = node
            .superclasses
            .iter()
            .filter_map(|id| self.edge(*id))
            .any(|edge| edge.provenance == GraphEdgeProvenance::Explicit);
        let mut chosen: Option<StoredGraphEdgeFact> = None;
        for edge in node.superclasses.iter().filter_map(|id| self.edge(*id)) {
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
        if self.node_definition_files.remove(&file_id) {
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
        }

        if let Some(unresolved) = self.unresolved_by_file.remove(&file_id) {
            for edge in unresolved {
                self.remove_unresolved_explicit_superclass_source(edge);
            }
        }

        let Some(stale_edges) = self.edges_by_file.remove(&file_id) else {
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
            self.add_unresolved_edge(edge);
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
        self.unresolved_explicit_superclasses_by_source.clear();
        pending
            .into_values()
            .flat_map(|edges| edges.into_iter())
            .collect()
    }

    pub fn add_unresolved_edge(&mut self, edge: StoredUnresolvedGraphEdgeFact) {
        if edge.kind == GraphEdgeKind::Superclass
            && edge.provenance == GraphEdgeProvenance::Explicit
        {
            let count = self
                .unresolved_explicit_superclasses_by_source
                .entry(edge.source)
                .or_default();
            *count = count.checked_add(1).expect(
                "INVARIANT VIOLATED: unresolved explicit superclass source count overflowed usize. This is a bug because one graph cannot contain more edges than addressable memory. Fix: inspect duplicate unresolved edge insertion.",
            );
        }
        self.unresolved_by_file
            .entry(edge.range.file_id)
            .or_default()
            .push(edge);
    }

    fn remove_unresolved_explicit_superclass_source(
        &mut self,
        edge: StoredUnresolvedGraphEdgeFact,
    ) {
        if edge.kind != GraphEdgeKind::Superclass
            || edge.provenance != GraphEdgeProvenance::Explicit
        {
            return;
        }
        let count = self
            .unresolved_explicit_superclasses_by_source
            .get_mut(&edge.source)
            .expect(
                "INVARIANT VIOLATED: unresolved explicit superclass edge has no source count. This is a bug because the source index and file-owned edge were inserted atomically. Fix: update both indexes on every unresolved edge lifecycle operation.",
            );
        *count = count.checked_sub(1).expect(
            "INVARIANT VIOLATED: unresolved explicit superclass source count underflowed. This is a bug because an edge was removed more than once. Fix: remove each file-owned unresolved edge exactly once.",
        );
        if *count == 0 {
            self.unresolved_explicit_superclasses_by_source
                .remove(&edge.source);
        }
    }

    pub fn estimated_heap_bytes(&self) -> usize {
        map_table_bytes(&self.nodes)
            + vec_payload_bytes(&self.edges)
            + vec_payload_bytes(&self.free_edges)
            + set_table_bytes(&self.node_definition_files)
            + map_table_bytes(&self.edges_by_file)
            + map_table_bytes(&self.unresolved_explicit_superclasses_by_source)
            + self
                .nodes
                .values()
                .map(|node| {
                    vec_payload_bytes(&node.definitions)
                        + vec_payload_bytes(&node.superclasses)
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
        self.node_definition_files.shrink_to_fit();
        self.edges_by_file.shrink_to_fit();
        self.unresolved_by_file.shrink_to_fit();
        self.unresolved_explicit_superclasses_by_source
            .shrink_to_fit();
        for node in self.nodes.values_mut() {
            node.definitions.shrink_to_fit();
            node.superclasses.shrink_to_fit();
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
}

impl GraphNode {
    fn push_outgoing(&mut self, kind: GraphEdgeKind, id: GraphEdgeId) {
        match kind {
            GraphEdgeKind::Superclass => self.superclasses.push(id),
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
            GraphEdgeKind::Superclass => self.superclasses.retain(|id| *id != stale),
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
}

fn node_has_no_edges(node: &GraphNode) -> bool {
    node.superclasses.is_empty()
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

    #[test]
    fn superclass_candidates_survive_independent_file_lifecycles() {
        let first_file = SourceFileId(1);
        let second_file = SourceFileId(2);
        let source = FqnId(1);
        let first_target = FqnId(2);
        let second_target = FqnId(3);
        let mut store = SemanticGraph::new();
        store.add_edge(StoredGraphEdgeFact::new(
            source,
            first_target,
            GraphEdgeKind::Superclass,
            TextRange::new(first_file, 0, 10),
        ));
        store.add_edge(StoredGraphEdgeFact::new(
            source,
            second_target,
            GraphEdgeKind::Superclass,
            TextRange::new(second_file, 0, 10),
        ));

        let candidates = store.edges_from(source);
        assert_eq!(candidates.len(), 2);
        assert!(candidates.iter().any(|edge| edge.target == first_target));
        assert!(candidates.iter().any(|edge| edge.target == second_target));

        store.replace_file(second_file, [], [], []);
        assert_eq!(
            store.edges_from(source),
            vec![StoredGraphEdgeFact::new(
                source,
                first_target,
                GraphEdgeKind::Superclass,
                TextRange::new(first_file, 0, 10),
            )]
        );
    }

    #[test]
    fn superclass_resolution_is_proof_first_and_ignores_only_implicit_object() {
        let source = FqnId(1);
        let object = FqnId(2);
        let parent = FqnId(3);
        let alternative = FqnId(4);
        let mut store = SemanticGraph::new();
        store.add_edge(
            StoredGraphEdgeFact::new(
                source,
                object,
                GraphEdgeKind::Superclass,
                TextRange::new(SourceFileId(1), 0, 10),
            )
            .with_provenance(GraphEdgeProvenance::ImplicitObject),
        );
        let parent_fact = StoredGraphEdgeFact::new(
            source,
            parent,
            GraphEdgeKind::Superclass,
            TextRange::new(SourceFileId(2), 4, 10),
        );
        store.add_edge(parent_fact);
        assert_eq!(
            store.superclass_resolution(source),
            StoredSuperclassResolution::Unique(parent_fact)
        );

        store.add_edge(StoredGraphEdgeFact::new(
            source,
            alternative,
            GraphEdgeKind::Superclass,
            TextRange::new(SourceFileId(3), 4, 10),
        ));
        assert_eq!(
            store.superclass_resolution(source),
            StoredSuperclassResolution::Ambiguous
        );
    }

    #[test]
    fn unresolved_explicit_superclass_source_index_tracks_take_and_reinsert() {
        let file_id = SourceFileId(1);
        let source = FqnId(1);
        let unresolved = StoredUnresolvedGraphEdgeFact::new(
            source,
            ConstLookupId(1),
            GraphEdgeKind::Superclass,
            TextRange::new(file_id, 0, 10),
        );
        let mut store = SemanticGraph::new();
        store.add_unresolved_edge(unresolved);
        assert!(store.has_unresolved_explicit_superclass(source));

        assert_eq!(store.take_unresolved_edges(), vec![unresolved]);
        assert!(!store.has_unresolved_explicit_superclass(source));

        store.add_unresolved_edge(unresolved);
        store.replace_file(file_id, [], [], []);
        assert!(!store.has_unresolved_explicit_superclass(source));
    }

    #[test]
    fn node_definition_file_index_tracks_only_files_that_require_node_cleanup() {
        let node_file = SourceFileId(1);
        let edge_only_file = SourceFileId(2);
        let source = FqnId(1);
        let target = FqnId(2);
        let mut store = SemanticGraph::new();

        store.add_node(StoredGraphNodeFact::new(
            source,
            GraphNodeKind::Class,
            TextRange::new(node_file, 0, 10),
        ));
        store.add_edge(StoredGraphEdgeFact::new(
            source,
            target,
            GraphEdgeKind::Include,
            TextRange::new(edge_only_file, 0, 10),
        ));

        assert!(store.node_definition_files.contains(&node_file));
        assert!(
            !store.node_definition_files.contains(&edge_only_file),
            "edge endpoint nodes must not trigger a global definition cleanup scan"
        );

        store.replace_file(node_file, [], [], []);
        assert!(!store.node_definition_files.contains(&node_file));
        assert!(store.nodes_for(source).is_empty());
    }

    #[test]
    fn edge_queries_are_deterministic_independent_of_insertion_order() {
        let source = FqnId(1);
        let first_target = FqnId(2);
        let second_target = FqnId(3);
        let first = StoredGraphEdgeFact::new(
            source,
            first_target,
            GraphEdgeKind::Include,
            TextRange::new(SourceFileId(2), 4, 8),
        );
        let second = StoredGraphEdgeFact::new(
            source,
            second_target,
            GraphEdgeKind::Include,
            TextRange::new(SourceFileId(1), 12, 16),
        );

        let mut forward = SemanticGraph::new();
        forward.add_edge(first);
        forward.add_edge(second);
        let mut reverse = SemanticGraph::new();
        reverse.add_edge(second);
        reverse.add_edge(first);

        assert_eq!(forward.edges_from(source), reverse.edges_from(source));
        assert_eq!(
            forward.edges_to(first_target),
            reverse.edges_to(first_target)
        );
        assert_eq!(
            forward.edges_to(second_target),
            reverse.edges_to(second_target)
        );
    }
}
