//! Source registration, file-owned fact replacement, and resolve passes.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::core::{
    DiagnosticFact, ExecutionContextFact, InferenceTelemetry, RubyType, SourceFileId, SourceKind,
    TypeProvenance, TypeSubject,
};

use super::fingerprint::{SemanticChange, SemanticExportFingerprint};
use super::inference::{StoredTypeInferenceOutcome, TypeInferenceOutcomeRef};
use super::storage::source_hash;
use super::{
    elapsed_ns, AnalysisEngine, FileFacts, ResolveMode, ResolvePassStats, SourceFile,
    SourceFileInput, SourceFileSnapshot, SourceLineIndex,
};

impl AnalysisEngine {
    pub fn register_file(&mut self, file: SourceFileInput) -> SourceFileId {
        let line_index = SourceLineIndex::new(&file.content);
        let content_hash = source_hash(&file.content);
        let source = if line_index.is_ascii() {
            None
        } else {
            Some(file.content)
        };
        self.register_indexed_file(file.path, file.kind, line_index, content_hash, source, None)
    }

    /// Register a locked gem source with explicit package identity for library-tree grouping.
    pub fn register_gem_file(
        &mut self,
        file: SourceFileInput,
        package: crate::core::LibraryPackageId,
    ) -> SourceFileId {
        assert!(
            file.kind == SourceKind::Gem,
            "INVARIANT VIOLATED: register_gem_file received SourceKind::{:?}. \
             This is a bug because only Gem sources carry locked package identity. \
             Fix: use register_file for non-gem sources or pass SourceKind::Gem.",
            file.kind
        );
        let line_index = SourceLineIndex::new(&file.content);
        let content_hash = source_hash(&file.content);
        let source = if line_index.is_ascii() {
            None
        } else {
            Some(file.content)
        };
        self.register_indexed_file(
            file.path,
            file.kind,
            line_index,
            content_hash,
            source,
            Some(package),
        )
    }

    /// Register source whose caller retains the owned buffer. ASCII files need
    /// only their line index and content hash after collection; non-ASCII files
    /// retain one engine-owned copy for exact UTF-16 conversion.
    pub fn register_file_borrowed(
        &mut self,
        path: PathBuf,
        content: &str,
        kind: SourceKind,
    ) -> SourceFileId {
        let line_index = SourceLineIndex::new(content);
        let content_hash = source_hash(content);
        let source = if line_index.is_ascii() {
            None
        } else {
            Some(content.to_string())
        };
        self.register_indexed_file(path, kind, line_index, content_hash, source, None)
    }

    fn register_indexed_file(
        &mut self,
        path: PathBuf,
        kind: SourceKind,
        line_index: SourceLineIndex,
        content_hash: u64,
        source: Option<String>,
        library_package: Option<crate::core::LibraryPackageId>,
    ) -> SourceFileId {
        let id = self.sources.ids.get_or_insert(&path);
        if self.sources.files.get(&id).is_some_and(|existing| {
            existing.path == path
                && existing.kind == kind
                && existing.line_index == line_index
                && existing.content_hash == content_hash
                && existing.source == source
                && existing.library_package == library_package
        }) {
            return id;
        }
        self.next_source_revision = self.next_source_revision.checked_add(1).expect(
            "INVARIANT VIOLATED: analysis engine source revision exhausted u64. This is a bug because stale background fact commits require monotonic source identity. Fix: widen the source snapshot revision before registering u64::MAX distinct source snapshots.",
        );
        self.sources.files.insert(
            id,
            SourceFile {
                id,
                path: path.components().collect(),
                source,
                line_index,
                content_hash,
                kind,
                revision: self.next_source_revision,
                library_package,
            },
        );
        id
    }

    pub fn source_snapshot_for_path(&self, path: impl AsRef<Path>) -> Option<SourceFileSnapshot> {
        let file_id = self.file_id(path)?;
        let file = self.file(file_id)?;
        Some(SourceFileSnapshot {
            engine_instance_id: self.instance_id,
            file_id,
            revision: file.revision,
        })
    }

    /// Register a source only if no newer registration occurred after the
    /// caller captured `expected_snapshot`.
    pub fn register_file_borrowed_if_snapshot(
        &mut self,
        path: PathBuf,
        content: &str,
        kind: SourceKind,
        expected_snapshot: Option<SourceFileSnapshot>,
    ) -> Option<SourceFileSnapshot> {
        if let Some(expected) = expected_snapshot {
            assert_eq!(
                expected.engine_instance_id,
                self.instance_id,
                "INVARIANT VIOLATED: conditional source registration received a snapshot from another analysis engine. This is a bug because source revisions are engine-local lifecycle identities. Fix: capture and commit the snapshot through the same isolated project engine."
            );
        }
        if self.source_snapshot_for_path(&path) != expected_snapshot {
            return None;
        }
        let file_id = self.register_file_borrowed(path, content, kind);
        let file = self.file(file_id).unwrap_or_else(|| {
            panic!(
                "INVARIANT VIOLATED: conditional source registration lost file id {:?}. This is a bug because registration and revision capture occur under one engine write borrow. Fix: keep SourceRegistry insertion atomic.",
                file_id
            )
        });
        Some(SourceFileSnapshot {
            engine_instance_id: self.instance_id,
            file_id,
            revision: file.revision,
        })
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

    /// Commit facts only while the source snapshot used to collect them is
    /// still current. A mismatch is an expected concurrent-edit outcome.
    pub fn replace_facts_if_source_snapshot(
        &mut self,
        expected_snapshot: SourceFileSnapshot,
        facts: FileFacts,
        mode: ResolveMode,
    ) -> Option<SemanticChange> {
        assert_eq!(
            expected_snapshot.engine_instance_id,
            self.instance_id,
            "INVARIANT VIOLATED: fact replacement received a source snapshot from another analysis engine. This is a bug because isolated projects cannot share mutable source lifecycle identity. Fix: commit collected facts through the same engine that issued the snapshot."
        );
        if self
            .file(expected_snapshot.file_id)
            .map(|file| file.revision)
            != Some(expected_snapshot.revision)
        {
            return None;
        }
        Some(self.replace_facts(expected_snapshot.file_id, facts, mode))
    }

    pub fn resolve(&mut self) {
        let mut stats = ResolvePassStats::default();
        let graph_retry_started = Instant::now();
        self.retry_unresolved_graph_edges();
        stats.graph_retry_ns = elapsed_ns(graph_retry_started);
        self.resolve_constant_type_equations();
        stats.method_return_equation_solve_runs =
            usize::from(self.resolve_method_return_equations());
        self.resolve_reference_candidates(&mut stats);
        self.last_resolve_pass = stats;
    }

    /// Profiler evidence for the most recent full `resolve()` pass.
    pub fn last_resolve_stats(&self) -> &ResolvePassStats {
        &self.last_resolve_pass
    }

    pub fn resolve_file(&mut self, file_id: SourceFileId) {
        self.resolve_files(&[file_id]);
    }

    pub fn resolve_files(&mut self, file_ids: &[SourceFileId]) {
        let mut unique = HashSet::with_capacity(file_ids.len());
        for file_id in file_ids {
            self.assert_known_file_id(
                *file_id,
                "selected-file resolve references unknown source file id",
            );
            assert!(
                unique.insert(*file_id),
                "INVARIANT VIOLATED: selected-file semantic resolution contains duplicate file \
                 id {:?}. This is a bug because one staged resolution must replace each file's \
                 references and diagnostics exactly once. Fix: sort and deduplicate the selected \
                 file ids before calling AnalysisEngine::resolve_files.",
                file_id
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
    fn replace_facts_deferred(&mut self, file_id: SourceFileId, mut facts: FileFacts) {
        self.semantic_revision = self.semantic_revision.checked_add(1).expect(
            "INVARIANT VIOLATED: analysis engine semantic revision exhausted u64. \
             This is a bug because cached queries require monotonic invalidation. \
             Fix: widen the semantic revision before performing u64::MAX replacements.",
        );
        *self.top_level_method_lookup_chain_cache.get_mut() = None;
        *self.universal_object_method_lookup_chain_cache.get_mut() = None;
        self.assert_known_file_id(file_id, "file analysis references unknown source file id");
        for (range, ruby_type) in facts.local_read_types.as_ref() {
            assert_eq!(
                range.file_id, file_id,
                "INVARIANT VIOLATED: compact local-read type belongs to a different file. This is a bug because inference evidence must be replaced atomically with its source. Fix: attach the registered SourceFileId while converting TypeTracker offsets."
            );
            assert!(
                *ruby_type != RubyType::Unknown,
                "INVARIANT VIOLATED: compact local-read evidence contains Unknown at {range:?}. This is a bug because only proven flow types may enter local_read_types. Fix: retain the failure in expression_unknown_reasons instead."
            );
        }
        for adjacent in facts.local_read_types.windows(2) {
            assert!(
                adjacent[0].0 < adjacent[1].0,
                "INVARIANT VIOLATED: compact local-read evidence is duplicated or unsorted. This is a bug because deterministic range queries require one result per AST read. Fix: sort and deduplicate TypeTracker results before engine replacement."
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
            .expect(
                "INVARIANT VIOLATED: retained shape telemetry lost its file-owned inference evidence. This is a bug because the refresh runs only after atomic evidence insertion. Fix: keep telemetry refresh inside the file replacement lifecycle.",
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
            assert_eq!(
                context.range.file_id, file_id,
                "INVARIANT VIOLATED: execution context range belongs to a different file. This is a bug because FileFacts replacement must be file-local. Fix: construct execution context ranges from the owning RubyDocument."
            );
            assert!(
                context.lexical_namespace.namespace_kind().is_some()
                    && context.implicit_receiver.namespace_kind().is_some()
                    && context.method_definition_owner.namespace_kind().is_some(),
                "INVARIANT VIOLATED: execution context contains a non-namespace semantic target. This is a bug because receiver and definition ownership require namespace identities. Fix: validate and convert extension targets before engine ingestion."
            );
            assert!(
                !context.extension_id.is_empty(),
                "INVARIANT VIOLATED: execution context has empty extension provenance. This is a bug because generated runtime semantics must remain attributable. Fix: retain the validated manifest ID in ExecutionContextFact."
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
            assert!(
                pair[0].range != pair[1].range,
                "INVARIANT VIOLATED: multiple execution contexts own the same block range. This is a bug because extension context conflicts must be resolved before engine ingestion. Fix: deterministically reject incompatible contexts at the host boundary."
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
        assert_eq!(
            expected_snapshot.engine_instance_id,
            self.instance_id,
            "INVARIANT VIOLATED: require diagnostic replacement received another engine's source snapshot. This is a bug because isolated projects cannot share mutable diagnostic ownership. Fix: commit through the engine that issued the snapshot."
        );
        if self
            .file(expected_snapshot.file_id)
            .map(|file| file.revision)
            != Some(expected_snapshot.revision)
        {
            return false;
        }
        let file_id = expected_snapshot.file_id;
        for fact in &require_diagnostics {
            assert_eq!(
                fact.range.file_id, file_id,
                "INVARIANT VIOLATED: unresolved-require diagnostic belongs to a different file. \
                 This is a bug because require diagnostic refresh is file-local. \
                 Fix: construct DiagnosticFact ranges from the owning SourceFileId."
            );
            assert_eq!(
                fact.code, "unresolved-require",
                "INVARIANT VIOLATED: replace_unresolved_require_diagnostics received code `{}`. \
                 This is a bug because this API only swaps unresolved-require facts. \
                 Fix: filter non-require diagnostics before calling this method.",
                fact.code
            );
        }
        self.semantic_revision = self.semantic_revision.checked_add(1).expect(
            "INVARIANT VIOLATED: analysis engine semantic revision exhausted u64. \
             This is a bug because cached queries require monotonic invalidation. \
             Fix: widen the semantic revision before performing u64::MAX replacements.",
        );
        let mut diagnostics = self
            .facts
            .diagnostics
            .resolved
            .facts_in_file(file_id)
            .into_iter()
            .filter(|fact| fact.code != "unresolved-require")
            .collect::<Vec<_>>();
        diagnostics.extend(require_diagnostics);
        self.facts
            .diagnostics
            .resolved
            .replace_file(file_id, diagnostics);
        true
    }
}
