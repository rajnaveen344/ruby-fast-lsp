pub mod handlers;

use anyhow::Result;
use dashmap::DashMap;
use log::{info, warn};
use lsp_types::*;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::{Client, LanguageServer};

use self::handlers::RubyLspHandlers;
use crate::parser::document::RubyDocument;
use crate::workspace::WorkspaceManager;
use std::sync::{Arc, Mutex};

pub struct RubyLanguageServer {
    pub client: Client,
    pub document_map: DashMap<Url, RubyDocument>,
    handlers: Option<Arc<Mutex<RubyLspHandlers>>>,
    workspace_manager: Arc<Mutex<WorkspaceManager>>,
}

impl RubyLanguageServer {
    pub fn new(client: Client) -> Result<Self> {
        let workspace_manager = WorkspaceManager::new();
        let handlers = Some(Arc::new(Mutex::new(RubyLspHandlers::new()?)));

        Ok(Self {
            client,
            document_map: DashMap::new(),
            handlers,
            workspace_manager: Arc::new(Mutex::new(workspace_manager)),
        })
    }

    pub fn new_fallback(client: Client) -> Self {
        warn!("Creating fallback Ruby LSP server with limited functionality");
        let workspace_manager = WorkspaceManager::new();

        Self {
            client,
            document_map: DashMap::new(),
            handlers: None,
            workspace_manager: Arc::new(Mutex::new(workspace_manager)),
        }
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
}

#[tower_lsp::async_trait]
impl LanguageServer for RubyLanguageServer {
    async fn initialize(&self, params: InitializeParams) -> LspResult<InitializeResult> {
        info!("Initializing Ruby LSP server");

        // Set the workspace root URI if provided
        if let Some(workspace_folders) = params.workspace_folders {
            if let Some(first_folder) = workspace_folders.first() {
                // Create a scope for the mutex lock to ensure it's dropped properly
                {
                    if let Ok(mut workspace) = self.workspace_manager.lock() {
                        if let Err(e) = workspace.set_root_uri(first_folder.uri.clone()) {
                            warn!("Failed to set workspace root URI: {}", e);
                        } else {
                            info!("Workspace root URI set to: {}", first_folder.uri);
                        }
                    }
                }

                // Start scanning the workspace in the background
                let workspace_manager = self.workspace_manager.clone();
                let client = self.client.clone();

                tokio::spawn(async move {
                    info!("Starting workspace scan");
                    // Create a separate scope for the mutex lock to ensure it's dropped before any await
                    let count = {
                        // Use a separate block to ensure the MutexGuard is dropped
                        let scan_result = {
                            if let Ok(workspace) = workspace_manager.lock() {
                                workspace.scan_workspace()
                            } else {
                                return; // Early return if we can't lock the workspace
                            }
                        };

                        // Now the MutexGuard is dropped, process the result
                        match scan_result {
                            Ok(count) => Some(count),
                            Err(e) => {
                                let message = format!("Workspace scan failed: {}", e);
                                warn!("{}", message);
                                client.log_message(MessageType::WARNING, message).await;
                                None
                            }
                        }
                    };

                    if let Some(count) = count {
                        let message =
                            format!("Workspace scan complete: indexed {} Ruby files", count);
                        info!("{}", message);
                        client.log_message(MessageType::INFO, message).await;
                    }
                });
            }
        }

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
            workspace: Some(WorkspaceServerCapabilities {
                workspace_folders: Some(WorkspaceFoldersServerCapabilities {
                    supported: Some(true),
                    change_notifications: Some(OneOf::Left(true)),
                }),
                file_operations: None,
            }),
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

        self.client.log_message(MessageType::INFO, message).await;
    }

    async fn shutdown(&self) -> LspResult<()> {
        info!("Shutting down Ruby LSP server");
        Ok(())
    }

    // File events
    async fn did_change_watched_files(&self, params: DidChangeWatchedFilesParams) {
        for change in params.changes {
            match change.typ {
                FileChangeType::CREATED | FileChangeType::CHANGED => {
                    // Only update index if the file isn't open
                    if !self.document_map.contains_key(&change.uri) {
                        let uri = change.uri.clone();
                        let workspace_manager = self.workspace_manager.clone();

                        // Use a more robust approach to handle the mutex lock
                        let updated = workspace_manager
                            .lock()
                            .map(|workspace| workspace.update_index_for_file(&uri).unwrap_or(false))
                            .unwrap_or(false);

                        if updated {
                            info!("Updated index for file: {}", uri);
                        }
                    }
                }
                FileChangeType::DELETED => {
                    // Remove from index
                    let uri = change.uri.clone();

                    // Clone the workspace manager and use a separate scope to ensure the lock is dropped
                    let workspace_manager = self.workspace_manager.clone();

                    // Use a separate function to handle the lock to ensure it's dropped properly
                    let result = workspace_manager
                        .lock()
                        .map(|workspace| {
                            workspace.remove_from_index(&uri);
                            true
                        })
                        .unwrap_or(false);

                    if result {
                        info!("Removed file from index: {}", uri);
                    }
                }
                _ => {}
            }
        }
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
            // Get document either from open documents or from index
            if let Some(doc) = self.get_document(&uri) {
                if let Ok(handlers) = handlers.lock() {
                    return Ok(handlers.handle_hover(&doc, position));
                }
            }
        }

        // Fallback hover response
        Ok(Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: "Ruby LSP running in fallback mode. Limited functionality available."
                    .to_string(),
            }),
            range: None,
        }))
    }

    async fn completion(&self, params: CompletionParams) -> LspResult<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        info!("Completion request at position {:?} in {}", position, uri);

        if let Some(handlers) = &self.handlers {
            // Get document either from open documents or from index
            if let Some(doc) = self.get_document(&uri) {
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
            // Add more basic Ruby keywords as needed
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
            // Get document either from open documents or from index
            if let Some(doc) = self.get_document(&uri) {
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
