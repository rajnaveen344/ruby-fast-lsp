//! Watched-file changes replace and remove closed-file facts.

use super::*;

#[tokio::test]
async fn watched_closed_project_files_replace_and_remove_engine_facts() {
    let workspace = tempfile::TempDir::new().unwrap();
    let path = workspace.path().join("watched.rb");
    let uri = Url::from_file_path(&path).unwrap();
    let server = RubyLanguageServer::default();
    server.add_workspace(Url::from_directory_path(workspace.path()).unwrap());

    std::fs::write(&path, "class WatchedOne\nend\n").unwrap();
    handle_watched_files_changed(
        &server,
        DidChangeWatchedFilesParams {
            changes: vec![FileEvent {
                uri: uri.clone(),
                typ: FileChangeType::CREATED,
            }],
        },
    )
    .await;
    assert!(has_namespace(&server, &uri, "WatchedOne"));

    std::fs::write(&path, "class WatchedTwo\nend\n").unwrap();
    handle_watched_files_changed(
        &server,
        DidChangeWatchedFilesParams {
            changes: vec![FileEvent {
                uri: uri.clone(),
                typ: FileChangeType::CHANGED,
            }],
        },
    )
    .await;
    assert!(!has_namespace(&server, &uri, "WatchedOne"));
    assert!(has_namespace(&server, &uri, "WatchedTwo"));

    std::fs::remove_file(&path).unwrap();
    handle_watched_files_changed(
        &server,
        DidChangeWatchedFilesParams {
            changes: vec![FileEvent {
                uri: uri.clone(),
                typ: FileChangeType::DELETED,
            }],
        },
    )
    .await;
    assert!(!has_namespace(&server, &uri, "WatchedTwo"));

    let vendor_path = workspace.path().join("vendor/owned.rb");
    std::fs::create_dir_all(vendor_path.parent().unwrap()).unwrap();
    std::fs::write(&vendor_path, "class VendorOwned\nend\n").unwrap();
    let vendor_uri = Url::from_file_path(&vendor_path).unwrap();
    handle_watched_files_changed(
        &server,
        DidChangeWatchedFilesParams {
            changes: vec![FileEvent {
                uri: vendor_uri.clone(),
                typ: FileChangeType::CREATED,
            }],
        },
    )
    .await;
    assert!(!has_namespace(&server, &vendor_uri, "VendorOwned"));

    server.config.lock().indexing.included_patterns = vec!["vendor/owned.rb".to_string()];
    handle_watched_files_changed(
        &server,
        DidChangeWatchedFilesParams {
            changes: vec![FileEvent {
                uri: vendor_uri.clone(),
                typ: FileChangeType::CHANGED,
            }],
        },
    )
    .await;
    assert!(has_namespace(&server, &vendor_uri, "VendorOwned"));
}

#[tokio::test]
async fn watched_project_rbs_files_replace_and_remove_signature_facts() {
    let workspace = tempfile::TempDir::new().unwrap();
    let path = workspace.path().join("sig/native_widget.rbs");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let uri = Url::from_file_path(&path).unwrap();
    let server = RubyLanguageServer::default();
    server.add_workspace(Url::from_directory_path(workspace.path()).unwrap());

    std::fs::write(
        &path,
        "class NativeWidget\n  def encode: () -> String\nend\n",
    )
    .unwrap();
    handle_watched_files_changed(
        &server,
        DidChangeWatchedFilesParams {
            changes: vec![FileEvent {
                uri: uri.clone(),
                typ: FileChangeType::CREATED,
            }],
        },
    )
    .await;
    assert!(has_namespace(&server, &uri, "NativeWidget"));

    std::fs::write(
        &path,
        "class GeneratedWidget\n  def encode: () -> Integer\nend\n",
    )
    .unwrap();
    handle_watched_files_changed(
        &server,
        DidChangeWatchedFilesParams {
            changes: vec![FileEvent {
                uri: uri.clone(),
                typ: FileChangeType::CHANGED,
            }],
        },
    )
    .await;
    assert!(!has_namespace(&server, &uri, "NativeWidget"));
    assert!(has_namespace(&server, &uri, "GeneratedWidget"));

    std::fs::write(&path, "class GeneratedWidget\n  def broken: (\n").unwrap();
    handle_watched_files_changed(
        &server,
        DidChangeWatchedFilesParams {
            changes: vec![FileEvent {
                uri: uri.clone(),
                typ: FileChangeType::CHANGED,
            }],
        },
    )
    .await;
    assert!(
        !has_namespace(&server, &uri, "GeneratedWidget"),
        "malformed regenerated RBS must clear stale signature facts"
    );

    std::fs::remove_file(&path).unwrap();
    handle_watched_files_changed(
        &server,
        DidChangeWatchedFilesParams {
            changes: vec![FileEvent {
                uri: uri.clone(),
                typ: FileChangeType::DELETED,
            }],
        },
    )
    .await;
    assert!(!has_namespace(&server, &uri, "GeneratedWidget"));
}

#[tokio::test]
async fn watched_rbs_record_refreshes_an_early_open_consumer() {
    let workspace = tempfile::TempDir::new().unwrap();
    let consumer_path = workspace.path().join("consumer.rb");
    let consumer_uri = Url::from_file_path(&consumer_path).unwrap();
    let signature_path = workspace.path().join("sig/payload_factory.rbs");
    std::fs::create_dir_all(signature_path.parent().unwrap()).unwrap();
    let signature_uri = Url::from_file_path(&signature_path).unwrap();
    let server = RubyLanguageServer::default();
    server.add_workspace(Url::from_directory_path(workspace.path()).unwrap());

    handle_did_open(
        &server,
        DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: consumer_uri.clone(),
                language_id: "ruby".to_string(),
                version: 1,
                text: "payload = PayloadFactory.build\npayload[:name]\n".to_string(),
            },
        },
    )
    .await;
    std::fs::write(
        &signature_path,
        "class PayloadFactory\n  def self.build: () -> { id: Integer, ?name: String }\nend\n",
    )
    .unwrap();
    handle_watched_files_changed(
        &server,
        DidChangeWatchedFilesParams {
            changes: vec![FileEvent {
                uri: signature_uri.clone(),
                typ: FileChangeType::CREATED,
            }],
        },
    )
    .await;

    let hover = server
        .hover(HoverParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: consumer_uri.clone(),
                },
                position: Position::new(1, 14),
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
        })
        .await
        .expect("hover request must succeed")
        .expect("the refreshed consumer must publish a keyed-read hover");
    assert!(
        format!("{:?}", hover.contents).contains("String"),
        "the RBS record return must refresh the early-open consumer, got {:?}",
        hover.contents
    );

    std::fs::write(
        &signature_path,
        "class PayloadFactory\n  def self.build: () -> { id: Integer, ?name: Integer }\nend\n",
    )
    .unwrap();
    handle_watched_files_changed(
        &server,
        DidChangeWatchedFilesParams {
            changes: vec![FileEvent {
                uri: signature_uri.clone(),
                typ: FileChangeType::CHANGED,
            }],
        },
    )
    .await;
    let changed_hover = server
        .hover(HoverParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: consumer_uri.clone(),
                },
                position: Position::new(1, 14),
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
        })
        .await
        .expect("changed hover request must succeed")
        .expect("the refreshed consumer must retain a keyed-read hover");
    assert!(
        format!("{:?}", changed_hover.contents).contains("Integer"),
        "the replacement RBS record must replace String with Integer, got {:?}",
        changed_hover.contents
    );

    std::fs::remove_file(&signature_path).unwrap();
    handle_watched_files_changed(
        &server,
        DidChangeWatchedFilesParams {
            changes: vec![FileEvent {
                uri: signature_uri,
                typ: FileChangeType::DELETED,
            }],
        },
    )
    .await;
    let deleted_hover = server
        .hover(HoverParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: consumer_uri },
                position: Position::new(1, 14),
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
        })
        .await
        .expect("post-delete hover request must succeed")
        .expect("the keyed read must retain an explained Unknown hover");
    let deleted = format!("{:?}", deleted_hover.contents);
    assert!(
        deleted.contains("Unknown") && !deleted.contains("String") && !deleted.contains("Integer"),
        "deleting the RBS contract must remove every stale concrete shape, got {:?}",
        deleted_hover.contents
    );
}

#[tokio::test]
async fn watched_callable_signature_replaces_and_deletes_dependent_results() {
    async fn hover_label(server: &RubyLanguageServer, uri: &Url) -> String {
        let hover = server
            .hover(HoverParams {
                text_document_position_params: TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier { uri: uri.clone() },
                    position: Position::new(1, 2),
                },
                work_done_progress_params: WorkDoneProgressParams::default(),
            })
            .await
            .expect("callable lifecycle hover request must succeed")
            .expect("the callable result local must retain hover evidence");
        format!("{:?}", hover.contents)
    }

    let workspace = tempfile::TempDir::new().unwrap();
    let consumer_path = workspace.path().join("consumer.rb");
    let consumer_uri = Url::from_file_path(&consumer_path).unwrap();
    let signature_path = workspace.path().join("sig/converter.rbs");
    std::fs::create_dir_all(signature_path.parent().unwrap()).unwrap();
    let signature_uri = Url::from_file_path(&signature_path).unwrap();
    let server = RubyLanguageServer::default();
    server.add_workspace(Url::from_directory_path(workspace.path()).unwrap());
    let source = "result = Converter.new.apply(1) { |value| value.to_s }\nresult\n".to_string();

    handle_did_open(
        &server,
        DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: consumer_uri.clone(),
                language_id: "ruby".to_string(),
                version: 1,
                text: source,
            },
        },
    )
    .await;

    std::fs::write(
            &signature_path,
            "class Converter\n  def apply: [Input, Output] (Input value) { (Input) -> Output } -> Output\nend\n",
        )
        .unwrap();
    handle_watched_files_changed(
        &server,
        DidChangeWatchedFilesParams {
            changes: vec![FileEvent {
                uri: signature_uri.clone(),
                typ: FileChangeType::CREATED,
            }],
        },
    )
    .await;
    let created = hover_label(&server, &consumer_uri).await;
    assert!(
        created.contains("String"),
        "created callable signature did not refresh the consumer: {created}"
    );

    std::fs::write(
            &signature_path,
            "class Converter\n  def apply: [Input, Output] (Input value) { (Input) -> Output } -> Array[Output]\nend\n",
        )
        .unwrap();
    handle_watched_files_changed(
        &server,
        DidChangeWatchedFilesParams {
            changes: vec![FileEvent {
                uri: signature_uri.clone(),
                typ: FileChangeType::CHANGED,
            }],
        },
    )
    .await;
    let changed = hover_label(&server, &consumer_uri).await;
    assert!(
        changed.contains("Array&lt;String&gt;") || changed.contains("Array<String>"),
        "replacement callable signature did not replace the result: {changed}"
    );

    std::fs::remove_file(&signature_path).unwrap();
    handle_watched_files_changed(
        &server,
        DidChangeWatchedFilesParams {
            changes: vec![FileEvent {
                uri: signature_uri,
                typ: FileChangeType::DELETED,
            }],
        },
    )
    .await;
    let deleted = hover_label(&server, &consumer_uri).await;
    assert!(
        deleted.contains("Unknown") && !deleted.contains("String") && !deleted.contains("Array"),
        "deleted callable signature left a stale concrete result: {deleted}"
    );
}

#[tokio::test]
async fn opening_default_external_workspace_file_does_not_make_it_project_owned() {
    let workspace = tempfile::TempDir::new().unwrap();
    let path = workspace.path().join("vendor/opened.rb");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let uri = Url::from_file_path(&path).unwrap();
    let server = RubyLanguageServer::default();
    server.add_workspace(Url::from_directory_path(workspace.path()).unwrap());

    handle_did_open(
        &server,
        DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: "ruby".to_string(),
                version: 1,
                text: "class OpenedVendor\nend\n".to_string(),
            },
        },
    )
    .await;

    let analysis_engine = server.analysis_engine_for_uri(&uri);
    let engine = analysis_engine.read();
    let file_id = engine
        .view()
        .file_id(&path)
        .expect("opened workspace file must be registered");
    assert!(
        !engine
            .view()
            .file(file_id)
            .expect("registered file must exist")
            .kind
            .is_workspace_owned(),
        "default-external workspace files must not become project-owned when opened"
    );
    assert!(
        !AnalysisQuery::new(&engine)
            .symbols_for_fqn(&namespace("OpenedVendor"))
            .is_empty(),
        "opened excluded files must still receive interactive semantic analysis"
    );
    assert!(
        AnalysisQuery::new(&engine)
            .search_workspace_symbols("OpenedVendor", 100)
            .is_empty(),
        "default-external workspace files must stay out of workspace symbols"
    );
    drop(engine);

    handle_did_change(
        &server,
        DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri: uri.clone(),
                version: 2,
            },
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: "class ChangedVendor\nend\n".to_string(),
            }],
        },
    )
    .await;

    let analysis_engine = server.analysis_engine_for_uri(&uri);
    let engine = analysis_engine.read();
    assert!(
        !engine
            .view()
            .file(file_id)
            .expect("changed file must retain its registration")
            .kind
            .is_workspace_owned(),
        "didChange must preserve excluded workspace ownership"
    );
    assert!(
        AnalysisQuery::new(&engine)
            .search_workspace_symbols("ChangedVendor", 100)
            .is_empty(),
        "changed excluded workspace files must stay out of workspace symbols"
    );
    drop(engine);

    handle_did_close(
        &server,
        DidCloseTextDocumentParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
        },
    )
    .await;

    assert!(
        !has_namespace(&server, &uri, "ChangedVendor"),
        "closing an excluded workspace file must remove its interactive-only facts"
    );
}
