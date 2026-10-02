//! Project semantic state: source registration, file-owned facts, resolution
//! passes, and the engine-owned reads that queries build on.

mod decls;
mod files;
mod hierarchy;
mod lifecycle;
mod names;
mod solver;
mod types;
mod uses;

pub(in crate::engine) use decls::EffectiveMethodFactMatch;
pub use files::{SourceFile, SourceFileInput, SourceFileSnapshot};
pub(in crate::engine) use solver::resolve_constant_dependency_type;
pub(in crate::engine) use types::TypeInferenceOutcomeRef;

use std::sync::atomic::{AtomicU64, Ordering};

use crate::engine::diagnostics::Diagnostics;
use crate::engine::View;
use crate::stats::{self, StatsSnapshot};
use decls::DeclIndex;
use files::Files;
use hierarchy::Hierarchy;
use names::Names;
use solver::Solver;
use types::TypeTable;
use uses::UseIndex;

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
    /// Measurement counters for the most recent full `Project::resolve` pass.
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
    pub execution_contexts: usize,
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
            + self.execution_contexts
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
pub struct Project {
    instance_id: u64,
    semantic_revision: u64,
    pub(in crate::engine) files: Files,
    pub(in crate::engine) names: Names,
    pub(in crate::engine) types: TypeTable,
    pub(in crate::engine) hierarchy: Hierarchy,
    pub(in crate::engine) uses: UseIndex,
    pub(in crate::engine) diagnostics: Diagnostics,
    pub(in crate::engine) decls: DeclIndex,
    pub(in crate::engine) solver: Solver,
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

impl Default for Project {
    fn default() -> Self {
        Self {
            instance_id: next_analysis_engine_instance_id(),
            semantic_revision: 0,
            files: Files::default(),
            names: Names::default(),
            types: TypeTable::default(),
            hierarchy: Hierarchy::default(),
            uses: UseIndex::default(),
            diagnostics: Diagnostics::default(),
            decls: DeclIndex::default(),
            solver: Solver::default(),
            last_resolve_pass: StatsSnapshot::default(),
        }
    }
}

impl Clone for Project {
    fn clone(&self) -> Self {
        Self {
            instance_id: next_analysis_engine_instance_id(),
            semantic_revision: self.semantic_revision,
            files: self.files.clone(),
            names: self.names.clone(),
            types: self.types.clone(),
            hierarchy: self.hierarchy.clone(),
            uses: self.uses.clone(),
            diagnostics: self.diagnostics.clone(),
            decls: self.decls.clone(),
            solver: self.solver.clone(),
            last_resolve_pass: StatsSnapshot::default(),
        }
    }
}

impl Project {
    pub fn new() -> Self {
        Self::default()
    }

    /// The engine identity and semantic revision that key every query cache.
    pub(crate) fn query_cache_identity(&self) -> (u64, u64) {
        (self.instance_id, self.semantic_revision)
    }
}

impl Project {
    /// A read-only view of this project's current semantic state.
    pub fn view(&self) -> View<'_> {
        View::new(self)
    }

    pub fn shrink_to_fit(&mut self) {
        self.files.shrink_to_fit();
        self.names.shrink_to_fit();
        self.decls.shrink_to_fit();
        self.types.shrink_to_fit();
        self.hierarchy.shrink_to_fit();
        self.uses.shrink_to_fit();
        self.diagnostics.shrink_to_fit();
        self.solver.shrink_to_fit();
    }
}

impl<'a> View<'a> {
    pub fn stats(&self) -> StatsSnapshot<AnalysisStat> {
        let reference_candidate_stats = self.engine.uses.candidate_stats();
        let mut stats = StatsSnapshot::default();
        stats.set(AnalysisStat::Files, stats::count(self.engine.files.len()));
        stats.set(
            AnalysisStat::SourceBytes,
            stats::count(self.engine.files.source_bytes()),
        );
        stats.set(
            AnalysisStat::Symbols,
            stats::count(self.engine.decls.symbol_count()),
        );
        stats.set(
            AnalysisStat::Methods,
            stats::count(self.engine.decls.method_count()),
        );
        stats.set(
            AnalysisStat::ReferenceCandidates,
            stats::count(self.engine.uses.candidate_count()),
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
            stats::count(self.engine.uses.resolved_count()),
        );
        stats.set(
            AnalysisStat::Types,
            stats::count(self.engine.types.fact_count()),
        );
        stats.set(
            AnalysisStat::DiagnosticCandidates,
            stats::count(self.engine.diagnostics.candidate_count()),
        );
        stats.set(
            AnalysisStat::Diagnostics,
            stats::count(self.engine.diagnostics.fact_count()),
        );
        stats.set(
            AnalysisStat::GraphNodes,
            stats::count(self.engine.hierarchy.node_count()),
        );
        stats.set(
            AnalysisStat::GraphEdges,
            stats::count(self.engine.hierarchy.edge_count()),
        );
        stats.set(
            AnalysisStat::UnresolvedGraphEdges,
            stats::count(self.engine.hierarchy.unresolved_edge_count()),
        );
        stats
    }

    pub fn estimated_memory_stats(&self) -> AnalysisMemoryStats {
        AnalysisMemoryStats {
            names: self.engine.names.estimated_heap_bytes(),
            files: self.estimated_file_store_heap_bytes(),
            symbols: self.engine.decls.symbols_heap_bytes(),
            methods: self.engine.decls.methods_heap_bytes(),
            execution_contexts: self.engine.decls.execution_contexts_heap_bytes(),
            types: self.engine.types.facts_heap_bytes(),
            reference_candidates: self.engine.uses.candidates_heap_bytes(),
            references: self.engine.uses.resolved_heap_bytes(),
            diagnostics: self.engine.diagnostics.resolved_heap_bytes(),
            diagnostic_candidates: self.engine.diagnostics.candidates_heap_bytes(),
            graph: self.engine.hierarchy.graph_heap_bytes(),
            unresolved_graph_edges: self.engine.hierarchy.unresolved_heap_bytes(),
            query_caches: self.engine.hierarchy.method_lookup_chain_heap_bytes(),
        }
    }

    fn estimated_file_store_heap_bytes(&self) -> usize {
        self.engine.files.estimated_heap_bytes()
            + self.engine.solver.estimated_heap_bytes()
            + self.engine.types.file_outcomes_heap_bytes()
    }

    /// Profiler evidence for the most recent full `resolve()` pass.
    pub fn last_resolve_stats(&self) -> &'a StatsSnapshot<ResolveStat> {
        &self.engine.last_resolve_pass
    }
}

#[cfg(test)]
mod tests;
