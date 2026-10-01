//! Project semantic state: source registration, file-owned facts, resolution
//! passes, and the engine-owned reads that queries build on.

pub(in crate::engine) mod external_facts_template;
mod facts;
pub(in crate::engine) mod file_id_map;
pub(in crate::engine) mod fingerprint;
mod graph;
mod inference;
mod lifecycle;
mod storage;

pub(in crate::engine) use facts::EffectiveMethodFactMatch;
pub(in crate::engine) use inference::{resolve_constant_dependency_type, TypeInferenceOutcomeRef};

use std::collections::HashMap;
use std::mem::size_of;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use crate::core::storage::memory_estimate::{fqn_heap_bytes, vec_payload_bytes};
use crate::core::{
    DiagnosticCandidate, DiagnosticFact, ExecutionContextFact, FullyQualifiedName, GraphEdgeFact,
    GraphNodeFact, InferenceEvidence, MethodFact, MethodVisibilityOverrideFact, ReferenceCandidate,
    RubyType, SemanticGraph, SourceFileId, SourceKind, SymbolFact, TextRange, TypeFact,
    UnresolvedGraphEdgeFact,
};

use crate::engine::AnalysisQuery;
use fingerprint::SemanticExportFingerprint;
use inference::StoredTypeInferenceOutcome;
use parking_lot::Mutex;
use storage::{FactArena, NameRegistry, SourceRegistry};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFile {
    pub id: SourceFileId,
    pub path: PathBuf,
    pub source: Option<String>,
    pub line_index: SourceLineIndex,
    pub content_hash: u64,
    pub kind: SourceKind,
    revision: u64,
    /// Present for `SourceKind::Gem` files bound from a locked package.
    pub library_package: Option<crate::core::LibraryPackageId>,
}

/// Opaque identity of one registered file snapshot in one engine.
///
/// Keeping the engine, file, and revision identities together prevents a fact
/// batch from accidentally validating against another file or cloned engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceFileSnapshot {
    engine_instance_id: u64,
    file_id: SourceFileId,
    revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SourceLineIndex {
    line_offsets: Vec<u32>,
    len: u32,
    ascii: bool,
}

impl SourceLineIndex {
    fn new(source: &str) -> Self {
        let len = u32::try_from(source.len()).expect(
            "INVARIANT VIOLATED: source file byte length exceeded u32. This is a bug because \
             every analysis TextRange and SourceFileId-relative byte offset is represented as \
             u32. Fix: reject or segment files larger than u32::MAX before registration.",
        );
        let mut line_offsets = vec![0];
        for (idx, byte) in source.bytes().enumerate() {
            if byte == b'\n' {
                line_offsets.push(u32::try_from(idx + 1).expect(
                    "INVARIANT VIOLATED: source line offset exceeded u32 after the complete \
                     source length fit u32. This is a bug because a position within a bounded \
                     source cannot exceed its length. Fix: keep source length validation before \
                     line-index construction.",
                ));
            }
        }
        if line_offsets.last() != Some(&len) {
            line_offsets.push(len);
        }
        Self {
            line_offsets,
            len,
            ascii: source.is_ascii(),
        }
    }

    pub fn line_offsets(&self) -> &[u32] {
        &self.line_offsets
    }

    pub fn len(&self) -> usize {
        self.len as usize
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
        let target = byte_offset;
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
            let target = target as usize;
            let line_start = line_start as usize;
            if !source.is_char_boundary(target) {
                return None;
            }
            source[line_start..target]
                .chars()
                .map(char::len_utf16)
                .sum::<usize>()
                .try_into()
                .ok()?
        };
        Some((
            u32::try_from(line_index).expect(
                "INVARIANT VIOLATED: source line index exceeded u32. \
                 This is a bug because LSP positions require u32 lines. \
                 Fix: reject or segment files with more than u32::MAX lines.",
            ),
            character,
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
    pub execution_contexts: Vec<ExecutionContextFact>,
    pub inference: InferenceEvidence,
    pub local_read_types: Box<[(TextRange, RubyType)]>,
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

/// Measurement counters for the most recent full `AnalysisEngine::resolve` pass.
///
/// These are process-local profiler evidence only. They must not change semantic
/// resolution policy, diagnostic emission, or project ownership.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResolvePassStats {
    pub method_return_equation_solve_runs: usize,
    pub graph_retry_ns: u64,
    pub diagnostic_seed_ns: u64,
    pub constant_candidates_ns: u64,
    pub method_candidates_ns: u64,
    pub sort_all_ns: u64,
    pub diagnostic_rebuild_ns: u64,
    pub constant_cache_hits: usize,
    pub constant_cache_misses: usize,
    pub constant_cache_unique_keys: usize,
    pub method_cache_hits: usize,
    pub method_cache_misses: usize,
    pub method_cache_unique_keys: usize,
    pub method_lookup_chain_cache_entries: usize,
    pub method_namespace_exists_cache_entries: usize,
    pub method_suggestion_cache_entries: usize,
    pub incomplete_method_chain_cache_entries: usize,
    /// Method candidates whose explicit receiver is another call expression
    /// and therefore must wait for an earlier call outcome in source order.
    pub deferred_receiver_candidates: usize,
    /// Deferred receiver candidates whose inner call produced a concrete type.
    pub deferred_receiver_proven: usize,
    /// Deferred receiver candidates whose inner call remained Unknown.
    pub deferred_receiver_unknown: usize,
    pub method_return_cache_hits: usize,
    pub method_return_cache_misses: usize,
    pub method_return_cache_entries: usize,
    pub method_visibility_cache_hits: usize,
    pub method_visibility_cache_misses: usize,
    pub method_visibility_cache_entries: usize,
    pub ambiguous_method_return_cache_hits: usize,
    pub ambiguous_method_return_cache_misses: usize,
    pub ambiguous_method_return_cache_entries: usize,
}

pub(in crate::engine) fn elapsed_ns(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_nanos()).unwrap_or_else(|_| {
        panic!(
            "INVARIANT VIOLATED: resolve-pass elapsed nanoseconds overflowed u64. \
             This is a bug because one resolution pass cannot exceed u64::MAX nanoseconds. \
             Fix: inspect hung resolve instrumentation or widen the counter type."
        )
    })
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
    pub query_caches: usize,
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
            + self.query_caches
    }
}

/// Shared analysis state for editor and agent consumers.
#[derive(Debug)]
pub struct AnalysisEngine {
    instance_id: u64,
    semantic_revision: u64,
    next_source_revision: u64,
    pub(in crate::engine) sources: SourceRegistry,
    pub(in crate::engine) names: NameRegistry,
    pub(in crate::engine) facts: FactArena,
    pub(in crate::engine) graph: SemanticGraph,
    pub(in crate::engine) method_visibility_overrides: Vec<MethodVisibilityOverrideFact>,
    execution_contexts: HashMap<SourceFileId, Vec<ExecutionContextFact>>,
    inference_by_file: HashMap<SourceFileId, InferenceEvidence>,
    call_expression_outcomes_by_file:
        HashMap<SourceFileId, Box<[(TextRange, StoredTypeInferenceOutcome)]>>,
    local_read_types_by_file:
        HashMap<SourceFileId, Box<[(TextRange, crate::core::storage::type_store::RubyTypeId)]>>,
    method_return_equations_dirty: bool,
    constant_type_equations_dirty: bool,
    method_return_solution_spans_files: bool,
    semantic_export_fingerprints: HashMap<SourceFileId, SemanticExportFingerprint>,
    top_level_method_lookup_chain_cache: Mutex<Option<Vec<FullyQualifiedName>>>,
    universal_object_method_lookup_chain_cache: Mutex<Option<Vec<FullyQualifiedName>>>,
    last_resolve_pass: ResolvePassStats,
}

static NEXT_ANALYSIS_ENGINE_INSTANCE_ID: AtomicU64 = AtomicU64::new(1);

fn next_analysis_engine_instance_id() -> u64 {
    NEXT_ANALYSIS_ENGINE_INSTANCE_ID
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            current.checked_add(1)
        })
        .unwrap_or_else(|_| {
            panic!(
                "INVARIANT VIOLATED: analysis engine instance identity exhausted u64. \
                 This is a bug because query caches require a unique engine identity. \
                 Fix: widen the identity before creating u64::MAX engine instances."
            )
        })
}

impl Default for AnalysisEngine {
    fn default() -> Self {
        Self {
            instance_id: next_analysis_engine_instance_id(),
            semantic_revision: 0,
            next_source_revision: 0,
            sources: SourceRegistry::default(),
            names: NameRegistry::default(),
            facts: FactArena::default(),
            graph: SemanticGraph::default(),
            method_visibility_overrides: Vec::new(),
            execution_contexts: HashMap::new(),
            inference_by_file: HashMap::new(),
            call_expression_outcomes_by_file: HashMap::new(),
            local_read_types_by_file: HashMap::new(),
            method_return_equations_dirty: false,
            constant_type_equations_dirty: false,
            method_return_solution_spans_files: false,
            semantic_export_fingerprints: HashMap::new(),
            top_level_method_lookup_chain_cache: Mutex::new(None),
            universal_object_method_lookup_chain_cache: Mutex::new(None),
            last_resolve_pass: ResolvePassStats::default(),
        }
    }
}

impl Clone for AnalysisEngine {
    fn clone(&self) -> Self {
        Self {
            instance_id: next_analysis_engine_instance_id(),
            semantic_revision: self.semantic_revision,
            next_source_revision: self.next_source_revision,
            sources: self.sources.clone(),
            names: self.names.clone(),
            facts: self.facts.clone(),
            graph: self.graph.clone(),
            method_visibility_overrides: self.method_visibility_overrides.clone(),
            execution_contexts: self.execution_contexts.clone(),
            inference_by_file: self.inference_by_file.clone(),
            call_expression_outcomes_by_file: self.call_expression_outcomes_by_file.clone(),
            local_read_types_by_file: self.local_read_types_by_file.clone(),
            method_return_equations_dirty: self.method_return_equations_dirty,
            constant_type_equations_dirty: self.constant_type_equations_dirty,
            method_return_solution_spans_files: self.method_return_solution_spans_files,
            semantic_export_fingerprints: self.semantic_export_fingerprints.clone(),
            top_level_method_lookup_chain_cache: Mutex::new(None),
            universal_object_method_lookup_chain_cache: Mutex::new(None),
            last_resolve_pass: ResolvePassStats::default(),
        }
    }
}

impl AnalysisEngine {
    pub fn new() -> Self {
        Self::default()
    }
}

impl AnalysisEngine {
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
            query_caches: self.estimated_method_lookup_chain_cache_heap_bytes(),
        }
    }

    fn estimated_method_lookup_chain_cache_heap_bytes(&self) -> usize {
        let chain_bytes = |chain: &Vec<FullyQualifiedName>| {
            vec_payload_bytes(chain) + chain.iter().map(fqn_heap_bytes).sum::<usize>()
        };
        self.top_level_method_lookup_chain_cache
            .lock()
            .as_ref()
            .map(chain_bytes)
            .unwrap_or(0)
            + self
                .universal_object_method_lookup_chain_cache
                .lock()
                .as_ref()
                .map(chain_bytes)
                .unwrap_or(0)
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
            + self.inference_by_file.capacity()
                * (size_of::<SourceFileId>() + size_of::<InferenceEvidence>() + 1)
            + self
                .inference_by_file
                .values()
                .map(InferenceEvidence::estimated_heap_bytes)
                .sum::<usize>()
            + self.call_expression_outcomes_by_file.capacity()
                * (size_of::<SourceFileId>()
                    + size_of::<Box<[(TextRange, StoredTypeInferenceOutcome)]>>()
                    + 1)
            + self
                .call_expression_outcomes_by_file
                .values()
                .map(|outcomes| {
                    outcomes.len() * size_of::<(TextRange, StoredTypeInferenceOutcome)>()
                })
                .sum::<usize>()
            + self.local_read_types_by_file.capacity()
                * (size_of::<SourceFileId>()
                    + size_of::<Box<[(TextRange, crate::core::storage::type_store::RubyTypeId)]>>()
                    + 1)
            + self
                .local_read_types_by_file
                .values()
                .map(|reads| {
                    reads.len()
                        * size_of::<(TextRange, crate::core::storage::type_store::RubyTypeId)>()
                })
                .sum::<usize>()
    }
}

#[cfg(test)]
mod tests;
