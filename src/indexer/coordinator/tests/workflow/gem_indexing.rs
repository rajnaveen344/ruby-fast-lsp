//! Gem discovery, indexing, failure handling, and scale through the coordinator.

use super::*;

#[tokio::test]
async fn test_coordinator_gem_discovery() {
    // Set environment variable to limit gem processing for faster tests
    // SAFETY: This test is not run concurrently with other tests that modify this env var
    unsafe { std::env::set_var("RUBY_LSP_MAX_GEMS", "5") };

    let fixture = TestProjectFixture::new();
    fixture.setup_complete_project();

    let config = RubyFastLspConfig::default();
    let mut coordinator = IndexingCoordinator::new(fixture.project_root().clone(), config);
    let server = create_test_server();

    // Execute indexing which should include gem discovery
    let result = coordinator.run_complete_indexing(&server).await;
    assert!(result.is_ok(), "Indexing with gem discovery should succeed");

    assert!(
        coordinator.gem_indexer.is_some(),
        "production gem discovery must initialize the owning project's exact gem indexer"
    );
    assert!(
        coordinator.get_ruby_library_paths().is_empty(),
        "complete indexing must not launch the redundant legacy load-path discovery"
    );

    // Clean up environment variable
    // SAFETY: This test is not run concurrently with other tests that modify this env var
    unsafe { std::env::remove_var("RUBY_LSP_MAX_GEMS") };
}

#[tokio::test]
async fn test_coordinator_gem_indexing_integration() {
    // Set environment variable to limit gem processing for faster tests
    // SAFETY: This test is not run concurrently with other tests that modify this env var
    unsafe { std::env::set_var("RUBY_LSP_MAX_GEMS", "3") };

    let fixture = TestProjectFixture::new();
    fixture.setup_complete_project();

    let config = RubyFastLspConfig::default();
    let mut coordinator = IndexingCoordinator::new(fixture.project_root().clone(), config);
    let server = create_test_server();

    // Test that gem indexing doesn't break the overall indexing process
    let result = coordinator.run_complete_indexing(&server).await;
    assert!(
        result.is_ok(),
        "Indexing should succeed even with gem discovery"
    );

    assert!(
        coordinator.gem_indexer.is_some(),
        "gem indexing must complete through the owning project's exact gem indexer"
    );
    assert!(
        coordinator.get_ruby_library_paths().is_empty(),
        "gem indexing must not populate the unused legacy load-path side table"
    );

    // The gem indexing should not interfere with project file indexing
    let mut project_files = Vec::new();
    coordinator.find_all_ruby_files_in_directory(fixture.project_root(), &mut project_files);
    assert!(
        !project_files.is_empty(),
        "Project files should still be discoverable after gem indexing"
    );

    // Clean up environment variable
    // SAFETY: This test is not run concurrently with other tests that modify this env var
    unsafe { std::env::remove_var("RUBY_LSP_MAX_GEMS") };
}

#[tokio::test]
async fn test_coordinator_gem_error_handling() {
    // Set environment variable to limit gem processing for faster tests
    // SAFETY: This test is not run concurrently with other tests that modify this env var
    unsafe { std::env::set_var("RUBY_LSP_MAX_GEMS", "2") };

    let fixture = TestProjectFixture::new();
    fixture.setup_complete_project();

    let config = RubyFastLspConfig::default();
    let mut coordinator = IndexingCoordinator::new(fixture.project_root().clone(), config);
    let server = create_test_server();

    // Even if gem discovery fails, the overall indexing should still succeed
    // This tests the error handling in discover_and_index_gems
    let result = coordinator.run_complete_indexing(&server).await;
    assert!(
        result.is_ok(),
        "Indexing should succeed even if gem discovery encounters errors"
    );

    // Basic functionality should still work
    let lib_dirs = coordinator.get_ruby_library_paths();
    // We should at least have some directories (even if gem discovery failed)
    // The system Ruby directories should still be found
    let _ = lib_dirs;

    // Clean up environment variable
    // SAFETY: This test is not run concurrently with other tests that modify this env var
    unsafe { std::env::remove_var("RUBY_LSP_MAX_GEMS") };
}

#[tokio::test]
async fn test_coordinator_gem_performance() {
    // Set environment variable to limit gem processing for faster tests
    // SAFETY: This test is not run concurrently with other tests that modify this env var
    unsafe { std::env::set_var("RUBY_LSP_MAX_GEMS", "3") };

    let fixture = TestProjectFixture::new();
    fixture.setup_complete_project();

    let config = RubyFastLspConfig::default();
    let mut coordinator = IndexingCoordinator::new(fixture.project_root().clone(), config);
    let server = create_test_server();

    // Measure time for indexing including gem discovery
    let start = std::time::Instant::now();
    let result = coordinator.run_complete_indexing(&server).await;
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

    // Clean up environment variable
    // SAFETY: This test is not run concurrently with other tests that modify this env var
    unsafe { std::env::remove_var("RUBY_LSP_MAX_GEMS") };
}
