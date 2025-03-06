use anyhow::Result;
use log::info;
use lsp_types::*;
use std::collections::HashMap;
use tokio::sync::Mutex;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::{Client, LanguageServer};

use crate::analyzer::RubyAnalyzer;
use crate::capabilities::semantic_tokens::semantic_tokens_options;
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

    async fn find_definition_at_position(&self, uri: &Url, position: Position) -> Option<Location> {
        // Get document content from our cache
        let content = match self.get_document_content(uri).await {
            Some(content) => content,
            None => {
                info!("No document content found for {}", uri);
                return None;
            }
        };

        // Use the analyzer to find the identifier at the position and get its fully qualified name
        let mut analyzer = RubyAnalyzer::new();
        let fully_qualified_name = match analyzer.find_identifier_at_position(&content, position) {
            Some(name) => name,
            None => {
                info!("No identifier found at position {:?}", position);
                return None;
            }
        };

        info!("Looking for definition of: {}", fully_qualified_name);

        // Use the indexer to find the definition
        let indexer = self.indexer.lock().await;
        let entry = match indexer.index().find_definition(&fully_qualified_name) {
            Some(entry) => entry,
            None => {
                info!("No definition found for {}", fully_qualified_name);
                return None;
            }
        };

        info!("Found definition at {:?}", entry.location);

        // Return the location of the definition
        Some(Location {
            uri: entry.location.uri.clone(),
            range: entry.location.range,
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

    // Utility method to convert Position to string index
    fn position_to_index(&self, content: &str, position: Position) -> Option<usize> {
        let line = position.line as usize;
        let character = position.character as usize;

        let mut current_line = 0;
        let mut current_index = 0;
        let mut total_index = 0;

        for c in content.chars() {
            if current_line == line && current_index == character {
                return Some(total_index);
            }

            if c == '\n' {
                current_line += 1;
                current_index = 0;
            } else {
                current_index += 1;
            }
            total_index += 1;
        }

        None
    }

    // Find references to a symbol at the given position
    async fn find_references_at_position(
        &self,
        uri: &Url,
        position: Position,
        _include_declaration: bool,
    ) -> Option<Vec<Location>> {
        // Get document content from our cache
        let content = match self.get_document_content(uri).await {
            Some(content) => content,
            None => return None,
        };

        // Use the analyzer to find the identifier at the position and get its fully qualified name
        let mut analyzer = RubyAnalyzer::new();
        let fully_qualified_name = analyzer.find_identifier_at_position(&content, position)?;

        info!("Looking for references to: {}", fully_qualified_name);

        // Use the indexer to find all references
        let indexer = self.indexer.lock().await;
        let locations = indexer.index().find_references(&fully_qualified_name);

        if locations.is_empty() {
            return None;
        }

        Some(locations)
    }

    // Add a method to RubyLanguageServer to store workspace root
    async fn index_workspace_folder(&self, folder_uri: &Url) {
        self.client
            .log_message(
                MessageType::INFO,
                format!("Indexing workspace folder: {}", folder_uri),
            )
            .await;

        // Convert URI to filesystem path
        let folder_path = match folder_uri.to_file_path() {
            Ok(path) => path,
            Err(_) => {
                self.client
                    .log_message(
                        MessageType::ERROR,
                        format!("Failed to convert URI to file path: {}", folder_uri),
                    )
                    .await;
                return;
            }
        };

        // Find all Ruby files in the workspace
        match self.find_ruby_files(&folder_path).await {
            Ok(files) => {
                self.client
                    .log_message(
                        MessageType::INFO,
                        format!("Found {} Ruby files to index", files.len()),
                    )
                    .await;

                // Index each file
                for file_path in files {
                    match self.index_workspace_file(&file_path).await {
                        Ok(_) => {}
                        Err(e) => {
                            self.client
                                .log_message(
                                    MessageType::ERROR,
                                    format!("Error indexing file {}: {:?}", file_path.display(), e),
                                )
                                .await;
                        }
                    }
                }

                self.client
                    .log_message(MessageType::INFO, "Workspace indexing completed")
                    .await;
            }
            Err(e) => {
                self.client
                    .log_message(
                        MessageType::ERROR,
                        format!("Error finding Ruby files: {:?}", e),
                    )
                    .await;
            }
        }
    }

    // Helper to find all Ruby files in a directory recursively
    async fn find_ruby_files(&self, dir: &std::path::Path) -> Result<Vec<std::path::PathBuf>> {
        use tokio::fs;

        let mut ruby_files = Vec::new();
        let mut dirs_to_process = vec![dir.to_path_buf()];

        while let Some(current_dir) = dirs_to_process.pop() {
            let mut entries = match fs::read_dir(&current_dir).await {
                Ok(entries) => entries,
                Err(e) => {
                    info!("Error reading directory {}: {}", current_dir.display(), e);
                    continue;
                }
            };

            while let Ok(Some(entry)) = entries.next_entry().await {
                let path = entry.path();

                if path.is_dir() {
                    dirs_to_process.push(path);
                } else if let Some(ext) = path.extension() {
                    if ext == "rb" {
                        ruby_files.push(path);
                    }
                }
            }
        }

        Ok(ruby_files)
    }

    // Helper to index a single workspace file
    async fn index_workspace_file(&self, file_path: &std::path::Path) -> Result<()> {
        use tokio::fs;

        // Read the file content
        let content = fs::read_to_string(file_path).await?;

        // Convert path to URI
        let uri = Url::from_file_path(file_path)
            .map_err(|_| anyhow::anyhow!("Failed to convert path to URI"))?;

        // Index the file
        let mut indexer = self.indexer.lock().await;
        indexer
            .index_file_with_uri(uri, &content)
            .map_err(|e| anyhow::anyhow!("Failed to index file: {}", e))?;

        Ok(())
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for RubyLanguageServer {
    async fn initialize(&self, params: InitializeParams) -> LspResult<InitializeResult> {
        info!("Ruby LSP initializing...");

        // Store the workspace root URI
        if let Some(workspace_folders) = &params.workspace_folders {
            if !workspace_folders.is_empty() {
                // If there are workspace folders, use the first one
                let uri = workspace_folders[0].uri.clone();
                let mut workspace_root = self.workspace_root.lock().await;
                *workspace_root = Some(uri);
            }
        } else if let Some(root_uri) = params.root_uri.clone() {
            // No workspace folders, use root_uri if available
            let mut workspace_root = self.workspace_root.lock().await;
            *workspace_root = Some(root_uri);
        }

        let capabilities = ServerCapabilities {
            text_document_sync: Some(TextDocumentSyncCapability::Kind(
                TextDocumentSyncKind::INCREMENTAL,
            )),
            completion_provider: Some(CompletionOptions {
                resolve_provider: Some(false),
                trigger_characters: Some(vec![".".to_string(), ":".to_string()]),
                work_done_progress_options: Default::default(),
                all_commit_characters: None,
                completion_item: None,
            }),
            // TODO: Enable this when we have a semantic tokens provider
            // semantic_tokens_provider: Some(
            //     SemanticTokensServerCapabilities::SemanticTokensOptions(semantic_tokens_options()),
            // ),
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

    async fn initialized(&self, _params: InitializedParams) {
        info!("Ruby LSP initialized");

        // Register for file events so we can index files in the workspace
        let register_options = DidChangeWatchedFilesRegistrationOptions {
            watchers: vec![FileSystemWatcher {
                glob_pattern: GlobPattern::String("**/*.rb".to_string()),
                kind: Some(WatchKind::all()),
            }],
        };

        let registration = Registration {
            id: "watched-files".to_string(),
            method: "workspace/didChangeWatchedFiles".to_string(),
            register_options: Some(serde_json::to_value(register_options).unwrap()),
        };

        if let Err(e) = self.client.register_capability(vec![registration]).await {
            info!("Error registering for file events: {:?}", e);
        }

        // Index all Ruby files in the workspace
        self.client
            .log_message(MessageType::INFO, "Indexing Ruby files in workspace...")
            .await;

        // Use the workspace root that we set during initialize
        let workspace_root = self.workspace_root.lock().await.clone();
        if let Some(root_uri) = workspace_root {
            self.index_workspace_folder(&root_uri).await;
        } else {
            self.client
                .log_message(
                    MessageType::WARNING,
                    "No workspace root found, skipping initial indexing",
                )
                .await;
        }
    }

    async fn shutdown(&self) -> LspResult<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.text_document.text;

        // Store document in cache
        self.update_document_content(uri.clone(), text.clone())
            .await;

        // Index the file
        let mut indexer = self.indexer.lock().await;
        if let Err(e) = indexer.index_file_with_uri(uri.clone(), &text) {
            self.client
                .log_message(MessageType::ERROR, format!("Error indexing file: {:?}", e))
                .await;
        }
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;

        // Get the current document content
        let mut content = match self.get_document_content(&uri).await {
            Some(content) => content,
            None => String::new(),
        };

        // Apply the changes to our cached content
        for change in params.content_changes {
            if let Some(range) = change.range {
                // Convert range to string indices
                let start_pos = self.position_to_index(&content, range.start);
                let end_pos = self.position_to_index(&content, range.end);

                if let (Some(start), Some(end)) = (start_pos, end_pos) {
                    // Replace the range with the new text
                    content.replace_range(start..end, &change.text);
                }
            } else {
                // Full content replace
                content = change.text;
            }
        }

        // Update the cache
        self.update_document_content(uri.clone(), content.clone())
            .await;

        // Re-index the file
        let mut indexer = self.indexer.lock().await;
        // Remove old entries and re-index
        indexer.index_mut().remove_entries_for_uri(&uri);

        if let Err(e) = indexer.index_file_with_uri(uri.clone(), &content) {
            self.client
                .log_message(
                    MessageType::ERROR,
                    format!("Error re-indexing file: {:?}", e),
                )
                .await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        // Remove document from cache
        self.remove_document(&params.text_document.uri).await;
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> LspResult<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        info!(
            "goto_definition request at position {:?} in file {:?}",
            position, uri
        );

        match self.find_definition_at_position(&uri, position).await {
            Some(location) => {
                info!("Found definition at {:?}", location);
                Ok(Some(GotoDefinitionResponse::Scalar(location)))
            }
            None => {
                info!("No definition found");
                Ok(None)
            }
        }
    }

    async fn references(&self, params: ReferenceParams) -> LspResult<Option<Vec<Location>>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let include_declaration = params.context.include_declaration;

        info!(
            "references request at position {:?} in file {:?}",
            position, uri
        );

        match self
            .find_references_at_position(&uri, position, include_declaration)
            .await
        {
            Some(locations) => {
                info!("Found {} references", locations.len());
                Ok(Some(locations))
            }
            None => {
                info!("No references found");
                Ok(None)
            }
        }
    }
}
