use crate::capabilities::debug::{
    AncestorsParams, AncestorsResponse, ListCommandsResponse, LookupParams, LookupResponse,
    MethodsParams, MethodsResponse, StatsParams, StatsResponse,
};
use crate::capabilities::namespace_tree::{NamespaceTreeParams, NamespaceTreeResponse};
use crate::config::RubyFastLspConfig;
use crate::handlers::{notification, request};
use crate::indexer::index::RubyIndex;
use crate::type_inference::TypeNarrowingEngine;
use crate::types::ruby_document::RubyDocument;
use anyhow::Result;
use log::{debug, info, warn};
use parking_lot::{Mutex, RwLock};
use std::collections::HashMap;
use std::process::exit;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::{
    CodeLens, CodeLensParams, CompletionItem, CompletionParams, CompletionResponse, Diagnostic,
    DidChangeConfigurationParams, DidChangeTextDocumentParams, DidChangeWatchedFilesParams,
    DidCloseTextDocumentParams, DidOpenTextDocumentParams, DidSaveTextDocumentParams,
    DocumentOnTypeFormattingParams, DocumentSymbolParams, DocumentSymbolResponse, FoldingRange,
    FoldingRangeParams, GotoDefinitionParams, GotoDefinitionResponse, InitializeParams,
    InitializeResult, InitializedParams, InlayHintParams, Location, ReferenceParams,
    SemanticTokensParams, SemanticTokensResult, SymbolInformation, TextEdit, Url,
    WorkspaceSymbolParams,
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
        result != 0 && exit_code == windows_sys::Win32::System::Threading::STILL_ACTIVE
    }
}

#[derive(Clone)]
pub struct RubyLanguageServer {
    pub client: Option<Client>,
    pub index: Arc<Mutex<RubyIndex>>,
    pub docs: Arc<Mutex<HashMap<Url, Arc<RwLock<RubyDocument>>>>>,
    pub config: Arc<Mutex<RubyFastLspConfig>>,
    pub namespace_tree_cache: Arc<Mutex<Option<(u64, NamespaceTreeResponse)>>>,
    pub cache_invalidation_timer: Arc<Mutex<Option<Instant>>>,
    /// Timer for debounced reindexing on document changes
    pub reindex_timer: Arc<Mutex<Option<(Instant, Url)>>>,
    pub workspace_uri: Arc<Mutex<Option<Url>>>,
    /// The process ID of the parent process (VS Code extension host).
    /// Used to detect when the parent process dies so we can exit cleanly.
    pub parent_process_id: Arc<Mutex<Option<u32>>>,
    /// Type narrowing engine for CFG-based type analysis
    pub type_narrowing: Arc<TypeNarrowingEngine>,
    /// Whether initial indexing is complete
    pub indexing_complete: Arc<std::sync::atomic::AtomicBool>,
}

impl RubyLanguageServer {
    pub fn new(client: Client) -> Result<Self> {
        let index = RubyIndex::new();
        let config = RubyFastLspConfig::default();
        Ok(Self {
            client: Some(client),
            index: Arc::new(Mutex::new(index)),
            docs: Arc::new(Mutex::new(HashMap::new())),
            config: Arc::new(Mutex::new(config)),
            namespace_tree_cache: Arc::new(Mutex::new(None)),
            cache_invalidation_timer: Arc::new(Mutex::new(None)),
            reindex_timer: Arc::new(Mutex::new(None)),
            workspace_uri: Arc::new(Mutex::new(None)),
            parent_process_id: Arc::new(Mutex::new(None)),
            type_narrowing: Arc::new(TypeNarrowingEngine::new()),
            indexing_complete: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        })
    }

    /// Check if initial indexing is complete.
    pub fn is_indexing_complete(&self) -> bool {
        self.indexing_complete
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Mark initial indexing as complete.
    pub fn set_indexing_complete(&self) {
        self.indexing_complete
            .store(true, std::sync::atomic::Ordering::Relaxed);
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

    pub fn set_workspace_uri(&self, uri: Option<Url>) {
        *self.workspace_uri.lock() = uri;
    }

    pub fn get_workspace_uri(&self) -> Option<Url> {
        self.workspace_uri.lock().clone()
    }

    pub fn index(&self) -> Arc<Mutex<RubyIndex>> {
        self.index.clone()
    }

    pub fn get_doc(&self, uri: &Url) -> Option<RubyDocument> {
        self.docs
            .lock()
            .get(uri)
            .map(|doc_arc| doc_arc.read().clone())
    }

    /// Publish diagnostics for a document
    pub async fn publish_diagnostics(&self, uri: Url, diagnostics: Vec<Diagnostic>) {
        if let Some(client) = &self.client {
            let _ = client.publish_diagnostics(uri, diagnostics, None).await;
        }
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
            index: Arc::new(Mutex::new(RubyIndex::new())),
            docs: Arc::new(Mutex::new(HashMap::new())),
            config: Arc::new(Mutex::new(RubyFastLspConfig::default())),
            namespace_tree_cache: Arc::new(Mutex::new(None)),
            cache_invalidation_timer: Arc::new(Mutex::new(None)),
            reindex_timer: Arc::new(Mutex::new(None)),
            workspace_uri: Arc::new(Mutex::new(None)),
            parent_process_id: Arc::new(Mutex::new(None)),
            type_narrowing: Arc::new(TypeNarrowingEngine::new()),
            indexing_complete: Arc::new(std::sync::atomic::AtomicBool::new(false)),
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
}
