use crate::handlers::{notification, request};
use crate::indexer::RubyIndexer;
use anyhow::Result;
use lsp_types::*;
use tokio::sync::Mutex;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::{Client, LanguageServer};

pub struct RubyLanguageServer {
    pub client: Option<Client>,
    pub indexer: Mutex<RubyIndexer>,
}

impl RubyLanguageServer {
    pub fn new(client: Client) -> Result<Self> {
        let indexer = RubyIndexer::new().map_err(|e| anyhow::anyhow!(e))?;

        Ok(Self {
            client: Some(client),
            indexer: Mutex::new(indexer),
        })
    }
}

impl Default for RubyLanguageServer {
    fn default() -> Self {
        RubyLanguageServer {
            client: None,
            indexer: Mutex::new(RubyIndexer::new().unwrap()),
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
