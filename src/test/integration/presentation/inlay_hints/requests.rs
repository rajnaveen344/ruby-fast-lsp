//! Inlay hint requests: end labels, implicit returns, and waiting for the
//! document's current semantic commit.

use crate::features::presentation::inlay_hints;
use crate::lsp::lifecycle::indexing;
use crate::server::RubyLanguageServer;
use std::sync::Arc;
use std::time::Duration;
use tower_lsp::lsp_types::{
    DidChangeTextDocumentParams, DidOpenTextDocumentParams, InitializeParams, InlayHintLabel,
    InlayHintParams, Position, Range, TextDocumentContentChangeEvent, TextDocumentIdentifier,
    TextDocumentItem, Url, VersionedTextDocumentIdentifier,
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
    let uri = crate::test::harness::fixture_uri("/test_end_labels.rb");
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

    let hints = inlay_hints::handle(&server, inlay_params)
        .await
        .unwrap()
        .unwrap();

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
    let uri = crate::test::harness::fixture_uri("/test_implicit.rb");
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

    let hints = inlay_hints::handle(&server, inlay_params)
        .await
        .unwrap()
        .unwrap();

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
    server
        .indexing
        .set_resources(crate::utils::admission::IndexingResourceGovernor::new(
            crate::utils::admission::IndexingResourcePolicy::with_limits(
                1,
                1,
                256 * 1024 * 1024,
                1,
            ),
        ));
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
    let holder_resources = server.indexing.resources().clone();
    let holder_root = workspace.path().to_path_buf();
    let holder = tokio::spawn(async move {
        holder_resources
            .run_async_with_resources(
                "inlay semantic commit contention holder",
                crate::utils::admission::IndexingWorkSpec::new(
                    Some(holder_root),
                    crate::utils::admission::IndexingResourcePriority::Background,
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
        while server.indexing.resources().snapshot().active_tasks != 1 {
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
        while server.indexing.resources().snapshot().queued_tasks != 1 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("didChange must hold the document semantic lock while waiting for admission");

    let hint_server = server.clone();
    let hint_uri = uri.clone();
    let hints = tokio::spawn(async move {
        inlay_hints::handle(
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
    let hints = hints.await.unwrap().unwrap().unwrap();
    assert!(hints.iter().any(|hint| {
        hint.position.line == 5 && crate::test::harness::get_hint_label(hint) == ": String"
    }));
}
