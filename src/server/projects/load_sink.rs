//! The server's side of the loader's [`LoadSink`]: each operation applies
//! one loader write or lookup to the owning project's live server state.
use super::Workspace;
use crate::environment::config::runtime::SelectedRuntimeDescriptor;
use crate::environment::extensions::{
    ExtensionRegistryHandle, ProjectContextSeed, ProjectContextSnapshot,
};
use crate::loader::context::{IndexingRunState, LoadSink, LoadTarget};
use crate::loader::jruby_add_on::JrubyAddOn;
use crate::loader::require_paths::RequireFeatureIndex;
use crate::loader::scheduling::navigation_demand::NavigationDemandController;
use crate::loader::scheduling::status::{IndexingPhase, IndexingRun};
use crate::server::Server;
use parking_lot::RwLock;
use ruby_analysis::core::{SourceFileId, SourceKind};
use ruby_analysis::indexer::RubyDocument;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tower_lsp::lsp_types::Url;

impl Server {
    /// The registered project rooted exactly at `root`.
    fn project_at_root(&self, root: &Path) -> Option<Workspace> {
        self.list_workspaces()
            .into_iter()
            .find(|workspace| workspace.root_path == root)
    }
}

#[tower_lsp::async_trait]
impl LoadSink for Server {
    fn target_for_uri(&self, uri: &Url) -> Arc<dyn LoadTarget> {
        self.project_for_uri(uri).load_target()
    }

    fn indexing_run_state(&self, root: &Path, run: &IndexingRun) -> IndexingRunState {
        match self.project_at_root(root) {
            None => IndexingRunState::Unregistered,
            Some(workspace) if workspace.indexing_status.is_current_run(run) => {
                IndexingRunState::Current
            }
            Some(_) => IndexingRunState::Superseded,
        }
    }

    async fn transition_indexing_phase(
        &self,
        root: &Path,
        generation: u64,
        phase: IndexingPhase,
    ) -> IndexingRunState {
        let Some(workspace) = self.project_at_root(root) else {
            return IndexingRunState::Unregistered;
        };
        if workspace
            .indexing_status
            .transition(generation, phase, None, None)
            .is_none()
        {
            return IndexingRunState::Superseded;
        }
        self.publish_indexing_status().await;
        IndexingRunState::Current
    }

    fn extension_registry(&self) -> ExtensionRegistryHandle {
        self.extensions.registry().clone()
    }

    fn extension_context_seed(&self, root: &Path) -> Option<Arc<RwLock<ProjectContextSeed>>> {
        self.extension_project_context_seed_for_root(&root.to_path_buf())
    }

    fn select_runtime(&self, root: &Path, runtime: Option<SelectedRuntimeDescriptor>) {
        self.set_effective_runtime(&root.to_path_buf(), runtime);
    }

    fn set_ruby_version(&self, root: &Path, ruby_version: Option<String>) {
        self.set_extension_project_ruby_version(&root.to_path_buf(), ruby_version);
    }

    async fn refresh_inlay_hints(&self, root: &Path) {
        self.refresh_inlay_hints_for_workspace(root).await;
    }

    fn publish_require_roots(
        &self,
        root: &Path,
        target: &Arc<dyn LoadTarget>,
        paths: Vec<PathBuf>,
        index: Arc<RequireFeatureIndex>,
    ) {
        if let Some(workspace) = self.project_at_root(root) {
            if workspace.handle().is_target(target) {
                workspace
                    .handle()
                    .set_dependency_require_resolution(paths, index);
            }
        }
    }

    fn publish_declared_require_paths(
        &self,
        root: &Path,
        target: &Arc<dyn LoadTarget>,
        declared: Vec<String>,
    ) {
        if let Some(workspace) = self.project_at_root(root) {
            if workspace.handle().is_target(target) {
                workspace.handle().set_declared_require_paths(declared);
            }
        }
    }

    async fn refresh_require_diagnostics(&self, root: &Path) {
        if let Some(workspace) = self.project_at_root(root) {
            self.refresh_unresolved_require_diagnostics_for_workspace(&workspace)
                .await;
        }
    }

    fn navigation_demands(&self, root: &Path) -> Option<NavigationDemandController> {
        self.project_at_root(root)
            .map(|workspace| workspace.navigation_demands)
    }

    fn project_root_for_uri(&self, uri: &Url) -> Option<PathBuf> {
        self.workspace_for_uri(uri)
            .map(|workspace| workspace.root_path)
    }

    fn register_source(&self, uri: &Url, content: &str, kind: SourceKind) -> SourceFileId {
        self.open_or_update_analysis_file_with_kind(uri, content, kind)
    }

    fn extension_context_snapshot(
        &self,
        uri: &Url,
        kind: SourceKind,
    ) -> Option<ProjectContextSnapshot> {
        self.extension_project_context_snapshot_for_uri(uri, kind)
    }

    fn mark_document_indexed(&self, uri: &Url, document: RubyDocument) {
        self.documents
            .insert(uri.clone(), Arc::new(RwLock::new(document)));
        if let Some(document) = self.documents.read().get(uri) {
            let mut document = document.write();
            document.indexed_version = Some(document.version);
        }
    }

    fn report_project_progress(
        &self,
        root: &Path,
        generation: Option<u64>,
        completed: u64,
        total: u64,
    ) {
        self.report_project_indexing_progress(root, generation, completed, total);
    }

    async fn project_facts_ready(
        &self,
        root: &Path,
        target: &Arc<dyn LoadTarget>,
        run: Option<&IndexingRun>,
    ) -> IndexingRunState {
        self.publish_project_facts_diagnostics(root, target, run)
            .await
    }

    fn set_jruby_add_on(&self, root: &Path, add_on: Option<JrubyAddOn>) {
        Server::set_jruby_add_on(self, root, add_on);
    }

    #[cfg(test)]
    fn test_schedule(&self) -> Arc<crate::loader::scheduling::test_schedule::TestSchedule> {
        self.indexing.schedule.clone()
    }
}
