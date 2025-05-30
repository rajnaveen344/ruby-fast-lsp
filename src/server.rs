use crate::analyzer_prism::visitors::index_visitor::IndexVisitor;
use crate::handlers::{notification, request};
use crate::indexer::index::RubyIndex;
use crate::types::ruby_document::RubyDocument;
use anyhow::Result;
use log::debug;
use lsp_types::*;
use ruby_prism::Visit;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::{Client, LanguageServer};

#[derive(Clone)]
pub struct RubyLanguageServer {
    pub client: Option<Client>,
    pub index: Arc<Mutex<RubyIndex>>,
    pub docs: Arc<Mutex<HashMap<Url, RubyDocument>>>,
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

    pub fn process_file(&mut self, uri: Url, content: &str) -> Result<(), String> {
        self.index.lock().unwrap().remove_entries_for_uri(&uri);

        // Create or update document in the docs HashMap
        let document = RubyDocument::new(uri.clone(), content.to_string(), 0);
        self.docs.lock().unwrap().insert(uri.clone(), document);

        let parse_result = ruby_prism::parse(content.as_bytes());
        let node = parse_result.node();
        let mut visitor = IndexVisitor::new(self, uri.clone());

        visitor.visit(&node);

        debug!("Processed file: {}", uri);
        Ok(())
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
        let start_time = Instant::now();
        let result = request::handle_goto_definition(self, params).await;

        debug!(
            "[PERF] Goto definition completed in {:?}",
            start_time.elapsed()
        );

        result
    }

    async fn references(&self, params: ReferenceParams) -> LspResult<Option<Vec<Location>>> {
        request::handle_references(self, params).await
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> LspResult<Option<SemanticTokensResult>> {
        eprintln!(
            "textDocument/semanticTokens/full: {:?}",
            params.text_document.uri
        );
        request::handle_semantic_tokens_full(self, params).await
    }
}
