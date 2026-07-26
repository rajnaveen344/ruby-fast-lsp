use crate::capabilities::debug::{
    AncestorsParams, AncestorsResponse, ListCommandsResponse, LookupParams, LookupResponse,
    MethodsParams, MethodsResponse, StatsParams, StatsResponse,
};
use crate::config::{runtime::SelectedRuntimeDescriptor, RubyFastLspConfig};
use crate::extensions::{
    ExtensionRegistryHandle, ExtensionStatusParams, ExtensionStatusResponse, ProjectContextSeed,
};
use crate::handlers::{notification, request};
use crate::query::namespace_tree::{NamespaceTreeParams, NamespaceTreeResponse};
use crate::runtime::catalog::{
    DiscoveredRuntime, ProjectRuntimeStatus, RuntimeCatalog, RuntimeDiscoverParams, RuntimeStatus,
    RuntimeStatusParams,
};
use anyhow::Result;
use log::{debug, info, warn};
use parking_lot::{Mutex, RwLock};
use ruby_analysis::core::{SourceFileId, SourceKind};
use ruby_analysis::engine::AnalysisEngine;
use ruby_analysis::indexer::RubyDocument;
use ruby_fast_lsp_extension_api::ProjectContext;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::exit;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::{
    CallHierarchyIncomingCall, CallHierarchyIncomingCallsParams, CallHierarchyItem,
    CallHierarchyOutgoingCall, CallHierarchyOutgoingCallsParams, CallHierarchyPrepareParams,
    CodeActionOrCommand, CodeActionParams, CodeLens, CodeLensParams, CompletionItem,
    CompletionParams, CompletionResponse, Diagnostic, DidChangeConfigurationParams,
    DidChangeTextDocumentParams, DidChangeWatchedFilesParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DidSaveTextDocumentParams, DocumentFormattingParams,
    DocumentHighlight, DocumentHighlightParams, DocumentOnTypeFormattingParams,
    DocumentSymbolParams, DocumentSymbolResponse, FoldingRange, FoldingRangeParams,
    GotoDefinitionParams, GotoDefinitionResponse, InitializeParams, InitializeResult,
    InitializedParams, InlayHintParams, Location, PrepareRenameResponse, ReferenceParams,
    RenameParams, SelectionRange, SelectionRangeParams, SemanticTokensParams, SemanticTokensResult,
    SignatureHelp, SignatureHelpParams, SymbolInformation, TextDocumentPositionParams, TextEdit,
    TypeHierarchyItem, TypeHierarchyPrepareParams, TypeHierarchySubtypesParams,
    TypeHierarchySupertypesParams, Url, WorkspaceEdit, WorkspaceSymbolParams,
};
use tower_lsp::{Client, LanguageServer};

/// Check if a process with the given PID is still running.
/// Returns true if the process is alive, false if it has exited.
#[cfg(unix)]
fn is_process_alive(pid: u32) -> bool {
    // On Unix, sending signal 0 to a process checks if it exists without actually sending a signal
    // kill(pid, 0) returns 0 if the process exists and we have permission to send it signals
    // It returns -1 with ESRCH if the process doesn't exist
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

#[cfg(windows)]
fn is_process_alive(pid: u32) -> bool {
    use std::ptr::null_mut;

    // On Windows, we try to open the process with minimal access rights
    // If the process doesn't exist, OpenProcess returns NULL
    unsafe {
        let handle = windows_sys::Win32::System::Threading::OpenProcess(
            windows_sys::Win32::System::Threading::PROCESS_QUERY_LIMITED_INFORMATION,
            0, // bInheritHandle = FALSE
            pid,
        );

        if handle.is_null() {
            return false;
        }

        // Check if the process has exited
        let mut exit_code: u32 = 0;
        let result =
            windows_sys::Win32::System::Threading::GetExitCodeProcess(handle, &mut exit_code);

        windows_sys::Win32::Foundation::CloseHandle(handle);

        // STILL_ACTIVE (259) means the process is still running
        const STILL_ACTIVE: u32 = 259;
        result != 0 && exit_code == STILL_ACTIVE
    }
}

/// One Ruby project root. Files are routed to the workspace whose `root_path`
/// is the longest prefix of the file's path. Each project owns an isolated
/// analysis engine; external documents may retain one project's context.
#[derive(Clone)]
pub struct Workspace {
    pub root_uri: Url,
    pub root_path: PathBuf,
    pub indexing_complete: Arc<AtomicBool>,
    pub analysis_engine: Arc<RwLock<AnalysisEngine>>,
    pub effective_runtime: Arc<RwLock<Option<SelectedRuntimeDescriptor>>>,
    pub runtime_classpath_fingerprint_sha256: Arc<RwLock<Option<String>>>,
    pub(crate) extension_project_context_seed: Arc<RwLock<ProjectContextSeed>>,
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
            root_path,
            indexing_complete: Arc::new(AtomicBool::new(false)),
            analysis_engine: Arc::new(RwLock::new(AnalysisEngine::new())),
            effective_runtime: Arc::new(RwLock::new(None)),
            runtime_classpath_fingerprint_sha256: Arc::new(RwLock::new(None)),
            extension_project_context_seed,
            workspace_folder_uris: Arc::new(RwLock::new(std::collections::HashSet::from([
                workspace_folder_uri,
            ]))),
        }
    }
}

#[derive(Clone)]
pub struct RubyLanguageServer {
    pub client: Option<Client>,
    /// Registered workspace folders. Routed by longest-prefix path match in
    /// `workspace_for_uri`.
    pub workspaces: Arc<RwLock<Vec<Workspace>>>,
    pub docs: Arc<Mutex<HashMap<Url, Arc<RwLock<RubyDocument>>>>>,
    /// Analysis state for open files outside every registered Ruby project.
    /// Project facts live in `Workspace::analysis_engine` and must never be
    /// written here.
    pub analysis_engine: Arc<RwLock<AnalysisEngine>>,
    /// Project context retained for external dependency documents reached from
    /// a project-owned navigation result. Values are project root URIs rather
    /// than engine handles so workspace removal cannot leave a live stale
    /// semantic owner behind.
    external_document_projects: Arc<RwLock<HashMap<Url, Url>>>,
    pub config: Arc<Mutex<RubyFastLspConfig>>,
    discovered_runtimes: Arc<tokio::sync::OnceCell<Vec<DiscoveredRuntime>>>,
    pub extension_registry: ExtensionRegistryHandle,
    pub extension_watch_dynamic_registration: Arc<AtomicBool>,
    pub extension_watch_registration: Arc<tokio::sync::Mutex<Vec<String>>>,
    pub namespace_tree_cache: Arc<Mutex<Option<(u64, NamespaceTreeResponse)>>>,
    pub cache_invalidation_timer: Arc<Mutex<Option<Instant>>>,
    /// Timer for debounced reindexing on document changes
    pub reindex_timer: Arc<Mutex<Option<(Instant, Url)>>>,
    /// The process ID of the parent process (VS Code extension host).
    /// Used to detect when the parent process dies so we can exit cleanly.
    pub parent_process_id: Arc<Mutex<Option<u32>>>,
    #[cfg(test)]
    published_diagnostics: Arc<Mutex<HashMap<Url, Vec<Diagnostic>>>>,
    #[cfg(test)]
    user_cache_root_override: Arc<Mutex<Option<PathBuf>>>,
}

impl RubyLanguageServer {
    pub fn new(client: Client) -> Result<Self> {
        let config = RubyFastLspConfig::default();
        let extension_registry = ExtensionRegistryHandle::from_config(&config);
        Ok(Self {
            client: Some(client),
            workspaces: Arc::new(RwLock::new(Vec::new())),
            docs: Arc::new(Mutex::new(HashMap::new())),
            analysis_engine: Arc::new(RwLock::new(AnalysisEngine::new())),
            external_document_projects: Arc::new(RwLock::new(HashMap::new())),
            config: Arc::new(Mutex::new(config)),
            discovered_runtimes: Arc::new(tokio::sync::OnceCell::new()),
            extension_registry,
            extension_watch_dynamic_registration: Arc::new(AtomicBool::new(false)),
            extension_watch_registration: Arc::new(tokio::sync::Mutex::new(Vec::new())),
            namespace_tree_cache: Arc::new(Mutex::new(None)),
            cache_invalidation_timer: Arc::new(Mutex::new(None)),
            reindex_timer: Arc::new(Mutex::new(None)),
            parent_process_id: Arc::new(Mutex::new(None)),
            #[cfg(test)]
            published_diagnostics: Arc::new(Mutex::new(HashMap::new())),
            #[cfg(test)]
            user_cache_root_override: Arc::new(Mutex::new(None)),
        })
    }

    /// Returns true once every registered workspace has finished its initial
    /// indexing pass. With no workspaces registered, returns true vacuously
    /// (orphan-only mode has no coordinator to wait on).
    pub fn is_indexing_complete(&self) -> bool {
        self.workspaces.read().iter().all(|w| {
            w.indexing_complete
                .load(std::sync::atomic::Ordering::Relaxed)
        })
    }

    /// Set the parent process ID and start monitoring it.
    /// If the parent process dies, the LSP server will exit.
    pub fn set_parent_process_id(&self, pid: Option<u32>) {
        *self.parent_process_id.lock() = pid;
        if let Some(pid) = pid {
            self.start_parent_process_monitor(pid);
        }
    }

    /// Start a background task that monitors the parent process.
    /// If the parent process is no longer running, exit the server.
    fn start_parent_process_monitor(&self, parent_pid: u32) {
        info!("Starting parent process monitor for PID: {}", parent_pid);

        tokio::spawn(async move {
            let check_interval = Duration::from_secs(5);

            loop {
                sleep(check_interval).await;

                if !is_process_alive(parent_pid) {
                    warn!(
                        "Parent process (PID: {}) is no longer running. Exiting LSP server.",
                        parent_pid
                    );
                    // Give a moment for any pending operations to complete
                    sleep(Duration::from_millis(100)).await;
                    exit(0);
                }
            }
        });
    }

    // ========================================================================
    // Multi-workspace routing
    // ========================================================================

    /// Find the registered workspace whose `root_path` is the longest prefix
    /// of the given URI's filesystem path. Returns `None` if the URI does not
    /// belong to any registered workspace.
    pub fn workspace_for_uri(&self, uri: &Url) -> Option<Workspace> {
        let file_path = uri.to_file_path().ok()?;
        let workspaces = self.workspaces.read();
        let mut best: Option<&Workspace> = None;
        let mut best_len = 0usize;
        for ws in workspaces.iter() {
            if file_path.starts_with(&ws.root_path) {
                let len = ws.root_path.as_os_str().len();
                if len >= best_len {
                    best_len = len;
                    best = Some(ws);
                }
            }
        }
        best.cloned()
    }

    /// Return the isolated semantic engine that owns a document URI. Files
    /// outside registered workspaces use the orphan engine.
    pub fn analysis_engine_for_uri(&self, uri: &Url) -> Arc<RwLock<AnalysisEngine>> {
        self.analysis_workspace_for_uri(uri)
            .map(|workspace| workspace.analysis_engine)
            .unwrap_or_else(|| self.analysis_engine.clone())
    }

    pub(crate) fn extension_project_context_for_uri(
        &self,
        uri: &Url,
        source_kind: SourceKind,
    ) -> Option<ProjectContext> {
        self.analysis_workspace_for_uri(uri).map(|workspace| {
            workspace
                .extension_project_context_seed
                .read()
                .context(uri.to_string(), source_kind)
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
        self.workspaces
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
        seed.write().ruby_version = ruby_version;
    }

    pub(crate) fn set_runtime_classpath_fingerprint(
        &self,
        root: &PathBuf,
        fingerprint: Option<String>,
    ) {
        if let Some(workspace) = self
            .workspaces
            .read()
            .iter()
            .find(|workspace| &workspace.root_path == root)
        {
            *workspace.runtime_classpath_fingerprint_sha256.write() = fingerprint;
        }
    }

    pub(crate) fn set_effective_runtime(
        &self,
        root: &PathBuf,
        runtime: Option<SelectedRuntimeDescriptor>,
    ) {
        if let Some(workspace) = self
            .workspaces
            .read()
            .iter()
            .find(|workspace| &workspace.root_path == root)
        {
            *workspace.effective_runtime.write() = runtime;
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
        if let Some(workspace) = self.workspace_for_uri(uri) {
            return Some(workspace);
        }

        if let Some(root_uri) = self.external_document_projects.read().get(uri).cloned() {
            if let Some(workspace) = self
                .workspaces
                .read()
                .iter()
                .find(|workspace| workspace.root_uri == root_uri)
                .cloned()
            {
                return Some(workspace);
            }
            self.external_document_projects.write().remove(uri);
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

    /// Retain the originating project for external locations returned by a
    /// semantic request. LSP follow-up requests carry only the target URI, so
    /// this provenance is required to preserve the correct bundle context.
    pub fn retain_external_document_project(&self, uri: &Url, project: &Workspace) {
        if self.workspace_for_uri(uri).is_none() {
            self.external_document_projects
                .write()
                .insert(uri.clone(), project.root_uri.clone());
        }
    }

    pub fn release_external_document_project(&self, uri: &Url) {
        self.external_document_projects.write().remove(uri);
    }

    pub(crate) fn release_external_documents_for_project(&self, project_root_uri: &Url) {
        self.external_document_projects
            .write()
            .retain(|_, retained_root| retained_root != project_root_uri);
    }

    /// Snapshot every active project engine plus the orphan engine.
    pub fn analysis_engines(&self) -> Vec<Arc<RwLock<AnalysisEngine>>> {
        let mut engines = self
            .workspaces
            .read()
            .iter()
            .map(|workspace| workspace.analysis_engine.clone())
            .collect::<Vec<_>>();
        engines.push(self.analysis_engine.clone());
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
        let workspace_trusted = self.config.lock().workspace_trusted;
        let ws = Workspace::for_workspace_folder(root_uri, workspace_folder_uri, workspace_trusted);
        self.workspaces.write().push(ws.clone());
        ws
    }

    pub fn remove_workspace(&self, root_uri: &Url) {
        self.workspaces.write().retain(|w| w.root_uri != *root_uri);
    }

    pub fn remove_workspace_folder(&self, folder_uri: &Url) {
        self.workspaces.write().retain(|workspace| {
            let mut owners = workspace.workspace_folder_uris.write();
            owners.remove(folder_uri);
            !owners.is_empty()
        });
    }

    /// Snapshot of all currently registered workspaces.
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

    pub fn get_doc(&self, uri: &Url) -> Option<RubyDocument> {
        self.docs
            .lock()
            .get(uri)
            .map(|doc_arc| doc_arc.read().clone())
    }

    /// Publish diagnostics for a document
    pub async fn publish_diagnostics(&self, uri: Url, diagnostics: Vec<Diagnostic>) {
        #[cfg(test)]
        self.published_diagnostics
            .lock()
            .insert(uri.clone(), diagnostics.clone());
        if let Some(client) = &self.client {
            let _ = client.publish_diagnostics(uri, diagnostics, None).await;
        }
    }

    #[cfg(test)]
    pub fn last_published_diagnostics(&self, uri: &Url) -> Vec<Diagnostic> {
        self.published_diagnostics
            .lock()
            .get(uri)
            .cloned()
            .unwrap_or_default()
    }

    #[cfg(test)]
    pub(crate) fn set_user_cache_root_for_tests(&self, root: PathBuf) {
        *self.user_cache_root_override.lock() = Some(root);
    }

    #[cfg(test)]
    pub(crate) fn user_cache_root_for_tests(&self) -> Option<PathBuf> {
        self.user_cache_root_override.lock().clone()
    }

    /// Request the client to refresh inlay hints
    pub async fn refresh_inlay_hints(&self) {
        if let Some(client) = &self.client {
            // Send workspace/inlayHint/refresh request to client
            let _ = client
                .send_request::<tower_lsp::lsp_types::request::InlayHintRefreshRequest>(())
                .await;
        }
    }

    pub async fn handle_namespace_tree_request(
        &self,
        params: NamespaceTreeParams,
    ) -> LspResult<NamespaceTreeResponse> {
        request::handle_namespace_tree(self, params).await
    }

    // ========================================================================
    // Debug Request Handlers
    // ========================================================================

    /// Handle `$/listCommands` - return available custom debug commands.
    pub async fn handle_list_commands(&self) -> LspResult<ListCommandsResponse> {
        request::handle_list_commands(self).await
    }

    /// Handle `ruby-fast-lsp/debug/lookup` - query index for an FQN.
    pub async fn handle_debug_lookup(&self, params: LookupParams) -> LspResult<LookupResponse> {
        request::handle_debug_lookup(self, params).await
    }

    /// Handle `ruby-fast-lsp/debug/stats` - return index statistics.
    pub async fn handle_debug_stats(&self, params: StatsParams) -> LspResult<StatsResponse> {
        request::handle_debug_stats(self, params).await
    }

    /// Handle `ruby-fast-lsp/debug/ancestors` - return inheritance chain.
    pub async fn handle_debug_ancestors(
        &self,
        params: AncestorsParams,
    ) -> LspResult<AncestorsResponse> {
        request::handle_debug_ancestors(self, params).await
    }

    /// Handle `ruby-fast-lsp/debug/methods` - list methods for a class.
    pub async fn handle_debug_methods(&self, params: MethodsParams) -> LspResult<MethodsResponse> {
        request::handle_debug_methods(self, params).await
    }

    /// Handle `ruby-fast-lsp/debug/inference-stats` - get type inference statistics.
    pub async fn handle_debug_inference_stats(
        &self,
        params: crate::capabilities::debug::InferenceStatsParams,
    ) -> LspResult<crate::capabilities::debug::InferenceStatsResponse> {
        request::handle_debug_inference_stats(self, params).await
    }

    /// Handle `ruby/exportGraph` - export the inheritance graph as JSON.
    pub async fn handle_export_graph(
        &self,
        params: crate::capabilities::debug::ExportGraphParams,
    ) -> LspResult<crate::capabilities::debug::ExportGraphResponse> {
        request::handle_export_graph(self, params).await
    }

    /// Handle `ruby-fast-lsp/extensions/status` - list loaded extension states.
    pub async fn handle_extension_status(
        &self,
        params: ExtensionStatusParams,
    ) -> LspResult<ExtensionStatusResponse> {
        request::handle_extension_status(self, params).await
    }

    /// Handle `ruby-fast-lsp/runtime/discover` without exposing editor policy
    /// or mutating any project runtime selection.
    pub async fn handle_runtime_discover(
        &self,
        _params: RuntimeDiscoverParams,
    ) -> LspResult<RuntimeCatalog> {
        Ok(crate::runtime::catalog::runtime_catalog_for_projects(
            self.workspace_root_paths(),
            self.discovered_runtimes().await,
        ))
    }

    async fn discovered_runtimes(&self) -> Vec<DiscoveredRuntime> {
        self.discovered_runtimes
            .get_or_init(crate::runtime::catalog::discover_runtimes)
            .await
            .clone()
    }

    #[cfg(test)]
    pub(crate) fn set_discovered_runtimes_for_tests(&self, runtimes: Vec<DiscoveredRuntime>) {
        self.discovered_runtimes.set(runtimes).expect(
            "INVARIANT VIOLATED: test runtime catalog was initialized more than once. This is a bug because each test server must own one immutable discovery snapshot. Fix: create a fresh RubyLanguageServer per runtime test.",
        );
    }

    pub(crate) async fn resolve_auto_runtime(
        &self,
        project_root: &std::path::Path,
    ) -> Result<Option<SelectedRuntimeDescriptor>> {
        let Some(marker) = crate::runtime::catalog::project_runtime_marker(project_root)? else {
            return Ok(None);
        };
        let runtime = crate::runtime::catalog::select_runtime_for_marker(
            &marker,
            &self.discovered_runtimes().await,
        )?;
        let Some(runtime) = runtime else {
            warn!(
                "Project runtime marker `{marker}` has no exact installed runtime for {}",
                project_root.display()
            );
            return Ok(None);
        };
        if runtime.support_status != crate::runtime::catalog::RuntimeSupportStatus::Supported {
            return Err(anyhow::anyhow!(
                "project runtime marker `{marker}` selects unsupported {}",
                runtime.display_name
            ));
        }
        Ok(Some(runtime.into()))
    }

    pub async fn handle_runtime_status(
        &self,
        params: RuntimeStatusParams,
    ) -> LspResult<RuntimeStatus> {
        let config = self.config.lock().clone();
        let mut projects = self
            .list_workspaces()
            .into_iter()
            .filter(|workspace| {
                params
                    .project_root
                    .as_ref()
                    .is_none_or(|root| root == &workspace.root_path)
            })
            .map(|workspace| {
                let root = workspace.root_path.to_string_lossy();
                let selection = config
                    .runtime
                    .selection_for_project(&root, &config.ruby_version);
                let (
                    mode,
                    implementation,
                    family,
                    engine_version,
                    compatibility_version,
                    executable,
                    java_home,
                    stub_overlay,
                ) = match selection {
                    crate::config::runtime::EffectiveRuntimeSelection::Explicit(runtime) => {
                        let stub_overlay = (runtime.implementation
                            == crate::runtime::catalog::RuntimeImplementation::Jruby)
                            .then(|| runtime.family.clone());
                        (
                            "explicit".to_string(),
                            Some(runtime.implementation),
                            Some(runtime.family),
                            Some(runtime.engine_version),
                            Some(runtime.compatibility_version),
                            Some(runtime.executable),
                            runtime.java_home,
                            stub_overlay,
                        )
                    }
                    crate::config::runtime::EffectiveRuntimeSelection::Auto => {
                        if let Some(runtime) = workspace.effective_runtime.read().clone() {
                            let stub_overlay = (runtime.implementation
                                == crate::runtime::catalog::RuntimeImplementation::Jruby)
                                .then(|| runtime.family.clone());
                            (
                                "auto".to_string(),
                                Some(runtime.implementation),
                                Some(runtime.family),
                                Some(runtime.engine_version),
                                Some(runtime.compatibility_version),
                                Some(runtime.executable),
                                runtime.java_home,
                                stub_overlay,
                            )
                        } else {
                            ("auto".to_string(), None, None, None, None, None, None, None)
                        }
                    }
                    crate::config::runtime::EffectiveRuntimeSelection::LegacyMriCompatibility {
                        major,
                        minor,
                    } => {
                        let compatibility = format!("{major}.{minor}");
                        (
                            "legacy".to_string(),
                            Some(crate::runtime::catalog::RuntimeImplementation::Mri),
                            Some(compatibility.clone()),
                            None,
                            Some(compatibility),
                            None,
                            None,
                            None,
                        )
                    }
                };
                ProjectRuntimeStatus {
                    root: workspace.root_path,
                    mode,
                    implementation,
                    family,
                    engine_version,
                    compatibility_version,
                    executable,
                    java_home,
                    stub_overlay,
                    classpath_fingerprint_sha256: workspace
                        .runtime_classpath_fingerprint_sha256
                        .read()
                        .clone(),
                    indexing_complete: workspace
                        .indexing_complete
                        .load(std::sync::atomic::Ordering::Acquire),
                }
            })
            .collect::<Vec<_>>();
        projects.sort_by(|left, right| left.root.cmp(&right.root));
        Ok(RuntimeStatus { projects })
    }

    /// Invalidate namespace tree cache with debouncing (300ms delay)
    pub fn invalidate_namespace_tree_cache_debounced(&self) {
        let server = self.clone();
        tokio::spawn(async move {
            // Set the timer to current time
            {
                let mut timer = server.cache_invalidation_timer.lock();
                *timer = Some(Instant::now());
            }

            // Wait for the debounce period
            sleep(Duration::from_millis(300)).await;

            // Check if we should still invalidate (no newer timer was set)
            let should_invalidate = {
                let timer = server.cache_invalidation_timer.lock();
                if let Some(timer_instant) = *timer {
                    timer_instant.elapsed() >= Duration::from_millis(300)
                } else {
                    false
                }
            };

            if should_invalidate {
                *server.namespace_tree_cache.lock() = None;
                debug!("Namespace tree cache invalidated after debounce period");

                // Clear the timer
                *server.cache_invalidation_timer.lock() = None;
            }
        });
    }
}

impl Default for RubyLanguageServer {
    fn default() -> Self {
        Self {
            client: None,
            workspaces: Arc::new(RwLock::new(Vec::new())),
            docs: Arc::new(Mutex::new(HashMap::new())),
            analysis_engine: Arc::new(RwLock::new(AnalysisEngine::new())),
            external_document_projects: Arc::new(RwLock::new(HashMap::new())),
            config: Arc::new(Mutex::new(RubyFastLspConfig::default())),
            discovered_runtimes: Arc::new(tokio::sync::OnceCell::new()),
            namespace_tree_cache: Arc::new(Mutex::new(None)),
            cache_invalidation_timer: Arc::new(Mutex::new(None)),
            reindex_timer: Arc::new(Mutex::new(None)),
            parent_process_id: Arc::new(Mutex::new(None)),
            #[cfg(test)]
            published_diagnostics: Arc::new(Mutex::new(HashMap::new())),
            #[cfg(test)]
            user_cache_root_override: Arc::new(Mutex::new(None)),
            extension_registry: ExtensionRegistryHandle::from_environment(),
            extension_watch_dynamic_registration: Arc::new(AtomicBool::new(false)),
            extension_watch_registration: Arc::new(tokio::sync::Mutex::new(Vec::new())),
        }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for RubyLanguageServer {
    async fn initialize(&self, params: InitializeParams) -> LspResult<InitializeResult> {
        notification::handle_initialize(self, params).await
    }

    async fn initialized(&self, params: InitializedParams) {
        notification::handle_initialized(self, params).await
    }

    async fn shutdown(&self) -> LspResult<()> {
        notification::handle_shutdown(self).await
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        info!("Document opened: {}", params.text_document.uri.path());
        let start_time = Instant::now();
        notification::handle_did_open(self, params).await;
        info!(
            "[PERF] Document open handler completed in {:?}",
            start_time.elapsed()
        );
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        info!("Document changed: {}", params.text_document.uri.path());
        let start_time = Instant::now();
        notification::handle_did_change(self, params).await;
        info!(
            "[PERF] Document change handler completed in {:?}",
            start_time.elapsed()
        );
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        info!("Document closed: {}", params.text_document.uri.path());
        let start_time = Instant::now();
        notification::handle_did_close(self, params).await;
        info!(
            "[PERF] Document close handler completed in {:?}",
            start_time.elapsed()
        );
    }

    async fn did_change_configuration(&self, params: DidChangeConfigurationParams) {
        info!("Configuration changed");
        let start_time = Instant::now();
        notification::handle_did_change_configuration(self, params).await;
        info!(
            "[PERF] Configuration change handler completed in {:?}",
            start_time.elapsed()
        );
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        info!("Document saved: {}", params.text_document.uri.path());
        let start_time = Instant::now();
        notification::handle_did_save(self, params).await;
        info!(
            "[PERF] Document save handler completed in {:?}",
            start_time.elapsed()
        );
    }

    async fn did_change_watched_files(&self, params: DidChangeWatchedFilesParams) {
        info!("Watched files changed: {} files", params.changes.len());
        let start_time = Instant::now();
        notification::handle_did_change_watched_files(self, params).await;
        info!(
            "[PERF] Watched files change handler completed in {:?}",
            start_time.elapsed()
        );
    }

    async fn did_change_workspace_folders(
        &self,
        params: tower_lsp::lsp_types::DidChangeWorkspaceFoldersParams,
    ) {
        info!(
            "Workspace folders changed: +{} -{}",
            params.event.added.len(),
            params.event.removed.len()
        );
        let start_time = Instant::now();
        notification::handle_did_change_workspace_folders(self, params).await;
        info!(
            "[PERF] Workspace folder change handler completed in {:?}",
            start_time.elapsed()
        );
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> LspResult<Option<GotoDefinitionResponse>> {
        info!(
            "Goto definition request received for {:?}",
            params
                .text_document_position_params
                .text_document
                .uri
                .path()
        );
        let start_time = Instant::now();
        let result = request::handle_goto_definition(self, params).await;

        info!(
            "[PERF] Goto definition completed in {:?}",
            start_time.elapsed()
        );

        result
    }

    async fn goto_implementation(
        &self,
        params: GotoDefinitionParams,
    ) -> LspResult<Option<GotoDefinitionResponse>> {
        info!(
            "Goto implementation request received for {:?}",
            params
                .text_document_position_params
                .text_document
                .uri
                .path()
        );
        let start_time = Instant::now();
        let result = request::handle_goto_implementation(self, params).await;

        info!(
            "[PERF] Goto implementation completed in {:?}",
            start_time.elapsed()
        );

        result
    }

    async fn references(&self, params: ReferenceParams) -> LspResult<Option<Vec<Location>>> {
        info!(
            "References request received for {:?}",
            params.text_document_position.text_document.uri.path()
        );
        let start_time = Instant::now();
        let result = request::handle_references(self, params).await;

        info!("[PERF] References completed in {:?}", start_time.elapsed());

        result
    }

    async fn document_highlight(
        &self,
        params: DocumentHighlightParams,
    ) -> LspResult<Option<Vec<DocumentHighlight>>> {
        request::handle_document_highlight(self, params).await
    }

    async fn selection_range(
        &self,
        params: SelectionRangeParams,
    ) -> LspResult<Option<Vec<SelectionRange>>> {
        request::handle_selection_ranges(self, params).await
    }

    async fn signature_help(
        &self,
        params: SignatureHelpParams,
    ) -> LspResult<Option<SignatureHelp>> {
        request::handle_signature_help(self, params).await
    }

    async fn code_action(
        &self,
        params: CodeActionParams,
    ) -> LspResult<Option<Vec<CodeActionOrCommand>>> {
        request::handle_code_actions(self, params).await
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> LspResult<Option<SemanticTokensResult>> {
        info!(
            "Semantic tokens request received for {:?}",
            params.text_document.uri.path()
        );
        let start_time = Instant::now();
        let result = request::handle_semantic_tokens_full(self, params).await;

        info!(
            "[PERF] Semantic tokens completed in {:?}",
            start_time.elapsed()
        );

        result
    }

    async fn inlay_hint(
        &self,
        params: InlayHintParams,
    ) -> LspResult<Option<Vec<tower_lsp::lsp_types::InlayHint>>> {
        info!(
            "Inlay hint request received for {:?}",
            params.text_document.uri.path()
        );

        let start_time = Instant::now();
        let result = request::handle_inlay_hints(self, params).await;

        info!("[PERF] Inlay hint completed in {:?}", start_time.elapsed());

        result
    }

    async fn completion(&self, params: CompletionParams) -> LspResult<Option<CompletionResponse>> {
        info!(
            "Completion request received for {:?}",
            params.text_document_position.text_document.uri.path()
        );
        let start_time = Instant::now();
        let result = request::handle_completion(self, params).await;

        info!("[PERF] Completion completed in {:?}", start_time.elapsed());

        result
    }

    async fn completion_resolve(&self, params: CompletionItem) -> LspResult<CompletionItem> {
        info!(
            "Completion item resolve request received for {}",
            params.label
        );
        let start_time = Instant::now();
        let result = request::handle_completion_resolve(self, params).await;

        info!(
            "[PERF] Completion item resolve completed in {:?}",
            start_time.elapsed()
        );

        result
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> LspResult<Option<DocumentSymbolResponse>> {
        info!(
            "Document symbol request received for {:?}",
            params.text_document.uri.path()
        );

        let start_time = Instant::now();
        let result = request::handle_document_symbols(self, params).await;

        info!(
            "[PERF] Document symbols completed in {:?}",
            start_time.elapsed()
        );

        Ok(result)
    }

    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> LspResult<Option<Vec<SymbolInformation>>> {
        info!(
            "Workspace symbol request received for query: '{}'",
            params.query
        );

        let start_time = Instant::now();
        let result = request::handle_workspace_symbols(self, params).await;

        info!(
            "[PERF] Workspace symbols completed in {:?}",
            start_time.elapsed()
        );

        result
    }

    async fn on_type_formatting(
        &self,
        params: DocumentOnTypeFormattingParams,
    ) -> LspResult<Option<Vec<TextEdit>>> {
        info!(
            "Document on type formatting request received for {:?}",
            params.text_document_position.text_document.uri.path()
        );

        let start_time = Instant::now();
        let result = request::handle_document_on_type_formatting(self, params).await;

        info!(
            "[PERF] Document on type formatting completed in {:?}",
            start_time.elapsed()
        );

        result
    }

    async fn formatting(
        &self,
        params: DocumentFormattingParams,
    ) -> LspResult<Option<Vec<TextEdit>>> {
        info!(
            "Document formatting request received for {:?}",
            params.text_document.uri.path()
        );
        request::handle_document_formatting(self, params).await
    }

    async fn folding_range(
        &self,
        params: FoldingRangeParams,
    ) -> LspResult<Option<Vec<FoldingRange>>> {
        info!(
            "Folding range request received for {:?}",
            params.text_document.uri.path()
        );

        let start_time = Instant::now();
        let result = request::handle_folding_range(self, params).await;

        info!(
            "[PERF] Folding range completed in {:?}",
            start_time.elapsed()
        );

        result
    }

    async fn code_lens(&self, params: CodeLensParams) -> LspResult<Option<Vec<CodeLens>>> {
        info!(
            "CodeLens request received for {:?}",
            params.text_document.uri.path()
        );

        let start_time = Instant::now();
        let result = request::handle_code_lens(self, params).await;

        info!("[PERF] CodeLens completed in {:?}", start_time.elapsed());

        result
    }

    async fn hover(
        &self,
        params: tower_lsp::lsp_types::HoverParams,
    ) -> LspResult<Option<tower_lsp::lsp_types::Hover>> {
        info!(
            "Hover request received for {:?}",
            params
                .text_document_position_params
                .text_document
                .uri
                .path()
        );

        let start_time = Instant::now();
        let result = request::handle_hover(self, params).await;

        info!("[PERF] Hover completed in {:?}", start_time.elapsed());

        result
    }

    async fn prepare_type_hierarchy(
        &self,
        params: TypeHierarchyPrepareParams,
    ) -> LspResult<Option<Vec<TypeHierarchyItem>>> {
        request::handle_prepare_type_hierarchy(self, params).await
    }

    async fn supertypes(
        &self,
        params: TypeHierarchySupertypesParams,
    ) -> LspResult<Option<Vec<TypeHierarchyItem>>> {
        request::handle_supertypes(self, params).await
    }

    async fn subtypes(
        &self,
        params: TypeHierarchySubtypesParams,
    ) -> LspResult<Option<Vec<TypeHierarchyItem>>> {
        request::handle_subtypes(self, params).await
    }

    async fn prepare_call_hierarchy(
        &self,
        params: CallHierarchyPrepareParams,
    ) -> LspResult<Option<Vec<CallHierarchyItem>>> {
        request::handle_prepare_call_hierarchy(self, params).await
    }

    async fn incoming_calls(
        &self,
        params: CallHierarchyIncomingCallsParams,
    ) -> LspResult<Option<Vec<CallHierarchyIncomingCall>>> {
        request::handle_incoming_calls(self, params).await
    }

    async fn outgoing_calls(
        &self,
        params: CallHierarchyOutgoingCallsParams,
    ) -> LspResult<Option<Vec<CallHierarchyOutgoingCall>>> {
        request::handle_outgoing_calls(self, params).await
    }

    async fn rename(&self, params: RenameParams) -> LspResult<Option<WorkspaceEdit>> {
        request::handle_rename(self, params).await
    }

    async fn prepare_rename(
        &self,
        params: TextDocumentPositionParams,
    ) -> LspResult<Option<PrepareRenameResponse>> {
        request::handle_prepare_rename(self, params).await
    }
}

#[cfg(test)]
mod runtime_status_tests {
    use super::*;
    use crate::config::runtime::{
        ProjectRuntimeSelection, RuntimeMode, RuntimeSelection, RuntimeSelectionConfig,
        SelectedRuntimeDescriptor,
    };
    use crate::runtime::catalog::{
        DiscoveredRuntime, RuntimeDiscoverySource, RuntimeImplementation, RuntimeStatusParams,
        RuntimeSupportStatus,
    };

    #[tokio::test]
    async fn runtime_status_reports_server_owned_project_identity_and_classpath() {
        let fixture = tempfile::tempdir().unwrap();
        let admin = fixture.path().join("admin");
        let server_project = fixture.path().join("server");
        std::fs::create_dir_all(&admin).unwrap();
        std::fs::create_dir_all(&server_project).unwrap();
        let language_server = RubyLanguageServer::default();
        let admin_workspace =
            language_server.add_workspace(Url::from_directory_path(&admin).unwrap());
        language_server.add_workspace(Url::from_directory_path(&server_project).unwrap());
        *language_server.config.lock() = RubyFastLspConfig {
            runtime: RuntimeSelectionConfig {
                mode: RuntimeMode::Auto,
                projects: vec![ProjectRuntimeSelection {
                    root: admin.to_string_lossy().to_string(),
                    selection: RuntimeSelection::Explicit(SelectedRuntimeDescriptor {
                        implementation: RuntimeImplementation::Jruby,
                        family: "9.2".to_string(),
                        engine_version: "9.2.21.0".to_string(),
                        compatibility_version: "2.5".to_string(),
                        executable: fixture.path().join("jruby/bin/jruby"),
                        discovery_source: RuntimeDiscoverySource::Rvm,
                        java_home: Some(fixture.path().join("jdk")),
                    }),
                }],
            },
            ..RubyFastLspConfig::default()
        };
        language_server.set_runtime_classpath_fingerprint(&admin, Some("a".repeat(64)));
        admin_workspace
            .indexing_complete
            .store(true, std::sync::atomic::Ordering::Release);

        let status = language_server
            .handle_runtime_status(RuntimeStatusParams {
                project_root: Some(admin.clone()),
            })
            .await
            .unwrap();

        assert_eq!(status.projects.len(), 1);
        let status = &status.projects[0];
        assert_eq!(status.root, admin);
        assert_eq!(status.mode, "explicit");
        assert_eq!(status.implementation, Some(RuntimeImplementation::Jruby));
        assert_eq!(status.family.as_deref(), Some("9.2"));
        assert_eq!(status.engine_version.as_deref(), Some("9.2.21.0"));
        assert_eq!(status.compatibility_version.as_deref(), Some("2.5"));
        assert_eq!(status.stub_overlay.as_deref(), Some("9.2"));
        assert_eq!(
            status.classpath_fingerprint_sha256.as_deref(),
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        );
        assert!(status.indexing_complete);

        let auto_runtime = SelectedRuntimeDescriptor {
            implementation: RuntimeImplementation::Mri,
            family: "3.3".to_string(),
            engine_version: "3.3.11".to_string(),
            compatibility_version: "3.3".to_string(),
            executable: fixture.path().join("ruby-3.3.11/bin/ruby"),
            discovery_source: RuntimeDiscoverySource::Rbenv,
            java_home: None,
        };
        language_server.set_effective_runtime(&server_project, Some(auto_runtime));
        let auto = language_server
            .handle_runtime_status(RuntimeStatusParams {
                project_root: Some(server_project),
            })
            .await
            .unwrap();
        let auto = &auto.projects[0];
        assert_eq!(auto.mode, "auto");
        assert_eq!(auto.implementation, Some(RuntimeImplementation::Mri));
        assert_eq!(auto.engine_version.as_deref(), Some("3.3.11"));
        assert_eq!(
            auto.executable,
            Some(fixture.path().join("ruby-3.3.11/bin/ruby"))
        );
    }

    #[tokio::test]
    async fn auto_runtime_resolves_exact_project_marker_through_server_catalog() {
        let fixture = tempfile::tempdir().unwrap();
        let admin = fixture.path().join("admin");
        std::fs::create_dir_all(&admin).unwrap();
        std::fs::write(admin.join(".ruby-version"), "jruby-9.2.21.0\n").unwrap();
        let executable = fixture.path().join("jruby-9.2.21.0/bin/jruby");
        let java_home = fixture.path().join("jdk-17");
        let language_server = RubyLanguageServer::default();
        language_server.add_workspace(Url::from_directory_path(&admin).unwrap());
        language_server.set_discovered_runtimes_for_tests(vec![DiscoveredRuntime {
            implementation: RuntimeImplementation::Jruby,
            implementation_label: "JRuby".to_string(),
            family: "9.2".to_string(),
            family_label: "JRuby 9.2 (Ruby 2.5)".to_string(),
            compatibility_version: "2.5".to_string(),
            compatibility_label: "Ruby 2.5".to_string(),
            engine_version: "9.2.21.0".to_string(),
            display_name: "JRuby 9.2.21.0 (Ruby 2.5)".to_string(),
            executable: executable.clone(),
            discovery_source: RuntimeDiscoverySource::Rvm,
            support_status: RuntimeSupportStatus::Supported,
            java_home: Some(java_home.clone()),
        }]);

        let resolved = language_server
            .resolve_auto_runtime(&admin)
            .await
            .unwrap()
            .expect("the exact installed JRuby marker must resolve");
        assert_eq!(resolved.engine_version, "9.2.21.0");
        assert_eq!(resolved.compatibility_version, "2.5");
        assert_eq!(resolved.executable, executable);
        assert_eq!(resolved.java_home, Some(java_home));
    }
}
