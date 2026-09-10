mod diagnostics;
mod documents;
mod extensions;
mod indexing;
mod namespace_tree;
mod products;
mod projects;
mod watched_files;

use diagnostics::DiagnosticPublisher;
pub(crate) use documents::OpenDocuments;
pub(crate) use extensions::ExtensionServices;
pub(crate) use indexing::IndexingServices;
use namespace_tree::NamespaceTreeCache;
pub(crate) use products::RuntimeProducts;
pub use products::{RetainedProductSnapshot, RuntimeProductSnapshot};
use projects::ProjectRegistry;
pub use projects::Workspace;
use watched_files::WatchedFileChanges;

use crate::capabilities::debug::{
    AncestorsParams, AncestorsResponse, ListCommandsResponse, LookupParams, LookupResponse,
    MethodsParams, MethodsResponse, StatsParams, StatsResponse,
};
use crate::config::RubyFastLspConfig;
use crate::extensions::{ExtensionRegistryHandle, ExtensionStatusParams, ExtensionStatusResponse};
use crate::handlers::{notification, request};

use crate::query::namespace_tree::{NamespaceTreeParams, NamespaceTreeResponse};

use anyhow::Result;
use log::{info, warn};
use parking_lot::Mutex;

use ruby_analysis::indexer::RubyDocument;

use std::path::{Path, PathBuf};
use std::process::exit;

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::{
    CallHierarchyIncomingCall, CallHierarchyIncomingCallsParams, CallHierarchyItem,
    CallHierarchyOutgoingCall, CallHierarchyOutgoingCallsParams, CallHierarchyPrepareParams,
    CodeActionOrCommand, CodeActionParams, CodeLens, CodeLensParams, CompletionItem,
    CompletionParams, CompletionResponse, DidChangeConfigurationParams,
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

pub(crate) const WATCHED_FILE_DEBOUNCE_INTERVAL: Duration = Duration::from_millis(100);

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

/// LSP adapter: each owner preserves its own locks and shared clone identity.
#[derive(Clone)]
pub struct RubyLanguageServer {
    pub(crate) client: Option<Client>,
    pub(crate) config: Arc<Mutex<RubyFastLspConfig>>,
    pub(crate) documents: OpenDocuments,
    pub(self) projects: ProjectRegistry,
    pub(crate) indexing: IndexingServices,
    pub(crate) products: RuntimeProducts,
    pub(crate) extensions: ExtensionServices,
    pub(self) diagnostics: DiagnosticPublisher,
    pub(self) file_changes: WatchedFileChanges,
    pub(self) namespace_tree: NamespaceTreeCache,
}

impl RubyLanguageServer {
    /// Copy the accepted configuration without exposing its shared lock.
    pub fn configuration_snapshot(&self) -> RubyFastLspConfig {
        self.config.lock().clone()
    }

    pub(crate) fn document_semantic_lock(&self, uri: &Url) -> Arc<tokio::sync::Mutex<()>> {
        self.documents.semantic_lock(uri)
    }

    pub fn get_doc(&self, uri: &Url) -> Option<RubyDocument> {
        self.documents
            .read()
            .get(uri)
            .map(|doc_arc| doc_arc.read().clone())
    }

    pub fn new(client: Client) -> Result<Self> {
        Self::with_cache_root(Some(client), crate::utils::ruby_fast_lsp_user_cache_root()?)
    }

    /// Construct an embedded server with an explicit ordinary cache location.
    pub fn with_user_cache_root(root: PathBuf) -> Result<Self> {
        Self::with_cache_root(None, root)
    }

    pub(crate) fn with_cache_root(client: Option<Client>, root: PathBuf) -> Result<Self> {
        let products = RuntimeProducts::new(root);
        // A client server discovers extensions during initialize, under the
        // resource governor. Embedded use retains eager environment loading.
        let registry = if client.is_some() {
            ExtensionRegistryHandle::empty_with_cache(products.persistent().clone())
        } else {
            ExtensionRegistryHandle::from_environment_with_cache(products.persistent().clone())
        };
        Ok(Self {
            client,
            config: Arc::new(Mutex::new(RubyFastLspConfig::default())),
            documents: OpenDocuments::default(),
            projects: ProjectRegistry::default(),
            indexing: IndexingServices::default(),
            products,
            extensions: ExtensionServices::new(registry),
            diagnostics: DiagnosticPublisher::default(),
            file_changes: WatchedFileChanges::default(),
            namespace_tree: NamespaceTreeCache::default(),
        })
    }

    /// Set the parent process ID and start monitoring it.
    /// If the parent process dies, the LSP server will exit.
    pub fn set_parent_process_id(&self, pid: Option<u32>) {
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

    /// Request the client to refresh inlay hints
    pub async fn refresh_inlay_hints(&self) {
        if let Some(client) = &self.client {
            // Send workspace/inlayHint/refresh request to client
            let _ = client
                .send_request::<tower_lsp::lsp_types::request::InlayHintRefreshRequest>(())
                .await;
        }
    }

    /// Refresh inlay hints only when this isolated project currently owns an
    /// open document. The LSP refresh request is global, so closed projects in
    /// an umbrella workspace must not each create redundant client work when
    /// their cold indexing generation converges.
    pub async fn refresh_inlay_hints_for_workspace(&self, workspace_root: &Path) {
        let open_uris = self.documents.read().keys().cloned().collect::<Vec<_>>();
        let owns_open_document = open_uris.iter().any(|uri| {
            self.workspace_for_uri(uri)
                .is_some_and(|workspace| workspace.root_path == workspace_root)
        });
        if owns_open_document {
            self.refresh_inlay_hints().await;
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
}

impl Default for RubyLanguageServer {
    fn default() -> Self {
        let root = crate::utils::ruby_fast_lsp_user_cache_root().expect(
            "INVARIANT VIOLATED: the default server could not resolve an absolute user cache root. This is a bug because embedded construction requires deterministic derived-product ownership. Fix: configure an absolute user cache root.",
        );
        Self::with_user_cache_root(root).expect("INVARIANT VIOLATED: embedded server construction failed. This is a bug because the resolved cache root must support ordinary server construction. Fix: inspect the cache root and extension initialization.")
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
        if let Some(workspace) = self.workspace_for_uri(&params.text_document.uri) {
            self.prioritize_indexing_project(&workspace.root_path);
        }
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
#[path = "server/tests.rs"]
mod runtime_status_tests;
