use anyhow::Result;
use dashmap::DashMap;
use log::{info, warn};
use lsp_types::*;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::{Client, LanguageServer};

use crate::parser::{document::RubyDocument, RubyParser};
use crate::workspace::WorkspaceManager;
use std::sync::{Arc, Mutex};

pub struct RubyLanguageServer {
    client: Client,
    parser: Option<RubyParser>,
    workspace_manager: Arc<Mutex<WorkspaceManager>>,
    document_map: DashMap<Url, RubyDocument>,
}

impl RubyLanguageServer {
    pub fn new(client: Client) -> Result<Self> {
        let workspace_manager = WorkspaceManager::new();

        let parser = match RubyParser::new() {
            Ok(parser) => Some(parser),
            Err(e) => {
                warn!("Failed to initialize Ruby parser: {}", e);
                None
            }
        };

        Ok(Self {
            client,
            document_map: DashMap::new(),
            parser,
            workspace_manager: Arc::new(Mutex::new(workspace_manager)),
        })
    }

    // Helper method to get a document either from open documents or from the index
    fn get_document(&self, uri: &Url) -> Option<RubyDocument> {
        // First check if the document is open
        if let Some(doc) = self.document_map.get(uri) {
            return Some(doc.value().clone());
        }

        // If not open, check the workspace index
        // Create a scope for the mutex lock to ensure it's dropped properly
        let document = {
            if let Ok(workspace) = self.workspace_manager.lock() {
                workspace.get_document(uri)
            } else {
                None
            }
        };

        document
    }

    // Helper method to parse a document and return the tree
    fn parse_document(&self, document: &RubyDocument) -> Option<tree_sitter::Tree> {
        // If parser is not available, return None
        let parser = match &self.parser {
            Some(parser) => parser,
            None => {
                warn!("Parser not available for document parsing");
                return None;
            }
        };

        // Try to parse the document
        parser.parse(document.get_content())
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for RubyLanguageServer {
    async fn initialize(&self, params: InitializeParams) -> LspResult<InitializeResult> {
        info!("Initializing Ruby LSP server");

        let capabilities = ServerCapabilities::default();

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
}
