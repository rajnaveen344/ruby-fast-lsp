//! Current-source diagnostic projection and latest-per-document publication.
use super::projects::ProjectRegistry;
use super::{RubyLanguageServer, Workspace};
use crate::invariant::ExpectInvariant;
use crate::loader::context::{IndexingRunState, LoadSink, SourceReader};
use crate::loader::scheduling::status::{IndexingPhase, IndexingRun};
use log::{info, warn};
use parking_lot::{Mutex, RwLock};
use ruby_analysis::core::{
    DiagnosticFact, DiagnosticSeverity as AnalysisDiagnosticSeverity, TextRange,
};
use ruby_analysis::engine::{AnalysisEngine, SourceFile};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Position, Range, Url};
use tower_lsp::Client;

#[derive(Debug, Default)]
pub(super) struct DiagnosticPublicationState {
    /// Latest diagnostics per URI waiting for the dedicated sender task.
    /// Intermediate updates are dropped so publish storms cannot fill
    /// tower-lsp's capacity-1 client channel and stall request dispatch.
    pending: BTreeMap<String, (Url, Vec<Diagnostic>)>,
    sender_scheduled: bool,
    /// Presentation-only results for currently open documents. A semantic
    /// dependency refresh can reuse lint output only for the exact source
    /// snapshot that was linted; edits and close remove the retained result.
    external_linter_results:
        HashMap<Url, (ruby_analysis::engine::SourceFileSnapshot, Vec<Diagnostic>)>,
}

impl DiagnosticPublicationState {
    pub(super) fn queue(&mut self, uri: Url, diagnostics: Vec<Diagnostic>) -> bool {
        self.pending.insert(uri.to_string(), (uri, diagnostics));
        if self.sender_scheduled {
            return false;
        }
        self.sender_scheduled = true;
        true
    }

    pub(super) fn take_next(&mut self) -> Option<(Url, Vec<Diagnostic>)> {
        match self.pending.pop_first() {
            Some((_key, value)) => Some(value),
            None => {
                self.sender_scheduled = false;
                None
            }
        }
    }
}

#[derive(Clone, Default)]
pub(super) struct DiagnosticPublisher {
    state: Arc<Mutex<DiagnosticPublicationState>>,
    // Submission observations remain separate from delivered LSP messages.
    #[cfg(test)]
    submitted: Arc<Mutex<HashMap<Url, Vec<Diagnostic>>>>,
}

impl RubyLanguageServer {
    pub(crate) fn diagnostic_source_snapshot(
        &self,
        uri: &Url,
    ) -> Option<ruby_analysis::engine::SourceFileSnapshot> {
        let path = uri.to_file_path().ok()?;
        self.analysis_engine_for_uri(uri)
            .read()
            .source_snapshot_for_path(path)
    }

    pub(crate) fn retain_external_linter_diagnostics(
        &self,
        uri: &Url,
        snapshot: ruby_analysis::engine::SourceFileSnapshot,
        diagnostics: &[Diagnostic],
    ) -> bool {
        if !self.documents.read().contains_key(uri)
            || self.diagnostic_source_snapshot(uri) != Some(snapshot)
        {
            return false;
        }
        let mut publication = self.diagnostics.state.lock();
        if diagnostics.is_empty() {
            publication.external_linter_results.remove(uri);
        } else {
            publication
                .external_linter_results
                .insert(uri.clone(), (snapshot, diagnostics.to_vec()));
        }
        true
    }

    pub(crate) fn append_current_external_linter_diagnostics(
        &self,
        uri: &Url,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let snapshot = self.diagnostic_source_snapshot(uri);
        self.append_external_linter_diagnostics_for_snapshot(uri, snapshot, diagnostics);
    }

    /// Reuse presentation output while the caller holds the source engine read
    /// lock, without recursively acquiring that lock to capture its snapshot.
    pub(crate) fn append_external_linter_diagnostics_for_snapshot(
        &self,
        uri: &Url,
        snapshot: Option<ruby_analysis::engine::SourceFileSnapshot>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let mut publication = self.diagnostics.state.lock();
        if let Some((retained_snapshot, retained)) = publication.external_linter_results.get(uri) {
            if Some(*retained_snapshot) == snapshot {
                diagnostics.extend(retained.iter().cloned());
            } else {
                publication.external_linter_results.remove(uri);
            }
        }
    }

    pub(crate) fn clear_external_linter_diagnostics(&self, uri: &Url) {
        self.diagnostics
            .state
            .lock()
            .external_linter_results
            .remove(uri);
    }

    /// Publish diagnostics for a document.
    ///
    /// Client IO is latest-wins and off the caller: awaiting every
    /// `publishDiagnostics` on tower-lsp's capacity-1 outbound channel can
    /// backpressure stdout and stall request dispatch (including goto).
    pub async fn publish_diagnostics(&self, uri: Url, diagnostics: Vec<Diagnostic>) {
        self.queue_diagnostics(uri, diagnostics);
    }

    /// Enqueue without yielding so a producer can retain its document/source
    /// guards through publication. Only the dedicated sender performs client IO.

    #[cfg(test)]
    pub fn last_published_diagnostics(&self, uri: &Url) -> Vec<Diagnostic> {
        self.last_diagnostic_publication(uri).unwrap_or_default()
    }

    /// Test observation of the publication boundary. None means no publication,
    /// which must not be mistaken for an explicitly published empty clear.
    #[cfg(test)]
    pub fn last_diagnostic_publication(&self, uri: &Url) -> Option<Vec<Diagnostic>> {
        self.diagnostics.submitted.lock().get(uri).cloned()
    }

    /// Recompute `unresolved-require` after dependency require roots change.
    ///
    /// Project files may be indexed before gem/stdlib roots exist, which leaves
    /// false-positive require diagnostics in the engine and on open documents.
    /// After roots are published, refresh those facts and republish open-file
    /// diagnostics — including empty clears — so the client does not keep red
    /// squiggles until the user edits.
    pub async fn refresh_unresolved_require_diagnostics_for_workspace(
        &self,
        workspace: &Workspace,
    ) {
        use crate::loader::require_paths::{
            require_diagnostic_candidates, reresolve_unresolved_require_diagnostics,
            UNRESOLVED_REQUIRE_CODE,
        };

        let feature_index = workspace.require_feature_index();
        let generation = workspace.indexing_status.snapshot().generation;
        let load_paths = self
            .config
            .lock()
            .indexing
            .load_paths
            .paths_for_project(&workspace.root_path)
            .to_vec();
        let project_root = &workspace.root_path;

        let open_paths = self
            .documents
            .read()
            .keys()
            .filter_map(|uri| uri.to_file_path().ok())
            .collect::<std::collections::HashSet<_>>();

        let refresh_started = Instant::now();
        let mut open_files = 0usize;
        let mut closed_files = 0usize;
        let mut updates = {
            let engine = workspace.analysis_engine.read();
            engine
                .files()
                .filter(|file| file.kind.contributes_project_diagnostics())
                .filter_map(|file| {
                    let candidates = engine
                        .diagnostic_facts_in_file(file.id)
                        .into_iter()
                        .filter(|fact| fact.code == UNRESOLVED_REQUIRE_CODE)
                        .collect::<Vec<_>>();
                    if open_paths.contains(&file.path) {
                        open_files = open_files.checked_add(1).expect_invariant(
                            "unresolved-require open-file refresh counter overflowed usize",
                            "the document cache must fit addressable memory",
                            "inspect corrupt document-cache iteration",
                        );
                    } else {
                        if candidates.is_empty() {
                            return None;
                        }
                        closed_files = closed_files.checked_add(1).expect_invariant(
                            "unresolved-require closed-file refresh counter overflowed usize",
                            "project files must fit addressable memory",
                            "inspect corrupt file-store iteration",
                        );
                    }
                    let snapshot = engine
                        .source_snapshot_for_path(&file.path)
                        .expect_invariant(
                            "a registered project file has no source snapshot",
                            "delayed facts require exact source identity",
                            "keep file registration and snapshot lookup aligned",
                        );
                    let uri = Url::from_file_path(&file.path).expect_invariant(
                        "a project source cannot form a file URI",
                        "registered project paths must be absolute",
                        "normalize paths before file registration",
                    );
                    Some((file.path.clone(), uri, file.id, snapshot, candidates))
                })
                .collect::<Vec<_>>()
        };
        updates.sort_unstable_by(|left, right| left.0.cmp(&right.0));

        for (path, uri, file_id, snapshot, candidates) in updates {
            #[cfg(test)]
            self.indexing
                .schedule
                .checkpoint(
                    crate::loader::scheduling::test_schedule::Point::RequireRefreshCollected,
                    &path,
                )
                .await;

            'commit: {
                let semantic_lock = self.document_semantic_lock(&uri);
                let _semantic_guard = semantic_lock.lock().await;
                let document = self.get_doc(&uri);
                let mut diagnostics = document.as_ref().map(|document| {
                    let parse = document.parse();
                    crate::loader::file_processor::syntax_diagnostics::generate_diagnostics(
                        &parse, document,
                    )
                });
                // An initially closed file may have opened while collection
                // waited. Open documents provide all static requires, including
                // previously resolved targets. Closed files reuse stored misses
                // without rereading or parsing disk sources.
                let candidates = if let Some(document) = &document {
                    require_diagnostic_candidates(document.analysis_content(), file_id)
                } else {
                    candidates
                };
                let workspaces = self.projects.read();
                let Some(owner) = ProjectRegistry::workspace_for_path(&workspaces, &path) else {
                    break 'commit;
                };
                if !Arc::ptr_eq(&workspace.analysis_engine, &owner.analysis_engine) {
                    break 'commit;
                }
                // Retain the immutable root identity through commit/publication:
                // a newer dependency refresh must never be overwritten by this one.
                let current_features = workspace.require_feature_guard();
                if !Arc::ptr_eq(&current_features, &feature_index)
                    || self
                        .config
                        .lock()
                        .indexing
                        .load_paths
                        .paths_for_project(project_root)
                        != load_paths
                {
                    break 'commit;
                }
                let mut engine = workspace.analysis_engine.write();
                let status = workspace.indexing_status.snapshot();
                if status.generation != generation
                    || matches!(
                        status.phase,
                        IndexingPhase::Cancelled | IndexingPhase::Failed
                    )
                    || engine.source_snapshot_for_path(&path) != Some(snapshot)
                {
                    break 'commit;
                }
                let Some(file) = engine.file_id(&path).and_then(|id| engine.file(id)) else {
                    break 'commit;
                };
                if !file.kind.contributes_project_diagnostics()
                    || document
                        .as_ref()
                        .is_some_and(|doc| !engine.file_content_matches(file.id, &doc.content))
                {
                    break 'commit;
                }
                // Project targets can change without changing the consumer's
                // source or dependency roots. Resolve candidates against the
                // current engine only after acquiring the commit guard.
                let requires = reresolve_unresolved_require_diagnostics(
                    &path,
                    project_root,
                    &load_paths,
                    &feature_index,
                    Some(&engine),
                    &candidates,
                );
                if !engine
                    .replace_unresolved_require_diagnostics_if_source_snapshot(snapshot, requires)
                {
                    break 'commit;
                }
                if let Some(diagnostics) = diagnostics.as_mut() {
                    diagnostics.extend(unresolved_diagnostics_from_engine(&engine, &uri));
                    self.append_external_linter_diagnostics_for_snapshot(
                        &uri,
                        Some(snapshot),
                        diagnostics,
                    );
                }
                if let Some(diagnostics) = diagnostics {
                    // Closed files retain engine facts but receive no publication.
                    // Synchronous enqueue keeps edits and resolution outside the
                    // projection-to-publication interval.
                    self.queue_diagnostics(uri, diagnostics);
                }
            }
            #[cfg(test)]
            self.indexing
                .schedule
                .checkpoint(
                    crate::loader::scheduling::test_schedule::Point::RequireRefreshAttempted,
                    &path,
                )
                .await;
        }
        info!(
            "[PERF][unresolved-require refresh] project={} open_files={} closed_files={} elapsed={:?}",
            project_root.display(), open_files, closed_files, refresh_started.elapsed()
        );
    }
}

/// Project one file's engine diagnostic facts while the caller retains the
/// engine guard through publication.
pub(crate) fn unresolved_diagnostics_from_engine(
    engine: &AnalysisEngine,
    uri: &Url,
) -> Vec<Diagnostic> {
    let path = uri
        .to_file_path()
        .unwrap_or_else(|_| PathBuf::from(uri.to_string()));
    let Some(file_id) = engine.file_id(&path) else {
        return Vec::new();
    };

    engine
        .diagnostic_facts_in_file(file_id)
        .into_iter()
        .filter_map(|fact| diagnostic_from_fact(engine, &fact))
        .collect()
}

/// Project a fact through its owning source, which must form a file URI.
fn diagnostic_from_fact(engine: &AnalysisEngine, fact: &DiagnosticFact) -> Option<Diagnostic> {
    let file = engine.file(fact.range.file_id)?;
    Url::from_file_path(&file.path).ok()?;
    diagnostic_from_fact_fast(file, fact)
}

fn diagnostic_from_fact_fast(file: &SourceFile, fact: &DiagnosticFact) -> Option<Diagnostic> {
    Some(Diagnostic {
        range: lsp_range_for_text_range_fast(file, fact.range)?,
        severity: Some(lsp_diagnostic_severity(fact.severity)),
        code: Some(NumberOrString::String(fact.code.clone())),
        code_description: None,
        source: Some("ruby-fast-lsp".to_string()),
        message: fact.message.clone(),
        related_information: None,
        tags: None,
        data: None,
    })
}

fn lsp_diagnostic_severity(severity: AnalysisDiagnosticSeverity) -> DiagnosticSeverity {
    match severity {
        AnalysisDiagnosticSeverity::Error => DiagnosticSeverity::ERROR,
        AnalysisDiagnosticSeverity::Warning => DiagnosticSeverity::WARNING,
        AnalysisDiagnosticSeverity::Information => DiagnosticSeverity::INFORMATION,
        AnalysisDiagnosticSeverity::Hint => DiagnosticSeverity::HINT,
    }
}

fn lsp_range_for_text_range_fast(file: &SourceFile, range: TextRange) -> Option<Range> {
    let (start_line, start_character) = file.byte_offset_to_line_character(range.start_byte)?;
    let (end_line, end_character) = file.byte_offset_to_line_character(range.end_byte)?;
    Some(Range::new(
        Position::new(start_line, start_character),
        Position::new(end_line, end_character),
    ))
}

impl RubyLanguageServer {
    /// Publish a current, complete diagnostic projection for every open
    /// document that `engine`, the engine of the project at `root`, owns.
    ///
    /// Documents publish in URI order. Each projection is read under the
    /// document's semantic lock and enqueued while the engine read lock is
    /// held, so edits and cross-file resolution cannot interleave. Stops with
    /// the run's state as soon as `run` is no longer current; a load without
    /// a run publishes unconditionally.
    pub(super) async fn publish_project_facts_diagnostics(
        &self,
        root: &Path,
        engine: &Arc<RwLock<AnalysisEngine>>,
        run: Option<&IndexingRun>,
    ) -> IndexingRunState {
        let run_state = || {
            run.map_or(IndexingRunState::Current, |run| {
                LoadSink::indexing_run_state(self, root, run)
            })
        };
        let mut open_uris = self.documents.open_uris();
        open_uris.retain(|uri| Arc::ptr_eq(engine, &self.analysis_engine_for_uri(uri)));
        open_uris.sort_unstable_by(|left, right| left.as_str().cmp(right.as_str()));

        for uri in open_uris {
            let state = run_state();
            if state != IndexingRunState::Current {
                return state;
            }
            #[cfg(test)]
            let publication_path = uri
                .to_file_path()
                .expect("coordinator diagnostics must target a file URI");
            #[cfg(test)]
            self.indexing
                .schedule
                .checkpoint(
                    crate::loader::scheduling::test_schedule::Point::ColdDiagnosticsPending,
                    &publication_path,
                )
                .await;

            {
                // Edits and close may complete while this producer waits. Read
                // diagnostics only after acquiring the same document lock used
                // by those handlers, and recheck the indexing generation then.
                let semantic_lock = self.document_semantic_lock(&uri);
                let _semantic_guard = semantic_lock.lock().await;
                let state = run_state();
                if state != IndexingRunState::Current {
                    return state;
                }
                if !Arc::ptr_eq(engine, &self.analysis_engine_for_uri(&uri)) {
                    continue;
                }
                let Some(document) = self.documents.open_document(&uri) else {
                    continue;
                };
                let Ok(path) = uri.to_file_path() else {
                    continue;
                };
                let mut diagnostics = {
                    let parse = document.parse();
                    crate::loader::file_processor::syntax_diagnostics::generate_diagnostics(
                        &parse, &document,
                    )
                };
                let engine = engine.read();
                let Some(file) = engine.file_id(&path).and_then(|id| engine.file(id)) else {
                    continue;
                };
                if !file.kind.contributes_project_diagnostics()
                    || !engine.file_content_matches(file.id, &document.content)
                {
                    continue;
                }
                diagnostics.extend(
                    engine
                        .diagnostic_facts_in_file(file.id)
                        .iter()
                        .filter_map(|fact| diagnostic_from_fact_fast(file, fact)),
                );
                self.append_external_linter_diagnostics_for_snapshot(
                    &uri,
                    engine.source_snapshot_for_path(&path),
                    &mut diagnostics,
                );
                let state = run_state();
                if state != IndexingRunState::Current {
                    return state;
                }
                // Keep the engine read lock through the synchronous enqueue:
                // cross-file resolution cannot invalidate this projection in
                // between. Empty results must clear errors resolved at startup.
                self.queue_diagnostics(uri, diagnostics);
            }
            #[cfg(test)]
            self.indexing
                .schedule
                .checkpoint(
                    crate::loader::scheduling::test_schedule::Point::ColdDiagnosticsAttempted,
                    &publication_path,
                )
                .await;
        }
        IndexingRunState::Current
    }
}

impl DiagnosticPublisher {
    fn enqueue(&self, client: Option<Client>, uri: Url, diagnostics: Vec<Diagnostic>) {
        #[cfg(test)]
        self.submitted
            .lock()
            .insert(uri.clone(), diagnostics.clone());
        if client.is_none() {
            return;
        }
        let schedule_sender = {
            let mut publication = self.state.lock();
            publication.queue(uri, diagnostics)
        };
        if schedule_sender {
            let publisher = self.clone();
            tokio::spawn(async move {
                publisher.drain(client).await;
            });
        }
    }

    async fn drain(&self, client: Option<Client>) {
        let Some(client) = &client else {
            let mut publication = self.state.lock();
            publication.pending.clear();
            publication.sender_scheduled = false;
            return;
        };
        loop {
            let Some((uri, diagnostics)) = ({
                let mut publication = self.state.lock();
                publication.take_next()
            }) else {
                return;
            };
            let start = Instant::now();
            let _ = client.publish_diagnostics(uri, diagnostics, None).await;
            let elapsed = start.elapsed();
            if elapsed >= Duration::from_millis(50) {
                warn!(
                    "[PERF] publishDiagnostics send took {:?} — stdout backpressure can stall \
                     LSP request dispatch when the client falls behind",
                    elapsed
                );
            }
        }
    }
}
impl RubyLanguageServer {
    pub(crate) fn queue_diagnostics(&self, uri: Url, diagnostics: Vec<Diagnostic>) {
        self.diagnostics
            .enqueue(self.client.clone(), uri, diagnostics);
    }
}
