//! Current-source diagnostic projection and latest-per-document publication.
use super::projects::ProjectRegistry;
use super::{RubyLanguageServer, Workspace};
use crate::invariant::ExpectInvariant;
use crate::loader::context::{IndexingRunState, LoadSink, LoadTarget, SourceReader};
use crate::loader::scheduling::status::{IndexingPhase, IndexingRun};
use crate::utils::lsp::lsp_file_range;
use log::{info, warn};
use parking_lot::Mutex;
use ruby_analysis::core::DiagnosticSeverity as AnalysisDiagnosticSeverity;
use ruby_analysis::engine::View;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Url};
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
        self.project_for_uri(uri)
            .view(|view| view.source_snapshot_for_path(path))
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

    /// The one composition of a document's published diagnostics: `syntax`,
    /// then the engine's facts, then linter output retained for the exact
    /// current source. Everything is read through the caller's single view,
    /// whose engine guard the caller keeps through the synchronous enqueue.
    pub(crate) fn compose_diagnostics(
        &self,
        view: &View<'_>,
        uri: &Url,
        mut diagnostics: Vec<Diagnostic>,
    ) -> Vec<Diagnostic> {
        diagnostics.extend(engine_diagnostics(view, uri));
        let snapshot = uri
            .to_file_path()
            .ok()
            .and_then(|path| view.source_snapshot_for_path(path));
        let mut publication = self.diagnostics.state.lock();
        if let Some((retained_snapshot, retained)) = publication.external_linter_results.get(uri) {
            if Some(*retained_snapshot) == snapshot {
                diagnostics.extend(retained.iter().cloned());
            } else {
                publication.external_linter_results.remove(uri);
            }
        }
        diagnostics
    }

    /// Compose `syntax` with the document's engine and linter diagnostics and
    /// enqueue the result while holding one engine read guard.
    pub(crate) fn publish_document_diagnostics(&self, uri: Url, syntax: Vec<Diagnostic>) {
        self.project_for_uri(&uri).view(|view| {
            let diagnostics = self.compose_diagnostics(view, &uri, syntax);
            self.queue_diagnostics(uri, diagnostics);
        });
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

    /// Test observation of retained linter output, which close releases.
    #[cfg(test)]
    pub fn has_retained_linter_diagnostics(&self, uri: &Url) -> bool {
        self.diagnostics
            .state
            .lock()
            .external_linter_results
            .contains_key(uri)
    }

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
        };
        use ruby_analysis::engine::UNRESOLVED_REQUIRE_CODE;

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
        let mut updates = workspace.handle().view(|view| {
            view.files()
                .filter(|file| file.kind.contributes_project_diagnostics())
                .filter_map(|file| {
                    let candidates = view
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
                    let snapshot = view.source_snapshot_for_path(&file.path).expect_invariant(
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
        });
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
                let syntax = document.as_ref().map(|document| {
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
                if !workspace.handle().is_same(owner.handle()) {
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
                workspace.handle().update(|engine| {
                    let status = workspace.indexing_status.snapshot();
                    if status.generation != generation
                        || matches!(
                            status.phase,
                            IndexingPhase::Cancelled | IndexingPhase::Failed
                        )
                        || engine.view().source_snapshot_for_path(&path) != Some(snapshot)
                    {
                        return;
                    }
                    let view = engine.view();
                    let Some(file) = view.file_id(&path).and_then(|id| view.file(id)) else {
                        return;
                    };
                    if !file.kind.contributes_project_diagnostics()
                        || document
                            .as_ref()
                            .is_some_and(|doc| !view.file_content_matches(file.id, &doc.content))
                    {
                        return;
                    }
                    // Project targets can change without changing the consumer's
                    // source or dependency roots. Resolve candidates against the
                    // current engine only inside the commit update.
                    let requires = reresolve_unresolved_require_diagnostics(
                        &path,
                        project_root,
                        &load_paths,
                        &feature_index,
                        Some(&view),
                        &candidates,
                    );
                    if !engine.replace_unresolved_require_diagnostics_if_source_snapshot(
                        snapshot, requires,
                    ) {
                        return;
                    }
                    // Closed files retain engine facts but receive no publication.
                    // Synchronous enqueue keeps edits and resolution outside the
                    // projection-to-publication interval.
                    if let Some(syntax) = syntax {
                        let diagnostics = self.compose_diagnostics(&engine.view(), &uri, syntax);
                        self.queue_diagnostics(uri, diagnostics);
                    }
                });
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

/// The single engine-to-LSP diagnostic projection for one document, read
/// through the caller's one view. A source that cannot form a file URI, or a
/// document the engine does not know, has no engine diagnostics.
pub fn engine_diagnostics(view: &View<'_>, uri: &Url) -> Vec<Diagnostic> {
    let path = uri
        .to_file_path()
        .unwrap_or_else(|_| PathBuf::from(uri.to_string()));
    let Some(file_id) = view.file_id(&path) else {
        return Vec::new();
    };
    // Most documents have no facts; skip the source lookup and URI check.
    let facts = view.diagnostic_facts_in_file(file_id);
    if facts.is_empty() {
        return Vec::new();
    }
    let Some(file) = view
        .file(file_id)
        .filter(|file| Url::from_file_path(&file.path).is_ok())
    else {
        return Vec::new();
    };
    facts
        .into_iter()
        .filter_map(|fact| {
            Some(Diagnostic {
                range: lsp_file_range(file, fact.range)?,
                severity: Some(lsp_diagnostic_severity(fact.severity)),
                code: Some(NumberOrString::String(fact.code)),
                source: Some("ruby-fast-lsp".to_string()),
                message: fact.message,
                ..Diagnostic::default()
            })
        })
        .collect()
}

fn lsp_diagnostic_severity(severity: AnalysisDiagnosticSeverity) -> DiagnosticSeverity {
    match severity {
        AnalysisDiagnosticSeverity::Error => DiagnosticSeverity::ERROR,
        AnalysisDiagnosticSeverity::Warning => DiagnosticSeverity::WARNING,
        AnalysisDiagnosticSeverity::Information => DiagnosticSeverity::INFORMATION,
        AnalysisDiagnosticSeverity::Hint => DiagnosticSeverity::HINT,
    }
}

impl RubyLanguageServer {
    /// Publish a current, complete diagnostic projection for every open
    /// document that `target`, the engine of the project at `root`, owns.
    ///
    /// Documents publish in URI order. Each projection is read under the
    /// document's semantic lock and enqueued while the engine read lock is
    /// held, so edits and cross-file resolution cannot interleave. Stops with
    /// the run's state as soon as `run` is no longer current; a load without
    /// a run publishes unconditionally.
    pub(super) async fn publish_project_facts_diagnostics(
        &self,
        root: &Path,
        target: &Arc<dyn LoadTarget>,
        run: Option<&IndexingRun>,
    ) -> IndexingRunState {
        let run_state = || {
            run.map_or(IndexingRunState::Current, |run| {
                LoadSink::indexing_run_state(self, root, run)
            })
        };
        let mut open_uris = self.documents.open_uris();
        open_uris.retain(|uri| self.project_for_uri(uri).is_target(target));
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
                let project = self.project_for_uri(&uri);
                if !project.is_target(target) {
                    continue;
                }
                let Some(document) = self.documents.open_document(&uri) else {
                    continue;
                };
                let Ok(path) = uri.to_file_path() else {
                    continue;
                };
                let syntax = {
                    let parse = document.parse();
                    crate::loader::file_processor::syntax_diagnostics::generate_diagnostics(
                        &parse, &document,
                    )
                };
                // Keep one project view through the synchronous enqueue:
                // cross-file resolution cannot invalidate this projection in
                // between. Empty results must clear errors resolved at startup.
                let stopped = project.view(|view| {
                    let file = view.file_id(&path).and_then(|id| view.file(id))?;
                    if !file.kind.contributes_project_diagnostics()
                        || !view.file_content_matches(file.id, &document.content)
                    {
                        return None;
                    }
                    let diagnostics = self.compose_diagnostics(view, &uri, syntax);
                    let state = run_state();
                    if state != IndexingRunState::Current {
                        return Some(state);
                    }
                    self.queue_diagnostics(uri.clone(), diagnostics);
                    None
                });
                if let Some(state) = stopped {
                    return state;
                }
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
