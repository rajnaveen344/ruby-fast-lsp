//! Complete indexing workflow, shared core-stub templates, and isolated engines.

use super::*;

#[tokio::test]
async fn test_coordinator_complete_indexing_workflow() {
    let fixture = TestProjectFixture::new();
    fixture.setup_complete_project();

    let config = RubyFastLspConfig::default();
    let mut coordinator = IndexingCoordinator::new(fixture.project_root().clone(), config);
    let server = create_test_server();

    // Execute the complete indexing process
    let result = coordinator
        .run_complete_indexing(&server.load_context_for_project(coordinator.workspace_root()))
        .await;
    assert!(
        result.is_ok(),
        "Indexing should complete successfully: {result:?}"
    );

    let engine = server.orphan_engine().read();
    let query = ruby_analysis::engine::AnalysisQuery::new(&engine);
    for path in [
        fixture.project_root().join("Thorfile"),
        fixture.project_root().join("config.ru"),
    ] {
        let file_id = query.file_id(&path).unwrap_or_else(|| {
            panic!(
                "common Ruby entry point was not registered: {}",
                path.display()
            )
        });
        assert!(
            !query.symbol_facts_in_file(file_id).is_empty(),
            "common Ruby entry point produced no semantic facts: {}",
            path.display()
        );
    }

    assert!(
        coordinator.gem_indexer.is_some() && coordinator.stdlib_indexer.is_some(),
        "complete indexing must retain its exact gem and stdlib indexers"
    );
}

#[tokio::test]
async fn identical_core_stubs_use_one_template_but_keep_isolated_engines() {
    let fixture = TempDir::new().expect("multi-project fixture must be created");
    let admin = fixture.path().join("admin");
    let server_root = fixture.path().join("server");
    for root in [&admin, &server_root] {
        fs::create_dir_all(root).unwrap();
        fs::write(root.join("Gemfile"), "source 'https://rubygems.org'\n").unwrap();
        fs::write(root.join("app.rb"), "class App\nend\n").unwrap();
    }
    let server = create_test_server();
    let admin_workspace = server.add_workspace(Url::from_directory_path(&admin).unwrap());
    let server_workspace = server.add_workspace(Url::from_directory_path(&server_root).unwrap());

    for root in [&admin, &server_root] {
        let mut coordinator =
            IndexingCoordinator::new(root.to_path_buf(), RubyFastLspConfig::default());
        coordinator
            .run_complete_indexing(&server.load_context_for_project(coordinator.workspace_root()))
            .await
            .unwrap();
    }

    assert_eq!(
        server.products.core_templates().len(),
        1,
        "the same compatibility core must have one prepared template"
    );
    assert!(
        !Arc::ptr_eq(
            &admin_workspace.analysis_engine,
            &server_workspace.analysis_engine
        ),
        "projects must retain isolated mutable engines"
    );
    let unique = admin.join("only_admin.rb");
    admin_workspace
        .analysis_engine
        .write()
        .register_file(SourceFileInput {
            path: unique.clone(),
            content: "ADMIN_ONLY = true\n".to_string(),
            kind: ruby_analysis::core::SourceKind::Project,
        });
    assert!(
        server_workspace
            .analysis_engine
            .read()
            .view()
            .file_id(&unique)
            .is_none(),
        "mutating one engine must not change a sibling cloned from the same template"
    );
}

#[tokio::test]
async fn core_template_binding_preserves_an_open_unsaved_document() {
    let fixture = TempDir::new().expect("live-document fixture must be created");
    let project = fixture.path().join("app");
    fs::create_dir_all(&project).expect("project root must be created");
    fs::write(project.join("Gemfile"), "source 'https://rubygems.org'\n")
        .expect("Gemfile must be written");
    let path = project.join("live.rb");
    let content =
        "class LiveDocument\n  def unsaved_marker; end\n  def call; unsaved_marker; end\nend\n";
    let uri = Url::from_file_path(&path).expect("live document URI must be valid");
    let server = create_test_server();
    let workspace = server.add_workspace(Url::from_directory_path(&project).unwrap());
    indexing::handle_did_open(
        &server,
        DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: "ruby".to_string(),
                version: 1,
                text: content.to_string(),
            },
        },
    )
    .await;

    let mut coordinator = IndexingCoordinator::new(project, RubyFastLspConfig::default());
    coordinator
        .setup_file_processor(&server.load_context_for_project(coordinator.workspace_root()));
    coordinator
        .index_core_stubs(
            &server.load_context_for_project(coordinator.workspace_root()),
            Some(RubyVersion::new(3, 0)),
        )
        .await
        .expect("core stubs must bind successfully");

    let engine = workspace.analysis_engine.read();
    let file_id = engine
        .view()
        .file_id(&path)
        .expect("binding a core template must not erase the open document");
    assert!(
        engine.view().file_content_matches(file_id, content),
        "binding a core template must preserve the exact unsaved document content"
    );
    drop(engine);

    let definitions = definition::definition_locations(
        definition::find_definition_at_position(
            &server,
            uri,
            tower_lsp::lsp_types::Position::new(2, 14),
        )
        .await
        .expect("same-file definition lookup must remain available"),
    );
    assert_eq!(
        definitions.len(),
        1,
        "same-file navigation must survive core-template binding"
    );
    assert_eq!(definitions[0].range.start.line, 1);
}

#[tokio::test]
async fn dependency_core_seed_never_contains_an_open_project_document() {
    let fixture = TempDir::new().expect("dependency-seed fixture must be created");
    let clean_project = fixture.path().join("clean");
    let live_project = fixture.path().join("live");
    for project in [&clean_project, &live_project] {
        fs::create_dir_all(project).expect("project root must be created");
        fs::write(project.join("Gemfile"), "source 'https://rubygems.org'\n")
            .expect("Gemfile must be written");
    }

    let server = create_test_server();
    server.add_workspace(Url::from_directory_path(&clean_project).unwrap());
    server.add_workspace(Url::from_directory_path(&live_project).unwrap());

    let mut clean_coordinator =
        IndexingCoordinator::new(clean_project, RubyFastLspConfig::default());
    clean_coordinator
        .setup_file_processor(&server.load_context_for_project(clean_coordinator.workspace_root()));
    let clean_seed = clean_coordinator
        .index_core_stubs(
            &server.load_context_for_project(clean_coordinator.workspace_root()),
            Some(RubyVersion::new(3, 0)),
        )
        .await
        .expect("clean core seed must be prepared");

    let live_path = live_project.join("live.rb");
    let live_uri = Url::from_file_path(&live_path).expect("live document URI must be valid");
    indexing::handle_did_open(
        &server,
        DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: live_uri,
                language_id: "ruby".to_string(),
                version: 1,
                text: "class ProjectOnly; end\n".to_string(),
            },
        },
    )
    .await;
    let mut live_coordinator = IndexingCoordinator::new(live_project, RubyFastLspConfig::default());
    live_coordinator
        .setup_file_processor(&server.load_context_for_project(live_coordinator.workspace_root()));
    let live_seed = live_coordinator
        .index_core_stubs(
            &server.load_context_for_project(live_coordinator.workspace_root()),
            Some(RubyVersion::new(3, 0)),
        )
        .await
        .expect("live-document core seed must be prepared");

    assert!(
        live_seed.view().file_id(&live_path).is_none(),
        "the reusable dependency seed must never inherit project-owned open-document facts"
    );
    assert_eq!(
        clean_seed.view().semantic_context_fingerprint(),
        live_seed.view().semantic_context_fingerprint(),
        "editor open timing must not change the immutable dependency seed identity"
    );
}

#[tokio::test]
async fn test_coordinator_with_missing_directories() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let project_root = temp_dir.path().to_path_buf();

    let config = RubyFastLspConfig::default();
    let mut coordinator = IndexingCoordinator::new(project_root, config);
    let server = create_test_server();

    // Test indexing with missing directories (should not panic)
    let result = coordinator
        .run_complete_indexing(&server.load_context_for_project(coordinator.workspace_root()))
        .await;
    assert!(
        result.is_ok(),
        "Indexing should handle missing directories gracefully"
    );
}
