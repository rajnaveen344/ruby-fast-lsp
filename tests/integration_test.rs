use anyhow::Result;
use std::path::PathBuf;

/// Helper function to create absolute paths for test fixtures
fn fixture_path(relative_path: &str) -> PathBuf {
    let root = std::env::current_dir().expect("Failed to get current directory");
    root.join("tests").join("fixtures").join(relative_path)
}

/// Test that basic Ruby fixture files exist
#[tokio::test]
async fn test_fixture_files_exist() -> Result<()> {
    let fixtures = vec![
        "class_declaration.rb",
        "nested_classes.rb",
        "module_with_methods.rb",
        "variables.rb",
        "control_flow.rb",
    ];

    for fixture in fixtures {
        let path = fixture_path(fixture);
        assert!(path.exists(), "Fixture file {} should exist", fixture);

        let content = std::fs::read_to_string(&path)?;
        assert!(
            !content.is_empty(),
            "Fixture file {} should not be empty",
            fixture
        );
        println!("Successfully read fixture: {}", fixture);
    }

    Ok(())
}

/// Test that LSP-specific fixtures exist
#[tokio::test]
async fn test_lsp_fixtures_exist() -> Result<()> {
    // Test that LSP-specific fixtures exist
    let fixtures = vec![
        "definition_goto_test.rb",
        "references_test.rb",
        "symbols_test.rb",
        "completion_test.rb",
        "hover_test.rb",
    ];

    for fixture in fixtures {
        let path = fixture_path(fixture);
        assert!(path.exists(), "Fixture file {} should exist", fixture);

        let content = std::fs::read_to_string(&path)?;
        assert!(
            !content.is_empty(),
            "Fixture file {} should not be empty",
            fixture
        );
        println!("Successfully read fixture: {}", fixture);
    }

    Ok(())
}

// Note: We've removed the redundant LSP message construction and serialization tests
// since they test functionality already covered by the tower_lsp crate.
// Future integration tests should focus on testing our specific Ruby LSP functionality.
