//! Dependency scanning and cold collection that keeps open documents authoritative.

use super::*;

#[test]
fn configured_project_files_drive_dependency_scanning() {
    let workspace = TempDir::new().unwrap();
    let root = workspace.path();
    std::fs::create_dir_all(root.join("bin")).unwrap();
    std::fs::create_dir_all(root.join("vendor")).unwrap();
    std::fs::write(root.join("app.rb"), "gem 'rack'\n").unwrap();
    std::fs::write(root.join("bin/console"), "gem 'rails'\n").unwrap();
    std::fs::write(root.join("vendor/generated.rb"), "gem 'debug'\n").unwrap();

    let indexer = IndexerProject::new(
        root.to_path_buf(),
        FileProcessor::new(),
        IndexingConfig {
            included_patterns: vec!["bin/*".to_string()],
            excluded_patterns: vec!["vendor/**/*".to_string()],
            ..IndexingConfig::default()
        },
    );

    indexer.scan_for_dependencies().unwrap();

    assert!(indexer.requires_gem("rack"));
    assert!(indexer.requires_gem("rails"));
    assert!(!indexer.requires_gem("debug"));
}

#[tokio::test]
async fn project_stage_resolves_open_documents_and_defers_closed_candidates() {
    let workspace = TempDir::new().unwrap();
    let root = workspace.path();
    let open_path = root.join("open.rb");
    let closed_path = root.join("closed.rb");
    let definition_path = root.join("user.rb");
    std::fs::write(&open_path, "User.new\n").unwrap();
    std::fs::write(&closed_path, "User.new\n").unwrap();
    std::fs::write(&definition_path, "class User\nend\n").unwrap();

    let server = RubyLanguageServer::default();
    let workspace = server.add_workspace(Url::from_directory_path(root).unwrap());
    let open_uri = Url::from_file_path(&open_path).unwrap();
    crate::lsp::capabilities::indexing::handle_did_open(
        &server,
        DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: open_uri,
                language_id: "ruby".to_string(),
                version: 1,
                text: "User.new\n".to_string(),
            },
        },
    )
    .await;

    let mut indexer = IndexerProject::new(
        root.to_path_buf(),
        FileProcessor::new(),
        IndexingConfig::default(),
    );
    indexer
        .collect_project_facts(&server.load_context_for_project(indexer.workspace_root()))
        .unwrap();

    let engine = workspace.analysis_engine.read();
    let open_file = engine.file_id(&open_path).unwrap();
    let closed_file = engine.file_id(&closed_path).unwrap();
    let query = AnalysisQuery::new(&engine);
    assert!(
        !query.references_in_file(open_file).is_empty(),
        "the open document must have its project references resolved"
    );
    assert!(
        query.references_in_file(closed_file).is_empty(),
        "closed-file candidates must remain deferred during project-navigation staging"
    );
    drop(engine);

    workspace.analysis_engine.write().resolve();
    assert!(
        !AnalysisQuery::new(&workspace.analysis_engine.read())
            .references_in_file(closed_file)
            .is_empty(),
        "the final complete resolution must materialize the deferred closed-file candidate"
    );
}

#[tokio::test]
async fn cold_project_collection_cannot_overwrite_newer_open_document_facts() {
    let workspace = TempDir::new().unwrap();
    let root = workspace.path();
    let utility_path = root.join("utility.rb");
    let caller_path = root.join("caller.rb");
    let stale_disk_source = "module Example\n  module Utility\n  end\nend\n";
    let open_source = "module Example\n  module Utility\n    def self.lookup(value)\n      value\n    end\n  end\nend\n";
    let caller_source = "Example::Utility.lookup(\"value\")\n";
    std::fs::write(&utility_path, stale_disk_source).unwrap();
    std::fs::write(&caller_path, caller_source).unwrap();

    let server = RubyLanguageServer::default();
    let workspace_state = server.add_workspace(Url::from_directory_path(root).unwrap());
    for (path, text) in [(&utility_path, open_source), (&caller_path, caller_source)] {
        crate::lsp::capabilities::indexing::handle_did_open(
            &server,
            DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri: Url::from_file_path(path).unwrap(),
                    language_id: "ruby".to_string(),
                    version: 1,
                    text: text.to_string(),
                },
            },
        )
        .await;
    }

    let caller_file = workspace_state
        .analysis_engine
        .read()
        .file_id(&caller_path)
        .unwrap();
    assert!(
        !AnalysisQuery::new(&workspace_state.analysis_engine.read())
            .resolved_reference_definition_ranges_at(caller_file, 19)
            .is_empty(),
        "the open-document pass must initially resolve the singleton method"
    );

    let mut indexer = IndexerProject::new(
        root.to_path_buf(),
        FileProcessor::new(),
        IndexingConfig::default(),
    );
    indexer
        .collect_project_facts(&server.load_context_for_project(indexer.workspace_root()))
        .unwrap();
    workspace_state.analysis_engine.write().resolve();

    let engine = workspace_state.analysis_engine.read();
    let utility_file = engine.file_id(&utility_path).unwrap();
    assert!(
        engine.file_content_matches(utility_file, open_source),
        "cold indexing must retain the editor's newer source snapshot"
    );
    assert!(
        !AnalysisQuery::new(&engine)
            .resolved_reference_definition_ranges_at(caller_file, 19)
            .is_empty(),
        "a stale cold-index batch must not erase method facts from a newer open document"
    );
}

#[test]
fn cold_project_result_is_independent_of_a_prior_identical_file_pass() {
    let workspace = TempDir::new().unwrap();
    let root = workspace.path();
    let path = root.join("user.rb");
    let source = "class User\n  def normalized_name\n    name.upcase\n  end\n\n  def name\n    \"Ada\"\n  end\nend\n\nUser.new.normalized_name\n";
    std::fs::write(&path, source).unwrap();

    let collect = |preindex: bool| {
        let server = RubyLanguageServer::default();
        let workspace_state = server.add_workspace(Url::from_directory_path(root).unwrap());
        if preindex {
            let uri = Url::from_file_path(&path).unwrap();
            let ctx = server.load_context_for_uri(&uri);
            FileProcessor::new()
                .analyze_file_current_file_resolution_forced(&uri, source, &ctx)
                .unwrap()
                .commit(&ctx);
        }
        let mut indexer = IndexerProject::new(
            root.to_path_buf(),
            FileProcessor::new(),
            IndexingConfig::default(),
        );
        indexer
            .collect_project_facts(&server.load_context_for_project(indexer.workspace_root()))
            .unwrap();
        workspace_state.analysis_engine.write().resolve();
        let fingerprint = workspace_state
            .analysis_engine
            .read()
            .semantic_result_fingerprint();
        fingerprint
    };

    assert_eq!(
        collect(false),
        collect(true),
        "byte-identical project collection must not consume stale facts from an earlier pass of the same file"
    );
}
