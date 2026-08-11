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

use crate::query::{EngineQuery, InlayHintData, InlayHintKind};
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
/// 2. Delegates to EngineQuery::get_inlay_hints()
/// 3. Converts InlayHintData to LSP InlayHint
pub async fn handle_inlay_hints(
    server: &RubyLanguageServer,
    params: InlayHintParams,
) -> Vec<InlayHint> {
    let uri = params.text_document.uri;
    let range = params.range;
    let semantic_lock = server.document_semantic_lock(&uri);
    let _semantic_guard = semantic_lock.lock().await;

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

    // Create query context.
    let query =
        EngineQuery::with_doc_and_engine(doc_arc.clone(), server.analysis_engine_for_uri(&uri));

    // Get document for query
    let document = doc_arc.read();

    // Delegate to query layer
    let hints = query.get_inlay_hints(&document, &range, &content);

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
    use crate::capabilities::indexing;
    use crate::server::RubyLanguageServer;
    use std::sync::Arc;
    use std::time::Duration;
    use tower_lsp::lsp_types::{
        DidChangeTextDocumentParams, DidOpenTextDocumentParams, InitializeParams, Position, Range,
        TextDocumentContentChangeEvent, TextDocumentIdentifier, TextDocumentItem, Url,
        VersionedTextDocumentIdentifier,
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

    #[tokio::test(flavor = "current_thread")]
    async fn inlay_hints_wait_for_the_current_document_semantic_commit() {
        let workspace = tempfile::TempDir::new().unwrap();
        std::fs::write(
            workspace.path().join("Gemfile"),
            "source 'https://rubygems.org'\n",
        )
        .unwrap();
        let path = workspace.path().join("consumer.rb");
        let uri = Url::from_file_path(&path).unwrap();
        let source = "module ErrorCatalog\n  RETRY = \"retry\".freeze\nend\n\ndef value\n  code = ErrorCatalog::RETRY\n  code\nend\n";
        std::fs::write(&path, source).unwrap();

        let mut server = RubyLanguageServer::default();
        server.indexing_resources = crate::indexing_resources::IndexingResourceGovernor::new(
            crate::indexing_resources::IndexingResourcePolicy::with_limits(
                1,
                1,
                256 * 1024 * 1024,
                1,
            ),
        );
        server.add_workspace(Url::from_directory_path(workspace.path()).unwrap());
        indexing::handle_did_open(
            &server,
            DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri: uri.clone(),
                    language_id: "ruby".to_string(),
                    version: 1,
                    text: source.to_string(),
                },
            },
        )
        .await;

        let release = Arc::new(tokio::sync::Notify::new());
        let holder_release = release.clone();
        let holder_resources = server.indexing_resources.clone();
        let holder_root = workspace.path().to_path_buf();
        let holder = tokio::spawn(async move {
            holder_resources
                .run_async_with_resources(
                    "inlay semantic commit contention holder",
                    crate::indexing_resources::IndexingWorkSpec::new(
                        Some(holder_root),
                        crate::indexing_resources::IndexingResourcePriority::Background,
                        1,
                        256 * 1024 * 1024,
                        1,
                    ),
                    None,
                    async move {
                        holder_release.notified().await;
                    },
                )
                .await
                .unwrap();
        });
        tokio::time::timeout(Duration::from_secs(1), async {
            while server.indexing_resources.snapshot().active_tasks != 1 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("contention holder must be admitted");

        let changed_source = format!("{source}\n# typing must not expose partial facts\n");
        let change_server = server.clone();
        let change_uri = uri.clone();
        let change = tokio::spawn(async move {
            indexing::handle_did_change(
                &change_server,
                DidChangeTextDocumentParams {
                    text_document: VersionedTextDocumentIdentifier {
                        uri: change_uri,
                        version: 2,
                    },
                    content_changes: vec![TextDocumentContentChangeEvent {
                        range: None,
                        range_length: None,
                        text: changed_source,
                    }],
                },
            )
            .await;
        });
        tokio::time::timeout(Duration::from_secs(1), async {
            while server.indexing_resources.snapshot().queued_tasks != 1 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("didChange must hold the document semantic lock while waiting for admission");

        let hint_server = server.clone();
        let hint_uri = uri.clone();
        let hints = tokio::spawn(async move {
            handle_inlay_hints(
                &hint_server,
                InlayHintParams {
                    work_done_progress_params: Default::default(),
                    text_document: TextDocumentIdentifier { uri: hint_uri },
                    range: Range::new(Position::new(0, 0), Position::new(20, 0)),
                },
            )
            .await
        });
        assert!(
            tokio::time::timeout(Duration::from_millis(50), async {
                while !hints.is_finished() {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .is_err(),
            "an inlay request must not read the new document with pre-commit or partially replaced engine facts"
        );

        release.notify_one();
        holder.await.unwrap();
        change.await.unwrap();
        let hints = hints.await.unwrap();
        assert!(hints.iter().any(|hint| {
            hint.position.line == 5
                && matches!(&hint.label, InlayHintLabel::String(label) if label == ": String")
        }));
    }
}
