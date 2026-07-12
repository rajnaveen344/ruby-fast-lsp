use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::mem::size_of;
use std::path::{Path, PathBuf};

use crate::core::memory_estimate::{fqn_heap_bytes, vec_payload_bytes};
use crate::core::{
    ConstLookup, ConstLookupId, ConstantPath, DiagnosticCandidate, DiagnosticCandidateStore,
    DiagnosticFact, DiagnosticStore, FqnId, FullyQualifiedName, GraphEdgeFact, GraphNodeFact,
    MethodFact, MethodStore, MethodVisibilityOverrideFact, ReferenceCandidate,
    ReferenceCandidateKind, ReferenceCandidateStore, ReferenceFact, ReferenceStore, RubyConstant,
    SemanticGraph, SourceFileId, SourceKind, StoredGraphEdgeFact, StoredGraphNodeFact,
    StoredMethodFact, StoredReferenceCandidate, StoredSymbolFact, StoredUnresolvedGraphEdgeFact,
    SymbolFact, SymbolKind, SymbolStore, TextRange, TypeFact, TypeResolution, TypeStore,
    TypeSubject, UnresolvedGraphEdgeFact,
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
            source[line_start..target]
                .chars()
                .map(char::len_utf16)
                .sum()
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
    pub method_visibility_overrides: Vec<MethodVisibilityOverrideFact>,
    pub types: Vec<TypeFact>,
    pub graph_nodes: Vec<GraphNodeFact>,
    pub graph_edges: Vec<GraphEdgeFact>,
    pub unresolved_graph_edges: Vec<UnresolvedGraphEdgeFact>,
    pub reference_candidates: Vec<ReferenceCandidate>,
    pub diagnostic_candidates: Vec<DiagnosticCandidate>,
    pub diagnostics: Vec<DiagnosticFact>,
}

impl SemanticExportFingerprint {
    fn from_facts(facts: &FileFacts) -> Self {
        let mut exports = Vec::new();

        for fact in &facts.symbols {
            if matches!(
                fact.kind,
                SymbolKind::Class | SymbolKind::Module | SymbolKind::Constant
            ) {
                exports.push(export_hash(|hasher| {
                    1u8.hash(hasher);
                    fact.fqn.hash(hasher);
                    fact.kind.hash(hasher);
                }));
            }
        }
        for fact in &facts.methods {
            exports.push(export_hash(|hasher| {
                2u8.hash(hasher);
                fact.fqn.hash(hasher);
                fact.owner.hash(hasher);
                fact.params.hash(hasher);
                fact.param_facts.len().hash(hasher);
                for parameter in &fact.param_facts {
                    parameter.name.hash(hasher);
                    parameter.kind.hash(hasher);
                    parameter.type_label.hash(hasher);
                    parameter.documentation.hash(hasher);
                }
                fact.delegate_receiver.hash(hasher);
                fact.visibility.hash(hasher);
                fact.documentation.hash(hasher);
                fact.return_type_label.hash(hasher);
            }));
        }
        for fact in &facts.method_visibility_overrides {
            exports.push(export_hash(|hasher| {
                3u8.hash(hasher);
                fact.owner.hash(hasher);
                fact.method.hash(hasher);
                fact.visibility.hash(hasher);
            }));
        }
        for fact in &facts.types {
            if matches!(
                fact.subject,
                TypeSubject::Constant(_)
                    | TypeSubject::MethodReturn(_)
                    | TypeSubject::Parameter { .. }
            ) {
                exports.push(export_hash(|hasher| {
                    4u8.hash(hasher);
                    fact.subject.hash(hasher);
                    fact.ruby_type.hash(hasher);
                    fact.provenance.hash(hasher);
                }));
            }
        }
        for fact in &facts.graph_nodes {
            exports.push(export_hash(|hasher| {
                5u8.hash(hasher);
                fact.fqn.hash(hasher);
                fact.kind.hash(hasher);
            }));
        }
        for fact in &facts.graph_edges {
            exports.push(export_hash(|hasher| {
                6u8.hash(hasher);
                fact.source.hash(hasher);
                fact.target.hash(hasher);
                fact.kind.hash(hasher);
            }));
        }
        for fact in &facts.unresolved_graph_edges {
            exports.push(export_hash(|hasher| {
                7u8.hash(hasher);
                fact.source.hash(hasher);
                fact.target_parts.hash(hasher);
                fact.absolute.hash(hasher);
                fact.context.hash(hasher);
                fact.kind.hash(hasher);
            }));
        }

        exports.sort_unstable_by_key(|fingerprint| (fingerprint.high, fingerprint.low));
        export_hash(|hasher| {
            exports.len().hash(hasher);
            for fingerprint in &exports {
                fingerprint.high.hash(hasher);
                fingerprint.low.hash(hasher);
            }
        })
    }
}

impl SemanticChange {
    pub fn classify(
        previous: Option<SemanticExportFingerprint>,
        current: SemanticExportFingerprint,
    ) -> Self {
        match previous {
            None => Self::InitialIndex,
            Some(previous) if previous == current => Self::BodyOnly,
            Some(_) => Self::ExportsChanged,
        }
    }
}

fn export_hash(mut hash_fields: impl FnMut(&mut StableExportHasher)) -> SemanticExportFingerprint {
    let mut high = StableExportHasher::new(0xcbf2_9ce4_8422_2325);
    hash_fields(&mut high);
    let mut low = StableExportHasher::new(0x8422_2325_cbf2_9ce4);
    hash_fields(&mut low);
    SemanticExportFingerprint {
        high: high.finish(),
        low: low.finish(),
    }
}

struct StableExportHasher {
    state: u64,
}

impl StableExportHasher {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }
}

impl Hasher for StableExportHasher {
    fn finish(&self) -> u64 {
        self.state
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.state ^= u64::from(*byte);
            self.state = self.state.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemanticExportFingerprint {
    high: u64,
    low: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticChange {
    InitialIndex,
    BodyOnly,
    ExportsChanged,
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
pub(super) struct SourceRegistry {
    pub(super) ids: FileIdMap,
    pub(super) files: HashMap<SourceFileId, SourceFile>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct NameRegistry {
    state: NameRegistryState,
}

impl NameRegistry {
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

    pub(super) fn intern_const_lookup(&mut self, lookup: ConstLookup) -> ConstLookupId {
        let state = &mut self.state;
        if let Some(id) = state.by_const_lookup.get(&lookup) {
            return *id;
        }
        let id = ConstLookupId(u32::try_from(state.const_lookups.len()).expect(
            "INVARIANT VIOLATED: constant lookup interner exceeded u32 ids. \
                 This is a bug because ConstLookupId stores u32. \
                 Fix: widen ConstLookupId before interning more than u32::MAX lookups.",
        ));
        state.const_lookups.push(lookup.clone());
        state.by_const_lookup.insert(lookup, id);
        id
    }

    pub(super) fn const_lookup(&self, id: ConstLookupId) -> Option<&ConstLookup> {
        self.state.const_lookups.get(id.0 as usize)
    }

    fn estimated_heap_bytes(&self) -> usize {
        let state = &self.state;
        state.by_fqn.capacity() * (size_of::<FullyQualifiedName>() + size_of::<FqnId>() + 1)
            + vec_payload_bytes(&state.fqns)
            + state.fqns.iter().map(fqn_heap_bytes).sum::<usize>()
            + state.by_const_lookup.capacity()
                * (size_of::<ConstLookup>() + size_of::<ConstLookupId>() + 1)
            + vec_payload_bytes(&state.const_lookups)
            + state
                .const_lookups
                .iter()
                .map(const_lookup_heap_bytes)
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

fn const_lookup_heap_bytes(lookup: &ConstLookup) -> usize {
    constant_path_heap_bytes(&lookup.path)
}

#[derive(Debug, Clone, Default)]
struct NameRegistryState {
    by_fqn: HashMap<FullyQualifiedName, FqnId>,
    fqns: Vec<FullyQualifiedName>,
    by_const_lookup: HashMap<ConstLookup, ConstLookupId>,
    const_lookups: Vec<ConstLookup>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct FactArena {
    pub(super) definitions: DefinitionFacts,
    pub(super) references: ReferenceFacts,
    pub(super) types: TypeStore,
    pub(super) diagnostics: DiagnosticFacts,
}

#[derive(Debug, Clone, Default)]
pub(super) struct DefinitionFacts {
    pub(super) symbols: SymbolStore,
    pub(super) methods: MethodStore,
}

#[derive(Debug, Clone, Default)]
pub(super) struct ReferenceFacts {
    pub(super) candidates: ReferenceCandidateStore,
    pub(super) resolved: ReferenceStore,
}

#[derive(Debug, Clone, Default)]
pub(super) struct DiagnosticFacts {
    pub(super) candidates: DiagnosticCandidateStore,
    pub(super) resolved: DiagnosticStore,
}

/// Shared analysis state for editor and agent consumers.
#[derive(Debug, Clone, Default)]
pub struct AnalysisEngine {
    pub(super) sources: SourceRegistry,
    pub(super) names: NameRegistry,
    pub(super) facts: FactArena,
    pub(super) graph: SemanticGraph,
    pub(super) method_visibility_overrides: Vec<MethodVisibilityOverrideFact>,
    semantic_export_fingerprints: HashMap<SourceFileId, SemanticExportFingerprint>,
}

impl AnalysisEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_file(&mut self, file: SourceFileInput) -> SourceFileId {
        let id = self.sources.ids.get_or_insert(&file.path);
        let line_index = SourceLineIndex::new(&file.content);
        let content_hash = source_hash(&file.content);
        let source = if line_index.is_ascii() {
            None
        } else {
            Some(file.content)
        };
        self.sources.files.insert(
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

    pub fn replace_facts(
        &mut self,
        file_id: SourceFileId,
        facts: FileFacts,
        mode: ResolveMode,
    ) -> SemanticChange {
        let fingerprint = SemanticExportFingerprint::from_facts(&facts);
        let previous = self
            .semantic_export_fingerprints
            .insert(file_id, fingerprint);
        let change = SemanticChange::classify(previous, fingerprint);
        self.replace_facts_deferred(file_id, facts);
        match mode {
            ResolveMode::Immediate => self.resolve(),
            ResolveMode::Deferred => {}
        }
        change
    }

    pub fn resolve(&mut self) {
        self.retry_unresolved_graph_edges();
        self.resolve_reference_candidates();
    }

    pub fn resolve_file(&mut self, file_id: SourceFileId) {
        self.assert_known_file_id(
            file_id,
            "file-local resolve references unknown source file id",
        );
        self.retry_unresolved_graph_edges();
        self.resolve_reference_candidates_in_file(file_id);
    }

    pub fn shrink_to_fit(&mut self) {
        self.sources.ids.shrink_to_fit();
        self.sources.files.shrink_to_fit();
        for file in self.sources.files.values_mut() {
            file.path.shrink_to_fit();
            if let Some(source) = &mut file.source {
                source.shrink_to_fit();
            }
            file.line_index.shrink_to_fit();
        }

        self.names.state.by_fqn.shrink_to_fit();
        self.names.state.fqns.shrink_to_fit();
        self.names.state.by_const_lookup.shrink_to_fit();
        self.names.state.const_lookups.shrink_to_fit();

        self.facts.definitions.symbols.shrink_to_fit();
        self.facts.definitions.methods.shrink_to_fit();
        self.facts.types.shrink_to_fit();
        self.graph.shrink_to_fit();
        self.facts.references.candidates.shrink_to_fit();
        self.facts.references.resolved.shrink_to_fit();
        self.facts.diagnostics.candidates.shrink_to_fit();
        self.facts.diagnostics.resolved.shrink_to_fit();
    }

    pub fn query(&self) -> AnalysisQuery<'_> {
        AnalysisQuery::new(self)
    }

    pub fn stats(&self) -> AnalysisStats {
        let reference_candidate_stats = self.facts.references.candidates.stats();
        AnalysisStats {
            files: self.sources.files.len(),
            source_bytes: self
                .sources
                .files
                .values()
                .map(|file| file.line_index.len())
                .sum(),
            symbols: self.facts.definitions.symbols.fact_count(),
            methods: self.facts.definitions.methods.fact_count(),
            reference_candidates: self.facts.references.candidates.candidate_count(),
            constant_reference_candidates: reference_candidate_stats.constants,
            method_reference_candidates: reference_candidate_stats.methods,
            resolved_reference_candidates: reference_candidate_stats.resolved,
            references: self.facts.references.resolved.fact_count(),
            types: self.facts.types.fact_count(),
            diagnostic_candidates: self.facts.diagnostics.candidates.candidate_count(),
            diagnostics: self.facts.diagnostics.resolved.fact_count(),
            graph_nodes: self.graph.node_count(),
            graph_edges: self.graph.edge_count(),
            unresolved_graph_edges: self.graph.unresolved_edges().len(),
        }
    }

    pub fn estimated_memory_stats(&self) -> AnalysisMemoryStats {
        AnalysisMemoryStats {
            names: self.names.estimated_heap_bytes(),
            files: self.estimated_file_store_heap_bytes(),
            symbols: self.facts.definitions.symbols.estimated_heap_bytes(),
            methods: self.facts.definitions.methods.estimated_heap_bytes(),
            types: self.facts.types.estimated_heap_bytes(),
            reference_candidates: self.facts.references.candidates.estimated_heap_bytes(),
            references: self.facts.references.resolved.estimated_heap_bytes(),
            diagnostics: self.facts.diagnostics.resolved.estimated_heap_bytes(),
            diagnostic_candidates: self.facts.diagnostics.candidates.estimated_heap_bytes(),
            graph: self.graph.estimated_heap_bytes(),
            unresolved_graph_edges: self.graph.estimated_unresolved_heap_bytes(),
        }
    }

    fn estimated_file_store_heap_bytes(&self) -> usize {
        self.sources.ids.estimated_heap_bytes()
            + self.sources.files.capacity()
                * (size_of::<SourceFileId>() + size_of::<SourceFile>() + 1)
            + self
                .sources
                .files
                .values()
                .map(|file| {
                    file.path.as_os_str().len()
                        + file.source.as_ref().map(String::capacity).unwrap_or(0)
                        + vec_payload_bytes(&file.line_index.line_offsets)
                })
                .sum::<usize>()
            + self.semantic_export_fingerprints.capacity()
                * (size_of::<SourceFileId>() + size_of::<SemanticExportFingerprint>() + 1)
    }

    pub fn file_id(&self, path: impl AsRef<Path>) -> Option<SourceFileId> {
        self.sources.ids.get(path)
    }

    pub fn file(&self, id: SourceFileId) -> Option<&SourceFile> {
        self.sources.files.get(&id)
    }

    pub fn semantic_export_fingerprint(
        &self,
        file_id: SourceFileId,
    ) -> Option<SemanticExportFingerprint> {
        self.semantic_export_fingerprints.get(&file_id).copied()
    }

    pub fn files(&self) -> impl Iterator<Item = &SourceFile> {
        self.sources.files.values()
    }

    fn replace_facts_deferred(&mut self, file_id: SourceFileId, facts: FileFacts) {
        self.assert_known_file_id(file_id, "file analysis references unknown source file id");
        let symbols = self.intern_symbol_facts(facts.symbols);
        self.facts
            .definitions
            .symbols
            .replace_file(file_id, symbols);
        let methods = self.intern_method_facts(facts.methods);
        self.facts
            .definitions
            .methods
            .replace_file(file_id, methods);
        self.method_visibility_overrides
            .retain(|fact| fact.range.file_id != file_id);
        self.method_visibility_overrides
            .extend(facts.method_visibility_overrides);
        self.facts.types.replace_file(file_id, facts.types);
        let graph_nodes = self.intern_graph_node_facts(facts.graph_nodes);
        let graph_edges = self.intern_graph_edge_facts(facts.graph_edges);
        let unresolved_graph_edges =
            self.intern_unresolved_graph_edge_facts(facts.unresolved_graph_edges);
        self.graph
            .replace_file(file_id, graph_nodes, graph_edges, unresolved_graph_edges);

        let reference_candidates = self.intern_reference_candidates(facts.reference_candidates);
        self.facts
            .references
            .candidates
            .replace_file(file_id, reference_candidates);
        self.facts
            .diagnostics
            .candidates
            .replace_file(file_id, facts.diagnostic_candidates);
        self.facts
            .diagnostics
            .resolved
            .replace_file(file_id, facts.diagnostics);
    }

    pub fn type_at(
        &self,
        subject: &TypeSubject,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> TypeResolution {
        self.facts.types.type_at(subject, file_id, byte_offset)
    }

    pub fn type_facts_for(&self, subject: &TypeSubject) -> Vec<TypeFact> {
        self.facts.types.facts_for(subject)
    }

    pub fn symbol_facts_for(&self, fqn: &FullyQualifiedName) -> Vec<SymbolFact> {
        let Some(fqn_id) = self.names.fqn_id(fqn) else {
            return Vec::new();
        };
        self.facts
            .definitions
            .symbols
            .facts_for(fqn_id)
            .into_iter()
            .map(|fact| self.expand_symbol_fact(fact))
            .collect()
    }

    pub fn all_symbol_facts(&self) -> Vec<SymbolFact> {
        self.facts
            .definitions
            .symbols
            .all_facts()
            .into_iter()
            .map(|fact| self.expand_symbol_fact(fact))
            .collect()
    }

    pub fn symbol_facts_in_file(&self, file_id: SourceFileId) -> Vec<SymbolFact> {
        self.facts
            .definitions
            .symbols
            .facts_in_file(file_id)
            .into_iter()
            .map(|fact| self.expand_symbol_fact(fact))
            .collect()
    }

    pub fn reference_facts_for(&self, target: &FullyQualifiedName) -> &[ReferenceFact] {
        let Some(target_id) = self.names.fqn_id(target) else {
            return &[];
        };
        self.facts.references.resolved.facts_for(target_id)
    }

    pub fn fqn_for_id(&self, id: FqnId) -> Option<&FullyQualifiedName> {
        self.names.fqn(id)
    }

    pub fn method_facts_for(&self, fqn: &FullyQualifiedName) -> Vec<MethodFact> {
        let Some(fqn_id) = self.names.fqn_id(fqn) else {
            return Vec::new();
        };
        self.facts
            .definitions
            .methods
            .facts_for(fqn_id)
            .into_iter()
            .map(|fact| self.expand_method_fact(fact))
            .collect()
    }

    pub fn all_method_facts(&self) -> Vec<MethodFact> {
        self.facts
            .definitions
            .methods
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
            .definitions
            .methods
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
            .definitions
            .methods
            .facts_matching_owner_name(owner_id, method)
            .into_iter()
            .map(|fact| self.expand_method_fact(fact))
            .collect()
    }

    pub fn method_visibility_overrides_matching_owner_name(
        &self,
        owner: &FullyQualifiedName,
        method: &crate::core::RubyMethod,
    ) -> Vec<MethodVisibilityOverrideFact> {
        self.method_visibility_overrides
            .iter()
            .filter(|fact| {
                fact.method == *method
                    && fact.owner.namespace_parts() == owner.namespace_parts()
                    && fact.owner.namespace_kind() == owner.namespace_kind()
            })
            .cloned()
            .collect()
    }

    pub fn method_visibility_overrides_in_file(
        &self,
        file_id: SourceFileId,
    ) -> Vec<MethodVisibilityOverrideFact> {
        self.method_visibility_overrides
            .iter()
            .filter(|fact| fact.range.file_id == file_id)
            .cloned()
            .collect()
    }

    pub fn all_method_visibility_overrides(&self) -> Vec<MethodVisibilityOverrideFact> {
        self.method_visibility_overrides.clone()
    }

    pub fn method_names_for_owner(&self, owner: &FullyQualifiedName) -> Vec<&'static str> {
        let Some(owner_id) = self.names.fqn_id(owner) else {
            return Vec::new();
        };
        self.facts
            .definitions
            .methods
            .method_names_for_owner(owner_id)
    }

    pub fn method_facts_in_file(&self, file_id: SourceFileId) -> Vec<MethodFact> {
        self.facts
            .definitions
            .methods
            .facts_in_file(file_id)
            .into_iter()
            .map(|fact| self.expand_method_fact(fact))
            .collect()
    }

    pub fn graph_nodes_for(&self, fqn: &FullyQualifiedName) -> Vec<GraphNodeFact> {
        let Some(fqn_id) = self.names.fqn_id(fqn) else {
            return Vec::new();
        };
        self.graph
            .nodes_for(fqn_id)
            .into_iter()
            .map(|fact| self.expand_graph_node_fact(fact))
            .collect()
    }

    pub fn graph_edges_from(&self, source: &FullyQualifiedName) -> Vec<GraphEdgeFact> {
        let Some(source_id) = self.names.fqn_id(source) else {
            return Vec::new();
        };
        self.graph
            .edges_from(source_id)
            .into_iter()
            .map(|fact| self.expand_graph_edge_fact(fact))
            .collect()
    }

    pub fn graph_edges_to(&self, target: &FullyQualifiedName) -> Vec<GraphEdgeFact> {
        let Some(target_id) = self.names.fqn_id(target) else {
            return Vec::new();
        };
        self.graph
            .edges_to(target_id)
            .into_iter()
            .map(|fact| self.expand_graph_edge_fact(fact))
            .collect()
    }

    pub fn graph_nodes_in_file(&self, file_id: SourceFileId) -> Vec<GraphNodeFact> {
        self.graph
            .nodes_in_file(file_id)
            .into_iter()
            .map(|fact| self.expand_graph_node_fact(fact))
            .collect()
    }

    pub fn graph_edges_in_file(&self, file_id: SourceFileId) -> Vec<GraphEdgeFact> {
        self.graph
            .edges_in_file(file_id)
            .into_iter()
            .map(|fact| self.expand_graph_edge_fact(fact))
            .collect()
    }

    pub fn all_graph_nodes(&self) -> Vec<GraphNodeFact> {
        self.graph
            .all_nodes()
            .into_iter()
            .map(|fact| self.expand_graph_node_fact(fact))
            .collect()
    }

    pub fn all_graph_edges(&self) -> Vec<GraphEdgeFact> {
        self.graph
            .all_edges()
            .into_iter()
            .map(|fact| self.expand_graph_edge_fact(fact))
            .collect()
    }

    pub fn diagnostic_facts_in_file(&self, file_id: SourceFileId) -> Vec<DiagnosticFact> {
        self.facts.diagnostics.resolved.facts_in_file(file_id)
    }

    pub fn all_diagnostic_facts(&self) -> Vec<DiagnosticFact> {
        self.facts.diagnostics.resolved.all_facts()
    }

    pub fn unresolved_graph_edges(&self) -> Vec<UnresolvedGraphEdgeFact> {
        self.graph
            .unresolved_edges()
            .into_iter()
            .map(|edge| self.expand_unresolved_graph_edge_fact(edge))
            .collect()
    }

    pub fn reference_store(&self) -> &ReferenceStore {
        &self.facts.references.resolved
    }

    pub fn method_store(&self) -> &MethodStore {
        &self.facts.definitions.methods
    }

    pub fn symbol_store(&self) -> &SymbolStore {
        &self.facts.definitions.symbols
    }

    pub fn type_store(&self) -> &TypeStore {
        &self.facts.types
    }

    pub fn diagnostic_store(&self) -> &DiagnosticStore {
        &self.facts.diagnostics.resolved
    }

    pub fn reference_candidate_store(&self) -> &ReferenceCandidateStore {
        &self.facts.references.candidates
    }

    pub fn file_count(&self) -> usize {
        self.sources.files.len()
    }

    pub fn text_range(&self, file_id: SourceFileId, start_byte: u32, end_byte: u32) -> TextRange {
        self.assert_known_file_id(file_id, "TextRange requested for unknown source file id");
        TextRange::new(file_id, start_byte, end_byte)
    }

    fn assert_known_file_id(&self, file_id: SourceFileId, message: &str) {
        assert!(
            self.sources.files.contains_key(&file_id),
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
                    let context = self
                        .names
                        .intern_fqn(FullyQualifiedName::namespace(current_namespace));
                    let lookup = self
                        .names
                        .intern_const_lookup(ConstLookup::new(parts, false, context));
                    StoredReferenceCandidate::constant(candidate.range, lookup)
                }
                ReferenceCandidateKind::Method {
                    owner,
                    owner_kind,
                    method,
                    is_super,
                    access,
                    caller,
                    diagnostics,
                } => {
                    let root = self
                        .names
                        .intern_fqn(FullyQualifiedName::namespace(Vec::new()));
                    let owner = self
                        .names
                        .intern_const_lookup(ConstLookup::new(owner, true, root));
                    let caller = caller.map(|caller| self.names.intern_fqn(caller));
                    StoredReferenceCandidate::method(
                        candidate.range,
                        owner,
                        owner_kind,
                        method,
                        is_super,
                        access,
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
                StoredSymbolFact::new(fqn, fact.kind, fact.range).with_name_range(fact.name_range)
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
                    delegate_receiver: fact.delegate_receiver,
                    visibility: fact.visibility,
                    documentation: fact.documentation,
                    return_type_label: fact.return_type_label,
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

    fn intern_unresolved_graph_edge_facts(
        &mut self,
        facts: Vec<UnresolvedGraphEdgeFact>,
    ) -> Vec<StoredUnresolvedGraphEdgeFact> {
        facts
            .into_iter()
            .map(|fact| {
                let source = self.names.intern_fqn(fact.source);
                let context = self.names.intern_fqn(fact.context);
                let target = self.names.intern_const_lookup(ConstLookup::new(
                    ConstantPath::from_vec(fact.target_parts),
                    fact.absolute,
                    context,
                ));
                StoredUnresolvedGraphEdgeFact::new(source, target, fact.kind, fact.range)
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
        SymbolFact::new(fqn, fact.kind, fact.range).with_name_range(fact.name_range)
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
            delegate_receiver: fact.delegate_receiver,
            visibility: fact.visibility,
            documentation: fact.documentation,
            return_type_label: fact.return_type_label,
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

    fn expand_unresolved_graph_edge_fact(
        &self,
        fact: StoredUnresolvedGraphEdgeFact,
    ) -> UnresolvedGraphEdgeFact {
        let source = self
            .names
            .fqn(fact.source)
            .expect(
                "INVARIANT VIOLATED: unresolved graph edge points to missing source FQN id. \
                 This is a bug because unresolved graph edges must only store interned source FQN ids. \
                 Fix: intern unresolved graph edge sources before inserting facts.",
            )
            .clone();
        let lookup = self.names.const_lookup(fact.target).expect(
            "INVARIANT VIOLATED: unresolved graph edge points to missing constant lookup id. \
             This is a bug because unresolved graph edges must only store interned constant lookup ids. \
             Fix: intern unresolved graph edge targets before inserting facts.",
        );
        let context = self
            .names
            .fqn(lookup.context)
            .expect(
                "INVARIANT VIOLATED: unresolved graph edge lookup points to missing context FQN id. \
                 This is a bug because constant lookups must only store interned context FQN ids. \
                 Fix: intern constant lookup contexts before inserting facts.",
            )
            .clone();
        UnresolvedGraphEdgeFact::new(
            source,
            lookup.path.to_vec(),
            lookup.absolute,
            context,
            fact.kind,
            fact.range,
        )
    }

    fn retry_unresolved_graph_edges(&mut self) {
        if self.graph.unresolved_edges().is_empty() {
            return;
        }

        let pending = self.graph.take_unresolved_edges();
        for unresolved in pending {
            if let Some(target) = self.resolve_unresolved_graph_target(&unresolved) {
                let target = self.names.intern_fqn(target);
                self.graph.add_edge(StoredGraphEdgeFact::new(
                    unresolved.source,
                    target,
                    unresolved.kind,
                    unresolved.range,
                ));
            } else {
                self.graph.add_unresolved_edge(unresolved);
            }
        }
    }

    fn resolve_unresolved_graph_target(
        &self,
        unresolved: &StoredUnresolvedGraphEdgeFact,
    ) -> Option<FullyQualifiedName> {
        let lookup = self.names.const_lookup(unresolved.target).expect(
            "INVARIANT VIOLATED: unresolved graph edge points to missing constant lookup id. \
             This is a bug because unresolved graph edges must only store interned constant lookup ids. \
             Fix: intern unresolved graph edge targets before inserting facts.",
        );
        let context = self.names.fqn(lookup.context).expect(
            "INVARIANT VIOLATED: unresolved graph edge lookup points to missing context FQN id. \
             This is a bug because constant lookups must only store interned context FQN ids. \
             Fix: intern constant lookup contexts before inserting facts.",
        );
        let mut search_namespaces = if lookup.absolute {
            Vec::new()
        } else {
            context.namespace_parts()
        };

        loop {
            let mut probe = search_namespaces.clone();
            probe.extend(lookup.path.iter().cloned());
            let namespace_fqn = FullyQualifiedName::namespace(probe);
            if !self.graph_nodes_for(&namespace_fqn).is_empty() {
                return Some(namespace_fqn);
            }

            if lookup.absolute || search_namespaces.is_empty() {
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

#[cfg(test)]
#[path = "engine_tests.rs"]
mod tests;
