//! # Test Harness
//!
//! Connects the Model to the real LSP server.
//! Executes transitions on both and verifies invariants.

use super::{LspModel, Transition};
use crate::server::RubyLanguageServer;
use std::collections::HashMap;
use std::path::PathBuf;
use tempfile::TempDir;
use tower_lsp::lsp_types::*;
use tower_lsp::LanguageServer;

/// Test harness for simulation testing.
///
/// Maintains both the Model (oracle) and the real LSP server,
/// applying transitions to both and verifying they stay in sync.
pub struct SimulationHarness {
    /// The model (oracle) - what state SHOULD be
    pub model: LspModel,
    /// The real LSP server under test
    pub server: RubyLanguageServer,
    /// Temporary directory for test files
    temp_dir: TempDir,
    /// Map from logical filename to actual file path
    pub file_paths: HashMap<String, PathBuf>,
    /// Log of all transitions executed (for debugging failures)
    pub transition_log: Vec<String>,
}

impl SimulationHarness {
    /// Create a new test harness with initialized LSP server
    pub async fn new() -> Self {
        let server = RubyLanguageServer::default();

        // Initialize the server
        let _ = server.initialize(InitializeParams::default()).await;

        let temp_dir = TempDir::new().expect("Failed to create temp directory");

        Self {
            model: LspModel::new(),
            server,
            temp_dir,
            file_paths: HashMap::new(),
            transition_log: Vec::new(),
        }
    }

    /// Apply a transition to both the model and the real server
    pub async fn apply(&mut self, transition: &Transition) -> Result<(), SimulationError> {
        // Log the transition
        self.transition_log.push(transition.description());

        // Apply to model first (this is the oracle)
        self.apply_to_model(transition);

        // Apply to real server
        self.apply_to_server(transition).await?;

        // Verify invariants
        self.check_invariants().await?;

        Ok(())
    }

    /// Apply transition to the model (oracle)
    fn apply_to_model(&mut self, transition: &Transition) {
        match transition {
            Transition::DidOpen { filename, content } => {
                self.model.open(filename.clone(), content.clone());
            }
            Transition::DidChange {
                filename,
                range,
                new_text,
            } => {
                self.model.edit(filename, range, new_text);
            }
            Transition::DidSave { .. } => {
                // Save doesn't change content
            }
            Transition::DidClose { filename } => {
                self.model.close(filename);
            }
            // Read-only operations don't change model state
            _ => {}
        }
    }

    /// Apply transition to the real LSP server
    async fn apply_to_server(&mut self, transition: &Transition) -> Result<(), SimulationError> {
        match transition {
            Transition::DidOpen { filename, content } => {
                let uri = self.get_or_create_uri(filename);

                // Write file to disk (some operations may read from disk)
                let path = self.file_paths.get(filename).unwrap();
                std::fs::write(path, content).map_err(|e| SimulationError::Io(e.to_string()))?;

                // Notify server
                self.server
                    .did_open(DidOpenTextDocumentParams {
                        text_document: TextDocumentItem {
                            uri,
                            language_id: "ruby".to_string(),
                            version: 1,
                            text: content.clone(),
                        },
                    })
                    .await;
            }

            Transition::DidChange {
                filename,
                range: _,
                new_text: _,
            } => {
                let uri = self.get_uri(filename)?;

                // Get new version
                let version = self
                    .model
                    .files
                    .get(filename)
                    .map(|d| d.version)
                    .unwrap_or(1);

                // For full sync, we send the entire new content
                let new_content = self
                    .model
                    .get_content(filename)
                    .unwrap_or("")
                    .to_string();

                // Update file on disk
                let path = self.file_paths.get(filename).unwrap();
                std::fs::write(path, &new_content)
                    .map_err(|e| SimulationError::Io(e.to_string()))?;

                self.server
                    .did_change(DidChangeTextDocumentParams {
                        text_document: VersionedTextDocumentIdentifier {
                            uri,
                            version,
                        },
                        content_changes: vec![TextDocumentContentChangeEvent {
                            range: None, // Full sync
                            range_length: None,
                            text: new_content,
                        }],
                    })
                    .await;
            }

            Transition::DidSave { filename } => {
                let uri = self.get_uri(filename)?;
                self.server
                    .did_save(DidSaveTextDocumentParams {
                        text_document: TextDocumentIdentifier { uri },
                        text: None,
                    })
                    .await;
            }

            Transition::DidClose { filename } => {
                let uri = self.get_uri(filename)?;
                self.server
                    .did_close(DidCloseTextDocumentParams {
                        text_document: TextDocumentIdentifier { uri },
                    })
                    .await;
            }

            Transition::GotoDefinition { filename, position } => {
                let uri = self.get_uri(filename)?;
                let position = self.clamp_position(filename, position);

                // Just execute - we're testing for panics at Level 1
                let _ = self
                    .server
                    .goto_definition(GotoDefinitionParams {
                        text_document_position_params: TextDocumentPositionParams {
                            text_document: TextDocumentIdentifier { uri },
                            position,
                        },
                        work_done_progress_params: Default::default(),
                        partial_result_params: Default::default(),
                    })
                    .await;
            }

            Transition::FindReferences {
                filename,
                position,
                include_declaration,
            } => {
                let uri = self.get_uri(filename)?;
                let position = self.clamp_position(filename, position);

                let _ = self
                    .server
                    .references(ReferenceParams {
                        text_document_position: TextDocumentPositionParams {
                            text_document: TextDocumentIdentifier { uri },
                            position,
                        },
                        work_done_progress_params: Default::default(),
                        partial_result_params: Default::default(),
                        context: ReferenceContext {
                            include_declaration: *include_declaration,
                        },
                    })
                    .await;
            }

            Transition::Completion { filename, position } => {
                let uri = self.get_uri(filename)?;
                let position = self.clamp_position(filename, position);

                let _ = self
                    .server
                    .completion(CompletionParams {
                        text_document_position: TextDocumentPositionParams {
                            text_document: TextDocumentIdentifier { uri },
                            position,
                        },
                        work_done_progress_params: Default::default(),
                        partial_result_params: Default::default(),
                        context: None,
                    })
                    .await;
            }

            Transition::Hover { filename, position } => {
                // Hover is not implemented yet, skip
                let _ = (filename, position);
            }

            Transition::InlayHints { filename, range } => {
                let uri = self.get_uri(filename)?;
                let range = self.clamp_range(filename, range);

                let _ = self
                    .server
                    .inlay_hint(InlayHintParams {
                        text_document: TextDocumentIdentifier { uri },
                        range,
                        work_done_progress_params: Default::default(),
                    })
                    .await;
            }

            Transition::SemanticTokens { filename } => {
                let uri = self.get_uri(filename)?;

                let _ = self
                    .server
                    .semantic_tokens_full(SemanticTokensParams {
                        text_document: TextDocumentIdentifier { uri },
                        work_done_progress_params: Default::default(),
                        partial_result_params: Default::default(),
                    })
                    .await;
            }

            Transition::DocumentSymbols { filename } => {
                let uri = self.get_uri(filename)?;

                let _ = self
                    .server
                    .document_symbol(DocumentSymbolParams {
                        text_document: TextDocumentIdentifier { uri },
                        work_done_progress_params: Default::default(),
                        partial_result_params: Default::default(),
                    })
                    .await;
            }

            Transition::WorkspaceSymbols { query } => {
                let _ = self
                    .server
                    .symbol(WorkspaceSymbolParams {
                        query: query.clone(),
                        work_done_progress_params: Default::default(),
                        partial_result_params: Default::default(),
                    })
                    .await;
            }

            Transition::FoldingRange { filename } => {
                let uri = self.get_uri(filename)?;

                let _ = self
                    .server
                    .folding_range(FoldingRangeParams {
                        text_document: TextDocumentIdentifier { uri },
                        work_done_progress_params: Default::default(),
                        partial_result_params: Default::default(),
                    })
                    .await;
            }

            Transition::CodeLens { filename } => {
                let uri = self.get_uri(filename)?;

                let _ = self
                    .server
                    .code_lens(CodeLensParams {
                        text_document: TextDocumentIdentifier { uri },
                        work_done_progress_params: Default::default(),
                        partial_result_params: Default::default(),
                    })
                    .await;
            }

            Transition::OnTypeFormatting {
                filename,
                position,
                character,
            } => {
                let uri = self.get_uri(filename)?;
                let position = self.clamp_position(filename, position);

                let _ = self
                    .server
                    .on_type_formatting(DocumentOnTypeFormattingParams {
                        text_document_position: TextDocumentPositionParams {
                            text_document: TextDocumentIdentifier { uri },
                            position,
                        },
                        ch: character.to_string(),
                        options: FormattingOptions {
                            tab_size: 2,
                            insert_spaces: true,
                            ..Default::default()
                        },
                    })
                    .await;
            }
        }

        Ok(())
    }

    /// Check invariants after each transition
    async fn check_invariants(&self) -> Result<(), SimulationError> {
        // INVARIANT 1: Text synchronization
        // For each open file in the model, the server should have the same content
        for (filename, doc_state) in &self.model.files {
            let uri = match self.file_paths.get(filename) {
                Some(path) => Url::from_file_path(path).unwrap(),
                None => continue, // File not yet created
            };

            // Get content from server's document cache
            if let Some(server_doc) = self.server.get_doc(&uri) {
                if server_doc.content != doc_state.content {
                    return Err(SimulationError::TextSyncMismatch {
                        filename: filename.clone(),
                        model_content: doc_state.content.clone(),
                        server_content: server_doc.content.clone(),
                    });
                }
            }
        }

        // INVARIANT 2: No orphaned files in server
        // (Server shouldn't have files that model doesn't know about)
        // This is harder to check without access to server internals

        Ok(())
    }

    /// Get or create a URI for a filename
    fn get_or_create_uri(&mut self, filename: &str) -> Url {
        if let Some(path) = self.file_paths.get(filename) {
            return Url::from_file_path(path).unwrap();
        }

        let path = self.temp_dir.path().join(filename);
        self.file_paths.insert(filename.to_string(), path.clone());
        Url::from_file_path(path).unwrap()
    }

    /// Get URI for an existing file
    fn get_uri(&self, filename: &str) -> Result<Url, SimulationError> {
        self.file_paths
            .get(filename)
            .map(|p| Url::from_file_path(p).unwrap())
            .ok_or_else(|| SimulationError::FileNotFound(filename.to_string()))
    }

    /// Clamp a position to valid bounds for the file
    fn clamp_position(&self, filename: &str, position: &Position) -> Position {
        let line_count = self.model.line_count(filename);
        let line = (position.line as usize).min(line_count.saturating_sub(1));
        let line_len = self.model.line_length(filename, line);
        let character = (position.character as usize).min(line_len);

        Position {
            line: line as u32,
            character: character as u32,
        }
    }

    /// Clamp a range to valid bounds for the file
    fn clamp_range(&self, filename: &str, range: &Range) -> Range {
        Range {
            start: self.clamp_position(filename, &range.start),
            end: self.clamp_position(filename, &range.end),
        }
    }

    /// Get the transition log for debugging
    pub fn get_log(&self) -> &[String] {
        &self.transition_log
    }
}

/// Errors that can occur during simulation
#[derive(Debug)]
pub enum SimulationError {
    /// File not found in harness
    FileNotFound(String),
    /// IO error
    Io(String),
    /// Text synchronization mismatch between model and server
    TextSyncMismatch {
        filename: String,
        model_content: String,
        server_content: String,
    },
    /// LSP error
    LspError(String),
}

impl std::fmt::Display for SimulationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SimulationError::FileNotFound(name) => write!(f, "File not found: {}", name),
            SimulationError::Io(msg) => write!(f, "IO error: {}", msg),
            SimulationError::TextSyncMismatch {
                filename,
                model_content,
                server_content,
            } => {
                write!(
                    f,
                    "Text sync mismatch for {}:\n  Model:  {:?}\n  Server: {:?}",
                    filename, model_content, server_content
                )
            }
            SimulationError::LspError(msg) => write!(f, "LSP error: {}", msg),
        }
    }
}

impl std::error::Error for SimulationError {}

