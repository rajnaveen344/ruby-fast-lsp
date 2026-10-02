//! Project file discovery, RBS facts, scale, and cold diagnostics publication.

use super::*;
use ruby_analysis::engine::AnalysisStat;

#[tokio::test]
async fn test_coordinator_project_file_collection() {
    let fixture = TestProjectFixture::new();
    fixture.setup_complete_project();

    // Test Ruby file collection
    let files = crate::utils::file_ops::collect_ruby_files(fixture.project_root());

    assert!(!files.is_empty(), "Should find Ruby files in project");

    // Verify specific files are found
    let file_names: Vec<String> = files
        .iter()
        .filter_map(|p| p.file_name()?.to_str())
        .map(|s| s.to_string())
        .collect();

    assert!(file_names.contains(&"application.rb".to_string()));
    assert!(file_names.contains(&"user.rb".to_string()));
    assert!(file_names.contains(&"user_service.rb".to_string()));
    assert!(file_names.contains(&"user_test.rb".to_string()));
    assert!(file_names.contains(&"Thorfile".to_string()));
    assert!(file_names.contains(&"config.ru".to_string()));
}

#[tokio::test]
async fn project_rbs_declarations_enter_engine_method_facts() {
    let temp_dir = TempDir::new().expect("test workspace must be created");
    let sig_dir = temp_dir.path().join("sig");
    fs::create_dir_all(&sig_dir).expect("sig directory must be created");
    let signature_path = sig_dir.join("native_widget.rbs");
    fs::write(
        &signature_path,
        "class NativeWidget\n  def encode: (String value) -> String\nend\n",
    )
    .expect("RBS fixture must be written");
    let usage_path = temp_dir.path().join("native_usage.rb");
    let usage = "widget = NativeWidget.new\nwidget.encode(\"value\")\n";
    fs::write(&usage_path, usage).expect("Ruby usage fixture must be written");

    let mut coordinator =
        IndexingCoordinator::new(temp_dir.path().to_path_buf(), RubyFastLspConfig::default());
    let server = create_test_server();
    coordinator
        .run_complete_indexing(&server.load_context_for_project(coordinator.workspace_root()))
        .await
        .expect("workspace indexing must succeed");

    let engine = server.orphan_engine().read();
    let query = ruby_analysis::engine::AnalysisQuery::new(&engine);
    assert!(
        query.file_id(&signature_path).is_some(),
        "conventional sig/**/*.rbs files must be registered"
    );
    let method = ruby_analysis::core::FullyQualifiedName::method(
        vec![ruby_analysis::core::RubyConstant::new("NativeWidget")
            .expect("test class name must be valid")],
        ruby_analysis::core::RubyMethod::new("encode").expect("test method name must be valid"),
    );
    let facts = query.method_facts_for(&method);
    assert_eq!(facts.len(), 1, "RBS method must become one engine fact");
    assert_eq!(facts[0].return_type_label.as_deref(), Some("String"));
    drop(engine);

    let usage_uri = Url::from_file_path(&usage_path).expect("usage URI must be valid");
    indexing::handle_did_open(
        &server,
        tower_lsp::lsp_types::DidOpenTextDocumentParams {
            text_document: tower_lsp::lsp_types::TextDocumentItem {
                uri: usage_uri.clone(),
                language_id: "ruby".to_string(),
                version: 1,
                text: usage.to_string(),
            },
        },
    )
    .await;
    let document = server
        .documents
        .read()
        .get(&usage_uri)
        .cloned()
        .expect("opened usage document must exist");
    let query = crate::features::cursor::EngineQuery::with_doc_and_engine(
        document,
        server.orphan_engine().clone(),
    );
    let definitions = query
        .find_definitions_at_position(&usage_uri, tower_lsp::lsp_types::Position::new(1, 9), usage)
        .expect("native RBS method call must resolve");
    assert_eq!(definitions.len(), 1);
    assert_eq!(
        definitions[0].uri,
        Url::from_file_path(signature_path).unwrap()
    );
    let hover = query
        .get_hover_at_position(&usage_uri, tower_lsp::lsp_types::Position::new(1, 9), usage)
        .expect("RBS method return must produce hover information");
    assert!(hover.content.contains("String"));
}

#[test]
fn test_coordinator_ruby_file_detection() {
    // Test various Ruby file extensions
    assert!(crate::utils::file_ops::should_index_file(&PathBuf::from(
        "test.rb"
    )));
    assert!(crate::utils::file_ops::should_index_file(&PathBuf::from(
        "test.ruby"
    )));
    assert!(crate::utils::file_ops::should_index_file(&PathBuf::from(
        "test.rake"
    )));
    assert!(crate::utils::file_ops::should_index_file(&PathBuf::from(
        "show.html.erb"
    )));
    assert!(crate::utils::file_ops::should_index_file(&PathBuf::from(
        "Rakefile"
    )));
    assert!(crate::utils::file_ops::should_index_file(&PathBuf::from(
        "Gemfile"
    )));
    assert!(crate::utils::file_ops::should_index_file(&PathBuf::from(
        "Guardfile"
    )));
    assert!(crate::utils::file_ops::should_index_file(&PathBuf::from(
        "Capfile"
    )));

    // Test non-Ruby files
    assert!(!crate::utils::file_ops::should_index_file(&PathBuf::from(
        "test.js"
    )));
    assert!(!crate::utils::file_ops::should_index_file(&PathBuf::from(
        "test.py"
    )));
    assert!(!crate::utils::file_ops::should_index_file(&PathBuf::from(
        "README.md"
    )));
}

#[tokio::test]
async fn test_coordinator_performance_with_large_project() {
    let fixture = TestProjectFixture::new();
    fixture.setup_complete_project();

    // Create additional files to simulate a larger project
    let large_project_dir = fixture.project_root().join("large_project");
    fs::create_dir_all(&large_project_dir).expect("Failed to create large project dir");

    // Create 50 Ruby files
    for i in 0..50 {
        let file_content = format!(
            r#"
class TestClass{}
  def initialize
    @value = {}
  end

  def process
    # Some processing logic
  end
end
"#,
            i, i
        );
        fs::write(
            large_project_dir.join(format!("test_class_{}.rb", i)),
            file_content,
        )
        .expect("Failed to write test file");
    }

    let config = RubyFastLspConfig::default();
    let mut coordinator = IndexingCoordinator::new(fixture.project_root().clone(), config);
    let server = create_test_server();

    // Measure indexing time
    let start = std::time::Instant::now();
    let result = coordinator
        .run_complete_indexing(&server.load_context_for_project(coordinator.workspace_root()))
        .await;
    let duration = start.elapsed();

    assert!(
        result.is_ok(),
        "Large project indexing should complete successfully"
    );
    println!("Large project indexing took: {:?}", duration);

    // Performance assertion - should complete within reasonable time
    assert!(
        duration.as_secs() < 45,
        "Indexing should complete within 45 seconds"
    );
}

#[tokio::test]
async fn test_coordinator_collects_all_ruby_files() {
    // Test that all Ruby files are collected, including vendor directories.
    // File source (Project/Gem/Stdlib) is determined by indexers based on
    // discovered paths from tools (bundler, rubygems), not by exclusion patterns.
    let fixture = TestProjectFixture::new();
    fixture.setup_complete_project();

    // Create a vendor directory with Ruby files
    let vendor_dir = fixture.project_root().join("vendor");
    fs::create_dir_all(&vendor_dir).expect("Failed to create vendor directory");

    let vendor_bundle_dir = vendor_dir.join("bundle");
    fs::create_dir_all(&vendor_bundle_dir).expect("Failed to create vendor/bundle directory");

    // Create Ruby files in vendor
    let vendor_ruby_file = vendor_dir.join("vendor_gem.rb");
    fs::write(&vendor_ruby_file, "class VendorGem\nend").expect("Failed to write vendor Ruby file");

    let vendor_bundle_ruby_file = vendor_bundle_dir.join("bundled_gem.rb");
    fs::write(&vendor_bundle_ruby_file, "class BundledGem\nend")
        .expect("Failed to write vendor/bundle Ruby file");

    // Collect Ruby files from the project
    let collected_files = crate::utils::file_ops::collect_ruby_files(fixture.project_root());

    // Verify that vendor files ARE collected (no exclusion)
    let vendor_files: Vec<_> = collected_files
        .iter()
        .filter(|path| path.to_string_lossy().contains("vendor"))
        .collect();

    assert!(
        !vendor_files.is_empty(),
        "Vendor directory files should be collected (source tagging handles categorization)"
    );

    // Verify that non-vendor files are also collected
    let non_vendor_files: Vec<_> = collected_files
        .iter()
        .filter(|path| !path.to_string_lossy().contains("vendor"))
        .collect();

    assert!(
        !non_vendor_files.is_empty(),
        "Non-vendor Ruby files should also be collected"
    );
}

#[tokio::test]
async fn cold_indexing_retains_but_does_not_publish_closed_file_diagnostics() {
    let workspace = TempDir::new().unwrap();
    let file_path = workspace.path().join("app/service.rb");
    fs::create_dir_all(file_path.parent().unwrap()).unwrap();
    let source = "MissingService.call\n";
    fs::write(&file_path, source).unwrap();
    let uri = Url::from_file_path(&file_path).unwrap();
    let workspace_uri = Url::from_directory_path(workspace.path()).unwrap();
    let server = RubyLanguageServer::default();
    server.add_workspace(workspace_uri);
    let mut coordinator =
        IndexingCoordinator::new(workspace.path().to_path_buf(), RubyFastLspConfig::default());

    coordinator
        .run_complete_indexing(&server.load_context_for_project(coordinator.workspace_root()))
        .await
        .unwrap();

    assert!(
        server
            .analysis_engine_for_uri(&uri)
            .read()
            .view()
            .stats()
            .get(AnalysisStat::Diagnostics)
            > 0,
        "cold indexing must retain workspace diagnostics in the engine"
    );
    assert!(
        server.last_diagnostic_publication(&uri).is_none(),
        "closed-file engine diagnostics must not flood the LSP client"
    );

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
    assert!(
        !server.last_published_diagnostics(&uri).is_empty(),
        "opening the file must publish its current diagnostics"
    );
}
