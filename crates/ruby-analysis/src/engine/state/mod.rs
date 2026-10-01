//! Project semantic state: source registration, file-owned facts, resolution
//! passes, and the engine-owned reads that queries build on.

mod facts;
mod files;
mod graph;
mod inference;
mod lifecycle;
mod names;
mod storage;

pub(in crate::engine) use facts::EffectiveMethodFactMatch;
pub use files::{SourceFile, SourceFileInput, SourceFileSnapshot};
pub(in crate::engine) use inference::{resolve_constant_dependency_type, TypeInferenceOutcomeRef};

use std::collections::HashMap;
use std::mem::size_of;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::core::storage::graph_store::SemanticGraph;
use crate::core::storage::memory_estimate::{fqn_heap_bytes, vec_payload_bytes};
use crate::core::{
    ExecutionContextFact, FullyQualifiedName, InferenceEvidence, MethodVisibilityOverrideFact,
    SourceFileId, TextRange,
};

use crate::engine::diagnostics::Diagnostics;
use crate::engine::AnalysisQuery;
use crate::stats::{self, StatsSnapshot};
use files::Files;
use inference::StoredTypeInferenceOutcome;
use names::Names;
use parking_lot::Mutex;
use storage::FactArena;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolveMode {
    Immediate,
    Deferred,
}

crate::stat_set! {
    /// Fact counts observed in one engine at snapshot time.
    pub enum AnalysisStat {
        Files = "files",
        SourceBytes = "source_bytes",
        Symbols = "symbols",
        Methods = "methods",
        ReferenceCandidates = "reference_candidates",
        ConstantReferenceCandidates = "constant_reference_candidates",
        MethodReferenceCandidates = "method_reference_candidates",
        ResolvedReferenceCandidates = "resolved_reference_candidates",
        References = "references",
        Types = "types",
        DiagnosticCandidates = "diagnostic_candidates",
        Diagnostics = "diagnostics",
        GraphNodes = "graph_nodes",
        GraphEdges = "graph_edges",
        UnresolvedGraphEdges = "unresolved_graph_edges",
    }
}

crate::stat_set! {
    /// Measurement counters for the most recent full `AnalysisEngine::resolve` pass.
    ///
    /// These are process-local profiler evidence only. They must not change semantic
    /// resolution policy, diagnostic emission, or project ownership.
    pub enum ResolveStat {
        MethodReturnEquationSolveRuns = "method_return_equation_solve_runs",
        GraphRetryNs = "graph_retry_ns",
        DiagnosticSeedNs = "diagnostic_seed_ns",
        ConstantCandidatesNs = "constant_candidates_ns",
        MethodCandidatesNs = "method_candidates_ns",
        SortAllNs = "sort_all_ns",
        DiagnosticRebuildNs = "diagnostic_rebuild_ns",
        ConstantCacheHits = "constant_cache_hits",
        ConstantCacheMisses = "constant_cache_misses",
        ConstantCacheUniqueKeys = "constant_cache_unique_keys",
        MethodCacheHits = "method_cache_hits",
        MethodCacheMisses = "method_cache_misses",
        MethodCacheUniqueKeys = "method_cache_unique_keys",
        MethodLookupChainCacheEntries = "method_lookup_chain_cache_entries",
        MethodNamespaceExistsCacheEntries = "method_namespace_exists_cache_entries",
        MethodSuggestionCacheEntries = "method_suggestion_cache_entries",
        IncompleteMethodChainCacheEntries = "incomplete_method_chain_cache_entries",
        /// Method candidates whose explicit receiver is another call expression
        /// and therefore must wait for an earlier call outcome in source order.
        DeferredReceiverCandidates = "deferred_receiver_candidates",
        /// Deferred receiver candidates whose inner call produced a concrete type.
        DeferredReceiverProven = "deferred_receiver_proven",
        /// Deferred receiver candidates whose inner call remained Unknown.
        DeferredReceiverUnknown = "deferred_receiver_unknown",
        MethodReturnCacheHits = "method_return_cache_hits",
        MethodReturnCacheMisses = "method_return_cache_misses",
        MethodReturnCacheEntries = "method_return_cache_entries",
        MethodVisibilityCacheHits = "method_visibility_cache_hits",
        MethodVisibilityCacheMisses = "method_visibility_cache_misses",
        MethodVisibilityCacheEntries = "method_visibility_cache_entries",
        AmbiguousMethodReturnCacheHits = "ambiguous_method_return_cache_hits",
        AmbiguousMethodReturnCacheMisses = "ambiguous_method_return_cache_misses",
        AmbiguousMethodReturnCacheEntries = "ambiguous_method_return_cache_entries",
    }
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
    pub(in crate::engine) files: Files,
    pub(in crate::engine) names: Names,
    pub(in crate::engine) facts: FactArena,
    pub(in crate::engine) graph: SemanticGraph,
    pub(in crate::engine) diagnostics: Diagnostics,
    pub(in crate::engine) method_visibility_overrides: Vec<MethodVisibilityOverrideFact>,
    pub(in crate::engine) execution_contexts: HashMap<SourceFileId, Vec<ExecutionContextFact>>,
    inference_by_file: HashMap<SourceFileId, InferenceEvidence>,
    call_expression_outcomes_by_file:
        HashMap<SourceFileId, Box<[(TextRange, StoredTypeInferenceOutcome)]>>,
    pub(in crate::engine) local_read_types_by_file:
        HashMap<SourceFileId, Box<[(TextRange, crate::core::storage::type_store::RubyTypeId)]>>,
    method_return_equations_dirty: bool,
    constant_type_equations_dirty: bool,
    method_return_solution_spans_files: bool,
    top_level_method_lookup_chain_cache: Mutex<Option<Vec<FullyQualifiedName>>>,
    universal_object_method_lookup_chain_cache: Mutex<Option<Vec<FullyQualifiedName>>>,
    last_resolve_pass: StatsSnapshot<ResolveStat>,
}

static NEXT_ANALYSIS_ENGINE_INSTANCE_ID: AtomicU64 = AtomicU64::new(1);

fn next_analysis_engine_instance_id() -> u64 {
    NEXT_ANALYSIS_ENGINE_INSTANCE_ID
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            current.checked_add(1)
        })
        .unwrap_or_else(|_| {
            unreachable_invariant!(
                what = "analysis engine instance identity exhausted u64",
                why = "query caches require a unique engine identity",
                fix = "widen the identity before creating u64::MAX engine instances",
            )
        })
}

impl Default for AnalysisEngine {
    fn default() -> Self {
        Self {
            instance_id: next_analysis_engine_instance_id(),
            semantic_revision: 0,
            files: Files::default(),
            names: Names::default(),
            facts: FactArena::default(),
            graph: SemanticGraph::default(),
            diagnostics: Diagnostics::default(),
            method_visibility_overrides: Vec::new(),
            execution_contexts: HashMap::new(),
            inference_by_file: HashMap::new(),
            call_expression_outcomes_by_file: HashMap::new(),
            local_read_types_by_file: HashMap::new(),
            method_return_equations_dirty: false,
            constant_type_equations_dirty: false,
            method_return_solution_spans_files: false,
            top_level_method_lookup_chain_cache: Mutex::new(None),
            universal_object_method_lookup_chain_cache: Mutex::new(None),
            last_resolve_pass: StatsSnapshot::default(),
        }
    }
}

impl Clone for AnalysisEngine {
    fn clone(&self) -> Self {
        Self {
            instance_id: next_analysis_engine_instance_id(),
            semantic_revision: self.semantic_revision,
            files: self.files.clone(),
            names: self.names.clone(),
            facts: self.facts.clone(),
            graph: self.graph.clone(),
            diagnostics: self.diagnostics.clone(),
            method_visibility_overrides: self.method_visibility_overrides.clone(),
            execution_contexts: self.execution_contexts.clone(),
            inference_by_file: self.inference_by_file.clone(),
            call_expression_outcomes_by_file: self.call_expression_outcomes_by_file.clone(),
            local_read_types_by_file: self.local_read_types_by_file.clone(),
            method_return_equations_dirty: self.method_return_equations_dirty,
            constant_type_equations_dirty: self.constant_type_equations_dirty,
            method_return_solution_spans_files: self.method_return_solution_spans_files,
            top_level_method_lookup_chain_cache: Mutex::new(None),
            universal_object_method_lookup_chain_cache: Mutex::new(None),
            last_resolve_pass: StatsSnapshot::default(),
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

    pub fn stats(&self) -> StatsSnapshot<AnalysisStat> {
        let reference_candidate_stats = self.facts.references.candidates.stats();
        let mut stats = StatsSnapshot::default();
        stats.set(AnalysisStat::Files, stats::count(self.files.len()));
        stats.set(
            AnalysisStat::SourceBytes,
            stats::count(self.files.source_bytes()),
        );
        stats.set(
            AnalysisStat::Symbols,
            stats::count(self.facts.definitions.symbols.fact_count()),
        );
        stats.set(
            AnalysisStat::Methods,
            stats::count(self.facts.definitions.methods.fact_count()),
        );
        stats.set(
            AnalysisStat::ReferenceCandidates,
            stats::count(self.facts.references.candidates.candidate_count()),
        );
        stats.set(
            AnalysisStat::ConstantReferenceCandidates,
            stats::count(reference_candidate_stats.constants),
        );
        stats.set(
            AnalysisStat::MethodReferenceCandidates,
            stats::count(reference_candidate_stats.methods),
        );
        stats.set(
            AnalysisStat::ResolvedReferenceCandidates,
            stats::count(reference_candidate_stats.resolved),
        );
        stats.set(
            AnalysisStat::References,
            stats::count(self.facts.references.resolved.fact_count()),
        );
        stats.set(
            AnalysisStat::Types,
            stats::count(self.facts.types.fact_count()),
        );
        stats.set(
            AnalysisStat::DiagnosticCandidates,
            stats::count(self.diagnostics.candidate_count()),
        );
        stats.set(
            AnalysisStat::Diagnostics,
            stats::count(self.diagnostics.fact_count()),
        );
        stats.set(
            AnalysisStat::GraphNodes,
            stats::count(self.graph.node_count()),
        );
        stats.set(
            AnalysisStat::GraphEdges,
            stats::count(self.graph.edge_count()),
        );
        stats.set(
            AnalysisStat::UnresolvedGraphEdges,
            stats::count(self.graph.unresolved_edges().len()),
        );
        stats
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
            diagnostics: self.diagnostics.resolved_heap_bytes(),
            diagnostic_candidates: self.diagnostics.candidates_heap_bytes(),
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
        self.files.estimated_heap_bytes()
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
