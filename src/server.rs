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
    pub client: Option<Client>,
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
            client: Some(client),
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

    // Add a method to RubyLanguageServer to index workspace with caching
    pub async fn index_workspace_folder(&self, folder_uri: &Url) {
        let mut indexer = self.indexer.lock().await;

        if let Err(e) = events::index_workspace_folder(&mut indexer, folder_uri, |msg| {
            let client = self.client.clone();
            if let Some(client) = client {
                tokio::spawn(async move {
                    client.log_message(MessageType::INFO, msg).await;
                });
            }
        })
        .await
        {
            if let Some(client) = self.client.clone() {
                client
                    .log_message(
                        MessageType::ERROR,
                        format!("Error indexing workspace: {:?}", e),
                    )
                    .await;
            }
        }

        // Cache the content of all indexed files
        if let Ok(folder_path) = folder_uri.to_file_path() {
            if let Ok(files) = events::find_ruby_files(&folder_path).await {
                for file_path in files {
                    if let Ok(content) = tokio::fs::read_to_string(&file_path).await {
                        if let Ok(file_uri) = Url::from_file_path(&file_path) {
                            self.update_document_content(file_uri, content).await;
                        }
                    }
                }
            }
        }
    }

    pub async fn index_file(&self, file_uri: &Url) {
        let file_path = file_uri.to_file_path().unwrap_or_default();
        if file_path.extension().map_or(false, |ext| ext == "rb") {
            // Read the file content and cache it
            if let Ok(content) = std::fs::read_to_string(&file_path) {
                // Cache the content in document_cache
                self.update_document_content(file_uri.clone(), content)
                    .await;

                // Index the file
                let mut indexer = self.indexer.lock().await;
                let _ = events::index_workspace_file(&mut indexer, &file_path).await;
            }
        }
    }
}

impl Default for RubyLanguageServer {
    fn default() -> Self {
        RubyLanguageServer {
            client: None,
            indexer: Mutex::new(RubyIndexer::new().unwrap()),
            document_cache: Mutex::new(HashMap::new()),
            workspace_root: Mutex::new(None),
        }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for RubyLanguageServer {
    async fn initialize(&self, params: InitializeParams) -> LspResult<InitializeResult> {
        request::handle_initialize(self, params).await
    }

    async fn initialized(&self, params: InitializedParams) {
        notification::handle_initialized(self, params).await
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
