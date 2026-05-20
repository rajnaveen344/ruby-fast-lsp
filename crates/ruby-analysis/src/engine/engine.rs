use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::core::{
    DiagnosticCandidate, DiagnosticCandidateStore, DiagnosticFact, DiagnosticStore,
    FullyQualifiedName, GraphEdgeFact, GraphNodeFact, GraphStore, MethodFact, MethodStore,
    ReferenceCandidate, ReferenceCandidateStore, ReferenceFact, ReferenceStore, SourceFileId,
    SourceKind, SymbolFact, SymbolStore, TextRange, TypeFact, TypeResolution, TypeStore,
    TypeSubject, UnresolvedGraphEdgeFact,
};

use crate::FileIdMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFile {
    pub id: SourceFileId,
    pub path: PathBuf,
    pub source: String,
    pub kind: SourceKind,
}

#[derive(Debug, Clone, Default)]
pub struct FileAnalysisFacts {
    pub symbols: Vec<SymbolFact>,
    pub methods: Vec<MethodFact>,
    pub types: Vec<TypeFact>,
    pub graph_nodes: Vec<GraphNodeFact>,
    pub graph_edges: Vec<GraphEdgeFact>,
    pub unresolved_graph_edges: Vec<UnresolvedGraphEdgeFact>,
    pub reference_candidates: Vec<ReferenceCandidate>,
    pub diagnostic_candidates: Vec<DiagnosticCandidate>,
    pub diagnostics: Vec<DiagnosticFact>,
}

/// Shared analysis state for editor and agent consumers.
#[derive(Debug, Clone, Default)]
pub struct AnalysisEngine {
    pub(super) file_ids: FileIdMap,
    pub(super) files: HashMap<SourceFileId, SourceFile>,
    pub(super) graph_store: GraphStore,
    pub(super) unresolved_graph_edges: Vec<UnresolvedGraphEdgeFact>,
    pub(super) method_store: MethodStore,
    pub(super) reference_candidate_store: ReferenceCandidateStore,
    pub(super) reference_store: ReferenceStore,
    pub(super) diagnostic_candidate_store: DiagnosticCandidateStore,
    pub(super) symbol_store: SymbolStore,
    pub(super) type_store: TypeStore,
    pub(super) diagnostic_store: DiagnosticStore,
}

impl AnalysisEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn open_or_update_file(
        &mut self,
        path: impl AsRef<Path>,
        source: impl Into<String>,
    ) -> SourceFileId {
        self.open_or_update_file_with_kind(path, source, SourceKind::Project)
    }

    pub fn open_or_update_file_with_kind(
        &mut self,
        path: impl AsRef<Path>,
        source: impl Into<String>,
        kind: SourceKind,
    ) -> SourceFileId {
        let path = path.as_ref();
        let id = self.file_ids.get_or_insert(path);
        self.files.insert(
            id,
            SourceFile {
                id,
                path: path.components().collect(),
                source: source.into(),
                kind,
            },
        );
        id
    }

    pub fn file_id(&self, path: impl AsRef<Path>) -> Option<SourceFileId> {
        self.file_ids.get(path)
    }

    pub fn file(&self, id: SourceFileId) -> Option<&SourceFile> {
        self.files.get(&id)
    }

    pub fn add_type_fact(&mut self, fact: TypeFact) {
        self.assert_known_file_id(
            fact.range.file_id,
            "type fact references unknown source file id",
        );
        self.type_store.add(fact);
    }

    pub fn add_symbol_fact(&mut self, fact: SymbolFact) {
        self.assert_known_file_id(
            fact.range.file_id,
            "symbol fact references unknown source file id",
        );
        self.symbol_store.add(fact);
    }

    pub fn add_reference_fact(&mut self, fact: ReferenceFact) {
        self.assert_known_file_id(
            fact.range.file_id,
            "reference fact references unknown source file id",
        );
        self.reference_store.add(fact);
    }

    pub fn add_method_fact(&mut self, fact: MethodFact) {
        self.assert_known_file_id(
            fact.range.file_id,
            "method fact references unknown source file id",
        );
        self.method_store.add(fact);
    }

    pub fn add_graph_node_fact(&mut self, fact: GraphNodeFact) {
        self.assert_known_file_id(
            fact.range.file_id,
            "graph node fact references unknown source file id",
        );
        self.graph_store.add_node(fact);
    }

    pub fn add_graph_edge_fact(&mut self, fact: GraphEdgeFact) {
        self.assert_known_file_id(
            fact.range.file_id,
            "graph edge fact references unknown source file id",
        );
        self.graph_store.add_edge(fact);
    }

    pub fn add_diagnostic_fact(&mut self, fact: DiagnosticFact) {
        self.assert_known_file_id(
            fact.range.file_id,
            "diagnostic fact references unknown source file id",
        );
        self.diagnostic_store.add(fact);
    }

    pub fn replace_symbol_facts_for_file(
        &mut self,
        file_id: SourceFileId,
        facts: impl IntoIterator<Item = SymbolFact>,
    ) {
        self.assert_known_file_id(
            file_id,
            "symbol fact replacement references unknown source file id",
        );
        self.symbol_store.replace_file(file_id, facts);
        self.resolve_reference_candidates();
    }

    pub fn replace_reference_facts_for_file(
        &mut self,
        file_id: SourceFileId,
        facts: impl IntoIterator<Item = ReferenceFact>,
    ) {
        self.assert_known_file_id(
            file_id,
            "reference fact replacement references unknown source file id",
        );
        self.reference_store.replace_file(file_id, facts);
    }

    pub fn replace_reference_candidates_for_file(
        &mut self,
        file_id: SourceFileId,
        candidates: impl IntoIterator<Item = ReferenceCandidate>,
    ) {
        self.assert_known_file_id(
            file_id,
            "reference candidate replacement references unknown source file id",
        );
        self.reference_candidate_store
            .replace_file(file_id, candidates);
        self.resolve_reference_candidates();
    }

    pub fn replace_file_reference_analysis(
        &mut self,
        file_id: SourceFileId,
        candidates: impl IntoIterator<Item = ReferenceCandidate>,
        diagnostic_candidates: impl IntoIterator<Item = DiagnosticCandidate>,
        diagnostics: impl IntoIterator<Item = DiagnosticFact>,
    ) {
        self.assert_known_file_id(
            file_id,
            "file reference analysis references unknown source file id",
        );
        self.reference_candidate_store
            .replace_file(file_id, candidates);
        self.diagnostic_candidate_store
            .replace_file(file_id, diagnostic_candidates);
        self.diagnostic_store.replace_file(file_id, diagnostics);
        self.resolve_reference_candidates();
    }

    pub fn replace_method_facts_for_file(
        &mut self,
        file_id: SourceFileId,
        facts: impl IntoIterator<Item = MethodFact>,
    ) {
        self.assert_known_file_id(
            file_id,
            "method fact replacement references unknown source file id",
        );
        self.method_store.replace_file(file_id, facts);
        self.resolve_reference_candidates();
    }

    pub fn replace_graph_facts_for_file(
        &mut self,
        file_id: SourceFileId,
        nodes: impl IntoIterator<Item = GraphNodeFact>,
        edges: impl IntoIterator<Item = GraphEdgeFact>,
    ) {
        self.assert_known_file_id(
            file_id,
            "graph fact replacement references unknown source file id",
        );
        self.graph_store.replace_file(file_id, nodes, edges);
        self.resolve_reference_candidates();
    }

    pub fn replace_graph_update_for_file(
        &mut self,
        file_id: SourceFileId,
        nodes: impl IntoIterator<Item = GraphNodeFact>,
        edges: impl IntoIterator<Item = GraphEdgeFact>,
        unresolved_edges: impl IntoIterator<Item = UnresolvedGraphEdgeFact>,
    ) {
        self.assert_known_file_id(
            file_id,
            "graph update replacement references unknown source file id",
        );
        self.graph_store.remove_file(file_id);
        self.unresolved_graph_edges
            .retain(|edge| edge.range.file_id != file_id);

        for node in nodes {
            assert!(
                node.range.file_id == file_id,
                "INVARIANT VIOLATED: replacement graph node belongs to a different file id. \
                 This is a bug because graph updates must only receive facts for the target file. \
                 Fix: partition graph facts by SourceFileId before replacing."
            );
            self.graph_store.add_node(node);
        }
        for edge in edges {
            assert!(
                edge.range.file_id == file_id,
                "INVARIANT VIOLATED: replacement graph edge belongs to a different file id. \
                 This is a bug because graph updates must only receive facts for the target file. \
                 Fix: partition graph facts by SourceFileId before replacing."
            );
            self.graph_store.add_edge(edge);
        }
        for edge in unresolved_edges {
            assert!(
                edge.range.file_id == file_id,
                "INVARIANT VIOLATED: unresolved graph edge belongs to a different file id. \
                 This is a bug because graph updates must only receive facts for the target file. \
                 Fix: partition graph facts by SourceFileId before replacing."
            );
            self.unresolved_graph_edges.push(edge);
        }
        self.retry_unresolved_graph_edges();
        self.resolve_reference_candidates();
    }

    pub fn replace_type_facts_for_file(
        &mut self,
        file_id: SourceFileId,
        facts: impl IntoIterator<Item = TypeFact>,
    ) {
        self.assert_known_file_id(
            file_id,
            "type fact replacement references unknown source file id",
        );
        self.type_store.replace_file(file_id, facts);
        self.resolve_reference_candidates();
    }

    pub fn replace_diagnostic_facts_for_file(
        &mut self,
        file_id: SourceFileId,
        facts: impl IntoIterator<Item = DiagnosticFact>,
    ) {
        self.assert_known_file_id(
            file_id,
            "diagnostic fact replacement references unknown source file id",
        );
        self.diagnostic_store.replace_file(file_id, facts);
    }

    pub fn replace_file_analysis(&mut self, file_id: SourceFileId, facts: FileAnalysisFacts) {
        self.assert_known_file_id(file_id, "file analysis references unknown source file id");
        self.symbol_store.replace_file(file_id, facts.symbols);
        self.method_store.replace_file(file_id, facts.methods);
        self.type_store.replace_file(file_id, facts.types);
        self.graph_store.remove_file(file_id);
        self.unresolved_graph_edges
            .retain(|edge| edge.range.file_id != file_id);

        for node in facts.graph_nodes {
            assert!(
                node.range.file_id == file_id,
                "INVARIANT VIOLATED: file analysis graph node belongs to a different file id. \
                 This is a bug because replace_file_analysis must only receive facts for one file. \
                 Fix: partition collected file facts before ingest."
            );
            self.graph_store.add_node(node);
        }
        for edge in facts.graph_edges {
            assert!(
                edge.range.file_id == file_id,
                "INVARIANT VIOLATED: file analysis graph edge belongs to a different file id. \
                 This is a bug because replace_file_analysis must only receive facts for one file. \
                 Fix: partition collected file facts before ingest."
            );
            self.graph_store.add_edge(edge);
        }
        for edge in facts.unresolved_graph_edges {
            assert!(
                edge.range.file_id == file_id,
                "INVARIANT VIOLATED: file analysis unresolved graph edge belongs to a different file id. \
                 This is a bug because replace_file_analysis must only receive facts for one file. \
                 Fix: partition collected file facts before ingest."
            );
            self.unresolved_graph_edges.push(edge);
        }

        self.reference_candidate_store
            .replace_file(file_id, facts.reference_candidates);
        self.diagnostic_candidate_store
            .replace_file(file_id, facts.diagnostic_candidates);
        self.diagnostic_store
            .replace_file(file_id, facts.diagnostics);
        self.retry_unresolved_graph_edges();
        self.resolve_reference_candidates();
    }

    pub fn type_at(
        &self,
        subject: &TypeSubject,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> TypeResolution {
        self.type_store.type_at(subject, file_id, byte_offset)
    }

    pub fn type_facts_for(&self, subject: &TypeSubject) -> &[TypeFact] {
        self.type_store.facts_for(subject)
    }

    pub fn symbol_facts_for(&self, fqn: &FullyQualifiedName) -> &[SymbolFact] {
        self.symbol_store.facts_for(fqn)
    }

    pub fn all_symbol_facts(&self) -> Vec<SymbolFact> {
        self.symbol_store.all_facts()
    }

    pub fn reference_facts_for(&self, target: &FullyQualifiedName) -> &[ReferenceFact] {
        self.reference_store.facts_for(target)
    }

    pub fn method_facts_for(&self, fqn: &FullyQualifiedName) -> &[MethodFact] {
        self.method_store.facts_for(fqn)
    }

    pub fn all_method_facts(&self) -> Vec<MethodFact> {
        self.method_store.all_facts()
    }

    pub fn graph_nodes_for(&self, fqn: &FullyQualifiedName) -> &[GraphNodeFact] {
        self.graph_store.nodes_for(fqn)
    }

    pub fn graph_edges_from(&self, source: &FullyQualifiedName) -> &[GraphEdgeFact] {
        self.graph_store.edges_from(source)
    }

    pub fn all_graph_edges(&self) -> Vec<GraphEdgeFact> {
        self.graph_store.all_edges()
    }

    pub fn diagnostic_facts_in_file(&self, file_id: SourceFileId) -> Vec<DiagnosticFact> {
        self.diagnostic_store.facts_in_file(file_id)
    }

    pub fn all_diagnostic_facts(&self) -> Vec<DiagnosticFact> {
        self.diagnostic_store.all_facts()
    }

    pub fn graph_store(&self) -> &GraphStore {
        &self.graph_store
    }

    pub fn unresolved_graph_edges(&self) -> &[UnresolvedGraphEdgeFact] {
        &self.unresolved_graph_edges
    }

    pub fn reference_store(&self) -> &ReferenceStore {
        &self.reference_store
    }

    pub fn method_store(&self) -> &MethodStore {
        &self.method_store
    }

    pub fn symbol_store(&self) -> &SymbolStore {
        &self.symbol_store
    }

    pub fn type_store(&self) -> &TypeStore {
        &self.type_store
    }

    pub fn diagnostic_store(&self) -> &DiagnosticStore {
        &self.diagnostic_store
    }

    pub fn reference_candidate_store(&self) -> &ReferenceCandidateStore {
        &self.reference_candidate_store
    }

    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    pub fn text_range(&self, file_id: SourceFileId, start_byte: u32, end_byte: u32) -> TextRange {
        self.assert_known_file_id(file_id, "TextRange requested for unknown source file id");
        TextRange::new(file_id, start_byte, end_byte)
    }

    fn assert_known_file_id(&self, file_id: SourceFileId, message: &str) {
        assert!(
            self.files.contains_key(&file_id),
            "INVARIANT VIOLATED: {message}. \
             This is a bug because analysis facts and ranges must only reference registered files. \
             Fix: call AnalysisEngine::open_or_update_file before adding file facts."
        );
    }

    fn retry_unresolved_graph_edges(&mut self) {
        if self.unresolved_graph_edges.is_empty() {
            return;
        }

        let pending = std::mem::take(&mut self.unresolved_graph_edges);
        for unresolved in pending {
            if let Some(target) = self.resolve_unresolved_graph_target(&unresolved) {
                self.graph_store.add_edge(GraphEdgeFact::new(
                    unresolved.source,
                    target,
                    unresolved.kind,
                    unresolved.range,
                ));
            } else {
                self.unresolved_graph_edges.push(unresolved);
            }
        }
    }

    fn resolve_unresolved_graph_target(
        &self,
        unresolved: &UnresolvedGraphEdgeFact,
    ) -> Option<FullyQualifiedName> {
        let mut search_namespaces = if unresolved.absolute {
            Vec::new()
        } else {
            unresolved.context.namespace_parts()
        };

        loop {
            let mut probe = search_namespaces.clone();
            probe.extend(unresolved.target_parts.iter().cloned());
            let namespace_fqn = FullyQualifiedName::namespace(probe);
            if !self.graph_store.nodes_for(&namespace_fqn).is_empty() {
                return Some(namespace_fqn);
            }

            if unresolved.absolute || search_namespaces.is_empty() {
                break;
            }
            search_namespaces.pop();
        }

        None
    }
}

#[cfg(test)]
#[path = "engine_tests.rs"]
mod tests;
