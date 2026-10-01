//! Gem discovery, indexing, failure handling, and scale through the coordinator.

use super::*;

#[tokio::test]
async fn test_coordinator_gem_discovery() {
    let fixture = TestProjectFixture::new();
    fixture.setup_complete_project();

    let config = RubyFastLspConfig::default();
    let mut coordinator = IndexingCoordinator::new(fixture.project_root().clone(), config);
    let server = create_test_server();

    // Execute indexing which should include gem discovery
    let result = coordinator
        .run_complete_indexing(
            &server.load_context_for_project(coordinator.workspace_root()),
            &server,
        )
        .await;
    assert!(result.is_ok(), "Indexing with gem discovery should succeed");

    assert!(
        coordinator.gem_indexer.is_some(),
        "production gem discovery must initialize the owning project's exact gem indexer"
    );
}

#[tokio::test]
async fn test_coordinator_gem_indexing_integration() {
    let fixture = TestProjectFixture::new();
    fixture.setup_complete_project();

    let config = RubyFastLspConfig::default();
    let mut coordinator = IndexingCoordinator::new(fixture.project_root().clone(), config);
    let server = create_test_server();

    // Test that gem indexing doesn't break the overall indexing process
    let result = coordinator
        .run_complete_indexing(
            &server.load_context_for_project(coordinator.workspace_root()),
            &server,
        )
        .await;
    assert!(
        result.is_ok(),
        "Indexing should succeed even with gem discovery"
    );

    assert!(
        coordinator.gem_indexer.is_some(),
        "gem indexing must complete through the owning project's exact gem indexer"
    );

    // The gem indexing should not interfere with project file indexing
    let project_files = crate::utils::file_ops::collect_ruby_files(fixture.project_root());
    assert!(
        !project_files.is_empty(),
        "Project files should still be discoverable after gem indexing"
    );
}

#[tokio::test]
async fn test_coordinator_gem_error_handling() {
    let fixture = TestProjectFixture::new();
    fixture.setup_complete_project();

    let config = RubyFastLspConfig::default();
    let mut coordinator = IndexingCoordinator::new(fixture.project_root().clone(), config);
    let server = create_test_server();

    // Even if gem discovery fails, the overall indexing should still succeed
    // This tests the error handling in discover_and_index_gems
    let result = coordinator
        .run_complete_indexing(
            &server.load_context_for_project(coordinator.workspace_root()),
            &server,
        )
        .await;
    assert!(
        result.is_ok(),
        "Indexing should succeed even if gem discovery encounters errors"
    );
}

#[tokio::test]
async fn test_coordinator_gem_performance() {
    let fixture = TestProjectFixture::new();
    fixture.setup_complete_project();

    let config = RubyFastLspConfig::default();
    let mut coordinator = IndexingCoordinator::new(fixture.project_root().clone(), config);
    let server = create_test_server();

    // Measure time for indexing including gem discovery
    let start = std::time::Instant::now();
    let result = coordinator
        .run_complete_indexing(
            &server.load_context_for_project(coordinator.workspace_root()),
            &server,
        )
        .await;
    let elapsed = start.elapsed();

    assert!(
        result.is_ok(),
        "Indexing with gem discovery should complete successfully"
    );

    // Gem discovery should not significantly slow down the indexing process
    // Allow up to 30 seconds for gem discovery in addition to regular indexing
    assert!(
        elapsed.as_secs() < 30,
        "Indexing with gem discovery should complete within 30 seconds, took {}s",
        elapsed.as_secs()
    );

    println!(
        "Indexing with gem discovery completed in {}ms",
        elapsed.as_millis()
    );
}
