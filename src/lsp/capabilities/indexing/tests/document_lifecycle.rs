//! Open and change notifications keep engine facts and diagnostics current.

use super::*;

#[tokio::test]
async fn did_open_registers_source_in_analysis_engine() {
    let server = RubyLanguageServer::default();
    let uri = crate::test::harness::fixture_uri("/tmp/user.rb");

    handle_did_open(
        &server,
        DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: "ruby".to_string(),
                version: 1,
                text: "A = 1".to_string(),
            },
        },
    )
    .await;

    let path = uri.to_file_path().expect("file URI must convert to path");
    let engine = server.orphan_engine().read();
    let file_id = engine
        .file_id(path)
        .expect("did_open must register file in analysis engine");
    let file = engine.file(file_id).unwrap();
    assert_eq!(file.line_index.len(), "A = 1".len());
    assert!(file.source_text().is_none());
}

#[tokio::test(flavor = "current_thread")]
async fn did_open_semantic_pass_waits_for_weighted_admission_without_blocking_reactor() {
    let workspace = tempfile::TempDir::new().unwrap();
    let workspace_root = workspace.path().to_path_buf();
    std::fs::write(
        workspace.path().join("Gemfile"),
        "source 'https://rubygems.org'\n",
    )
    .unwrap();
    let path = workspace.path().join("opened.rb");
    let uri = Url::from_file_path(&path).unwrap();
    let mut server = RubyLanguageServer::default();
    server.indexing.set_resources(
        crate::loader::scheduling::resources::IndexingResourceGovernor::new(
            crate::loader::scheduling::resources::IndexingResourcePolicy::with_limits(
                1,
                1,
                256 * 1024 * 1024,
                1,
            ),
        ),
    );
    server.add_workspace(Url::from_directory_path(workspace.path()).unwrap());

    let release = Arc::new(tokio::sync::Notify::new());
    let holder_release = release.clone();
    let holder_resources = server.indexing.resources().clone();
    let holder_root = workspace_root.clone();
    let holder = tokio::spawn(async move {
        holder_resources
            .run_async_with_resources(
                "interactive semantic contention holder",
                crate::loader::scheduling::resources::IndexingWorkSpec::new(
                    Some(holder_root),
                    crate::loader::scheduling::resources::IndexingResourcePriority::Background,
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
    .expect("resource holder must be admitted before didOpen");

    let open_server = server.clone();
    let open_uri = uri.clone();
    let open = tokio::spawn(async move {
        handle_did_open(
            &open_server,
            DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri: open_uri,
                    language_id: "ruby".to_string(),
                    version: 1,
                    text: "class OpenedUnderPressure\nend\n".to_string(),
                },
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
    .expect("didOpen semantic work must queue behind the weighted holder");
    tokio::time::timeout(
        Duration::from_millis(50),
        tokio::time::sleep(Duration::from_millis(1)),
    )
    .await
    .expect("queued didOpen must not block the current-thread Tokio reactor");
    assert!(
        !open.is_finished(),
        "didOpen must not bypass weighted admission while resources are saturated"
    );

    release.notify_one();
    holder.await.unwrap();
    open.await.unwrap();
    assert!(has_namespace(&server, &uri, "OpenedUnderPressure"));
    let complete = server.indexing.resources().snapshot();
    assert_eq!(complete.active_tasks, 0);
    assert_eq!(complete.queued_tasks, 0);
    assert_eq!(complete.completed_tasks, 2);
}

#[tokio::test(flavor = "current_thread")]
async fn overlapping_did_change_versions_cannot_publish_older_semantic_facts() {
    let workspace = tempfile::TempDir::new().unwrap();
    std::fs::write(
        workspace.path().join("Gemfile"),
        "source 'https://rubygems.org'\n",
    )
    .unwrap();
    let path = workspace.path().join("changing.rb");
    let uri = Url::from_file_path(&path).unwrap();
    let mut server = RubyLanguageServer::default();
    server.indexing.set_resources(
        crate::loader::scheduling::resources::IndexingResourceGovernor::new(
            crate::loader::scheduling::resources::IndexingResourcePolicy::with_limits(
                1,
                1,
                256 * 1024 * 1024,
                1,
            ),
        ),
    );
    server.add_workspace(Url::from_directory_path(workspace.path()).unwrap());
    handle_did_open(
        &server,
        DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: "ruby".to_string(),
                version: 1,
                text: "class InitialVersion\nend\n".to_string(),
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
                "didChange ordering contention holder",
                crate::loader::scheduling::resources::IndexingWorkSpec::new(
                    Some(holder_root),
                    crate::loader::scheduling::resources::IndexingResourcePriority::Background,
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

    let version_two_server = server.clone();
    let version_two_uri = uri.clone();
    let version_two = tokio::spawn(async move {
        handle_did_change(
            &version_two_server,
            DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier {
                    uri: version_two_uri,
                    version: 2,
                },
                content_changes: vec![TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text: "class VersionTwo\nend\n".to_string(),
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
    .expect("version two must queue behind the holder");

    let version_three_server = server.clone();
    let version_three_uri = uri.clone();
    let version_three = tokio::spawn(async move {
        handle_did_change(
            &version_three_server,
            DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier {
                    uri: version_three_uri,
                    version: 3,
                },
                content_changes: vec![TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text: "class VersionThree\nend\n".to_string(),
                }],
            },
        )
        .await;
    });
    tokio::task::yield_now().await;

    release.notify_one();
    holder.await.unwrap();
    version_two.await.unwrap();
    version_three.await.unwrap();
    assert!(
        has_namespace(&server, &uri, "VersionThree"),
        "the newest document version must own the final semantic facts"
    );
    assert!(
        !has_namespace(&server, &uri, "VersionTwo"),
        "an older queued pass must not mark a newer buffer as already indexed"
    );
    assert_eq!(
        server.get_doc(&uri).unwrap().version,
        3,
        "the document cache and semantic facts must agree on the final version"
    );
}

#[tokio::test]
async fn did_open_preserves_known_external_file_without_reprocessing() {
    let server = RubyLanguageServer::default();
    let uri = crate::test::harness::fixture_uri("/tmp/rubystubs33/kernel.rb");
    let file_id = server.open_or_update_analysis_file_with_kind(
        &uri,
        "module Kernel\n  def puts\n  end\nend".to_string(),
        SourceKind::Stub,
    );
    let kernel = RubyConstant::new("Kernel").expect("test constant must be valid");
    let puts = RubyMethod::new("puts").expect("test method must be valid");
    let puts_fqn = FullyQualifiedName::method(vec![kernel], puts);
    server.orphan_engine().write().update(
        file_id,
        FileAnalysis {
            methods: vec![MethodFact::new(
                puts_fqn.clone(),
                FullyQualifiedName::namespace(vec![kernel]),
                TextRange::new(file_id, 16, 20),
            )],
            ..Default::default()
        },
        ResolveMode::Deferred,
    );

    handle_did_open(
        &server,
        DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: "ruby".to_string(),
                version: 1,
                text: "module Kernel\n  def generated_after_open\n  end\nend".to_string(),
            },
        },
    )
    .await;

    let path = uri.to_file_path().expect("file URI must convert to path");
    let engine = server.orphan_engine().read();
    let file_id = engine
        .file_id(path)
        .expect("known external file must remain registered");
    let file = engine.file(file_id).expect("registered file must exist");
    assert_eq!(file.kind, SourceKind::Stub);
    let query = AnalysisQuery::new(&engine);
    assert_eq!(query.methods_for_fqn(&puts_fqn).len(), 1);
    let generated_fqn = FullyQualifiedName::method(
        vec![kernel],
        RubyMethod::new("generated_after_open").expect("test method must be valid"),
    );
    assert!(
        query.methods_for_fqn(&generated_fqn).is_empty(),
        "known external didOpen must not reprocess and replace indexed stub facts"
    );
}

#[tokio::test]
async fn did_change_updates_analysis_engine_source() {
    let server = RubyLanguageServer::default();
    let uri = crate::test::harness::fixture_uri("/tmp/user.rb");

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
                text: "A = 2".to_string(),
            }],
        },
    )
    .await;

    let path = uri.to_file_path().expect("file URI must convert to path");
    let engine = server.orphan_engine().read();
    let file_id = engine
        .file_id(path)
        .expect("did_change must register file in analysis engine");
    let file = engine.file(file_id).unwrap();
    assert_eq!(file.line_index.len(), "A = 2".len());
    assert!(file.source_text().is_none());
}

#[tokio::test]
async fn did_change_replaces_analysis_engine_symbol_facts() {
    let server = RubyLanguageServer::default();
    let uri = crate::test::harness::fixture_uri("/tmp/user.rb");

    handle_did_open(
        &server,
        DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: "ruby".to_string(),
                version: 1,
                text: "class User\nend".to_string(),
            },
        },
    )
    .await;
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
                text: "class Account\nend".to_string(),
            }],
        },
    )
    .await;

    let user_fqn = FullyQualifiedName::namespace(vec![RubyConstant::new("User").unwrap()]);
    let account_fqn = FullyQualifiedName::namespace(vec![RubyConstant::new("Account").unwrap()]);
    let engine = server.orphan_engine().read();
    assert!(
        engine.symbol_facts_for(&user_fqn).is_empty(),
        "stale User symbol facts must be removed after reindex"
    );
    let account_facts = engine.symbol_facts_for(&account_fqn);
    assert_eq!(account_facts.len(), 1);
    assert_eq!(account_facts[0].kind, SymbolKind::Class);
}

#[tokio::test]
async fn exported_api_change_refreshes_open_consumer_diagnostics() {
    let server = RubyLanguageServer::default();
    let definition_uri = crate::test::harness::fixture_uri("/tmp/user.rb");
    let consumer_uri = crate::test::harness::fixture_uri("/tmp/use_user.rb");

    handle_did_open(
        &server,
        DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: definition_uri.clone(),
                language_id: "ruby".to_string(),
                version: 1,
                text: "class User\n  def name\n    'A'\n  end\nend\n".to_string(),
            },
        },
    )
    .await;
    handle_did_open(
        &server,
        DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: consumer_uri.clone(),
                language_id: "ruby".to_string(),
                version: 1,
                text: "class User\n  def show\n    name\n  end\nend\n".to_string(),
            },
        },
    )
    .await;
    assert!(
        server
            .last_published_diagnostics(&consumer_uri)
            .iter()
            .all(|diagnostic| diagnostic.code
                != Some(NumberOrString::String("unresolved-method".to_string()))),
        "existing exported method must keep the open consumer diagnostic-free"
    );

    handle_did_change(
        &server,
        DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri: definition_uri,
                version: 2,
            },
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: "class User\nend\n".to_string(),
            }],
        },
    )
    .await;

    assert!(
        server
            .last_published_diagnostics(&consumer_uri)
            .iter()
            .any(|diagnostic| diagnostic.code
                == Some(NumberOrString::String("unresolved-method".to_string()))),
        "removing an exported method must refresh diagnostics for its open consumer"
    );
}

#[tokio::test]
async fn body_only_change_does_not_refresh_other_open_files() {
    let server = RubyLanguageServer::default();
    let definition_uri = crate::test::harness::fixture_uri("/tmp/user.rb");
    let consumer_uri = crate::test::harness::fixture_uri("/tmp/use_user.rb");
    for (uri, text) in [
        (
            definition_uri.clone(),
            "class User\n  def name\n    'A'\n  end\nend\n",
        ),
        (consumer_uri.clone(), "class User\n  name\nend\n"),
    ] {
        handle_did_open(
            &server,
            DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri,
                    language_id: "ruby".to_string(),
                    version: 1,
                    text: text.to_string(),
                },
            },
        )
        .await;
    }
    server
        .publish_diagnostics(
            consumer_uri.clone(),
            vec![Diagnostic::new_simple(
                Range::default(),
                "sentinel".to_string(),
            )],
        )
        .await;

    handle_did_change(
        &server,
        DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri: definition_uri,
                version: 2,
            },
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: "class User\n  def name\n    'B'\n  end\nend\n".to_string(),
            }],
        },
    )
    .await;

    assert_eq!(
        server.last_published_diagnostics(&consumer_uri),
        vec![Diagnostic::new_simple(
            Range::default(),
            "sentinel".to_string(),
        )],
        "body-only typing must not reprocess unrelated open documents"
    );
}

#[tokio::test]
async fn open_diagnostic_refresh_targets_are_sorted_and_capped() {
    let server = RubyLanguageServer::default();
    let changed_uri = crate::test::harness::fixture_uri("/tmp/changed.rb");
    for index in (0..12).rev() {
        let uri = Url::parse(&format!("file:///tmp/consumer_{index:02}.rb")).unwrap();
        handle_did_open(
            &server,
            DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri,
                    language_id: "ruby".to_string(),
                    version: 1,
                    text: format!("VALUE_{index} = {index}\n"),
                },
            },
        )
        .await;
    }

    let targets = bounded_open_diagnostic_refresh_targets(&server, &changed_uri);
    assert_eq!(targets.len(), MAX_OPEN_DIAGNOSTIC_REFRESH_FILES);
    assert_eq!(targets[0].0.path(), "/tmp/consumer_00.rb");
    assert_eq!(targets[7].0.path(), "/tmp/consumer_07.rb");
}

#[tokio::test]
async fn did_open_mirrors_reference_facts_into_analysis_engine() {
    let server = RubyLanguageServer::default();
    let uri = crate::test::harness::fixture_uri("/tmp/user.rb");

    handle_did_open(
        &server,
        DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: "ruby".to_string(),
                version: 1,
                text: "class User\nend\nUser.new".to_string(),
            },
        },
    )
    .await;

    let user_fqn = FullyQualifiedName::namespace(vec![RubyConstant::new("User").unwrap()]);
    let engine = server.orphan_engine().read();
    let query = AnalysisQuery::new(&engine);
    assert_eq!(query.references_for_fqn(&user_fqn).len(), 2);
}

#[tokio::test]
async fn did_open_mirrors_graph_facts_into_analysis_engine() {
    let server = RubyLanguageServer::default();
    let uri = crate::test::harness::fixture_uri("/tmp/user.rb");

    handle_did_open(
        &server,
        DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: "ruby".to_string(),
                version: 1,
                text: "module Auth\nend\nclass User\n  include Auth\nend".to_string(),
            },
        },
    )
    .await;

    let user_fqn = FullyQualifiedName::namespace(vec![RubyConstant::new("User").unwrap()]);
    let auth_fqn = FullyQualifiedName::namespace(vec![RubyConstant::new("Auth").unwrap()]);
    let engine = server.orphan_engine().read();
    let query = AnalysisQuery::new(&engine);
    let edges = query.graph_edges_from(&user_fqn);
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].target, auth_fqn);
    assert_eq!(edges[0].kind, GraphEdgeKind::Include);
}

#[tokio::test]
async fn did_open_refreshes_late_resolved_graph_facts_into_analysis_engine() {
    let server = RubyLanguageServer::default();
    let user_uri = crate::test::harness::fixture_uri("/tmp/user.rb");
    let auth_uri = crate::test::harness::fixture_uri("/tmp/auth.rb");

    handle_did_open(
        &server,
        DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: user_uri.clone(),
                language_id: "ruby".to_string(),
                version: 1,
                text: "class User\n  include Auth\nend".to_string(),
            },
        },
    )
    .await;
    handle_did_open(
        &server,
        DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: auth_uri.clone(),
                language_id: "ruby".to_string(),
                version: 1,
                text: "module Auth\nend".to_string(),
            },
        },
    )
    .await;

    let user_fqn = FullyQualifiedName::namespace(vec![RubyConstant::new("User").unwrap()]);
    let auth_fqn = FullyQualifiedName::namespace(vec![RubyConstant::new("Auth").unwrap()]);
    let engine = server.orphan_engine().read();
    let query = AnalysisQuery::new(&engine);
    let edges = query.graph_edges_from(&user_fqn);
    assert!(
        edges
            .iter()
            .any(|edge| edge.target == auth_fqn && edge.kind == GraphEdgeKind::Include),
        "analysis graph must refresh pending mixin edges once the target module is indexed"
    );
}

#[tokio::test]
async fn did_open_mirrors_normalized_extend_edges_into_analysis_engine() {
    let server = RubyLanguageServer::default();
    let uri = crate::test::harness::fixture_uri("/tmp/user.rb");

    handle_did_open(
        &server,
        DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: "ruby".to_string(),
                version: 1,
                text: "module Auth\nend\nclass User\n  extend Auth\nend".to_string(),
            },
        },
    )
    .await;

    let user_singleton =
        FullyQualifiedName::singleton_namespace(vec![RubyConstant::new("User").unwrap()]);
    let auth_fqn = FullyQualifiedName::namespace(vec![RubyConstant::new("Auth").unwrap()]);
    let engine = server.orphan_engine().read();
    let query = AnalysisQuery::new(&engine);
    let edges = query.graph_edges_from(&user_singleton);
    assert!(
        edges
            .iter()
            .any(|edge| edge.target == auth_fqn && edge.kind == GraphEdgeKind::Include),
        "extend must be mirrored as a singleton include for analysis method lookup"
    );
}

#[tokio::test]
async fn did_open_mirrors_method_facts_into_analysis_engine() {
    let server = RubyLanguageServer::default();
    let uri = crate::test::harness::fixture_uri("/tmp/user.rb");

    handle_did_open(
        &server,
        DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: "ruby".to_string(),
                version: 1,
                text: "class User\n  def name\n  end\n  def self.find\n  end\nend".to_string(),
            },
        },
    )
    .await;

    let user = RubyConstant::new("User").unwrap();
    let name_fqn = FullyQualifiedName::method(
        vec![user],
        RubyMethod::new("name").expect("test method must be valid"),
    );
    let find_fqn = FullyQualifiedName::method(
        vec![user],
        RubyMethod::new("find").expect("test method must be valid"),
    );

    let engine = server.orphan_engine().read();
    let query = AnalysisQuery::new(&engine);
    let name_facts = query.methods_for_fqn(&name_fqn);
    assert_eq!(name_facts.len(), 1);
    assert_eq!(
        name_facts[0].owner.namespace_kind(),
        Some(NamespaceKind::Instance)
    );

    let find_facts = query.methods_for_fqn(&find_fqn);
    assert_eq!(find_facts.len(), 1);
    assert_eq!(
        find_facts[0].owner.namespace_kind(),
        Some(NamespaceKind::Singleton)
    );
}
