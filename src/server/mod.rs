pub mod handlers;

use anyhow::Result;
use dashmap::DashMap;
use log::{info, warn};
use lsp_types::*;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::{Client, LanguageServer};

use crate::parser::document::RubyDocument;
use self::handlers::RubyLspHandlers;
use std::sync::{Arc, Mutex};

pub struct RubyLanguageServer {
    pub client: Client,
    pub document_map: DashMap<Url, RubyDocument>,
    handlers: Option<Arc<Mutex<RubyLspHandlers>>>,
}

impl RubyLanguageServer {
    pub fn new(client: Client) -> Result<Self> {
        let handlers = Some(Arc::new(Mutex::new(RubyLspHandlers::new()?)));
        
        Ok(Self {
            client,
            document_map: DashMap::new(),
            handlers,
        })
    }
    
    pub fn new_fallback(client: Client) -> Self {
        warn!("Creating fallback Ruby LSP server with limited functionality");
        
        Self {
            client,
            document_map: DashMap::new(),
            handlers: None,
        }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for RubyLanguageServer {
    async fn initialize(&self, _: InitializeParams) -> LspResult<InitializeResult> {
        info!("Initializing Ruby LSP server");
        
        let capabilities = ServerCapabilities {
            text_document_sync: Some(TextDocumentSyncCapability::Kind(
                TextDocumentSyncKind::INCREMENTAL,
            )),
            hover_provider: Some(HoverProviderCapability::Simple(true)),
            completion_provider: Some(CompletionOptions {
                resolve_provider: Some(false),
                trigger_characters: Some(vec![".".to_string(), "::".to_string()]),
                work_done_progress_options: Default::default(),
                all_commit_characters: None,
                completion_item: None,
            }),
            definition_provider: Some(OneOf::Left(true)),
            // Add more capabilities as you implement them
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
        info!("Ruby LSP server initialized!");
        
        let message = if self.handlers.is_some() {
            "Ruby LSP server initialized with full functionality!"
        } else {
            "Ruby LSP server initialized with limited functionality. Some features may not work correctly."
        };
        
        self.client
            .log_message(MessageType::INFO, message)
            .await;
    }

    async fn shutdown(&self) -> LspResult<()> {
        info!("Shutting down Ruby LSP server");
        Ok(())
    }

    // Document synchronization methods
    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.text_document.text;
        let version = params.text_document.version;
        
        info!("File opened: {}", uri);
        
        self.document_map.insert(
            uri.clone(),
            RubyDocument {
                content: text.clone(),
                version,
            },
        );
        
        self.client
            .log_message(MessageType::INFO, &format!("Opened document: {}", uri))
            .await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let version = params.text_document.version;
        
        info!("File changed: {}", uri);
        
        if let Some(mut doc) = self.document_map.get_mut(&uri) {
            for change in params.content_changes {
                if let Some(_range) = change.range {
                    // Handle incremental updates
                    doc.content = change.text;
                } else {
                    // Full document update
                    doc.content = change.text;
                }
                doc.version = version;
            }
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        
        info!("File closed: {}", uri);
        
        self.document_map.remove(&uri);
        
        self.client
            .log_message(MessageType::INFO, &format!("Closed document: {}", uri))
            .await;
    }

    // Language features
    async fn hover(&self, params: HoverParams) -> LspResult<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        
        info!("Hover request at position {:?} in {}", position, uri);
        
        if let Some(handlers) = &self.handlers {
            if let Some(doc) = self.document_map.get(&uri) {
                if let Ok(handlers) = handlers.lock() {
                    return Ok(handlers.handle_hover(&doc, position));
                }
            }
        }
        
        // Fallback hover response
        Ok(Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: "Ruby LSP running in fallback mode. Limited functionality available.".to_string(),
            }),
            range: None,
        }))
    }

    async fn completion(&self, params: CompletionParams) -> LspResult<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        
        info!("Completion request at position {:?} in {}", position, uri);
        
        if let Some(handlers) = &self.handlers {
            if let Some(doc) = self.document_map.get(&uri) {
                if let Ok(handlers) = handlers.lock() {
                    return Ok(Some(handlers.handle_completion(&doc, position)));
                }
            }
        }
        
        // Fallback completion response with basic Ruby keywords
        let items = vec![
            CompletionItem {
                label: "def".to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some("Define a method".to_string()),
                ..CompletionItem::default()
            },
            CompletionItem {
                label: "class".to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some("Define a class".to_string()),
                ..CompletionItem::default()
            },
            CompletionItem {
                label: "module".to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some("Define a module".to_string()),
                ..CompletionItem::default()
            },
        ];
        
        Ok(Some(CompletionResponse::Array(items)))
    }
    
    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> LspResult<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        
        info!("Definition request at position {:?} in {}", position, uri);
        
        if let Some(handlers) = &self.handlers {
            if let Some(doc) = self.document_map.get(&uri) {
                if let Ok(handlers) = handlers.lock() {
                    if let Some(range) = handlers.handle_definition(&doc, position) {
                        return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                            uri: uri.clone(),
                            range,
                        })));
                    }
                }
            }
        }
        
        Ok(None)
    }
}
