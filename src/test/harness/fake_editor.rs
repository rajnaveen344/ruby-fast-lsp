//! FakeEditor - a stateful editor simulation for lifecycle testing.
//!
//! FakeEditor routes all operations through the real LSP handlers
//! (`handle_did_open`, `handle_did_change`, `handle_did_close`, `handle_did_save`),
//! ensuring tests exercise the exact same code paths as a real editor.
//!
//! # Tag-based assertions (simple feature tests)
//!
//! ```ignore
//! let mut editor = FakeEditor::new().await;
//! editor.open("foo.rb", "class Foo\n  def greet; end\nend").await;
//! editor.check("foo.rb", r#"
//! class Foo
//!   def $0greet; end
//! end
//! "#).await;
//! ```
//!
//! # Programmatic assertions (complex scenarios)
//!
//! ```ignore
//! let mut editor = FakeEditor::new().await;
//! editor.open("test.rb", "user = User.new\nuser.").await;
//!
//! // Type "na" after the dot
//! editor.type_at("test.rb", 1, 5, "na").await;
//!
//! // Check completions filter to "name"
//! let items = editor.complete_with_trigger("test.rb", 1, 7, ".").await;
//! assert!(items.iter().any(|i| i.label == "name"));
//!
//! // Backspace and retype
//! editor.backspace_at("test.rb", 1, 7, 2).await;
//! editor.type_at("test.rb", 1, 5, "to").await;
//! let items = editor.complete_with_trigger("test.rb", 1, 7, ".").await;
//! assert!(items.iter().any(|i| i.label == "to_s"));
//! ```
//!
//! # Available methods
//!
//! **Lifecycle**: `open`, `set`, `save`, `close`
//! **Editing**: `type_at`, `backspace_at`
//! **Queries**: `complete_at`, `complete_with_trigger`, `hover_at`, `goto_def_at`,
//!             `references_at`, `inlay_hints`, `code_lens`, `diagnostics`, `rename_at`
//! **Apply**: `apply_edit` (applies WorkspaceEdit from rename/code actions)
//! **Assertions**: `check` (tag-based), `content` (get current file content)

use std::collections::HashMap;

use tower_lsp::lsp_types::{
    CallHierarchyIncomingCall, CallHierarchyIncomingCallsParams, CallHierarchyItem,
    CallHierarchyOutgoingCall, CallHierarchyOutgoingCallsParams, CallHierarchyPrepareParams,
    CodeLens, CodeLensParams, CompletionContext, CompletionItem, CompletionParams,
    CompletionResponse, CompletionTriggerKind, Diagnostic, DiagnosticSeverity,
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    DidSaveTextDocumentParams, DocumentSymbol, DocumentSymbolParams, DocumentSymbolResponse,
    GotoDefinitionParams, GotoDefinitionResponse, Hover, HoverParams, InitializeParams, InlayHint,
    InlayHintParams, Location, NumberOrString, PartialResultParams, Position, Range,
    ReferenceContext, ReferenceParams, RenameParams, TextDocumentContentChangeEvent,
    TextDocumentIdentifier, TextDocumentItem, TextDocumentPositionParams, Url,
    VersionedTextDocumentIdentifier, WorkDoneProgressParams, WorkspaceEdit,
};
use tower_lsp::LanguageServer;

use crate::capabilities::indexing;
use crate::server::RubyLanguageServer;

use super::check::{run_checks_on_fixture, strip_all_markers};

/// A stateful editor simulation for testing LSP lifecycle scenarios.
///
/// Routes all operations through the real LSP handlers, ensuring tests
/// exercise the exact same code paths as a real editor. Tracks open files
/// with their content and version numbers for assertion verification.
pub struct FakeEditor {
    server: RubyLanguageServer,
    /// Tracks open files: filename -> (clean_content, version)
    buffers: HashMap<String, (String, i32)>,
}

impl FakeEditor {
    /// Create a new FakeEditor with a fresh, initialized server.
    pub async fn new() -> Self {
        let server = RubyLanguageServer::default();
        let _ = server.initialize(InitializeParams::default()).await;
        FakeEditor {
            server,
            buffers: HashMap::new(),
        }
    }

    /// Register a workspace folder with the underlying server.
    ///
    /// Use this to test multi-root behavior. Files opened with paths under
    /// `root` (e.g. `add_workspace("workspace_a"); open("workspace_a/foo.rb", ..)`)
    /// will route to that workspace's index instead of the orphan index.
    pub fn add_workspace(&self, root: &str) {
        let uri = Url::parse(&format!("file:///{}/", root.trim_end_matches('/')))
            .expect("Invalid workspace URI");
        self.server.add_workspace(uri);
    }

    /// Remove a previously added workspace folder.
    pub fn remove_workspace(&self, root: &str) {
        let uri = Url::parse(&format!("file:///{}/", root.trim_end_matches('/')))
            .expect("Invalid workspace URI");
        self.server.remove_workspace(&uri);
    }

    /// Number of registered workspaces (excluding orphan).
    pub fn workspace_count(&self) -> usize {
        self.server.list_workspaces().len()
    }

    /// Look up the workspace handle (index, root, indexing_complete) for a file path.
    pub fn workspace_for(&self, filename: &str) -> Option<crate::server::Workspace> {
        let uri = Self::filename_to_uri(filename);
        self.server.workspace_for_uri(&uri)
    }

    /// Open a file in the editor with the given content.
    ///
    /// Routes through the real `handle_did_open` handler.
    /// Panics if the file is already open (use `set()` to update).
    pub async fn open(&mut self, filename: &str, content: &str) {
        assert!(
            !self.buffers.contains_key(filename),
            "INVARIANT VIOLATED: File '{}' is already open. Use set() to update content.",
            filename
        );

        let uri = Self::filename_to_uri(filename);
        let version = 1;

        let params = DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri,
                language_id: "ruby".to_string(),
                version,
                text: content.to_string(),
            },
        };
        indexing::handle_did_open(&self.server, params).await;

        self.buffers
            .insert(filename.to_string(), (content.to_string(), version));
    }

    /// Update an open file's content.
    ///
    /// Routes through the real `handle_did_change` handler.
    /// Panics if the file is not open (use `open()` first).
    pub async fn set(&mut self, filename: &str, new_content: &str) {
        let (_, version) = self.buffers.get(filename).unwrap_or_else(|| {
            panic!(
                "INVARIANT VIOLATED: File '{}' is not open. Call open() before set().",
                filename
            )
        });
        let new_version = version + 1;

        let uri = Self::filename_to_uri(filename);

        let params = DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri,
                version: new_version,
            },
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: new_content.to_string(),
            }],
        };
        indexing::handle_did_change(&self.server, params).await;

        self.buffers
            .insert(filename.to_string(), (new_content.to_string(), new_version));
    }

    /// Save a file in the editor.
    ///
    /// Routes through the real `handle_did_save` handler.
    /// Triggers YARD diagnostics and inlay hint refresh.
    /// Panics if the file is not open.
    pub async fn save(&mut self, filename: &str) {
        assert!(
            self.buffers.contains_key(filename),
            "INVARIANT VIOLATED: File '{}' is not open. Call open() before save().",
            filename
        );

        let uri = Self::filename_to_uri(filename);

        let params = DidSaveTextDocumentParams {
            text_document: TextDocumentIdentifier { uri },
            text: None,
        };
        indexing::handle_did_save(&self.server, params).await;
    }

    /// Close a file in the editor.
    ///
    /// Routes through the real `handle_did_close` handler.
    /// Index entries are preserved (matching real LSP behavior).
    pub async fn close(&mut self, filename: &str) {
        assert!(
            self.buffers.remove(filename).is_some(),
            "INVARIANT VIOLATED: File '{}' is not open. Cannot close a file that was never opened.",
            filename
        );

        let uri = Self::filename_to_uri(filename);

        let params = DidCloseTextDocumentParams {
            text_document: TextDocumentIdentifier { uri },
        };
        indexing::handle_did_close(&self.server, params).await;
    }

    /// Run tag-based assertions against a file's current state.
    ///
    /// The `fixture` contains markers ($0, <def>, <ref>, <hint>, etc.) embedded
    /// in the expected file content. The clean content extracted from the fixture
    /// must match the file's current buffer content.
    pub async fn check(&self, filename: &str, fixture: &str) {
        let (buffer_content, _) = self.buffers.get(filename).unwrap_or_else(|| {
            panic!(
                "INVARIANT VIOLATED: File '{}' is not open. Call open() before check().",
                filename
            )
        });

        let fixture_content = strip_all_markers(fixture);

        // Verify fixture content matches current buffer (catches test authoring bugs)
        assert_eq!(
            buffer_content.trim(),
            fixture_content.trim(),
            "Fixture content doesn't match buffer for '{}'. \
             The clean content from the fixture must match what was passed to open() or set().\n\
             Buffer:\n{}\n\nFixture (cleaned):\n{}",
            filename,
            buffer_content,
            fixture_content
        );

        let uri = Self::filename_to_uri(filename);
        run_checks_on_fixture(&self.server, &uri, buffer_content, fixture, None).await;
    }

    /// Get a reference to the underlying server.
    pub fn server(&self) -> &RubyLanguageServer {
        &self.server
    }

    /// Consume the editor and return the underlying server.
    pub fn into_server(self) -> RubyLanguageServer {
        self.server
    }

    // ─── Editing Helpers ───────────────────────────────────────────────

    /// Insert text at a 0-indexed position, triggering a `did_change`.
    ///
    /// Simulates the user typing at a specific cursor position.
    /// The position is in the file's current content (before insertion).
    pub async fn type_at(&mut self, filename: &str, line: u32, character: u32, text: &str) {
        let (content, _) = self.buffers.get(filename).unwrap_or_else(|| {
            panic!(
                "INVARIANT VIOLATED: File '{}' is not open. Call open() before type_at().",
                filename
            )
        });

        let offset = Self::position_to_byte_offset(content, line, character);
        let mut new_content = String::with_capacity(content.len() + text.len());
        new_content.push_str(&content[..offset]);
        new_content.push_str(text);
        new_content.push_str(&content[offset..]);

        self.set(filename, &new_content).await;
    }

    /// Delete `count` characters before a 0-indexed position, triggering a `did_change`.
    ///
    /// Simulates the user pressing backspace at a specific cursor position.
    pub async fn backspace_at(&mut self, filename: &str, line: u32, character: u32, count: usize) {
        let (content, _) = self.buffers.get(filename).unwrap_or_else(|| {
            panic!(
                "INVARIANT VIOLATED: File '{}' is not open. Call open() before backspace_at().",
                filename
            )
        });

        let offset = Self::position_to_byte_offset(content, line, character);
        let delete_start = offset.saturating_sub(count);
        let mut new_content = String::with_capacity(content.len() - (offset - delete_start));
        new_content.push_str(&content[..delete_start]);
        new_content.push_str(&content[offset..]);

        self.set(filename, &new_content).await;
    }

    // ─── Query Methods ───────────────────────────────────────────────

    /// Returns completion items at a 0-indexed position.
    ///
    /// Sends `context: None` (equivalent to user pressing Ctrl+Space).
    /// Use `complete_with_trigger` to test trigger-character behavior.
    pub async fn complete_at(
        &self,
        filename: &str,
        line: u32,
        character: u32,
    ) -> Vec<CompletionItem> {
        self.complete_with_context(filename, line, character, None)
            .await
    }

    /// Returns completion items with a trigger character context.
    ///
    /// Simulates the editor auto-triggering completion after typing
    /// a trigger character like `.` or `:`.
    pub async fn complete_with_trigger(
        &self,
        filename: &str,
        line: u32,
        character: u32,
        trigger: &str,
    ) -> Vec<CompletionItem> {
        let context = Some(CompletionContext {
            trigger_kind: CompletionTriggerKind::TRIGGER_CHARACTER,
            trigger_character: Some(trigger.to_string()),
        });
        self.complete_with_context(filename, line, character, context)
            .await
    }

    /// Returns hover information at a 0-indexed position.
    pub async fn hover_at(&self, filename: &str, line: u32, character: u32) -> Option<Hover> {
        self.assert_open(filename, "hover_at");
        let uri = Self::filename_to_uri(filename);
        let params = HoverParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri },
                position: Position::new(line, character),
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
        };
        self.server.hover(params).await.ok().flatten()
    }

    /// Returns goto-definition locations at a 0-indexed position.
    pub async fn goto_def_at(&self, filename: &str, line: u32, character: u32) -> Vec<Location> {
        self.assert_open(filename, "goto_def_at");
        let uri = Self::filename_to_uri(filename);
        let params = GotoDefinitionParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri },
                position: Position::new(line, character),
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };
        match self.server.goto_definition(params).await {
            Ok(Some(GotoDefinitionResponse::Scalar(loc))) => vec![loc],
            Ok(Some(GotoDefinitionResponse::Array(locs))) => locs,
            Ok(Some(GotoDefinitionResponse::Link(links))) => links
                .into_iter()
                .map(|link| Location {
                    uri: link.target_uri,
                    range: link.target_range,
                })
                .collect(),
            _ => vec![],
        }
    }

    /// Returns all implementation locations at a 0-indexed position.
    pub async fn goto_impl_at(&self, filename: &str, line: u32, character: u32) -> Vec<Location> {
        self.assert_open(filename, "goto_impl_at");
        let uri = Self::filename_to_uri(filename);
        let params = GotoDefinitionParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri },
                position: Position::new(line, character),
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };
        match self.server.goto_implementation(params).await {
            Ok(Some(GotoDefinitionResponse::Scalar(loc))) => vec![loc],
            Ok(Some(GotoDefinitionResponse::Array(locs))) => locs,
            Ok(Some(GotoDefinitionResponse::Link(links))) => links
                .into_iter()
                .map(|link| Location {
                    uri: link.target_uri,
                    range: link.target_range,
                })
                .collect(),
            _ => vec![],
        }
    }

    /// Prepares call hierarchy at a 0-indexed position, returning the CallHierarchyItem.
    pub async fn prepare_call_hierarchy_at(
        &self,
        filename: &str,
        line: u32,
        character: u32,
    ) -> Vec<CallHierarchyItem> {
        self.assert_open(filename, "prepare_call_hierarchy_at");
        let uri = Self::filename_to_uri(filename);
        let params = CallHierarchyPrepareParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri },
                position: Position::new(line, character),
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
        };
        self.server
            .prepare_call_hierarchy(params)
            .await
            .ok()
            .flatten()
            .unwrap_or_default()
    }

    /// Returns incoming calls for a CallHierarchyItem.
    pub async fn incoming_calls_for(
        &self,
        item: CallHierarchyItem,
    ) -> Vec<CallHierarchyIncomingCall> {
        let params = CallHierarchyIncomingCallsParams {
            item,
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };
        self.server
            .incoming_calls(params)
            .await
            .ok()
            .flatten()
            .unwrap_or_default()
    }

    /// Returns outgoing calls for a CallHierarchyItem.
    pub async fn outgoing_calls_for(
        &self,
        item: CallHierarchyItem,
    ) -> Vec<CallHierarchyOutgoingCall> {
        let params = CallHierarchyOutgoingCallsParams {
            item,
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };
        self.server
            .outgoing_calls(params)
            .await
            .ok()
            .flatten()
            .unwrap_or_default()
    }

    /// Returns all references at a 0-indexed position.
    pub async fn references_at(&self, filename: &str, line: u32, character: u32) -> Vec<Location> {
        self.assert_open(filename, "references_at");
        let uri = Self::filename_to_uri(filename);
        let params = ReferenceParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri },
                position: Position::new(line, character),
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
            context: ReferenceContext {
                include_declaration: true,
            },
        };
        self.server
            .references(params)
            .await
            .ok()
            .flatten()
            .unwrap_or_default()
    }

    /// Returns inlay hints for an entire file.
    pub async fn inlay_hints(&self, filename: &str) -> Vec<InlayHint> {
        self.assert_open(filename, "inlay_hints");
        let uri = Self::filename_to_uri(filename);
        let (content, _) = &self.buffers[filename];
        let line_count = content.lines().count() as u32;

        let params = InlayHintParams {
            text_document: TextDocumentIdentifier { uri },
            range: Range::new(Position::new(0, 0), Position::new(line_count, 0)),
            work_done_progress_params: WorkDoneProgressParams::default(),
        };
        self.server
            .inlay_hint(params)
            .await
            .ok()
            .flatten()
            .unwrap_or_default()
    }

    /// Returns code lenses for a file.
    pub async fn code_lens(&self, filename: &str) -> Vec<CodeLens> {
        self.assert_open(filename, "code_lens");
        let uri = Self::filename_to_uri(filename);
        let params = CodeLensParams {
            text_document: TextDocumentIdentifier { uri },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };
        self.server
            .code_lens(params)
            .await
            .ok()
            .flatten()
            .unwrap_or_default()
    }

    /// Returns document symbols for a file.
    pub async fn document_symbols(&self, filename: &str) -> Vec<DocumentSymbol> {
        self.assert_open(filename, "document_symbols");
        let uri = Self::filename_to_uri(filename);
        let params = DocumentSymbolParams {
            text_document: TextDocumentIdentifier { uri },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };

        match self.server.document_symbol(params).await.ok().flatten() {
            Some(DocumentSymbolResponse::Nested(symbols)) => symbols,
            Some(DocumentSymbolResponse::Flat(_)) => panic!(
                "INVARIANT VIOLATED: FakeEditor document_symbols received flat symbols. \
                 This is a bug because Ruby Fast LSP document symbol capability returns nested symbols. \
                 Fix: update the harness if flat symbols become supported."
            ),
            None => Vec::new(),
        }
    }

    /// Returns diagnostics for a file by re-generating them from current state.
    pub async fn diagnostics(&self, filename: &str) -> Vec<Diagnostic> {
        self.assert_open(filename, "diagnostics");
        let uri = Self::filename_to_uri(filename);
        let (content, _) = &self.buffers[filename];

        let document = self.server.docs.lock().get(&uri).unwrap().read().clone();
        let parse_result = ruby_prism::parse(content.as_bytes());

        let mut diagnostics =
            crate::capabilities::diagnostics::generate_diagnostics(&parse_result, &document);

        {
            use ruby_analysis::indexer::fact_collector::FactCollector;
            use ruby_prism::Visit;
            use std::sync::Arc;

            let mut visitor = FactCollector::analysis_only(
                document.clone(),
                Arc::new(self.server.extension_registry.clone()),
                self.server.analysis_engine.clone(),
            );
            visitor.visit(&parse_result.node());
            self.server
                .analysis_engine
                .lock()
                .replace_file_reference_analysis(
                    visitor.document.analysis_file_id(),
                    visitor.reference_candidates,
                    visitor.diagnostic_candidates,
                    visitor.analysis_diagnostics,
                );
        }

        // Add unresolved entry diagnostics
        {
            let query = crate::query::EngineQuery::with_engine(self.server.analysis_engine.clone());
            diagnostics.extend(query.get_unresolved_diagnostics(&uri));
        }

        diagnostics
    }

    /// Assert the file has zero ERROR-severity diagnostics.
    ///
    /// Panics with a list of all errors if any are found. WARNING/INFO/HINT
    /// diagnostics are ignored — use `assert_no_diagnostics()` for stricter check.
    pub async fn assert_no_errors(&self, filename: &str) {
        let diags = self.diagnostics(filename).await;
        let errors: Vec<&Diagnostic> = diags
            .iter()
            .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
            .collect();
        assert!(
            errors.is_empty(),
            "Expected no errors in '{}', got {}: {:?}",
            filename,
            errors.len(),
            errors.iter().map(|e| describe(e)).collect::<Vec<_>>()
        );
    }

    /// Assert exact total diagnostic count for the file (all severities).
    pub async fn assert_diag_count(&self, filename: &str, expected: usize) {
        let diags = self.diagnostics(filename).await;
        assert_eq!(
            diags.len(),
            expected,
            "Expected {} diagnostics in '{}', got {}: {:?}",
            expected,
            filename,
            diags.len(),
            diags.iter().map(|d| describe(d)).collect::<Vec<_>>()
        );
    }

    /// Assert at least one ERROR diagnostic with the given code exists.
    /// Returns the matched diagnostic for further inspection.
    pub async fn assert_error_code(&self, filename: &str, code: &str) -> Diagnostic {
        let diags = self.diagnostics(filename).await;
        let found = diags.iter().find(|d| {
            d.severity == Some(DiagnosticSeverity::ERROR)
                && matches!(&d.code, Some(NumberOrString::String(s)) if s == code)
        });
        match found {
            Some(d) => d.clone(),
            None => panic!(
                "Expected error with code '{}' in '{}'. Actual diagnostics: {:?}",
                code,
                filename,
                diags.iter().map(|d| describe(d)).collect::<Vec<_>>()
            ),
        }
    }

    /// Performs a rename at a 0-indexed position and returns the workspace edit.
    pub async fn rename_at(
        &self,
        filename: &str,
        line: u32,
        character: u32,
        new_name: &str,
    ) -> Option<WorkspaceEdit> {
        self.assert_open(filename, "rename_at");
        let uri = Self::filename_to_uri(filename);
        let params = RenameParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri },
                position: Position::new(line, character),
            },
            new_name: new_name.to_string(),
            work_done_progress_params: WorkDoneProgressParams::default(),
        };
        self.server.rename(params).await.ok().flatten()
    }

    /// Applies a `WorkspaceEdit` to the editor's buffers.
    ///
    /// Updates affected files via `set()`, so changes go through the real
    /// `handle_did_change` handler. Only supports the `changes` field
    /// (not `document_changes`).
    pub async fn apply_edit(&mut self, edit: &WorkspaceEdit) {
        if let Some(changes) = &edit.changes {
            for (uri, text_edits) in changes {
                // Find the filename for this URI
                let filename = self
                    .buffers
                    .keys()
                    .find(|f| Self::filename_to_uri(f) == *uri)
                    .cloned();

                let filename = filename.unwrap_or_else(|| {
                    panic!(
                        "INVARIANT VIOLATED: WorkspaceEdit references URI '{}' which is not open in the editor.",
                        uri
                    )
                });

                let (content, _) = &self.buffers[&filename];
                let mut new_content = content.clone();

                // Apply edits in reverse order to preserve positions
                let mut sorted_edits = text_edits.clone();
                sorted_edits.sort_by(|a, b| {
                    b.range
                        .start
                        .line
                        .cmp(&a.range.start.line)
                        .then(b.range.start.character.cmp(&a.range.start.character))
                });

                for edit in &sorted_edits {
                    let start = Self::position_to_byte_offset(
                        &new_content,
                        edit.range.start.line,
                        edit.range.start.character,
                    );
                    let end = Self::position_to_byte_offset(
                        &new_content,
                        edit.range.end.line,
                        edit.range.end.character,
                    );
                    new_content = format!(
                        "{}{}{}",
                        &new_content[..start],
                        edit.new_text,
                        &new_content[end..]
                    );
                }

                self.set(&filename, &new_content).await;
            }
        }
    }

    /// Get the current content of an open file.
    pub fn content(&self, filename: &str) -> &str {
        let (content, _) = self.buffers.get(filename).unwrap_or_else(|| {
            panic!(
                "INVARIANT VIOLATED: File '{}' is not open. Call open() before content().",
                filename
            )
        });
        content
    }

    // ─── Internal Helpers ────────────────────────────────────────────

    /// Convert a filename to a virtual URI.
    pub(super) fn filename_to_uri(filename: &str) -> Url {
        Url::parse(&format!("file:///{}", filename)).expect("Invalid virtual URI")
    }

    /// Assert a file is open, panicking with a clear message if not.
    fn assert_open(&self, filename: &str, method: &str) {
        assert!(
            self.buffers.contains_key(filename),
            "INVARIANT VIOLATED: File '{}' is not open. Call open() before {}().",
            filename,
            method
        );
    }

    /// Convert a 0-indexed (line, character) position to a byte offset in content.
    fn position_to_byte_offset(content: &str, line: u32, character: u32) -> usize {
        let mut offset = 0;
        for (i, line_str) in content.split('\n').enumerate() {
            if i == line as usize {
                return offset + (character as usize).min(line_str.len());
            }
            offset += line_str.len() + 1; // +1 for '\n'
        }
        content.len()
    }

    /// Internal: completion with arbitrary context.
    async fn complete_with_context(
        &self,
        filename: &str,
        line: u32,
        character: u32,
        context: Option<CompletionContext>,
    ) -> Vec<CompletionItem> {
        self.assert_open(filename, "complete_at");
        let uri = Self::filename_to_uri(filename);
        let params = CompletionParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri },
                position: Position::new(line, character),
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
            context,
        };
        match self.server.completion(params).await {
            Ok(Some(CompletionResponse::Array(items))) => items,
            Ok(Some(CompletionResponse::List(list))) => list.items,
            _ => vec![],
        }
    }
}

/// Pretty-print a diagnostic for assertion failure messages.
fn describe(d: &Diagnostic) -> String {
    let code = match &d.code {
        Some(NumberOrString::String(s)) => s.clone(),
        Some(NumberOrString::Number(n)) => n.to_string(),
        None => "<no-code>".to_string(),
    };
    let sev = match d.severity {
        Some(DiagnosticSeverity::ERROR) => "ERROR",
        Some(DiagnosticSeverity::WARNING) => "WARNING",
        Some(DiagnosticSeverity::INFORMATION) => "INFO",
        Some(DiagnosticSeverity::HINT) => "HINT",
        _ => "?",
    };
    format!(
        "{}:{}-{}:{} [{} {}] {:?}",
        d.range.start.line,
        d.range.start.character,
        d.range.end.line,
        d.range.end.character,
        sev,
        code,
        d.message
    )
}
