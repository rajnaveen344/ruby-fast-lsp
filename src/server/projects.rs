//! Ruby project ownership, longest-root routing, and external provenance.
use super::RubyLanguageServer;
use crate::config::runtime::SelectedRuntimeDescriptor;
use crate::extensions::{ProjectContextSeed, ProjectContextSnapshot};
use crate::indexing_status::{IndexingRun, ProjectIndexingStatus};
use crate::navigation_demand::NavigationDemandController;
use crate::runtime::jruby::imports::JrubyImportProvider;
use parking_lot::RwLock;
use ruby_analysis::core::{SourceFileId, SourceKind};
use ruby_analysis::engine::AnalysisEngine;
use ruby_fast_lsp_extension_api::ProjectContext;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tower_lsp::lsp_types::Url;

fn new_orphan_analysis_engine() -> Arc<RwLock<AnalysisEngine>> {
    let engine = Arc::new(RwLock::new(AnalysisEngine::new()));
    crate::indexer::indexer_stdlib::IndexerStdlib::new(
        crate::indexer::file_processor::FileProcessor::new(),
        None,
    )
    .index_core_runtime_constants(None, engine.clone())
    .expect(
        "INVARIANT VIOLATED: the orphan engine could not seed embedded Ruby core runtime constants. This is a bug because loose files require the same universal constant facts as project engines. Fix: keep the embedded core RBS overlay parseable and register it before orphan documents.",
    );
    engine.write().resolve();
    engine
}

#[derive(Clone, Default)]
pub struct ProjectRuntimeState {
    selected: Arc<RwLock<Option<SelectedRuntimeDescriptor>>>,
    ruby_version: Arc<RwLock<Option<String>>>,
    classpath_fingerprint: Arc<RwLock<Option<String>>>,
    imports: Arc<RwLock<Option<Arc<JrubyImportProvider>>>>,
}
impl ProjectRuntimeState {
    pub fn selected(&self) -> &RwLock<Option<SelectedRuntimeDescriptor>> {
        &self.selected
    }
    pub fn ruby_version(&self) -> &RwLock<Option<String>> {
        &self.ruby_version
    }
    pub fn classpath_fingerprint(&self) -> &RwLock<Option<String>> {
        &self.classpath_fingerprint
    }
    pub(crate) fn imports(&self) -> &RwLock<Option<Arc<JrubyImportProvider>>> {
        &self.imports
    }
}

#[derive(Clone)]
struct DependencyRequireState {
    paths: Arc<RwLock<Vec<PathBuf>>>,
    index: Arc<RwLock<Arc<crate::indexer::require_paths::RequireFeatureIndex>>>,
}
impl Default for DependencyRequireState {
    fn default() -> Self {
        Self {
            paths: Arc::default(),
            index: Arc::new(RwLock::new(Arc::new(
                crate::indexer::require_paths::RequireFeatureIndex::empty(),
            ))),
        }
    }
}
impl DependencyRequireState {
    fn replace(
        &self,
        paths: Vec<PathBuf>,
        index: Arc<crate::indexer::require_paths::RequireFeatureIndex>,
    ) {
        *self.paths.write() = paths;
        *self.index.write() = index;
    }
}

/// One Ruby project root. Files are routed to the workspace whose `root_path`
/// is the longest prefix of the file's path. Each project owns an isolated
/// analysis engine; external documents may retain one project's context.
#[derive(Clone)]
pub struct Workspace {
    pub root_uri: Url,
    pub root_path: PathBuf,
    pub indexing_status: Arc<ProjectIndexingStatus>,
    pub analysis_engine: Arc<RwLock<AnalysisEngine>>,
    pub runtime: ProjectRuntimeState,
    pub(crate) extension_project_context_seed: Arc<RwLock<ProjectContextSeed>>,
    pub(crate) navigation_demands: NavigationDemandController,
    requires: DependencyRequireState,
    workspace_folder_uris: Arc<RwLock<std::collections::HashSet<Url>>>,
}

impl Workspace {
    pub fn new(root_uri: Url) -> Self {
        Self::for_workspace_folder(root_uri.clone(), root_uri, false)
    }

    fn for_workspace_folder(
        root_uri: Url,
        workspace_folder_uri: Url,
        workspace_trusted: bool,
    ) -> Self {
        let root_path = root_uri
            .to_file_path()
            .unwrap_or_else(|_| PathBuf::from(root_uri.path()));
        let extension_project_context_seed = Arc::new(RwLock::new(ProjectContextSeed::detect(
            root_uri.to_string(),
            &root_path,
            workspace_trusted,
            None,
        )));
        Self {
            root_uri,
            indexing_status: Arc::new(ProjectIndexingStatus::new(root_path.clone())),
            root_path,
            analysis_engine: Arc::new(RwLock::new(AnalysisEngine::new())),
            runtime: ProjectRuntimeState::default(),
            extension_project_context_seed,
            navigation_demands: NavigationDemandController::default(),
            requires: DependencyRequireState::default(),
            workspace_folder_uris: Arc::new(RwLock::new(std::collections::HashSet::from([
                workspace_folder_uri,
            ]))),
        }
    }

    #[doc(hidden)]
    pub fn begin_indexing_run(&self) -> IndexingRun {
        let run = self.indexing_status.begin_run();
        self.navigation_demands.begin_generation(run.generation());
        run
    }

    pub(crate) fn cancel_current_indexing_run(&self) {
        let generation = self.indexing_status.snapshot().generation;
        let _ = self.indexing_status.cancel_current();
        self.navigation_demands.cancel_generation(generation);
    }

    /// Absolute gem/stdlib require roots retained after dependency indexing.
    pub fn dependency_require_paths(&self) -> Vec<PathBuf> {
        self.requires.paths.read().clone()
    }

    #[cfg(test)]
    pub(crate) fn set_dependency_require_paths(&self, paths: Vec<PathBuf>) {
        let index = Arc::new(crate::indexer::require_paths::RequireFeatureIndex::build(
            &paths, None,
        ));
        self.set_dependency_require_resolution(paths, index);
    }

    /// Hold this identity guard through delayed require-fact commit/publication.
    pub(super) fn require_feature_guard(
        &self,
    ) -> parking_lot::RwLockReadGuard<'_, Arc<crate::indexer::require_paths::RequireFeatureIndex>>
    {
        self.requires.index.read()
    }

    pub fn require_feature_index(&self) -> Arc<crate::indexer::require_paths::RequireFeatureIndex> {
        self.requires.index.read().clone()
    }

    pub(crate) fn set_dependency_require_resolution(
        &self,
        paths: Vec<PathBuf>,
        index: Arc<crate::indexer::require_paths::RequireFeatureIndex>,
    ) {
        self.requires.replace(paths, index);
    }
}

#[derive(Clone)]
pub(super) struct ProjectRegistry {
    workspaces: Arc<RwLock<Vec<Workspace>>>,
    orphan_engine: Arc<RwLock<AnalysisEngine>>,
    external_documents: Arc<RwLock<HashMap<Url, Url>>>,
}
impl Default for ProjectRegistry {
    fn default() -> Self {
        Self {
            workspaces: Arc::default(),
            orphan_engine: new_orphan_analysis_engine(),
            external_documents: Arc::default(),
        }
    }
}
impl ProjectRegistry {
    /// Retain project ownership while committing delayed facts.
    pub(super) fn read(&self) -> parking_lot::RwLockReadGuard<'_, Vec<Workspace>> {
        self.workspaces.read()
    }

    pub(super) fn orphan_engine(&self) -> &Arc<RwLock<AnalysisEngine>> {
        &self.orphan_engine
    }
}

impl RubyLanguageServer {
    pub fn orphan_engine(&self) -> &Arc<RwLock<AnalysisEngine>> {
        self.projects.orphan_engine()
    }
    /// Find the registered workspace whose `root_path` is the longest prefix
    /// of the given URI's filesystem path. Returns `None` if the URI does not
    /// belong to any registered workspace.
    pub fn workspace_for_uri(&self, uri: &Url) -> Option<Workspace> {
        self.projects.workspace_for_uri(uri)
    }

    /// Return the isolated semantic engine that owns a document URI. Files
    /// outside registered workspaces use the orphan engine.
    pub fn analysis_engine_for_uri(&self, uri: &Url) -> Arc<RwLock<AnalysisEngine>> {
        self.projects.analysis_engine_for_uri(uri)
    }

    /// Absolute gem/stdlib require roots for the workspace that owns `uri`.
    pub fn dependency_require_paths_for_uri(&self, uri: &Url) -> Vec<PathBuf> {
        self.projects.dependency_require_paths_for_uri(uri)
    }

    pub fn require_feature_index_for_uri(
        &self,
        uri: &Url,
    ) -> Arc<crate::indexer::require_paths::RequireFeatureIndex> {
        self.projects.require_feature_index_for_uri(uri)
    }

    pub(crate) fn extension_project_context_for_uri(
        &self,
        uri: &Url,
        source_kind: SourceKind,
    ) -> Option<ProjectContext> {
        self.extension_project_context_snapshot_for_uri(uri, source_kind)
            .map(|snapshot| snapshot.context)
    }

    pub(crate) fn extension_project_context_snapshot_for_uri(
        &self,
        uri: &Url,
        source_kind: SourceKind,
    ) -> Option<ProjectContextSnapshot> {
        self.analysis_workspace_for_uri(uri).map(|workspace| {
            workspace
                .extension_project_context_seed
                .read()
                .context_snapshot(uri.to_string(), source_kind)
        })
    }

    pub(crate) fn extension_project_context_for_document(
        &self,
        uri: &Url,
    ) -> Option<ProjectContext> {
        let path = uri
            .to_file_path()
            .unwrap_or_else(|_| PathBuf::from(uri.to_string()));
        let engine = self.analysis_engine_for_uri(uri);
        let kind = {
            let engine = engine.read();
            engine
                .file_id(&path)
                .and_then(|file_id| engine.file(file_id).map(|file| file.kind))
                .unwrap_or(SourceKind::Excluded)
        };
        self.extension_project_context_for_uri(uri, kind)
    }

    pub(crate) fn extension_project_context_seed_for_root(
        &self,
        root: &PathBuf,
    ) -> Option<Arc<RwLock<ProjectContextSeed>>> {
        self.projects
            .read()
            .iter()
            .find(|workspace| &workspace.root_path == root)
            .map(|workspace| workspace.extension_project_context_seed.clone())
    }

    pub(crate) fn set_extension_project_ruby_version(
        &self,
        root: &PathBuf,
        ruby_version: Option<String>,
    ) {
        let Some(seed) = self.extension_project_context_seed_for_root(root) else {
            return;
        };
        seed.write().ruby_version = ruby_version.clone();
        if let Some(workspace) = self
            .projects
            .read()
            .iter()
            .find(|workspace| &workspace.root_path == root)
        {
            *workspace.runtime.ruby_version().write() = ruby_version;
        }
    }

    pub(crate) fn set_runtime_classpath_fingerprint(
        &self,
        root: &PathBuf,
        fingerprint: Option<String>,
    ) {
        if let Some(workspace) = self
            .projects
            .read()
            .iter()
            .find(|workspace| &workspace.root_path == root)
        {
            *workspace.runtime.classpath_fingerprint().write() = fingerprint;
        }
    }

    pub(crate) fn set_jruby_import_provider(
        &self,
        root: &Path,
        provider: Option<Arc<JrubyImportProvider>>,
    ) {
        if let Some(workspace) = self
            .projects
            .read()
            .iter()
            .find(|workspace| workspace.root_path == root)
        {
            *workspace.runtime.imports().write() = provider;
        }
    }

    pub(crate) fn jruby_import_provider_for_uri(
        &self,
        uri: &Url,
    ) -> Option<Arc<JrubyImportProvider>> {
        self.analysis_workspace_for_uri(uri)
            .and_then(|workspace| workspace.runtime.imports().read().clone())
    }

    pub(crate) fn set_effective_runtime(
        &self,
        root: &PathBuf,
        runtime: Option<SelectedRuntimeDescriptor>,
    ) {
        if let Some(workspace) = self
            .projects
            .read()
            .iter()
            .find(|workspace| &workspace.root_path == root)
        {
            *workspace.runtime.selected().write() = runtime;
        }
    }

    pub(crate) fn refresh_extension_project_dependencies_for_uri(&self, uri: &Url) {
        let Some(workspace) = self.analysis_workspace_for_uri(uri) else {
            return;
        };
        workspace
            .extension_project_context_seed
            .write()
            .refresh_dependencies(&workspace.root_path);
    }

    /// Return the project semantic context for a URI. Project-owned paths win,
    /// followed by retained external navigation provenance, followed by a
    /// unique project engine that already owns the exact dependency path.
    pub fn analysis_workspace_for_uri(&self, uri: &Url) -> Option<Workspace> {
        self.projects.analysis_workspace_for_uri(uri)
    }

    /// Retain the originating project for external locations returned by a
    /// semantic request. LSP follow-up requests carry only the target URI, so
    /// this provenance is required to preserve the correct bundle context.
    pub fn retain_external_document_project(&self, uri: &Url, project: &Workspace) {
        self.projects.retain_external_document_project(uri, project)
    }

    pub fn release_external_document_project(&self, uri: &Url) {
        self.projects.release_external_document_project(uri)
    }

    pub(crate) fn release_external_documents_for_project(&self, project_root_uri: &Url) {
        self.projects
            .release_external_documents_for_project(project_root_uri)
    }

    /// Snapshot every active project engine plus the orphan engine.
    pub fn analysis_engines(&self) -> Vec<Arc<RwLock<AnalysisEngine>>> {
        self.projects.analysis_engines()
    }

    pub fn clear_file_from_other_engines(&self, uri: &Url, owner: &Arc<RwLock<AnalysisEngine>>) {
        self.projects.clear_file_from_other_engines(uri, owner)
    }

    pub fn open_or_update_analysis_file(
        &self,
        uri: &Url,
        source: impl Into<String>,
    ) -> SourceFileId {
        self.projects.open_or_update_analysis_file(uri, source)
    }

    pub fn open_or_update_analysis_file_with_kind(
        &self,
        uri: &Url,
        source: impl Into<String>,
        kind: SourceKind,
    ) -> SourceFileId {
        self.projects
            .open_or_update_analysis_file_with_kind(uri, source, kind)
    }

    /// Register a new workspace. If a workspace with the same root URI is
    /// already registered, returns the existing one without creating a new
    /// index. Returns the (existing or newly created) `Workspace`.
    pub fn add_workspace(&self, root_uri: Url) -> Workspace {
        self.add_project(root_uri.clone(), root_uri)
    }

    /// Register an editor workspace folder, expanding a container folder into
    /// its nearest Gemfile-owned Ruby projects.
    pub fn add_workspace_folder(&self, folder_uri: Url) -> anyhow::Result<Vec<Workspace>> {
        let folder_path = folder_uri.to_file_path().map_err(|_| {
            anyhow::anyhow!(
                "Workspace folder URI is not a filesystem path: {}",
                folder_uri
            )
        })?;
        let explicit_roots = self.config.lock().indexing.project_roots.clone();
        let roots = crate::indexer::project_roots::discover_project_roots_with_explicit(
            &folder_path,
            &explicit_roots,
        )?;
        roots
            .into_iter()
            .map(|root| {
                let root_uri = Url::from_directory_path(&root).map_err(|_| {
                    anyhow::anyhow!(
                        "Ruby project root is not a valid file URI: {}",
                        root.display()
                    )
                })?;
                Ok(self.add_project(root_uri, folder_uri.clone()))
            })
            .collect()
    }

    fn add_project(&self, root_uri: Url, workspace_folder_uri: Url) -> Workspace {
        let trusted = self.config.lock().workspace_trusted;
        self.projects
            .add_project(root_uri, workspace_folder_uri, trusted)
    }

    pub fn remove_workspace(&self, root_uri: &Url) {
        self.projects.remove_workspace(root_uri)
    }

    pub fn remove_workspace_folder(&self, folder_uri: &Url) {
        self.projects.remove_workspace_folder(folder_uri)
    }

    pub fn cancel_all_indexing(&self) {
        self.projects.cancel_all_indexing()
    }

    /// Snapshot of all currently registered workspaces.
    pub fn list_workspaces(&self) -> Vec<Workspace> {
        self.projects.list_workspaces()
    }

    pub fn workspace_root_paths(&self) -> Vec<PathBuf> {
        self.projects.workspace_root_paths()
    }
}

impl ProjectRegistry {
    pub fn workspace_for_uri(&self, uri: &Url) -> Option<Workspace> {
        let file_path = uri.to_file_path().ok()?;
        Self::workspace_for_path(&self.workspaces.read(), &file_path).cloned()
    }

    pub fn analysis_engine_for_uri(&self, uri: &Url) -> Arc<RwLock<AnalysisEngine>> {
        self.analysis_workspace_for_uri(uri)
            .map(|workspace| workspace.analysis_engine)
            .unwrap_or_else(|| self.orphan_engine().clone())
    }

    pub fn dependency_require_paths_for_uri(&self, uri: &Url) -> Vec<PathBuf> {
        self.workspace_for_uri(uri)
            .map(|workspace| workspace.dependency_require_paths())
            .unwrap_or_default()
    }

    pub fn require_feature_index_for_uri(
        &self,
        uri: &Url,
    ) -> Arc<crate::indexer::require_paths::RequireFeatureIndex> {
        self.workspace_for_uri(uri)
            .map(|workspace| workspace.require_feature_index())
            .unwrap_or_else(
                || Arc::new(crate::indexer::require_paths::RequireFeatureIndex::empty()),
            )
    }

    pub fn analysis_workspace_for_uri(&self, uri: &Url) -> Option<Workspace> {
        if let Some(workspace) = self.workspace_for_uri(uri) {
            return Some(workspace);
        }

        if let Some(root_uri) = self.external_documents.read().get(uri).cloned() {
            if let Some(workspace) = self
                .workspaces
                .read()
                .iter()
                .find(|workspace| workspace.root_uri == root_uri)
                .cloned()
            {
                return Some(workspace);
            }
            self.external_documents.write().remove(uri);
        }

        let path = uri.to_file_path().ok()?;
        let mut owner = None;
        for workspace in self.workspaces.read().iter() {
            if workspace.analysis_engine.read().file_id(&path).is_none() {
                continue;
            }
            if owner.is_some() {
                return None;
            }
            owner = Some(workspace.clone());
        }
        owner
    }

    pub fn retain_external_document_project(&self, uri: &Url, project: &Workspace) {
        if self.workspace_for_uri(uri).is_none() {
            self.external_documents
                .write()
                .insert(uri.clone(), project.root_uri.clone());
        }
    }

    pub fn release_external_document_project(&self, uri: &Url) {
        self.external_documents.write().remove(uri);
    }

    pub(crate) fn release_external_documents_for_project(&self, project_root_uri: &Url) {
        self.external_documents
            .write()
            .retain(|_, retained_root| retained_root != project_root_uri);
    }

    pub fn analysis_engines(&self) -> Vec<Arc<RwLock<AnalysisEngine>>> {
        let mut engines = self
            .workspaces
            .read()
            .iter()
            .map(|workspace| workspace.analysis_engine.clone())
            .collect::<Vec<_>>();
        engines.push(self.orphan_engine().clone());
        engines
    }

    pub fn clear_file_from_other_engines(&self, uri: &Url, owner: &Arc<RwLock<AnalysisEngine>>) {
        let path = uri
            .to_file_path()
            .unwrap_or_else(|_| PathBuf::from(uri.to_string()));
        for analysis_engine in self.analysis_engines() {
            if Arc::ptr_eq(&analysis_engine, owner) {
                continue;
            }
            let mut engine = analysis_engine.write();
            if let Some(file_id) = engine.file_id(&path) {
                engine.replace_facts(
                    file_id,
                    ruby_analysis::engine::FileFacts::default(),
                    ruby_analysis::engine::ResolveMode::Immediate,
                );
            }
        }
    }

    pub fn open_or_update_analysis_file(
        &self,
        uri: &Url,
        source: impl Into<String>,
    ) -> SourceFileId {
        self.open_or_update_analysis_file_with_kind(uri, source, SourceKind::Project)
    }

    pub fn open_or_update_analysis_file_with_kind(
        &self,
        uri: &Url,
        source: impl Into<String>,
        kind: SourceKind,
    ) -> SourceFileId {
        let path = uri
            .to_file_path()
            .unwrap_or_else(|_| PathBuf::from(uri.to_string()));
        self.analysis_engine_for_uri(uri).write().register_file(
            ruby_analysis::engine::SourceFileInput {
                path,
                content: source.into(),
                kind,
            },
        )
    }

    pub fn remove_workspace(&self, root_uri: &Url) {
        self.workspaces.write().retain(|workspace| {
            if workspace.root_uri != *root_uri {
                return true;
            }
            workspace.cancel_current_indexing_run();
            false
        });
    }

    pub fn remove_workspace_folder(&self, folder_uri: &Url) {
        self.workspaces.write().retain(|workspace| {
            let mut owners = workspace.workspace_folder_uris.write();
            owners.remove(folder_uri);
            if owners.is_empty() {
                workspace.cancel_current_indexing_run();
                false
            } else {
                true
            }
        });
    }

    pub fn cancel_all_indexing(&self) {
        for workspace in self.workspaces.read().iter() {
            workspace.cancel_current_indexing_run();
        }
    }

    pub fn list_workspaces(&self) -> Vec<Workspace> {
        self.workspaces.read().clone()
    }

    pub fn workspace_root_paths(&self) -> Vec<PathBuf> {
        let mut roots = self
            .workspaces
            .read()
            .iter()
            .map(|workspace| workspace.root_path.clone())
            .collect::<Vec<_>>();
        roots.sort();
        roots.dedup();
        roots
    }

    pub(super) fn workspace_for_path<'a>(
        workspaces: &'a [Workspace],
        path: &Path,
    ) -> Option<&'a Workspace> {
        workspaces
            .iter()
            .filter(|workspace| path.starts_with(&workspace.root_path))
            .max_by_key(|workspace| workspace.root_path.as_os_str().len())
    }

    fn add_project(
        &self,
        root_uri: Url,
        workspace_folder_uri: Url,
        workspace_trusted: bool,
    ) -> Workspace {
        {
            let workspaces = self.workspaces.read();
            if let Some(existing) = workspaces.iter().find(|w| w.root_uri == root_uri) {
                existing
                    .workspace_folder_uris
                    .write()
                    .insert(workspace_folder_uri);
                return existing.clone();
            }
        }
        let ws = Workspace::for_workspace_folder(root_uri, workspace_folder_uri, workspace_trusted);
        self.workspaces.write().push(ws.clone());
        ws
    }
}
