//! LSP request helpers that observe real handler responses.

use tower_lsp::lsp_types::{
    CallHierarchyIncomingCall, CallHierarchyIncomingCallsParams, CallHierarchyItem,
    CallHierarchyOutgoingCall, CallHierarchyOutgoingCallsParams, CallHierarchyPrepareParams,
    CodeActionContext, CodeActionOrCommand, CodeActionParams, CodeLens, CodeLensParams,
    CompletionContext, CompletionItem, CompletionParams, CompletionResponse, CompletionTriggerKind,
    Diagnostic, DocumentFormattingParams, DocumentHighlight, DocumentHighlightParams,
    DocumentSymbol, DocumentSymbolParams, DocumentSymbolResponse, FormattingOptions,
    GotoDefinitionParams, GotoDefinitionResponse, Hover, HoverParams, InlayHint, InlayHintParams,
    Location, PartialResultParams, Position, PrepareRenameResponse, Range, ReferenceContext,
    ReferenceParams, RenameParams, SelectionRange, SelectionRangeParams, SignatureHelp,
    SignatureHelpParams, TextDocumentIdentifier, TextDocumentPositionParams, TextEdit,
    WorkDoneProgressParams, WorkspaceEdit,
};
use tower_lsp::LanguageServer;

use super::{observe_response, FakeEditor};

impl FakeEditor {
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
        observe_response("hover", self.server.hover(params).await)
    }

    /// Returns signature help at a 0-indexed position.
    pub async fn signature_help_at(
        &self,
        filename: &str,
        line: u32,
        character: u32,
    ) -> Option<SignatureHelp> {
        self.assert_open(filename, "signature_help_at");
        let uri = Self::filename_to_uri(filename);
        let params = SignatureHelpParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri },
                position: Position::new(line, character),
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            context: None,
        };
        observe_response("signature_help", self.server.signature_help(params).await)
    }

    /// Observe the complete definition response, including link selection ranges.
    pub async fn goto_def_response_at(
        &self,
        filename: &str,
        line: u32,
        character: u32,
    ) -> Option<GotoDefinitionResponse> {
        self.assert_open(filename, "goto_def_response_at");
        let uri = Self::filename_to_uri(filename);
        let params = GotoDefinitionParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri },
                position: Position::new(line, character),
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };
        observe_response("goto_definition", self.server.goto_definition(params).await)
    }

    /// Returns goto-definition locations at a 0-indexed position.
    pub async fn goto_def_at(&self, filename: &str, line: u32, character: u32) -> Vec<Location> {
        match self.goto_def_response_at(filename, line, character).await {
            Some(GotoDefinitionResponse::Scalar(loc)) => vec![loc],
            Some(GotoDefinitionResponse::Array(locs)) => locs,
            Some(GotoDefinitionResponse::Link(links)) => links
                .into_iter()
                .map(|link| Location {
                    uri: link.target_uri,
                    range: link.target_range,
                })
                .collect(),
            None => vec![],
        }
    }

    /// Returns definition LocationLinks (preserves originSelectionRange).
    pub async fn goto_def_links_at(
        &self,
        filename: &str,
        line: u32,
        character: u32,
    ) -> Vec<tower_lsp::lsp_types::LocationLink> {
        match self.goto_def_response_at(filename, line, character).await {
            Some(GotoDefinitionResponse::Link(links)) => links,
            Some(GotoDefinitionResponse::Scalar(loc)) => {
                vec![tower_lsp::lsp_types::LocationLink {
                    origin_selection_range: None,
                    target_uri: loc.uri,
                    target_range: loc.range,
                    target_selection_range: loc.range,
                }]
            }
            Some(GotoDefinitionResponse::Array(locs)) => locs
                .into_iter()
                .map(|loc| tower_lsp::lsp_types::LocationLink {
                    origin_selection_range: None,
                    target_uri: loc.uri,
                    target_range: loc.range,
                    target_selection_range: loc.range,
                })
                .collect(),
            None => vec![],
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
        match observe_response(
            "goto_implementation",
            self.server.goto_implementation(params).await,
        ) {
            Some(GotoDefinitionResponse::Scalar(loc)) => vec![loc],
            Some(GotoDefinitionResponse::Array(locs)) => locs,
            Some(GotoDefinitionResponse::Link(links)) => links
                .into_iter()
                .map(|link| Location {
                    uri: link.target_uri,
                    range: link.target_range,
                })
                .collect(),
            None => vec![],
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
        observe_response(
            "prepare_call_hierarchy",
            self.server.prepare_call_hierarchy(params).await,
        )
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
        observe_response("incoming_calls", self.server.incoming_calls(params).await)
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
        observe_response("outgoing_calls", self.server.outgoing_calls(params).await)
            .unwrap_or_default()
    }

    /// Returns all references at a 0-indexed position.
    pub async fn references_at(&self, filename: &str, line: u32, character: u32) -> Vec<Location> {
        self.references_with_declaration_at(filename, line, character, true)
            .await
    }

    pub async fn references_with_declaration_at(
        &self,
        filename: &str,
        line: u32,
        character: u32,
        include_declaration: bool,
    ) -> Vec<Location> {
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
                include_declaration,
            },
        };
        observe_response("references", self.server.references(params).await).unwrap_or_default()
    }

    /// Returns semantic highlights in the current document at a 0-indexed position.
    pub async fn document_highlights_at(
        &self,
        filename: &str,
        line: u32,
        character: u32,
    ) -> Vec<DocumentHighlight> {
        self.assert_open(filename, "document_highlights_at");
        let params = DocumentHighlightParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: Self::filename_to_uri(filename),
                },
                position: Position::new(line, character),
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };
        observe_response(
            "document_highlight",
            self.server.document_highlight(params).await,
        )
        .unwrap_or_default()
    }

    /// Returns one nested selection chain per requested position.
    pub async fn selection_ranges(
        &self,
        filename: &str,
        positions: &[Position],
    ) -> Vec<SelectionRange> {
        self.assert_open(filename, "selection_ranges");
        let params = SelectionRangeParams {
            text_document: TextDocumentIdentifier {
                uri: Self::filename_to_uri(filename),
            },
            positions: positions.to_vec(),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };
        observe_response("selection_range", self.server.selection_range(params).await)
            .unwrap_or_default()
    }

    /// Requests full-document formatting for the current unsaved buffer.
    pub async fn format(&self, filename: &str) -> Vec<TextEdit> {
        self.assert_open(filename, "format");
        observe_response(
            "formatting",
            self.server
                .formatting(DocumentFormattingParams {
                    text_document: TextDocumentIdentifier {
                        uri: Self::filename_to_uri(filename),
                    },
                    options: FormattingOptions {
                        tab_size: 2,
                        insert_spaces: true,
                        ..FormattingOptions::default()
                    },
                    work_done_progress_params: WorkDoneProgressParams::default(),
                })
                .await,
        )
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
        observe_response("inlay_hint", self.server.inlay_hint(params).await).unwrap_or_default()
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
        observe_response("code_lens", self.server.code_lens(params).await).unwrap_or_default()
    }

    pub async fn code_actions(
        &self,
        filename: &str,
        diagnostics: Vec<Diagnostic>,
    ) -> Vec<CodeActionOrCommand> {
        self.assert_open(filename, "code_actions");
        let uri = Self::filename_to_uri(filename);
        observe_response(
            "code_action",
            self.server
                .code_action(CodeActionParams {
                    text_document: TextDocumentIdentifier { uri },
                    range: Range::default(),
                    context: CodeActionContext {
                        diagnostics,
                        only: Some(vec![tower_lsp::lsp_types::CodeActionKind::QUICKFIX]),
                        trigger_kind: None,
                    },
                    work_done_progress_params: WorkDoneProgressParams::default(),
                    partial_result_params: PartialResultParams::default(),
                })
                .await,
        )
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

        match observe_response("document_symbol", self.server.document_symbol(params).await) {
            Some(DocumentSymbolResponse::Nested(symbols)) => symbols,
            Some(DocumentSymbolResponse::Flat(_)) => panic!(
                "INVARIANT VIOLATED: FakeEditor document_symbols received flat symbols. \
                 This is a bug because Ruby Fast LSP document symbol capability returns nested symbols. \
                 Fix: update the harness if flat symbols become supported."
            ),
            None => Vec::new(),
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
        observe_response("rename", self.server.rename(params).await)
    }

    /// Checks whether rename is valid at a 0-indexed position.
    pub async fn prepare_rename_at(
        &self,
        filename: &str,
        line: u32,
        character: u32,
    ) -> Option<PrepareRenameResponse> {
        self.assert_open(filename, "prepare_rename_at");
        let params = TextDocumentPositionParams {
            text_document: TextDocumentIdentifier {
                uri: Self::filename_to_uri(filename),
            },
            position: Position::new(line, character),
        };
        observe_response("prepare_rename", self.server.prepare_rename(params).await)
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
        match observe_response("completion", self.server.completion(params).await) {
            Some(CompletionResponse::Array(items)) => items,
            Some(CompletionResponse::List(list)) => list.items,
            None => vec![],
        }
    }
}
