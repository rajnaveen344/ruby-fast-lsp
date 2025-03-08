use crate::handlers::{notification, request};
use crate::indexer::events;
use crate::indexer::traverser::RubyIndexer;
use anyhow::Result;
use lsp_types::*;
use std::collections::HashMap;
use tokio::sync::Mutex;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::{Client, LanguageServer};

pub struct RubyLanguageServer {
    pub client: Client,
    pub indexer: Mutex<RubyIndexer>,
    pub document_cache: Mutex<HashMap<Url, String>>,
    pub workspace_root: Mutex<Option<Url>>,
}

impl RubyLanguageServer {
    pub fn new(client: Client) -> Result<Self> {
        let indexer = RubyIndexer::new().map_err(|e| anyhow::anyhow!(e))?;
        let document_cache = Mutex::new(HashMap::new());
        let workspace_root = Mutex::new(None);

        Ok(Self {
            client,
            indexer: Mutex::new(indexer),
            document_cache,
            workspace_root,
        })
    }

    // Helper method to get document content from cache
    pub async fn get_document_content(&self, uri: &Url) -> Option<String> {
        let cache = self.document_cache.lock().await;
        cache.get(uri).cloned()
    }

    // Helper method to update document content in cache
    pub async fn update_document_content(&self, uri: Url, content: String) {
        let mut cache = self.document_cache.lock().await;
        cache.insert(uri, content);
    }

    // Helper method to remove document from cache
    pub async fn remove_document(&self, uri: &Url) {
        let mut cache = self.document_cache.lock().await;
        cache.remove(uri);
    }

    // Add a method to RubyLanguageServer to index workspace
    pub async fn index_workspace_folder(&self, folder_uri: &Url) {
        let mut indexer = self.indexer.lock().await;

        if let Err(e) = events::index_workspace_folder(&mut indexer, folder_uri, |msg| {
            let client = self.client.clone();
            tokio::spawn(async move {
                client.log_message(MessageType::INFO, msg).await;
            });
        })
        .await
        {
            self.client
                .log_message(
                    MessageType::ERROR,
                    format!("Error indexing workspace: {:?}", e),
                )
                .await;
        }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for RubyLanguageServer {
    async fn initialize(&self, params: InitializeParams) -> LspResult<InitializeResult> {
        request::handle_initialize(self, params).await
    }

    async fn initialized(&self, params: InitializedParams) {
        request::handle_initialized(self, params).await
    }

    async fn shutdown(&self) -> LspResult<()> {
        request::handle_shutdown(self).await
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        notification::handle_did_open(self, params).await
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        notification::handle_did_change(self, params).await
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        notification::handle_did_close(self, params).await
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> LspResult<Option<GotoDefinitionResponse>> {
        request::handle_goto_definition(self, params).await
    }

    async fn references(&self, params: ReferenceParams) -> LspResult<Option<Vec<Location>>> {
        request::handle_references(self, params).await
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> LspResult<Option<SemanticTokensResult>> {
        request::handle_semantic_tokens_full(self, params).await
    }

    async fn semantic_tokens_range(
        &self,
        params: SemanticTokensRangeParams,
    ) -> LspResult<Option<SemanticTokensRangeResult>> {
        request::handle_semantic_tokens_range(self, params).await
    }
}
