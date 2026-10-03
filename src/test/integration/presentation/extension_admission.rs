//! Request-time extension work (code lenses, document symbols) waits for
//! weighted admission without blocking the Tokio reactor.

use crate::features::presentation::{code_lens, document_symbols};
use crate::server::Server;
use std::sync::Arc;
use std::time::Duration;
use tower_lsp::lsp_types::{
    CodeLensParams, DidOpenTextDocumentParams, DocumentSymbolParams, DocumentSymbolResponse,
    TextDocumentIdentifier, TextDocumentItem,
};

#[tokio::test(flavor = "current_thread")]
async fn request_time_extension_code_lenses_wait_for_admission_without_blocking_reactor() {
    let uri = crate::test::harness::fixture_uri("/tmp/governed_code_lenses.rb");
    let mut server = Server::default();
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
    server.extensions.registry().configure_from_config(
        &crate::environment::config::RubyFastLspConfig {
            extension_packages: vec![std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("extensions/rspec-ruby")
                .to_string_lossy()
                .into_owned()],
            ..crate::environment::config::RubyFastLspConfig::default()
        },
    );
    crate::lsp::lifecycle::indexing::handle_did_open(
        &server,
        DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: "ruby".to_string(),
                version: 1,
                text: "class GovernedCodeLens\nend\n".to_string(),
            },
        },
    )
    .await;

    let holder_release = Arc::new(tokio::sync::Notify::new());
    let holder_release_task = holder_release.clone();
    let holder_governor = server.indexing.resources().clone();
    let holder = tokio::spawn(async move {
        holder_governor
            .run_async_with_resources(
                "code lens contention holder",
                crate::utils::admission::IndexingWorkSpec::new(
                    None,
                    crate::utils::admission::IndexingResourcePriority::Background,
                    1,
                    256 * 1024 * 1024,
                    1,
                ),
                None,
                async move {
                    holder_release_task.notified().await;
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
    .expect("resource holder must be admitted before code-lens request");

    let request_server = server.clone();
    let request_uri = uri.clone();
    let request = tokio::spawn(async move {
        code_lens::handle(
            &request_server,
            CodeLensParams {
                text_document: TextDocumentIdentifier { uri: request_uri },
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            },
        )
        .await
    });
    tokio::time::timeout(Duration::from_secs(1), async {
        while server.indexing.resources().snapshot().queued_tasks != 1 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("code-lens request must queue behind the complete weighted claim");
    tokio::time::timeout(
        Duration::from_millis(50),
        tokio::time::sleep(Duration::from_millis(1)),
    )
    .await
    .expect("queued code-lens request must not block the current-thread Tokio reactor");
    assert!(
        !request.is_finished(),
        "code-lens request must not bypass weighted admission"
    );

    holder_release.notify_one();
    holder.await.unwrap();
    request
        .await
        .unwrap()
        .unwrap()
        .expect("open document must return a code-lens response");
    let complete = server.indexing.resources().snapshot();
    assert_eq!(complete.active_tasks, 0);
    assert_eq!(complete.queued_tasks, 0);
    assert_eq!(complete.completed_tasks, 3);
}

#[tokio::test(flavor = "current_thread")]
async fn request_time_extension_symbols_wait_for_admission_without_blocking_reactor() {
    let uri = crate::test::harness::fixture_uri("/tmp/governed_document_symbols.rb");
    let mut server = Server::default();
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
    server.extensions.registry().configure_from_config(
        &crate::environment::config::RubyFastLspConfig {
            extension_packages: vec![std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("extensions/rspec-ruby")
                .to_string_lossy()
                .into_owned()],
            ..crate::environment::config::RubyFastLspConfig::default()
        },
    );
    crate::lsp::lifecycle::indexing::handle_did_open(
        &server,
        DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: "ruby".to_string(),
                version: 1,
                text: "class GovernedDocumentSymbol\nend\n".to_string(),
            },
        },
    )
    .await;

    let holder_release = Arc::new(tokio::sync::Notify::new());
    let holder_release_task = holder_release.clone();
    let holder_governor = server.indexing.resources().clone();
    let holder = tokio::spawn(async move {
        holder_governor
            .run_async_with_resources(
                "document symbol contention holder",
                crate::utils::admission::IndexingWorkSpec::new(
                    None,
                    crate::utils::admission::IndexingResourcePriority::Background,
                    1,
                    256 * 1024 * 1024,
                    1,
                ),
                None,
                async move {
                    holder_release_task.notified().await;
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
    .expect("resource holder must be admitted before document-symbol request");

    let request_server = server.clone();
    let request_uri = uri.clone();
    let request = tokio::spawn(async move {
        document_symbols::handle(
            &request_server,
            DocumentSymbolParams {
                text_document: TextDocumentIdentifier { uri: request_uri },
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            },
        )
        .await
    });
    tokio::time::timeout(Duration::from_secs(1), async {
        while server.indexing.resources().snapshot().queued_tasks != 1 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("document-symbol request must queue behind the complete weighted claim");
    tokio::time::timeout(
        Duration::from_millis(50),
        tokio::time::sleep(Duration::from_millis(1)),
    )
    .await
    .expect("queued document-symbol request must not block the current-thread Tokio reactor");
    assert!(
        !request.is_finished(),
        "document-symbol request must not bypass weighted admission"
    );

    holder_release.notify_one();
    holder.await.unwrap();
    let response = request
        .await
        .unwrap()
        .unwrap()
        .expect("open document must return symbols");
    let DocumentSymbolResponse::Nested(symbols) = response else {
        unreachable_invariant!(
            what = "document-symbol handler returned flat symbols",
            why = "the handler always constructs a nested hierarchy",
            fix = "preserve nested document-symbol responses",
        );
    };
    assert!(symbols
        .iter()
        .any(|symbol| symbol.name == "GovernedDocumentSymbol"));
    let complete = server.indexing.resources().snapshot();
    assert_eq!(complete.active_tasks, 0);
    assert_eq!(complete.queued_tasks, 0);
    assert_eq!(complete.completed_tasks, 3);
}
