use anyhow::Result;
use log::info;
use lsp_types::*;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::{Client, LanguageServer};

use crate::analyzer::RubyAnalyzer;
use crate::capabilities::semantic_tokens::semantic_tokens_options;
use crate::indexer::traverser::RubyIndexer;
use crate::parser::RubyParser;

pub struct RubyLanguageServer {
    client: Client,
    parser: RubyParser,
    analyzer: RubyAnalyzer,
    indexer: RubyIndexer,
}

impl RubyLanguageServer {
    pub fn new(client: Client) -> Result<Self> {
        let analyzer = RubyAnalyzer {};
        let parser = RubyParser::new()?;
        let indexer = RubyIndexer::new().map_err(|e| anyhow::anyhow!(e))?;

        Ok(Self {
            client,
            parser,
            analyzer,
            indexer,
        })
    }

    async fn find_definition_at_position(
        &self,
        uri: &Url,
        _position: Position,
    ) -> Option<Location> {
        if let Some(source_code) = self.get_document_content(uri).await {
            if let Some(_tree) = self.parser.parse(&source_code) {
                None
            } else {
                None
            }
        } else {
            None
        }
    }

    async fn get_document_content(&self, _uri: &Url) -> Option<String> {
        None
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for RubyLanguageServer {
    async fn initialize(&self, _params: InitializeParams) -> LspResult<InitializeResult> {
        info!("Initializing Ruby LSP server");

        let capabilities = ServerCapabilities {
            text_document_sync: Some(TextDocumentSyncCapability::Kind(
                TextDocumentSyncKind::INCREMENTAL,
            )),
            semantic_tokens_provider: Some(
                SemanticTokensServerCapabilities::SemanticTokensOptions(semantic_tokens_options()),
            ),
            definition_provider: Some(OneOf::Left(true)),
            references_provider: Some(OneOf::Left(true)),
            ..ServerCapabilities::default()
        };

        Ok(InitializeResult {
            capabilities,
            server_info: Some(ServerInfo {
                name: "ruby-fast-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        info!("Server initialized");
    }

    async fn shutdown(&self) -> LspResult<()> {
        info!("Shutting down server");
        Ok(())
    }

    async fn did_change_watched_files(&self, params: DidChangeWatchedFilesParams) {
        info!("Files changed: {:?}", params);
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        info!("Document opened: {:?}", params);
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        info!("Document changed: {:?}", params);
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        info!("Document closed: {:?}", params);
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> LspResult<Option<SemanticTokensResult>> {
        info!("Semantic tokens requested: {:?}", params);
        Ok(None)
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> LspResult<Option<GotoDefinitionResponse>> {
        info!("Goto definition requested: {:?}", params);

        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        if let Some(location) = self.find_definition_at_position(&uri, position).await {
            Ok(Some(GotoDefinitionResponse::Scalar(location)))
        } else {
            Ok(None)
        }
    }

    async fn references(&self, params: ReferenceParams) -> LspResult<Option<Vec<Location>>> {
        info!("References requested: {:?}", params);

        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        if let Some(definition_location) = self.find_definition_at_position(&uri, position).await {
            Ok(Some(vec![definition_location]))
        } else {
            Ok(None)
        }
    }
}
