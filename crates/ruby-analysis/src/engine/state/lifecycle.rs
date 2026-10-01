//! File-owned fact replacement and resolve passes.

use crate::invariant::ExpectInvariant;
use crate::stats::StatsSnapshot;
use std::collections::HashSet;
use std::time::Instant;

use crate::core::{
    DiagnosticFact, ExecutionContextFact, FileAnalysis, InferenceTelemetry, RubyType, SourceFileId,
    TypeProvenance, TypeSubject,
};

use super::inference::{StoredTypeInferenceOutcome, TypeInferenceOutcomeRef};
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
        *self.top_level_method_lookup_chain_cache.get_mut() = None;
        *self.universal_object_method_lookup_chain_cache.get_mut() = None;
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
        let equations_changed = match self.inference_by_file.get(&file_id) {
            Some(previous) => {
                previous.method_return_equations != facts.inference.method_return_equations
            }
            None => !facts.inference.method_return_equations.is_empty(),
        };
        if !equations_changed {
            if let Some(previous) = self.inference_by_file.get(&file_id) {
                facts.inference.method_return_outcomes = previous.method_return_outcomes.clone();
                facts.inference.telemetry = previous.telemetry.clone();
                for fact in &mut facts.types {
                    if fact.provenance != TypeProvenance::Inferred {
                        continue;
                    }
                    let TypeSubject::MethodReturn(method) = &fact.subject else {
                        continue;
                    };
                    if let Some(outcome) = previous.method_return_outcomes.get(method) {
                        fact.ruby_type = outcome.clone().into_ruby_type();
                    }
                }
            }
        }
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
        self.replace_execution_contexts(file_id, facts.execution_contexts);
        let graph_nodes = self.intern_graph_node_facts(facts.graph_nodes);
        let graph_edges = self.intern_graph_edge_facts(facts.graph_edges);
        let unresolved_graph_edges =
            self.intern_unresolved_graph_edge_facts(facts.unresolved_graph_edges);
        self.graph
            .replace_file(file_id, graph_nodes, graph_edges, unresolved_graph_edges);

        self.uses
            .replace_candidates(&mut self.names, file_id, facts.reference_candidates);
        self.diagnostics
            .replace_file(file_id, facts.diagnostic_candidates, facts.diagnostics);
        let call_expression_outcomes =
            std::mem::take(&mut facts.inference.call_expression_outcomes);
        self.inference_by_file.insert(file_id, facts.inference);
        if call_expression_outcomes.is_empty() {
            self.call_expression_outcomes_by_file.remove(&file_id);
        } else {
            let outcomes = call_expression_outcomes
                .into_iter()
                .map(|(range, outcome)| {
                    (
                        range,
                        StoredTypeInferenceOutcome::from_domain(&mut self.facts.types, outcome),
                    )
                })
                .collect::<Vec<_>>()
                .into_boxed_slice();
            self.call_expression_outcomes_by_file
                .insert(file_id, outcomes);
        }
        if facts.local_read_types.is_empty() {
            self.local_read_types_by_file.remove(&file_id);
        } else {
            let local_read_types = facts
                .local_read_types
                .into_vec()
                .into_iter()
                .map(|(range, ruby_type)| (range, self.facts.types.intern_ruby_type(ruby_type)))
                .collect::<Vec<_>>()
                .into_boxed_slice();
            self.local_read_types_by_file
                .insert(file_id, local_read_types);
        }
        self.method_return_equations_dirty |= equations_changed;
        self.constant_type_equations_dirty = self.inference_by_file.values().any(|evidence| {
            !evidence.constant_type_equations.is_empty()
                || evidence
                    .method_return_equations
                    .iter()
                    .any(|equation| !equation.constant_dependencies().is_empty())
        });
        self.refresh_retained_shape_telemetry(file_id);
    }

    pub(super) fn refresh_retained_shape_telemetry(&mut self, file_id: SourceFileId) {
        let mut observed = InferenceTelemetry::default();
        for ruby_type in self.facts.types.ruby_types_in_file(file_id) {
            observed.observe_retained_type(ruby_type);
        }
        if let Some(reads) = self.local_read_type_views_in_file(file_id) {
            for (_, ruby_type) in reads {
                observed.observe_retained_type(ruby_type);
            }
        }
        if let Some(evidence) = self.inference_by_file.get(&file_id) {
            for outcome in evidence.method_return_outcomes.values() {
                if let Some(ruby_type) = outcome.proven_type() {
                    observed.observe_retained_type(ruby_type);
                }
                if let Some(reason) = outcome.unknown_reason() {
                    observed.observe_shape_unknown(reason);
                }
            }
            for (_, reason) in &evidence.expression_unknown_reasons {
                observed.observe_shape_unknown(*reason);
            }
        }
        if let Some(outcomes) = self.call_expression_outcome_views_in_file(file_id) {
            for (_, outcome) in outcomes {
                match outcome {
                    TypeInferenceOutcomeRef::Proven(ruby_type) => {
                        observed.observe_retained_type(ruby_type);
                    }
                    TypeInferenceOutcomeRef::Unknown(reason) => {
                        observed.observe_shape_unknown(reason);
                    }
                }
            }
        }
        self.inference_by_file
            .get_mut(&file_id)
            .expect_invariant(
                "retained shape telemetry lost its file-owned inference evidence",
                "the refresh runs only after atomic evidence insertion",
                "keep telemetry refresh inside the file replacement lifecycle",
            )
            .telemetry
            .replace_retained_shape_observations(&observed);
    }
}

impl AnalysisEngine {
    fn replace_execution_contexts(
        &mut self,
        file_id: SourceFileId,
        mut contexts: Vec<ExecutionContextFact>,
    ) {
        for context in &contexts {
            invariant_eq!(
                context.range.file_id,
                file_id,
                what = "execution context range belongs to a different file",
                why = "FileAnalysis replacement must be file-local",
                fix = "construct execution context ranges from the owning RubyDocument",
            );
            invariant!(
                context.lexical_namespace.namespace_kind().is_some()
                    && context.implicit_receiver.namespace_kind().is_some()
                    && context.method_definition_owner.namespace_kind().is_some(),
                what = "execution context contains a non-namespace semantic target",
                why = "receiver and definition ownership require namespace identities",
                fix = "validate and convert extension targets before engine ingestion",
            );
            invariant!(
                !context.extension_id.is_empty(),
                what = "execution context has empty extension provenance",
                why = "generated runtime semantics must remain attributable",
                fix = "retain the validated manifest ID in ExecutionContextFact",
            );
        }
        contexts.sort_by_key(|context| {
            (
                context.range.start_byte,
                std::cmp::Reverse(context.range.end_byte),
                context.extension_id.clone(),
            )
        });
        for pair in contexts.windows(2) {
            invariant!(
                pair[0].range != pair[1].range,
                what = "multiple execution contexts own the same block range",
                why = "extension context conflicts must be resolved before engine ingestion",
                fix = "deterministically reject incompatible contexts at the host boundary",
            );
        }
        if contexts.is_empty() {
            self.execution_contexts.remove(&file_id);
        } else {
            self.execution_contexts.insert(file_id, contexts);
        }
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
