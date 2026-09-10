//! Current-source diagnostic projection and latest-per-document publication.
use super::projects::ProjectRegistry;
use super::{RubyLanguageServer, Workspace};
use crate::indexing_status::IndexingPhase;
use log::{info, warn};
use parking_lot::Mutex;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tower_lsp::lsp_types::{Diagnostic, Url};
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
        use crate::indexer::require_paths::{
            require_diagnostic_candidates, reresolve_unresolved_require_diagnostics,
            UNRESOLVED_REQUIRE_CODE,
        };
        use crate::query::EngineQuery;

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
                    let candidates = engine.diagnostic_facts_in_file(file.id).into_iter()
                        .filter(|fact| fact.code == UNRESOLVED_REQUIRE_CODE)
                        .collect::<Vec<_>>();
                    if open_paths.contains(&file.path) {
                        open_files = open_files.checked_add(1).expect(
                            "INVARIANT VIOLATED: unresolved-require open-file refresh counter overflowed usize. This is a bug because the document cache must fit addressable memory. Fix: inspect corrupt document-cache iteration.",
                        );
                    } else {
                        if candidates.is_empty() {
                            return None;
                        }
                        closed_files = closed_files.checked_add(1).expect(
                            "INVARIANT VIOLATED: unresolved-require closed-file refresh counter overflowed usize. This is a bug because project files must fit addressable memory. Fix: inspect corrupt file-store iteration.",
                        );
                    }
                    let snapshot = engine.source_snapshot_for_path(&file.path).expect(
                        "INVARIANT VIOLATED: a registered project file has no source snapshot. This is a bug because delayed facts require exact source identity. Fix: keep file registration and snapshot lookup aligned.",
                    );
                    let uri = Url::from_file_path(&file.path).expect(
                        "INVARIANT VIOLATED: a project source cannot form a file URI. This is a bug because registered project paths must be absolute. Fix: normalize paths before file registration.",
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
                    crate::indexer::test_schedule::Point::RequireRefreshCollected,
                    &path,
                )
                .await;

            'commit: {
                let semantic_lock = self.document_semantic_lock(&uri);
                let _semantic_guard = semantic_lock.lock().await;
                let document = self.get_doc(&uri);
                let mut diagnostics = document.as_ref().map(|document| {
                    let parse = document.parse();
                    crate::capabilities::diagnostics::generate_diagnostics(&parse, document)
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
                    diagnostics.extend(EngineQuery::unresolved_diagnostics_from_engine(
                        &engine, &uri,
                    ));
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
                    crate::indexer::test_schedule::Point::RequireRefreshAttempted,
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
