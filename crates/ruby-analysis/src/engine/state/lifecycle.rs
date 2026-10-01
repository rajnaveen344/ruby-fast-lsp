//! File-owned fact replacement and resolve passes.

use crate::invariant::ExpectInvariant;
use crate::stats::StatsSnapshot;
use std::collections::HashSet;
use std::time::Instant;

use crate::core::{DiagnosticFact, FileAnalysis, RubyType, SourceFileId};

use super::{AnalysisEngine, ResolveMode, ResolveStat, SourceFileSnapshot};
use crate::engine::persist::fingerprint::{SemanticChange, SemanticExportFingerprint};

impl AnalysisEngine {
    pub fn replace_facts(
        &mut self,
        file_id: SourceFileId,
        facts: FileAnalysis,
        mode: ResolveMode,
    ) -> SemanticChange {
        let fingerprint = SemanticExportFingerprint::from_facts(&facts);
        let previous = self.files.record_export_fingerprint(file_id, fingerprint);
        let change = SemanticChange::classify(previous, fingerprint);
        self.replace_facts_deferred(file_id, facts);
        match mode {
            ResolveMode::Immediate => self.resolve(),
            ResolveMode::Deferred => {}
        }
        change
    }

    /// Commit facts only while the source snapshot used to collect them is
    /// still current. A mismatch is an expected concurrent-edit outcome.
    pub fn replace_facts_if_source_snapshot(
        &mut self,
        expected_snapshot: SourceFileSnapshot,
        facts: FileAnalysis,
        mode: ResolveMode,
    ) -> Option<SemanticChange> {
        invariant_eq!(
            expected_snapshot.engine_instance_id,
            self.instance_id,
            what = "fact replacement received a source snapshot from another analysis engine",
            why = "isolated projects cannot share mutable source lifecycle identity",
            fix = "commit collected facts through the same engine that issued the snapshot",
        );
        if !self.files.is_current(expected_snapshot) {
            return None;
        }
        Some(self.replace_facts(expected_snapshot.file_id, facts, mode))
    }

    pub fn resolve(&mut self) {
        let mut stats = StatsSnapshot::default();
        let graph_retry_started = Instant::now();
        self.retry_unresolved_graph_edges();
        stats.record_duration(ResolveStat::GraphRetryNs, graph_retry_started.elapsed());
        self.resolve_constant_type_equations();
        stats.record(
            ResolveStat::MethodReturnEquationSolveRuns,
            u64::from(self.resolve_method_return_equations()),
        );
        self.resolve_reference_candidates(&mut stats);
        self.last_resolve_pass = stats;
    }

    /// Profiler evidence for the most recent full `resolve()` pass.
    pub fn last_resolve_stats(&self) -> &StatsSnapshot<ResolveStat> {
        &self.last_resolve_pass
    }

    pub fn resolve_file(&mut self, file_id: SourceFileId) {
        self.resolve_files(&[file_id]);
    }

    pub fn resolve_files(&mut self, file_ids: &[SourceFileId]) {
        let mut unique = HashSet::with_capacity(file_ids.len());
        for file_id in file_ids {
            self.files.assert_known(
                *file_id,
                "selected-file resolve references unknown source file id",
            );
            invariant!(
                unique.insert(*file_id),
                what = "selected-file resolution contains duplicate file id {:?}",
                why = "one resolution replaces each file's references and diagnostics once",
                fix = "sort and dedupe file ids before AnalysisEngine::resolve_files",
                file_id,
            );
        }
        if file_ids.is_empty() {
            return;
        }
        self.retry_unresolved_graph_edges();
        self.resolve_constant_type_equations();
        self.resolve_method_return_equations();
        for file_id in file_ids {
            self.resolve_reference_candidates_in_file(*file_id);
        }
    }
}

impl AnalysisEngine {
    fn replace_facts_deferred(&mut self, file_id: SourceFileId, mut facts: FileAnalysis) {
        self.semantic_revision = self.semantic_revision.checked_add(1).expect_invariant(
            "analysis engine semantic revision exhausted u64",
            "cached queries require monotonic invalidation",
            "widen the semantic revision before performing u64::MAX replacements",
        );
        self.hierarchy.invalidate_method_lookup_chains();
        self.files
            .assert_known(file_id, "file analysis references unknown source file id");
        for (range, ruby_type) in facts.local_read_types.as_ref() {
            invariant_eq!(
                range.file_id,
                file_id,
                what = "compact local-read type belongs to a different file",
                why = "inference evidence must be replaced atomically with its source",
                fix = "attach the registered SourceFileId while converting TypeTracker offsets",
            );
            invariant!(
                *ruby_type != RubyType::Unknown,
                what = "compact local-read evidence contains Unknown at {range:?}",
                why = "only proven flow types may enter local_read_types",
                fix = "retain the failure in expression_unknown_reasons instead",
                range = range,
            );
        }
        for adjacent in facts.local_read_types.windows(2) {
            invariant!(
                adjacent[0].0 < adjacent[1].0,
                what = "compact local-read evidence is duplicated or unsorted",
                why = "deterministic range queries require one result per AST read",
                fix = "sort and deduplicate TypeTracker results before engine replacement",
            );
        }
        let equations_changed = self.solver.carry_over_unchanged_solution(
            file_id,
            &mut facts.inference,
            &mut facts.types,
        );
        self.decls.replace_file(
            &mut self.names,
            file_id,
            facts.symbols,
            facts.methods,
            facts.method_visibility_overrides,
            facts.execution_contexts,
        );
        self.hierarchy.replace_file(
            &mut self.names,
            file_id,
            facts.graph_nodes,
            facts.graph_edges,
            facts.unresolved_graph_edges,
        );

        self.uses
            .replace_candidates(&mut self.names, file_id, facts.reference_candidates);
        self.diagnostics
            .replace_file(file_id, facts.diagnostic_candidates, facts.diagnostics);
        let call_expression_outcomes =
            std::mem::take(&mut facts.inference.call_expression_outcomes);
        self.types.replace_file(
            file_id,
            facts.types,
            call_expression_outcomes,
            facts.local_read_types,
        );
        self.solver
            .replace_file(file_id, facts.inference, equations_changed);
        self.solver
            .refresh_retained_shape_telemetry(file_id, &self.types);
    }
}

impl AnalysisEngine {
    /// Replace only `unresolved-require` diagnostics for one file.
    ///
    /// Keeps every other resolved diagnostic fact, candidates, and semantic
    /// stores intact. Used when dependency require roots become available after
    /// project files were already indexed with incomplete load-path context.
    /// A removed or replaced source rejects the update, including empty clears.
    pub fn replace_unresolved_require_diagnostics_if_source_snapshot(
        &mut self,
        expected_snapshot: SourceFileSnapshot,
        require_diagnostics: Vec<DiagnosticFact>,
    ) -> bool {
        invariant_eq!(
            expected_snapshot.engine_instance_id,
            self.instance_id,
            what = "require diagnostic replacement received another engine's source snapshot",
            why = "isolated projects cannot share mutable diagnostic ownership",
            fix = "commit through the engine that issued the snapshot",
        );
        if !self.files.is_current(expected_snapshot) {
            return false;
        }
        let file_id = expected_snapshot.file_id;
        for fact in &require_diagnostics {
            invariant_eq!(
                fact.range.file_id,
                file_id,
                what = "unresolved-require diagnostic belongs to a different file",
                why = "require diagnostic refresh is file-local",
                fix = "construct DiagnosticFact ranges from the owning SourceFileId",
            );
            invariant_eq!(
                fact.code,
                "unresolved-require",
                what = "replace_unresolved_require_diagnostics received code `{}`",
                why = "this API only swaps unresolved-require facts",
                fix = "filter non-require diagnostics before calling this method",
                fact.code,
            );
        }
        self.semantic_revision = self.semantic_revision.checked_add(1).expect_invariant(
            "analysis engine semantic revision exhausted u64",
            "cached queries require monotonic invalidation",
            "widen the semantic revision before performing u64::MAX replacements",
        );
        self.diagnostics
            .replace_unresolved_require(file_id, require_diagnostics);
        true
    }
}
