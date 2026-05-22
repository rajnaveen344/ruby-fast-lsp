use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::mem::size_of;
use std::path::{Path, PathBuf};

use crate::core::memory_estimate::{fqn_heap_bytes, vec_payload_bytes};
use crate::core::{
    ConstantPath, ConstantPathId, DiagnosticCandidate, DiagnosticCandidateStore, DiagnosticFact,
    DiagnosticStore, FqnId, FullyQualifiedName, GraphEdgeFact, GraphNodeFact, GraphStore,
    MethodFact, MethodStore, ReferenceCandidate, ReferenceCandidateKind, ReferenceCandidateStore,
    ReferenceFact, ReferenceStore, RubyConstant, SourceFileId, SourceKind, StoredGraphEdgeFact,
    StoredGraphNodeFact, StoredMethodFact, StoredReferenceCandidate, StoredSymbolFact, SymbolFact,
    SymbolStore, TextRange, TypeFact, TypeResolution, TypeStore, TypeSubject,
    UnresolvedGraphEdgeFact,
};

use crate::engine::AnalysisQuery;
use crate::FileIdMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFile {
    pub id: SourceFileId,
    pub path: PathBuf,
    pub source: Option<String>,
    pub line_index: SourceLineIndex,
    pub content_hash: u64,
    pub kind: SourceKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SourceLineIndex {
    line_offsets: Vec<usize>,
    len: usize,
    ascii: bool,
}

impl SourceLineIndex {
    fn new(source: &str) -> Self {
        let mut line_offsets = vec![0];
        for (idx, byte) in source.bytes().enumerate() {
            if byte == b'\n' {
                line_offsets.push(idx + 1);
            }
        }
        if line_offsets.last() != Some(&source.len()) {
            line_offsets.push(source.len());
        }
        Self {
            line_offsets,
            len: source.len(),
            ascii: source.is_ascii(),
        }
    }

    pub fn line_offsets(&self) -> &[usize] {
        &self.line_offsets
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_ascii(&self) -> bool {
        self.ascii
    }

    fn shrink_to_fit(&mut self) {
        self.line_offsets.shrink_to_fit();
    }
}

impl SourceFile {
    pub fn source_text(&self) -> Option<&str> {
        self.source.as_deref()
    }

    pub fn byte_offset_to_line_character(&self, byte_offset: u32) -> Option<(u32, u32)> {
        let target = usize::try_from(byte_offset).ok()?;
        if target > self.line_index.len {
            return None;
        }
        let line_index = match self.line_index.line_offsets.binary_search(&target) {
            Ok(exact) => exact,
            Err(after) => after.saturating_sub(1),
        };
        let line_start = *self.line_index.line_offsets.get(line_index)?;
        let character = if self.line_index.ascii {
            target.checked_sub(line_start)?
        } else {
            let source = self.source.as_deref()?;
            if !source.is_char_boundary(target) {
                return None;
            }
            source[line_start..target].chars().count()
        };
        Some((
            u32::try_from(line_index).expect(
                "INVARIANT VIOLATED: source line index exceeded u32. \
                 This is a bug because LSP positions require u32 lines. \
                 Fix: reject or segment files with more than u32::MAX lines.",
            ),
            u32::try_from(character).expect(
                "INVARIANT VIOLATED: source character offset exceeded u32. \
                 This is a bug because LSP positions require u32 columns. \
                 Fix: reject or segment lines longer than u32::MAX characters.",
            ),
        ))
    }
}

#[derive(Debug, Clone, Default)]
pub struct FileFacts {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFileInput {
    pub path: PathBuf,
    pub content: String,
    pub kind: SourceKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolveMode {
    Immediate,
    Deferred,
}

#[derive(Debug, Clone, Default)]
pub struct AnalysisStats {
    pub files: usize,
    pub source_bytes: usize,
    pub symbols: usize,
    pub methods: usize,
    pub reference_candidates: usize,
    pub constant_reference_candidates: usize,
    pub method_reference_candidates: usize,
    pub resolved_reference_candidates: usize,
    pub references: usize,
    pub types: usize,
    pub diagnostic_candidates: usize,
    pub diagnostics: usize,
    pub graph_nodes: usize,
    pub graph_edges: usize,
    pub unresolved_graph_edges: usize,
}

#[derive(Debug, Clone, Default)]
pub struct AnalysisMemoryStats {
    pub names: usize,
    pub files: usize,
    pub symbols: usize,
    pub methods: usize,
    pub types: usize,
    pub reference_candidates: usize,
    pub references: usize,
    pub diagnostics: usize,
    pub diagnostic_candidates: usize,
    pub graph: usize,
    pub unresolved_graph_edges: usize,
}

impl AnalysisMemoryStats {
    pub fn total(&self) -> usize {
        self.files
            + self.names
            + self.symbols
            + self.methods
            + self.types
            + self.reference_candidates
            + self.references
            + self.diagnostics
            + self.diagnostic_candidates
            + self.graph
            + self.unresolved_graph_edges
    }
}

#[derive(Debug, Clone, Default)]
pub(super) struct FileStore {
    pub(super) ids: FileIdMap,
    pub(super) files: HashMap<SourceFileId, SourceFile>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct NameInterner {
    state: NameInternerState,
}

impl NameInterner {
    pub(super) fn intern_fqn(&mut self, fqn: FullyQualifiedName) -> FqnId {
        let state = &mut self.state;
        if let Some(id) = state.by_fqn.get(&fqn) {
            return *id;
        }
        let id = FqnId(u32::try_from(state.fqns.len()).expect(
            "INVARIANT VIOLATED: FQN interner exceeded u32 ids. \
                 This is a bug because FqnId stores u32. \
                 Fix: widen FqnId before interning more than u32::MAX names.",
        ));
        state.fqns.push(fqn.clone());
        state.by_fqn.insert(fqn, id);
        id
    }

    pub(super) fn fqn_id(&self, fqn: &FullyQualifiedName) -> Option<FqnId> {
        self.state.by_fqn.get(fqn).copied()
    }

    pub(super) fn fqn(&self, id: FqnId) -> Option<&FullyQualifiedName> {
        self.state.fqns.get(id.0 as usize)
    }

    pub(super) fn intern_constant_path(&mut self, path: ConstantPath) -> ConstantPathId {
        let state = &mut self.state;
        if let Some(id) = state.by_constant_path.get(&path) {
            return *id;
        }
        let id = ConstantPathId(u32::try_from(state.constant_paths.len()).expect(
            "INVARIANT VIOLATED: constant path interner exceeded u32 ids. \
                 This is a bug because ConstantPathId stores u32. \
                 Fix: widen ConstantPathId before interning more than u32::MAX paths.",
        ));
        state.constant_paths.push(path.clone());
        state.by_constant_path.insert(path, id);
        id
    }

    pub(super) fn constant_path(&self, id: ConstantPathId) -> Option<&ConstantPath> {
        self.state.constant_paths.get(id.0 as usize)
    }

    fn estimated_heap_bytes(&self) -> usize {
        let state = &self.state;
        state.by_fqn.capacity() * (size_of::<FullyQualifiedName>() + size_of::<FqnId>() + 1)
            + vec_payload_bytes(&state.fqns)
            + state.fqns.iter().map(fqn_heap_bytes).sum::<usize>()
            + state.by_constant_path.capacity()
                * (size_of::<ConstantPath>() + size_of::<ConstantPathId>() + 1)
            + vec_payload_bytes(&state.constant_paths)
            + state
                .constant_paths
                .iter()
                .map(constant_path_heap_bytes)
                .sum::<usize>()
    }
}

fn constant_path_heap_bytes(path: &ConstantPath) -> usize {
    if path.spilled() {
        path.capacity() * size_of::<RubyConstant>()
    } else {
        0
    }
}

#[derive(Debug, Clone, Default)]
struct NameInternerState {
    by_fqn: HashMap<FullyQualifiedName, FqnId>,
    fqns: Vec<FullyQualifiedName>,
    by_constant_path: HashMap<ConstantPath, ConstantPathId>,
    constant_paths: Vec<ConstantPath>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct FactArena {
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

#[derive(Debug, Clone, Default)]
pub(super) struct Indexes {}

#[derive(Debug, Clone, Default)]
pub(super) struct QueryCaches {
    pub(super) mro_by_namespace: RefCell<HashMap<FullyQualifiedName, Vec<FullyQualifiedName>>>,
    pub(super) namespace_exists: RefCell<HashMap<FullyQualifiedName, bool>>,
    pub(super) module_includers: RefCell<HashMap<FullyQualifiedName, Vec<FullyQualifiedName>>>,
    pub(super) descendants: RefCell<HashMap<FullyQualifiedName, Vec<FullyQualifiedName>>>,
}

impl QueryCaches {
    fn clear(&self) {
        self.mro_by_namespace.borrow_mut().clear();
        self.namespace_exists.borrow_mut().clear();
        self.module_includers.borrow_mut().clear();
        self.descendants.borrow_mut().clear();
    }
}

/// Shared analysis state for editor and agent consumers.
#[derive(Debug, Clone, Default)]
pub struct AnalysisEngine {
    pub(super) files: FileStore,
    #[allow(dead_code)]
    pub(super) names: NameInterner,
    pub(super) facts: FactArena,
    #[allow(dead_code)]
    pub(super) indexes: Indexes,
    #[allow(dead_code)]
    pub(super) caches: QueryCaches,
}

impl AnalysisEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_file(&mut self, file: SourceFileInput) -> SourceFileId {
        let id = self.files.ids.get_or_insert(&file.path);
        let line_index = SourceLineIndex::new(&file.content);
        let content_hash = source_hash(&file.content);
        let source = if line_index.is_ascii() {
            None
        } else {
            Some(file.content)
        };
        self.files.files.insert(
            id,
            SourceFile {
                id,
                path: file.path.components().collect(),
                source,
                line_index,
                content_hash,
                kind: file.kind,
            },
        );
        id
    }

    pub fn replace_facts(&mut self, file_id: SourceFileId, facts: FileFacts, mode: ResolveMode) {
        self.replace_facts_deferred(file_id, facts);
        match mode {
            ResolveMode::Immediate => self.resolve(),
            ResolveMode::Deferred => {}
        }
    }

    pub fn resolve(&mut self) {
        self.retry_unresolved_graph_edges();
        self.resolve_reference_candidates();
    }

    pub fn shrink_to_fit(&mut self) {
        self.files.ids.shrink_to_fit();
        self.files.files.shrink_to_fit();
        for file in self.files.files.values_mut() {
            file.path.shrink_to_fit();
            if let Some(source) = &mut file.source {
                source.shrink_to_fit();
            }
            file.line_index.shrink_to_fit();
        }

        self.names.state.by_fqn.shrink_to_fit();
        self.names.state.fqns.shrink_to_fit();
        self.names.state.by_constant_path.shrink_to_fit();
        self.names.state.constant_paths.shrink_to_fit();

        self.facts.symbol_store.shrink_to_fit();
        self.facts.method_store.shrink_to_fit();
        self.facts.type_store.shrink_to_fit();
        self.facts.graph_store.shrink_to_fit();
        self.facts.reference_candidate_store.shrink_to_fit();
        self.facts.reference_store.shrink_to_fit();
        self.facts.diagnostic_candidate_store.shrink_to_fit();
        self.facts.diagnostic_store.shrink_to_fit();
        self.facts.unresolved_graph_edges.shrink_to_fit();
    }

    pub fn query(&self) -> AnalysisQuery<'_> {
        AnalysisQuery::new(self)
    }

    pub fn stats(&self) -> AnalysisStats {
        let reference_candidate_stats = self.facts.reference_candidate_store.stats();
        AnalysisStats {
            files: self.files.files.len(),
            source_bytes: self
                .files
                .files
                .values()
                .map(|file| file.line_index.len())
                .sum(),
            symbols: self.facts.symbol_store.fact_count(),
            methods: self.facts.method_store.fact_count(),
            reference_candidates: self.facts.reference_candidate_store.candidate_count(),
            constant_reference_candidates: reference_candidate_stats.constants,
            method_reference_candidates: reference_candidate_stats.methods,
            resolved_reference_candidates: reference_candidate_stats.resolved,
            references: self.facts.reference_store.fact_count(),
            types: self.facts.type_store.fact_count(),
            diagnostic_candidates: self.facts.diagnostic_candidate_store.candidate_count(),
            diagnostics: self.facts.diagnostic_store.fact_count(),
            graph_nodes: self.facts.graph_store.node_count(),
            graph_edges: self.facts.graph_store.edge_count(),
            unresolved_graph_edges: self.facts.unresolved_graph_edges.len(),
        }
    }

    pub fn estimated_memory_stats(&self) -> AnalysisMemoryStats {
        AnalysisMemoryStats {
            names: self.names.estimated_heap_bytes(),
            files: self.estimated_file_store_heap_bytes(),
            symbols: self.facts.symbol_store.estimated_heap_bytes(),
            methods: self.facts.method_store.estimated_heap_bytes(),
            types: self.facts.type_store.estimated_heap_bytes(),
            reference_candidates: self.facts.reference_candidate_store.estimated_heap_bytes(),
            references: self.facts.reference_store.estimated_heap_bytes(),
            diagnostics: self.facts.diagnostic_store.estimated_heap_bytes(),
            diagnostic_candidates: self.facts.diagnostic_candidate_store.estimated_heap_bytes(),
            graph: self.facts.graph_store.estimated_heap_bytes(),
            unresolved_graph_edges: vec_payload_bytes(&self.facts.unresolved_graph_edges)
                + self
                    .facts
                    .unresolved_graph_edges
                    .iter()
                    .map(unresolved_graph_edge_heap_bytes)
                    .sum::<usize>(),
        }
    }

    fn estimated_file_store_heap_bytes(&self) -> usize {
        self.files.ids.estimated_heap_bytes()
            + self.files.files.capacity()
                * (size_of::<SourceFileId>() + size_of::<SourceFile>() + 1)
            + self
                .files
                .files
                .values()
                .map(|file| {
                    file.path.as_os_str().len()
                        + file.source.as_ref().map(String::capacity).unwrap_or(0)
                        + vec_payload_bytes(&file.line_index.line_offsets)
                })
                .sum::<usize>()
    }

    pub fn file_id(&self, path: impl AsRef<Path>) -> Option<SourceFileId> {
        self.files.ids.get(path)
    }

    pub fn file(&self, id: SourceFileId) -> Option<&SourceFile> {
        self.files.files.get(&id)
    }

    fn replace_facts_deferred(&mut self, file_id: SourceFileId, facts: FileFacts) {
        self.assert_known_file_id(file_id, "file analysis references unknown source file id");
        self.caches.clear();
        let symbols = self.intern_symbol_facts(facts.symbols);
        self.facts.symbol_store.replace_file(file_id, symbols);
        let methods = self.intern_method_facts(facts.methods);
        self.facts.method_store.replace_file(file_id, methods);
        self.facts.type_store.replace_file(file_id, facts.types);
        self.facts.graph_store.remove_file(file_id);
        self.facts
            .unresolved_graph_edges
            .retain(|edge| edge.range.file_id != file_id);

        for node in self.intern_graph_node_facts(facts.graph_nodes) {
            assert!(
                node.range.file_id == file_id,
                "INVARIANT VIOLATED: file analysis graph node belongs to a different file id. \
                 This is a bug because replace_file_analysis must only receive facts for one file. \
                 Fix: partition collected file facts before ingest."
            );
            self.facts.graph_store.add_node(node);
        }
        for edge in self.intern_graph_edge_facts(facts.graph_edges) {
            assert!(
                edge.range.file_id == file_id,
                "INVARIANT VIOLATED: file analysis graph edge belongs to a different file id. \
                 This is a bug because replace_file_analysis must only receive facts for one file. \
                 Fix: partition collected file facts before ingest."
            );
            self.facts.graph_store.add_edge(edge);
        }
        for edge in facts.unresolved_graph_edges {
            assert!(
                edge.range.file_id == file_id,
                "INVARIANT VIOLATED: file analysis unresolved graph edge belongs to a different file id. \
                 This is a bug because replace_file_analysis must only receive facts for one file. \
                 Fix: partition collected file facts before ingest."
            );
            self.facts.unresolved_graph_edges.push(edge);
        }

        let reference_candidates = self.intern_reference_candidates(facts.reference_candidates);
        self.facts
            .reference_candidate_store
            .replace_file(file_id, reference_candidates);
        self.facts
            .diagnostic_candidate_store
            .replace_file(file_id, facts.diagnostic_candidates);
        self.facts
            .diagnostic_store
            .replace_file(file_id, facts.diagnostics);
    }

    pub fn type_at(
        &self,
        subject: &TypeSubject,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> TypeResolution {
        self.facts.type_store.type_at(subject, file_id, byte_offset)
    }

    pub fn type_facts_for(&self, subject: &TypeSubject) -> Vec<TypeFact> {
        self.facts.type_store.facts_for(subject)
    }

    pub fn symbol_facts_for(&self, fqn: &FullyQualifiedName) -> Vec<SymbolFact> {
        let Some(fqn_id) = self.names.fqn_id(fqn) else {
            return Vec::new();
        };
        self.facts
            .symbol_store
            .facts_for(fqn_id)
            .into_iter()
            .map(|fact| self.expand_symbol_fact(fact))
            .collect()
    }

    pub fn all_symbol_facts(&self) -> Vec<SymbolFact> {
        self.facts
            .symbol_store
            .all_facts()
            .into_iter()
            .map(|fact| self.expand_symbol_fact(fact))
            .collect()
    }

    pub fn symbol_facts_in_file(&self, file_id: SourceFileId) -> Vec<SymbolFact> {
        self.facts
            .symbol_store
            .facts_in_file(file_id)
            .into_iter()
            .map(|fact| self.expand_symbol_fact(fact))
            .collect()
    }

    pub fn reference_facts_for(&self, target: &FullyQualifiedName) -> &[ReferenceFact] {
        let Some(target_id) = self.names.fqn_id(target) else {
            return &[];
        };
        self.facts.reference_store.facts_for(target_id)
    }

    pub fn fqn_for_id(&self, id: FqnId) -> Option<&FullyQualifiedName> {
        self.names.fqn(id)
    }

    pub fn method_facts_for(&self, fqn: &FullyQualifiedName) -> Vec<MethodFact> {
        let Some(fqn_id) = self.names.fqn_id(fqn) else {
            return Vec::new();
        };
        self.facts
            .method_store
            .facts_for(fqn_id)
            .into_iter()
            .map(|fact| self.expand_method_fact(fact))
            .collect()
    }

    pub fn all_method_facts(&self) -> Vec<MethodFact> {
        self.facts
            .method_store
            .all_facts()
            .into_iter()
            .map(|fact| self.expand_method_fact(fact))
            .collect()
    }

    pub fn method_facts_matching_owner(
        &self,
        owner: &FullyQualifiedName,
        partial: &str,
    ) -> Vec<MethodFact> {
        let Some(owner_id) = self.names.fqn_id(owner) else {
            return Vec::new();
        };
        self.facts
            .method_store
            .facts_matching_owner(owner_id, partial)
            .into_iter()
            .map(|fact| self.expand_method_fact(fact))
            .collect()
    }

    pub fn method_facts_matching_owner_name(
        &self,
        owner: &FullyQualifiedName,
        method: &crate::core::RubyMethod,
    ) -> Vec<MethodFact> {
        let Some(owner_id) = self.names.fqn_id(owner) else {
            return Vec::new();
        };
        self.facts
            .method_store
            .facts_matching_owner_name(owner_id, method)
            .into_iter()
            .map(|fact| self.expand_method_fact(fact))
            .collect()
    }

    pub fn method_names_for_owner(&self, owner: &FullyQualifiedName) -> Vec<&'static str> {
        let Some(owner_id) = self.names.fqn_id(owner) else {
            return Vec::new();
        };
        self.facts.method_store.method_names_for_owner(owner_id)
    }

    pub fn method_facts_in_file(&self, file_id: SourceFileId) -> Vec<MethodFact> {
        self.facts
            .method_store
            .facts_in_file(file_id)
            .into_iter()
            .map(|fact| self.expand_method_fact(fact))
            .collect()
    }

    pub fn graph_nodes_for(&self, fqn: &FullyQualifiedName) -> Vec<GraphNodeFact> {
        let Some(fqn_id) = self.names.fqn_id(fqn) else {
            return Vec::new();
        };
        self.facts
            .graph_store
            .nodes_for(fqn_id)
            .into_iter()
            .map(|fact| self.expand_graph_node_fact(fact))
            .collect()
    }

    pub fn graph_edges_from(&self, source: &FullyQualifiedName) -> Vec<GraphEdgeFact> {
        let Some(source_id) = self.names.fqn_id(source) else {
            return Vec::new();
        };
        self.facts
            .graph_store
            .edges_from(source_id)
            .into_iter()
            .map(|fact| self.expand_graph_edge_fact(fact))
            .collect()
    }

    pub fn graph_edges_to(&self, target: &FullyQualifiedName) -> Vec<GraphEdgeFact> {
        let Some(target_id) = self.names.fqn_id(target) else {
            return Vec::new();
        };
        self.facts
            .graph_store
            .edges_to(target_id)
            .into_iter()
            .map(|fact| self.expand_graph_edge_fact(fact))
            .collect()
    }

    pub fn graph_nodes_in_file(&self, file_id: SourceFileId) -> Vec<GraphNodeFact> {
        self.facts
            .graph_store
            .nodes_in_file(file_id)
            .into_iter()
            .map(|fact| self.expand_graph_node_fact(fact))
            .collect()
    }

    pub fn graph_edges_in_file(&self, file_id: SourceFileId) -> Vec<GraphEdgeFact> {
        self.facts
            .graph_store
            .edges_in_file(file_id)
            .into_iter()
            .map(|fact| self.expand_graph_edge_fact(fact))
            .collect()
    }

    pub fn all_graph_nodes(&self) -> Vec<GraphNodeFact> {
        self.facts
            .graph_store
            .all_nodes()
            .into_iter()
            .map(|fact| self.expand_graph_node_fact(fact))
            .collect()
    }

    pub fn all_graph_edges(&self) -> Vec<GraphEdgeFact> {
        self.facts
            .graph_store
            .all_edges()
            .into_iter()
            .map(|fact| self.expand_graph_edge_fact(fact))
            .collect()
    }

    pub fn diagnostic_facts_in_file(&self, file_id: SourceFileId) -> Vec<DiagnosticFact> {
        self.facts.diagnostic_store.facts_in_file(file_id)
    }

    pub fn all_diagnostic_facts(&self) -> Vec<DiagnosticFact> {
        self.facts.diagnostic_store.all_facts()
    }

    pub fn graph_store(&self) -> &GraphStore {
        &self.facts.graph_store
    }

    pub fn unresolved_graph_edges(&self) -> &[UnresolvedGraphEdgeFact] {
        &self.facts.unresolved_graph_edges
    }

    pub fn reference_store(&self) -> &ReferenceStore {
        &self.facts.reference_store
    }

    pub fn method_store(&self) -> &MethodStore {
        &self.facts.method_store
    }

    pub fn symbol_store(&self) -> &SymbolStore {
        &self.facts.symbol_store
    }

    pub fn type_store(&self) -> &TypeStore {
        &self.facts.type_store
    }

    pub fn diagnostic_store(&self) -> &DiagnosticStore {
        &self.facts.diagnostic_store
    }

    pub fn reference_candidate_store(&self) -> &ReferenceCandidateStore {
        &self.facts.reference_candidate_store
    }

    pub fn file_count(&self) -> usize {
        self.files.files.len()
    }

    pub fn text_range(&self, file_id: SourceFileId, start_byte: u32, end_byte: u32) -> TextRange {
        self.assert_known_file_id(file_id, "TextRange requested for unknown source file id");
        TextRange::new(file_id, start_byte, end_byte)
    }

    fn assert_known_file_id(&self, file_id: SourceFileId, message: &str) {
        assert!(
            self.files.files.contains_key(&file_id),
            "INVARIANT VIOLATED: {message}. \
             This is a bug because analysis facts and ranges must only reference registered files. \
             Fix: call AnalysisEngine::register_file before adding file facts."
        );
    }

    fn intern_reference_candidates(
        &mut self,
        candidates: Vec<ReferenceCandidate>,
    ) -> Vec<StoredReferenceCandidate> {
        candidates
            .into_iter()
            .map(|candidate| match candidate.kind {
                ReferenceCandidateKind::Constant {
                    parts,
                    current_namespace,
                } => {
                    let parts = self.names.intern_constant_path(parts);
                    let current_namespace = self.names.intern_constant_path(current_namespace);
                    StoredReferenceCandidate::constant(candidate.range, parts, current_namespace)
                }
                ReferenceCandidateKind::Method {
                    owner,
                    owner_kind,
                    method,
                    caller,
                    diagnostics,
                } => {
                    let owner = self.names.intern_constant_path(owner);
                    let caller = caller.map(|caller| self.names.intern_fqn(caller));
                    StoredReferenceCandidate::method(
                        candidate.range,
                        owner,
                        owner_kind,
                        method,
                        caller,
                        diagnostics,
                    )
                }
                ReferenceCandidateKind::Resolved { target, caller } => {
                    let target = self.names.intern_fqn(target);
                    let caller = caller.map(|caller| self.names.intern_fqn(caller));
                    StoredReferenceCandidate::resolved(candidate.range, target, caller)
                }
            })
            .collect()
    }

    fn intern_symbol_facts(&mut self, facts: Vec<SymbolFact>) -> Vec<StoredSymbolFact> {
        facts
            .into_iter()
            .map(|fact| {
                let fqn = self.names.intern_fqn(fact.fqn);
                StoredSymbolFact::new(fqn, fact.kind, fact.range)
            })
            .collect()
    }

    fn intern_method_facts(&mut self, facts: Vec<MethodFact>) -> Vec<StoredMethodFact> {
        facts
            .into_iter()
            .map(|fact| {
                let method = match &fact.fqn {
                    FullyQualifiedName::Method(_, method) => Some(*method),
                    FullyQualifiedName::Namespace(_, _)
                    | FullyQualifiedName::Constant(_)
                    | FullyQualifiedName::LocalVariable(_)
                    | FullyQualifiedName::InstanceVariable(_)
                    | FullyQualifiedName::ClassVariable(_)
                    | FullyQualifiedName::GlobalVariable(_) => None,
                };
                let fqn = self.names.intern_fqn(fact.fqn);
                let owner = self.names.intern_fqn(fact.owner);
                StoredMethodFact {
                    fqn,
                    owner,
                    method,
                    range: fact.range,
                    params: fact.params,
                    param_facts: fact.param_facts,
                }
            })
            .collect()
    }

    fn intern_graph_node_facts(&mut self, facts: Vec<GraphNodeFact>) -> Vec<StoredGraphNodeFact> {
        facts
            .into_iter()
            .map(|fact| {
                let fqn = self.names.intern_fqn(fact.fqn);
                StoredGraphNodeFact::new(fqn, fact.kind, fact.range)
            })
            .collect()
    }

    fn intern_graph_edge_facts(&mut self, facts: Vec<GraphEdgeFact>) -> Vec<StoredGraphEdgeFact> {
        facts
            .into_iter()
            .map(|fact| {
                let source = self.names.intern_fqn(fact.source);
                let target = self.names.intern_fqn(fact.target);
                StoredGraphEdgeFact::new(source, target, fact.kind, fact.range)
            })
            .collect()
    }

    fn expand_symbol_fact(&self, fact: StoredSymbolFact) -> SymbolFact {
        let fqn = self
            .names
            .fqn(fact.fqn)
            .expect(
                "INVARIANT VIOLATED: symbol fact points to missing FQN id. \
                 This is a bug because symbol facts must only store interned FQN ids. \
                 Fix: intern symbol FQNs before inserting facts.",
            )
            .clone();
        SymbolFact::new(fqn, fact.kind, fact.range)
    }

    fn expand_method_fact(&self, fact: StoredMethodFact) -> MethodFact {
        let fqn = self
            .names
            .fqn(fact.fqn)
            .expect(
                "INVARIANT VIOLATED: method fact points to missing FQN id. \
                 This is a bug because method facts must only store interned FQN ids. \
                 Fix: intern method FQNs before inserting facts.",
            )
            .clone();
        let owner = self
            .names
            .fqn(fact.owner)
            .expect(
                "INVARIANT VIOLATED: method fact points to missing owner FQN id. \
                 This is a bug because method facts must only store interned owner FQN ids. \
                 Fix: intern method owners before inserting facts.",
            )
            .clone();
        MethodFact {
            fqn,
            owner,
            range: fact.range,
            params: fact.params,
            param_facts: fact.param_facts,
        }
    }

    fn expand_graph_node_fact(&self, fact: StoredGraphNodeFact) -> GraphNodeFact {
        let fqn = self
            .names
            .fqn(fact.fqn)
            .expect(
                "INVARIANT VIOLATED: graph node points to missing FQN id. \
                 This is a bug because graph nodes must only store interned FQN ids. \
                 Fix: intern graph node FQNs before inserting facts.",
            )
            .clone();
        GraphNodeFact::new(fqn, fact.kind, fact.range)
    }

    fn expand_graph_edge_fact(&self, fact: StoredGraphEdgeFact) -> GraphEdgeFact {
        let source = self
            .names
            .fqn(fact.source)
            .expect(
                "INVARIANT VIOLATED: graph edge points to missing source FQN id. \
                 This is a bug because graph edges must only store interned source FQN ids. \
                 Fix: intern graph edge source FQNs before inserting facts.",
            )
            .clone();
        let target = self
            .names
            .fqn(fact.target)
            .expect(
                "INVARIANT VIOLATED: graph edge points to missing target FQN id. \
                 This is a bug because graph edges must only store interned target FQN ids. \
                 Fix: intern graph edge target FQNs before inserting facts.",
            )
            .clone();
        GraphEdgeFact::new(source, target, fact.kind, fact.range)
    }

    fn retry_unresolved_graph_edges(&mut self) {
        if self.facts.unresolved_graph_edges.is_empty() {
            return;
        }

        let pending = std::mem::take(&mut self.facts.unresolved_graph_edges);
        for unresolved in pending {
            if let Some(target) = self.resolve_unresolved_graph_target(&unresolved) {
                let source = self.names.intern_fqn(unresolved.source);
                let target = self.names.intern_fqn(target);
                self.facts.graph_store.add_edge(StoredGraphEdgeFact::new(
                    source,
                    target,
                    unresolved.kind,
                    unresolved.range,
                ));
            } else {
                self.facts.unresolved_graph_edges.push(unresolved);
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
            if !self.graph_nodes_for(&namespace_fqn).is_empty() {
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

fn source_hash(source: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut hasher);
    hasher.finish()
}

fn unresolved_graph_edge_heap_bytes(edge: &UnresolvedGraphEdgeFact) -> usize {
    fqn_heap_bytes(&edge.source)
        + vec_payload_bytes(&edge.target_parts)
        + fqn_heap_bytes(&edge.context)
}

#[cfg(test)]
#[path = "engine_tests.rs"]
mod tests;
