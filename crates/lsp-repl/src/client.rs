//! LSP Client implementation.
//!
//! Spawns a language server process and handles the LSP protocol lifecycle.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{anyhow, Context, Result};
use lsp_types::{
    ClientCapabilities, InitializeParams, InitializeResult, InitializedParams,
    TextDocumentIdentifier, TextDocumentItem, TextDocumentPositionParams, Url, WorkspaceFolder,
};
use tokio::process::{Child, Command};

use crate::protocol::{
    CommandDefinition, JsonRpcNotification, JsonRpcRequest, JsonRpcResponse, ListCommandsResponse,
};
use crate::transport::Transport;

/// An LSP client that communicates with a language server.
pub struct LspClient {
    /// The spawned language server process
    _process: Child,

    /// Transport layer for JSON-RPC communication
    transport: Transport,

    /// Next request ID
    next_id: AtomicU64,

    /// The workspace root
    pub workspace_root: Option<PathBuf>,

    /// Server name (from initialize result)
    pub server_name: Option<String>,

    /// Currently open documents (uri -> content)
    open_documents: HashMap<Url, String>,

    /// The currently focused document
    pub current_document: Option<Url>,

    /// Custom commands discovered from server
    pub custom_commands: Vec<CommandDefinition>,
}

/// Information about an open document
#[derive(Debug, Clone)]
pub struct OpenDocumentInfo {
    pub uri: Url,
    pub file_name: String,
    pub line_count: usize,
}

impl LspClient {
    /// Spawn a new language server and initialize it.
    pub async fn new(server_command: &str, workspace_root: Option<PathBuf>) -> Result<Self> {
        // Parse the command string into program and arguments
        let parts: Vec<&str> = server_command.split_whitespace().collect();
        if parts.is_empty() {
            return Err(anyhow!("Empty server command"));
        }

        let program = parts[0];
        let args = &parts[1..];

        // Spawn the server process
        let mut process = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit()) // Let server errors go to our stderr
            .kill_on_drop(true)
            .spawn()
            .with_context(|| format!("Failed to spawn server: {}", server_command))?;

        let stdin = process.stdin.take().expect("Failed to get stdin");
        let stdout = process.stdout.take().expect("Failed to get stdout");

        let transport = Transport::new(stdin, stdout);

        let mut client = Self {
            _process: process,
            transport,
            next_id: AtomicU64::new(1),
            workspace_root,
            server_name: None,
            open_documents: HashMap::new(),
            current_document: None,
            custom_commands: Vec::new(),
        };

        // Initialize the server
        client.initialize().await?;

        // Discover custom commands
        client.discover_commands().await?;

        Ok(client)
    }

    /// Get the next request ID.
    fn next_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::SeqCst)
    }

    /// Send a request and wait for a response.
    pub async fn request<T: serde::de::DeserializeOwned>(
        &mut self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<T> {
        let request = JsonRpcRequest::new(self.next_id(), method, params);
        let response = self.transport.send_request(&request).await?;

        if let Some(error) = response.error {
            return Err(anyhow!("LSP error {}: {}", error.code, error.message));
        }

        let result = response
            .result
            .ok_or_else(|| anyhow!("Missing result in response"))?;

        serde_json::from_value(result).context("Failed to parse response")
    }

    /// Send a request and return the raw JSON response.
    pub async fn request_raw(
        &mut self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<JsonRpcResponse> {
        let request = JsonRpcRequest::new(self.next_id(), method, params);
        self.transport.send_request(&request).await
    }

    /// Send a notification (no response expected).
    pub async fn notify(&mut self, method: &str, params: Option<serde_json::Value>) -> Result<()> {
        let notification = JsonRpcNotification::new(method, params);
        self.transport.send_notification(&notification).await
    }

    /// Initialize the language server.
    async fn initialize(&mut self) -> Result<()> {
        let workspace_folders = self.workspace_root.as_ref().map(|root| {
            vec![WorkspaceFolder {
                uri: Url::from_file_path(root).expect("Invalid workspace path"),
                name: root
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "workspace".to_string()),
            }]
        });

        let root_uri = self
            .workspace_root
            .as_ref()
            .map(|root| Url::from_file_path(root).expect("Invalid workspace path"));

        let params = InitializeParams {
            process_id: Some(std::process::id()),
            #[allow(deprecated)]
            root_path: None,
            #[allow(deprecated)]
            root_uri,
            initialization_options: None,
            capabilities: ClientCapabilities::default(),
            trace: None,
            workspace_folders,
            client_info: Some(lsp_types::ClientInfo {
                name: "lsp-repl".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
            locale: None,
            work_done_progress_params: Default::default(),
        };

        let result: InitializeResult = self
            .request("initialize", Some(serde_json::to_value(params)?))
            .await?;

        // Store server name
        self.server_name = result.server_info.map(|info| info.name);

        // Send initialized notification
        self.notify(
            "initialized",
            Some(serde_json::to_value(InitializedParams {})?),
        )
        .await?;

        Ok(())
    }

    /// Discover custom commands from the server via $/listCommands.
    async fn discover_commands(&mut self) -> Result<()> {
        let response = self.request_raw("$/listCommands", None).await;

        match response {
            Ok(resp) => {
                if let Some(result) = resp.result {
                    if let Ok(list) = serde_json::from_value::<ListCommandsResponse>(result) {
                        self.custom_commands = list.commands;
                    }
                }
            }
            Err(_) => {
                // Server doesn't support $/listCommands, that's fine
            }
        }

        Ok(())
    }

    /// Open a document in the server.
    pub async fn open_document(&mut self, path: &PathBuf) -> Result<()> {
        let content = tokio::fs::read_to_string(path)
            .await
            .with_context(|| format!("Failed to read file: {}", path.display()))?;

        let uri = Url::from_file_path(path).map_err(|_| anyhow!("Invalid file path"))?;

        // Determine language ID from extension
        let language_id = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| match ext {
                "rb" => "ruby",
                "rs" => "rust",
                "ts" => "typescript",
                "js" => "javascript",
                "py" => "python",
                "go" => "go",
                "java" => "java",
                "c" => "c",
                "cpp" | "cc" | "cxx" => "cpp",
                "h" | "hpp" => "c",
                _ => ext,
            })
            .unwrap_or("plaintext")
            .to_string();

        let params = lsp_types::DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id,
                version: 1,
                text: content.clone(),
            },
        };

        self.notify("textDocument/didOpen", Some(serde_json::to_value(params)?))
            .await?;

        self.open_documents.insert(uri.clone(), content);
        self.current_document = Some(uri);

        Ok(())
    }

    /// Close a document.
    pub async fn close_document(&mut self, uri: &Url) -> Result<()> {
        if !self.open_documents.contains_key(uri) {
            return Err(anyhow!("Document is not open"));
        }

        let params = lsp_types::DidCloseTextDocumentParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
        };

        self.notify("textDocument/didClose", Some(serde_json::to_value(params)?))
            .await?;

        self.open_documents.remove(uri);

        // If we closed the current document, switch to another open one
        if self.current_document.as_ref() == Some(uri) {
            self.current_document = self.open_documents.keys().next().cloned();
        }

        Ok(())
    }

    /// Wait for the server to finish indexing.
    /// This polls the server's stats command until indexing_complete is true.
    pub async fn wait_for_indexing(&mut self, timeout_secs: u64) -> Result<()> {
        use std::time::{Duration, Instant};

        let start = Instant::now();
        let timeout = Duration::from_secs(timeout_secs);
        let poll_interval = Duration::from_millis(500);

        loop {
            if start.elapsed() > timeout {
                return Err(anyhow!("Timeout waiting for indexing to complete"));
            }

            // Try to get stats from the server
            let result = self
                .request_raw("ruby-fast-lsp/debug/stats", Some(serde_json::json!({})))
                .await;

            if let Ok(response) = result {
                if let Some(result) = response.result {
                    // Check if indexing is complete (try both snake_case and camelCase)
                    let complete = result
                        .get("indexing_complete")
                        .or_else(|| result.get("indexingComplete"))
                        .and_then(|v| v.as_bool());
                    if complete == Some(true) {
                        return Ok(());
                    }
                }
            }

            tokio::time::sleep(poll_interval).await;
        }
    }

    /// Execute a custom command by method name.
    pub async fn execute_custom(
        &mut self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let response = self.request_raw(method, Some(params)).await?;

        if let Some(error) = response.error {
            return Err(anyhow!("LSP error {}: {}", error.code, error.message));
        }

        response
            .result
            .ok_or_else(|| anyhow!("No result in response"))
    }

    /// Get the current document's line count.
    pub fn current_line_count(&self) -> Option<usize> {
        self.current_document.as_ref().and_then(|uri| {
            self.open_documents
                .get(uri)
                .map(|content| content.lines().count())
        })
    }

    /// Get the current document's file name.
    pub fn current_file_name(&self) -> Option<String> {
        self.current_document.as_ref().and_then(|uri| {
            uri.path_segments()
                .and_then(|mut segments| segments.next_back())
                .map(|s| s.to_string())
        })
    }

    /// Get all open documents.
    pub fn open_documents(&self) -> Vec<OpenDocumentInfo> {
        self.open_documents
            .iter()
            .map(|(uri, content)| OpenDocumentInfo {
                uri: uri.clone(),
                file_name: uri
                    .path_segments()
                    .and_then(|mut segments| segments.next_back())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "unknown".to_string()),
                line_count: content.lines().count(),
            })
            .collect()
    }

    /// Find open documents matching a filename pattern.
    /// Returns matching documents. If filename is exact match, returns only that.
    /// If filename is a partial match, returns all matches.
    pub fn find_documents(&self, filename: &str) -> Vec<OpenDocumentInfo> {
        let docs = self.open_documents();

        // First try exact match
        let exact: Vec<_> = docs
            .iter()
            .filter(|d| d.file_name == filename)
            .cloned()
            .collect();

        if !exact.is_empty() {
            return exact;
        }

        // Then try partial match (contains)
        docs.into_iter()
            .filter(|d| d.file_name.contains(filename))
            .collect()
    }

    /// Set the current document by URI.
    pub fn set_current_document(&mut self, uri: &Url) -> bool {
        if self.open_documents.contains_key(uri) {
            self.current_document = Some(uri.clone());
            true
        } else {
            false
        }
    }

    /// Get hover information at a position for a specific document.
    pub async fn hover_in_document(
        &mut self,
        uri: &Url,
        line: u32,
        character: u32,
    ) -> Result<Option<lsp_types::Hover>> {
        let params = lsp_types::HoverParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                position: lsp_types::Position { line, character },
            },
            work_done_progress_params: Default::default(),
        };

        self.request("textDocument/hover", Some(serde_json::to_value(params)?))
            .await
    }

    /// Go to definition at a position for a specific document.
    pub async fn definition_in_document(
        &mut self,
        uri: &Url,
        line: u32,
        character: u32,
    ) -> Result<Option<lsp_types::GotoDefinitionResponse>> {
        let params = lsp_types::GotoDefinitionParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                position: lsp_types::Position { line, character },
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };

        self.request(
            "textDocument/definition",
            Some(serde_json::to_value(params)?),
        )
        .await
    }

    /// Find references at a position for a specific document.
    pub async fn references_in_document(
        &mut self,
        uri: &Url,
        line: u32,
        character: u32,
    ) -> Result<Option<Vec<lsp_types::Location>>> {
        let params = lsp_types::ReferenceParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                position: lsp_types::Position { line, character },
            },
            context: lsp_types::ReferenceContext {
                include_declaration: true,
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };

        self.request(
            "textDocument/references",
            Some(serde_json::to_value(params)?),
        )
        .await
    }

    /// Get completions at a position for a specific document.
    pub async fn completion_in_document(
        &mut self,
        uri: &Url,
        line: u32,
        character: u32,
    ) -> Result<Option<lsp_types::CompletionResponse>> {
        let params = lsp_types::CompletionParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                position: lsp_types::Position { line, character },
            },
            context: None,
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };

        self.request(
            "textDocument/completion",
            Some(serde_json::to_value(params)?),
        )
        .await
    }

    /// Get document symbols for a specific document.
    pub async fn document_symbols_in_document(
        &mut self,
        uri: &Url,
    ) -> Result<Option<lsp_types::DocumentSymbolResponse>> {
        let params = lsp_types::DocumentSymbolParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };

        self.request(
            "textDocument/documentSymbol",
            Some(serde_json::to_value(params)?),
        )
        .await
    }

    /// Shutdown the server gracefully.
    pub async fn shutdown(&mut self) -> Result<()> {
        // Send shutdown request
        let _: () = self.request("shutdown", None).await.unwrap_or(());

        // Send exit notification
        self.notify("exit", None).await.ok();

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_document_info() {
        let info = OpenDocumentInfo {
            uri: Url::parse("file:///path/to/user.rb").unwrap(),
            file_name: "user.rb".to_string(),
            line_count: 100,
        };
        assert_eq!(info.file_name, "user.rb");
        assert_eq!(info.line_count, 100);
    }

    #[test]
    fn test_file_name_extraction_from_uri() {
        let uri = Url::parse("file:///path/to/models/user.rb").unwrap();
        let file_name = uri
            .path_segments()
            .and_then(|segments| segments.last())
            .map(|s| s.to_string());
        assert_eq!(file_name, Some("user.rb".to_string()));
    }

    #[test]
    fn test_language_id_detection() {
        let path = std::path::PathBuf::from("/path/to/file.rb");
        let ext = path.extension().and_then(|e| e.to_str());
        assert_eq!(ext, Some("rb"));

        let path = std::path::PathBuf::from("/path/to/file.ts");
        let ext = path.extension().and_then(|e| e.to_str());
        assert_eq!(ext, Some("ts"));

        let path = std::path::PathBuf::from("/path/to/Gemfile");
        let ext = path.extension().and_then(|e| e.to_str());
        assert_eq!(ext, None);
    }

    #[test]
    fn test_language_id_mapping() {
        fn map_extension(ext: &str) -> &'static str {
            match ext {
                "rb" => "ruby",
                "rs" => "rust",
                "ts" => "typescript",
                "js" => "javascript",
                "py" => "python",
                "go" => "go",
                "java" => "java",
                "c" => "c",
                "cpp" | "cc" | "cxx" => "cpp",
                "h" | "hpp" => "c",
                _ => "unknown",
            }
        }

        assert_eq!(map_extension("rb"), "ruby");
        assert_eq!(map_extension("rs"), "rust");
        assert_eq!(map_extension("ts"), "typescript");
        assert_eq!(map_extension("py"), "python");
        assert_eq!(map_extension("cpp"), "cpp");
        assert_eq!(map_extension("xyz"), "unknown");
    }

    #[test]
    fn test_line_count_calculation() {
        let content = "line1\nline2\nline3";
        assert_eq!(content.lines().count(), 3);

        let content = "single line";
        assert_eq!(content.lines().count(), 1);

        let content = "";
        assert_eq!(content.lines().count(), 0);

        // lines() skips trailing empty line but counts internal empty lines
        let content = "\n\n\n";
        assert_eq!(content.lines().count(), 3);

        // With trailing newline
        let content = "a\nb\nc\n";
        assert_eq!(content.lines().count(), 3);
    }

    #[test]
    fn test_indexing_complete_field_detection() {
        // Snake case
        let result = serde_json::json!({"indexing_complete": true});
        let complete = result
            .get("indexing_complete")
            .or_else(|| result.get("indexingComplete"))
            .and_then(|v| v.as_bool());
        assert_eq!(complete, Some(true));

        // Camel case
        let result = serde_json::json!({"indexingComplete": false});
        let complete = result
            .get("indexing_complete")
            .or_else(|| result.get("indexingComplete"))
            .and_then(|v| v.as_bool());
        assert_eq!(complete, Some(false));

        // Neither
        let result = serde_json::json!({"other_field": "value"});
        let complete = result
            .get("indexing_complete")
            .or_else(|| result.get("indexingComplete"))
            .and_then(|v| v.as_bool());
        assert_eq!(complete, None);
    }
}
