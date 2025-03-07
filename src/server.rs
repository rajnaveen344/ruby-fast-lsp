use anyhow::Result;
use lsp_types::*;
use std::collections::HashMap;
use tokio::sync::Mutex;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::{Client, LanguageServer};

use crate::capabilities::definition;
use crate::capabilities::references;
use crate::capabilities::semantic_tokens::semantic_tokens_options;
use crate::indexer::events;
use crate::indexer::traverser::RubyIndexer;

pub struct RubyLanguageServer {
    client: Client,
    indexer: Mutex<RubyIndexer>,
    document_cache: Mutex<HashMap<Url, String>>,
    workspace_root: Mutex<Option<Url>>,
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
    async fn get_document_content(&self, uri: &Url) -> Option<String> {
        let cache = self.document_cache.lock().await;
        cache.get(uri).cloned()
    }

    // Helper method to update document content in cache
    async fn update_document_content(&self, uri: Url, content: String) {
        let mut cache = self.document_cache.lock().await;
        cache.insert(uri, content);
    }

    // Helper method to remove document from cache
    async fn remove_document(&self, uri: &Url) {
        let mut cache = self.document_cache.lock().await;
        cache.remove(uri);
    }

    // Add a method to RubyLanguageServer to index workspace
    async fn index_workspace_folder(&self, folder_uri: &Url) {
        let log_message = |message: String| {
            let client = self.client.clone();
            async move {
                client.log_message(MessageType::INFO, message).await;
            }
        };

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

    // Helper to index a single document
    async fn index_document(&self, uri: &Url, content: &str) -> Result<()> {
        let mut indexer = self.indexer.lock().await;
        events::handle_did_open(&mut indexer, uri.clone(), content)
            .map_err(|e| anyhow::anyhow!("Failed to index document: {}", e))
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for RubyLanguageServer {
    async fn initialize(&self, params: InitializeParams) -> LspResult<InitializeResult> {
        self.client
            .log_message(MessageType::INFO, "Ruby LSP server initialized")
            .await;

        // Store the workspace root
        if let Some(folder) = params
            .workspace_folders
            .and_then(|folders| folders.first().cloned())
        {
            let mut root = self.workspace_root.lock().await;
            *root = Some(folder.uri);
        } else if let Some(root_uri) = params.root_uri {
            let mut root = self.workspace_root.lock().await;
            *root = Some(root_uri);
        }

        // Start indexing the workspace
        if let Some(root_uri) = self.workspace_root.lock().await.clone() {
            self.index_workspace_folder(&root_uri).await;
        }

        let capabilities = ServerCapabilities {
            text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
            definition_provider: Some(OneOf::Left(true)),
            references_provider: Some(OneOf::Left(true)),
            semantic_tokens_provider: Some(
                SemanticTokensServerCapabilities::SemanticTokensOptions(semantic_tokens_options()),
            ),
            ..ServerCapabilities::default()
        };

        Ok(InitializeResult {
            capabilities,
            ..InitializeResult::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "Server initialized")
            .await;
    }

    async fn shutdown(&self) -> LspResult<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        let text = params.text_document.text.clone();

        // Update the document cache
        self.update_document_content(uri.clone(), text.clone())
            .await;

        // Index the document
        let mut indexer = self.indexer.lock().await;
        if let Err(e) = events::handle_did_open(&mut indexer, uri.clone(), &text) {
            self.client
                .log_message(
                    MessageType::ERROR,
                    format!("Error indexing document: {}", e),
                )
                .await;
        }
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.clone();

        // For full sync, we get the full content in the first change
        if let Some(change) = params.content_changes.first() {
            let text = change.text.clone();

            // Update the document cache
            self.update_document_content(uri.clone(), text.clone())
                .await;

            // Re-index the document with new content
            let mut indexer = self.indexer.lock().await;
            if let Err(e) = events::handle_did_change(&mut indexer, uri.clone(), &text) {
                self.client
                    .log_message(
                        MessageType::ERROR,
                        format!("Error re-indexing document: {}", e),
                    )
                    .await;
            }
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri.clone();

        // Remove from cache
        self.remove_document(&uri).await;

        // Remove from indexer
        let mut indexer = self.indexer.lock().await;
        events::handle_did_close(&mut indexer, &uri);
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> LspResult<Option<GotoDefinitionResponse>> {
        let uri = params
            .text_document_position_params
            .text_document
            .uri
            .clone();
        let position = params.text_document_position_params.position;

        // Get document content
        let content = match self.get_document_content(&uri).await {
            Some(content) => content,
            None => return Ok(None),
        };

        // Get indexer reference
        let indexer = self.indexer.lock().await;

        // Use the definition capability
        match definition::find_definition_at_position(&indexer, &uri, position, &content).await {
            Some(location) => Ok(Some(GotoDefinitionResponse::Scalar(location))),
            None => Ok(None),
        }
    }

    async fn references(&self, params: ReferenceParams) -> LspResult<Option<Vec<Location>>> {
        let uri = params.text_document_position.text_document.uri.clone();
        let position = params.text_document_position.position;
        let include_declaration = params.context.include_declaration;

        // Get document content
        let content = match self.get_document_content(&uri).await {
            Some(content) => content,
            None => return Ok(None),
        };

        // Get indexer reference
        let indexer = self.indexer.lock().await;

        // Use the references capability
        Ok(references::find_references_at_position(
            &indexer,
            &uri,
            position,
            &content,
            include_declaration,
        )
        .await)
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> LspResult<Option<SemanticTokensResult>> {
        let uri = params.text_document.uri;

        // Get document content from cache
        let content = match self.get_document_content(&uri).await {
            Some(content) => content,
            None => return Ok(None),
        };

        // Use the semantic tokens capability to generate tokens
        match crate::capabilities::semantic_tokens::generate_semantic_tokens(&content) {
            Ok(tokens) => Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
                result_id: None,
                data: tokens,
            }))),
            Err(e) => {
                self.client
                    .log_message(
                        MessageType::ERROR,
                        format!("Error generating semantic tokens: {}", e),
                    )
                    .await;
                Ok(None)
            }
        }
    }

    async fn semantic_tokens_range(
        &self,
        params: SemanticTokensRangeParams,
    ) -> LspResult<Option<SemanticTokensRangeResult>> {
        let uri = params.text_document.uri;
        let range = params.range;

        // Get document content from cache
        let content = match self.get_document_content(&uri).await {
            Some(content) => content,
            None => return Ok(None),
        };

        // Use the semantic tokens capability to generate tokens for the range
        match crate::capabilities::semantic_tokens::generate_semantic_tokens_for_range(
            &content, &range,
        ) {
            Ok(tokens) => Ok(Some(SemanticTokensRangeResult::Tokens(SemanticTokens {
                result_id: None,
                data: tokens,
            }))),
            Err(e) => {
                self.client
                    .log_message(
                        MessageType::ERROR,
                        format!("Error generating semantic tokens for range: {}", e),
                    )
                    .await;
                Ok(None)
            }
        }
    }
}
