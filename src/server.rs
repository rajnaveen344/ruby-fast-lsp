use crate::analyzer_prism::visitors::index_visitor::IndexVisitor;
use crate::handlers::{notification, request};
use crate::indexer::index::RubyIndex;
use crate::types::ruby_document::RubyDocument;
use anyhow::Result;
use log::{debug, info};
use tower_lsp::lsp_types::{
    CompletionItem, CompletionParams, CompletionResponse, Diagnostic, DidChangeTextDocumentParams,
    DidCloseTextDocumentParams, DidOpenTextDocumentParams, DocumentOnTypeFormattingParams,
    DocumentSymbolParams, DocumentSymbolResponse, FoldingRange, FoldingRangeParams,
    GotoDefinitionParams, GotoDefinitionResponse, InitializeParams, InitializeResult, 
    InitializedParams, InlayHintParams, Location, ReferenceParams, SemanticTokensParams, 
    SemanticTokensResult, SymbolInformation, TextEdit, Url, WorkspaceSymbolParams,
};
use parking_lot::{Mutex, RwLock};
use ruby_prism::Visit;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::{Client, LanguageServer};

#[derive(Clone)]
pub struct RubyLanguageServer {
    pub client: Option<Client>,
    pub index: Arc<Mutex<RubyIndex>>,
    pub docs: Arc<Mutex<HashMap<Url, Arc<RwLock<RubyDocument>>>>>,
}

impl RubyLanguageServer {
    pub fn new(client: Client) -> Result<Self> {
        let index = RubyIndex::new();
        Ok(Self {
            client: Some(client),
            index: Arc::new(Mutex::new(index)),
            docs: Arc::new(Mutex::new(HashMap::new())),
        })
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

    pub fn process_file(&mut self, uri: Url, content: &str) -> Result<(), String> {
        self.index.lock().remove_entries_for_uri(&uri);

        // Create or update document in the docs HashMap
        let document = RubyDocument::new(uri.clone(), content.to_string(), 0);
        self.docs
            .lock()
            .insert(uri.clone(), Arc::new(RwLock::new(document)));

        let parse_result = ruby_prism::parse(content.as_bytes());
        let node = parse_result.node();
        let mut visitor = IndexVisitor::new(self, uri.clone());

        visitor.visit(&node);

        // Persist mutations made by the visitor back to the server's document store
        // TODO: This is a temporary fix. We should be able to mutate the document in place
        //       using docs: Arc<Mutex<HashMap<Url, Arc<Mutex<RubyDocument>>>>>
        // self.docs
        //     .lock()
        //     .unwrap()
        //     .insert(uri.clone(), Arc::new(RwLock::new(visitor.document.clone())));

        debug!("Processed file: {}", uri);
        Ok(())
    }

    /// Publish diagnostics for a document
    pub async fn publish_diagnostics(&self, uri: Url, diagnostics: Vec<Diagnostic>) {
        if let Some(client) = &self.client {
            let _ = client.publish_diagnostics(uri, diagnostics, None).await;
        }
    }
}

impl Default for RubyLanguageServer {
    fn default() -> Self {
        RubyLanguageServer {
            client: None,
            index: Arc::new(Mutex::new(RubyIndex::new())),
            docs: Arc::new(Mutex::new(HashMap::new())),
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
}
