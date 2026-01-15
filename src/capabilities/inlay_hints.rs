//! Inlay Hints Capability Handler
//!
//! This is a thin handler that delegates all logic to the query layer.
//! The query layer handles:
//! - AST traversal via InlayNodeCollector
//! - Hint generation via generators
//! - Type inference coordination

use tower_lsp::lsp_types::{
    InlayHint, InlayHintKind as LspInlayHintKind, InlayHintLabel, InlayHintOptions,
    InlayHintParams, InlayHintServerCapabilities, InlayHintTooltip, WorkDoneProgressOptions,
};

use crate::query::{IndexQuery, InlayHintData, InlayHintKind};
use crate::server::RubyLanguageServer;

/// Get the server capability for inlay hints.
pub fn get_inlay_hints_capability() -> InlayHintServerCapabilities {
    InlayHintServerCapabilities::Options(InlayHintOptions {
        work_done_progress_options: WorkDoneProgressOptions::default(),
        resolve_provider: Some(false),
    })
}

/// Handle inlay hints request.
///
/// This is a thin handler that:
/// 1. Gets the document
/// 2. Delegates to IndexQuery::get_inlay_hints()
/// 3. Converts InlayHintData to LSP InlayHint
pub async fn handle_inlay_hints(
    server: &RubyLanguageServer,
    params: InlayHintParams,
) -> Vec<InlayHint> {
    let uri = params.text_document.uri;
    let range = params.range;

    // Get document content and Arc
    let (content, doc_arc) = {
        let doc_guard = server.docs.lock();
        match doc_guard.get(&uri) {
            Some(doc_arc) => {
                let doc = doc_arc.read();
                (doc.content.clone(), doc_arc.clone())
            }
            None => return Vec::new(),
        }
    };

    // Create query context
    let query = IndexQuery::with_doc(server.index.clone(), doc_arc.clone());

    // Get document for query
    let document = doc_arc.read();

    // Delegate to query layer
    let hints = query.get_inlay_hints(&document, &range, &content, Some(&server.type_narrowing));

    // Convert to LSP format
    hints.into_iter().map(to_lsp_hint).collect()
}

/// Convert InlayHintData to LSP InlayHint.
fn to_lsp_hint(hint: InlayHintData) -> InlayHint {
    InlayHint {
        position: hint.position,
        label: InlayHintLabel::String(hint.label),
        kind: Some(match hint.kind {
            InlayHintKind::EndLabel | InlayHintKind::ImplicitReturn => LspInlayHintKind::PARAMETER,
            InlayHintKind::VariableType
            | InlayHintKind::MethodReturn
            | InlayHintKind::ParameterType
            | InlayHintKind::ChainedMethodType => LspInlayHintKind::TYPE,
        }),
        text_edits: None,
        tooltip: hint.tooltip.map(InlayHintTooltip::String),
        padding_left: Some(hint.padding_left),
        padding_right: Some(hint.padding_right),
        data: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::RubyLanguageServer;
    use tower_lsp::lsp_types::{
        DidOpenTextDocumentParams, InitializeParams, Position, Range, TextDocumentIdentifier,
        TextDocumentItem, Url,
    };
    use tower_lsp::LanguageServer;

    async fn create_test_server() -> RubyLanguageServer {
        let server = RubyLanguageServer::default();
        let _ = server.initialize(InitializeParams::default()).await;
        server
    }

    #[tokio::test]
    async fn test_inlay_hints_end_labels() {
        let server = create_test_server().await;
        let uri = Url::parse("file:///test_end_labels.rb").unwrap();
        let content = "class Foo\nend";

        let params = DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: "ruby".into(),
                version: 1,
                text: content.to_string(),
            },
        };
        server.did_open(params).await;

        let inlay_params = InlayHintParams {
            work_done_progress_params: Default::default(),
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            range: Range {
                start: Position::new(0, 0),
                end: Position::new(10, 0),
            },
        };

        let hints = handle_inlay_hints(&server, inlay_params).await;

        // Should have "class Foo" end label
        let end_hint = hints.iter().find(|h| {
            if let InlayHintLabel::String(s) = &h.label {
                s.contains("class Foo")
            } else {
                false
            }
        });
        assert!(end_hint.is_some(), "Should have end label for class");
    }

    #[tokio::test]
    async fn test_inlay_hints_implicit_return() {
        let server = create_test_server().await;
        let uri = Url::parse("file:///test_implicit.rb").unwrap();
        let content = "def foo\n  42\nend";

        let params = DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: "ruby".into(),
                version: 1,
                text: content.to_string(),
            },
        };
        server.did_open(params).await;

        let inlay_params = InlayHintParams {
            work_done_progress_params: Default::default(),
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            range: Range {
                start: Position::new(0, 0),
                end: Position::new(10, 0),
            },
        };

        let hints = handle_inlay_hints(&server, inlay_params).await;

        // Should have "return" hint
        let return_hint = hints.iter().find(|h| {
            if let InlayHintLabel::String(s) = &h.label {
                s == "return"
            } else {
                false
            }
        });
        assert!(return_hint.is_some(), "Should have implicit return hint");
    }
}
