use crate::server::diagnostics::DiagnosticPublicationState;

use crate::server::RubyLanguageServer;
use parking_lot::RwLock;
use ruby_analysis::indexer::RubyDocument;
use std::sync::Arc;
use std::time::Duration;
use tower_lsp::lsp_types::{Diagnostic, FileChangeType, FileEvent, Url};

#[test]
fn diagnostic_publication_queue_keeps_only_the_latest_per_uri() {
    let mut publication = DiagnosticPublicationState::default();
    let first = crate::test::harness::fixture_uri("/project/a.rb");
    let second = crate::test::harness::fixture_uri("/project/b.rb");
    assert!(publication.queue(first.clone(), Vec::new()));
    assert!(!publication.queue(first.clone(), vec![Diagnostic::default()]));
    assert!(!publication.queue(second.clone(), Vec::new()));
    let (uri, diagnostics) = publication
        .take_next()
        .expect("first URI must stay pending");
    assert_eq!(uri, first);
    assert_eq!(
        diagnostics.len(),
        1,
        "latest diagnostics for a URI must win"
    );
    let (uri, diagnostics) = publication
        .take_next()
        .expect("second URI must stay pending");
    assert_eq!(uri, second);
    assert!(diagnostics.is_empty());
    assert!(publication.take_next().is_none());
}

#[tokio::test]
async fn semantic_convergence_refreshes_only_projects_with_open_documents() {
    use futures::{SinkExt, StreamExt};
    use serde_json::{json, Value};
    use tower::{Service, ServiceExt};
    use tower_lsp::jsonrpc::{Request, Response};
    use tower_lsp::LspService;

    let (mut service, mut socket) = LspService::new(|client| {
        RubyLanguageServer::new(client).expect("test language server must initialize")
    });
    let initialize = Request::build("initialize")
        .params(json!({"capabilities": {}}))
        .id(1)
        .finish();
    service
        .ready()
        .await
        .unwrap()
        .call(initialize)
        .await
        .unwrap()
        .expect("initialize must return a response");

    let fixture = tempfile::tempdir().unwrap();
    let open_project = fixture.path().join("open_project");
    let closed_project = fixture.path().join("closed_project");
    std::fs::create_dir_all(&open_project).unwrap();
    std::fs::create_dir_all(&closed_project).unwrap();
    let language_server = service.inner().clone();
    language_server.add_workspace(Url::from_directory_path(&open_project).unwrap());
    language_server.add_workspace(Url::from_directory_path(&closed_project).unwrap());
    let open_uri = Url::from_file_path(open_project.join("consumer.rb")).unwrap();
    language_server.documents.insert(
        open_uri.clone(),
        Arc::new(RwLock::new(RubyDocument::new(
            open_uri,
            "value = Shared::LABEL\n".to_string(),
            1,
        ))),
    );

    language_server
        .refresh_inlay_hints_for_workspace(&closed_project)
        .await;
    assert!(
        tokio::time::timeout(Duration::from_millis(25), socket.next())
            .await
            .is_err(),
        "a project with no open document must not issue a global inlay refresh"
    );

    let refresh_server = language_server.clone();
    let open_project_for_refresh = open_project.clone();
    let refresh = tokio::spawn(async move {
        refresh_server
            .refresh_inlay_hints_for_workspace(&open_project_for_refresh)
            .await;
    });
    let request = tokio::time::timeout(Duration::from_secs(1), socket.next())
        .await
        .expect("an open project's semantic convergence must issue a refresh")
        .expect("client socket must remain open");
    assert_eq!(request.method(), "workspace/inlayHint/refresh");
    let id = request
        .id()
        .cloned()
        .expect("inlay refresh must be a request with an id");
    socket
        .send(Response::from_ok(id, Value::Null))
        .await
        .unwrap();
    refresh.await.unwrap();
}

#[test]
fn watched_file_batches_keep_only_the_latest_event_per_uri_and_generation() {
    let fixture = tempfile::tempdir().unwrap();
    let admin = Url::from_file_path(fixture.path().join("admin.rb")).unwrap();
    let server_file = Url::from_file_path(fixture.path().join("server.rb")).unwrap();
    let language_server = RubyLanguageServer::default();

    let first = language_server.queue_watched_file_changes(vec![
        FileEvent {
            uri: server_file.clone(),
            typ: FileChangeType::CREATED,
        },
        FileEvent {
            uri: admin.clone(),
            typ: FileChangeType::CHANGED,
        },
    ]);
    let replacement = language_server.queue_watched_file_changes(vec![
        FileEvent {
            uri: server_file.clone(),
            typ: FileChangeType::DELETED,
        },
        FileEvent {
            uri: admin.clone(),
            typ: FileChangeType::CHANGED,
        },
    ]);

    assert!(
        language_server.take_watched_file_changes(first).is_none(),
        "an older debounce generation must never process a partial filesystem state"
    );
    let changes = language_server
        .take_watched_file_changes(replacement)
        .expect("the newest debounce generation must own the complete normalized batch");
    assert_eq!(
        changes,
        vec![
            FileEvent {
                uri: admin,
                typ: FileChangeType::CHANGED,
            },
            FileEvent {
                uri: server_file,
                typ: FileChangeType::DELETED,
            },
        ]
    );
    assert!(
        language_server
            .take_watched_file_changes(replacement)
            .is_none(),
        "a normalized watcher batch must be consumed exactly once"
    );
}
