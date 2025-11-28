//! Integration tests for YARD-based inlay hints and diagnostics

use std::path::PathBuf;
use tower_lsp::lsp_types::{InlayHintParams, Position, Range, TextDocumentIdentifier, Url};

use crate::{
    capabilities::{diagnostics::generate_yard_diagnostics, inlay_hints::handle_inlay_hints},
    test::integration_test::TestHarness,
};

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/test/fixtures")
}

fn path_to_uri(path: &std::path::Path) -> Url {
    Url::from_file_path(path).expect("Failed to convert path to file:// URI")
}

/// Test that YARD documentation is parsed and inlay hints are generated
/// with individual hints for each parameter and return type
#[tokio::test]
async fn test_yard_inlay_hints() {
    let harness = TestHarness::new().await;

    // Open the YARD types fixture
    harness.open_fixture_dir("yard_types.rb").await;

    let fixture_path = fixture_root().join("yard_types.rb");
    let uri = path_to_uri(&fixture_path);

    // Request inlay hints for the entire file
    let params = InlayHintParams {
        text_document: TextDocumentIdentifier { uri },
        range: Range {
            start: Position::new(0, 0),
            end: Position::new(150, 0),
        },
        work_done_progress_params: Default::default(),
    };

    let hints = handle_inlay_hints(harness.server(), params).await;

    // We should have inlay hints for methods with YARD documentation
    println!("Generated {} inlay hints", hints.len());
    for hint in &hints {
        println!("  Hint at {:?}: {:?}", hint.position, hint.label);
    }

    // Verify we got some hints (methods with YARD docs should have hints)
    assert!(
        !hints.is_empty(),
        "Expected inlay hints for methods with YARD documentation"
    );

    // Check for specific hints - now they should be individual hints
    let hint_labels: Vec<String> = hints
        .iter()
        .filter_map(|h| match &h.label {
            tower_lsp::lsp_types::InlayHintLabel::String(s) => Some(s.clone()),
            _ => None,
        })
        .collect();

    // Should have individual parameter type hints for regular params (with colon)
    assert!(
        hint_labels.iter().any(|h| h == ": String"),
        "Expected ': String' hint for regular params, got: {:?}",
        hint_labels
    );

    assert!(
        hint_labels.iter().any(|h| h == ": Integer"),
        "Expected ': Integer' hint for regular params, got: {:?}",
        hint_labels
    );

    // Should have keyword parameter hints WITHOUT leading colon (just space + type)
    // because keyword params already have a colon in Ruby syntax (name:, age:)
    assert!(
        hint_labels.iter().any(|h| h == " String"),
        "Expected ' String' hint for keyword params (no colon), got: {:?}",
        hint_labels
    );

    // Should have return type hints with -> prefix
    assert!(
        hint_labels.iter().any(|h| h == " -> User"),
        "Expected ' -> User' return type hint, got: {:?}",
        hint_labels
    );

    assert!(
        hint_labels.iter().any(|h| h == " -> Boolean"),
        "Expected ' -> Boolean' return type hint, got: {:?}",
        hint_labels
    );

    assert!(
        hint_labels.iter().any(|h| h == " -> Array<User>"),
        "Expected ' -> Array<User>' return type hint, got: {:?}",
        hint_labels
    );

    // Updated to use Hash{K => V} syntax
    assert!(
        hint_labels
            .iter()
            .any(|h| h == " -> Hash{Symbol => Object}"),
        "Expected ' -> Hash{{Symbol => Object}}' return type hint for keyword method, got: {:?}",
        hint_labels
    );
}

/// Test that methods without YARD docs don't get type hints
#[tokio::test]
async fn test_no_yard_no_method_type_hints() {
    let harness = TestHarness::new().await;

    // Open a fixture that has a method without YARD docs
    harness.open_fixture_dir("yard_types.rb").await;

    let fixture_path = fixture_root().join("yard_types.rb");
    let uri = path_to_uri(&fixture_path);

    let params = InlayHintParams {
        text_document: TextDocumentIdentifier { uri },
        range: Range {
            start: Position::new(0, 0),
            end: Position::new(150, 0),
        },
        work_done_progress_params: Default::default(),
    };

    let hints = handle_inlay_hints(harness.server(), params).await;

    // Check that hints exist for documented methods but not for undocumented ones
    let hint_labels: Vec<String> = hints
        .iter()
        .filter_map(|h| match &h.label {
            tower_lsp::lsp_types::InlayHintLabel::String(s) => Some(s.clone()),
            _ => None,
        })
        .collect();

    // The no_docs_method should not have type hints
    // It should not appear in any hint that contains type information
    let has_no_docs_hint = hint_labels.iter().any(|h| h.contains("no_docs"));
    assert!(
        !has_no_docs_hint,
        "Method without YARD docs should not have type hints"
    );
}

/// Test that YARD @param tags for non-existent parameters generate diagnostics
#[tokio::test]
async fn test_yard_mismatched_param_diagnostics() {
    let harness = TestHarness::new().await;

    // Open the YARD types fixture which has a method with mismatched params
    harness.open_fixture_dir("yard_types.rb").await;

    let fixture_path = fixture_root().join("yard_types.rb");
    let uri = path_to_uri(&fixture_path);

    // Get YARD diagnostics
    let index = harness.server().index.lock();
    let diagnostics = generate_yard_diagnostics(&index, &uri);

    println!("Generated {} YARD diagnostics", diagnostics.len());
    for diag in &diagnostics {
        println!("  {:?}: {}", diag.range, diag.message);
    }

    // Should have diagnostics for the mismatched params
    assert!(
        diagnostics.len() >= 2,
        "Expected at least 2 diagnostics for mismatched @param tags, got {}",
        diagnostics.len()
    );

    // Check that diagnostics mention the wrong param names
    let messages: Vec<&str> = diagnostics.iter().map(|d| d.message.as_str()).collect();

    assert!(
        messages.iter().any(|m| m.contains("wrong_name")),
        "Expected diagnostic for 'wrong_name' param, got: {:?}",
        messages
    );

    assert!(
        messages.iter().any(|m| m.contains("another_wrong")),
        "Expected diagnostic for 'another_wrong' param, got: {:?}",
        messages
    );

    // Check that diagnostics have correct severity (warning)
    for diag in &diagnostics {
        assert_eq!(
            diag.severity,
            Some(tower_lsp::lsp_types::DiagnosticSeverity::WARNING),
            "YARD param mismatch should be a warning"
        );
    }

    // Check that diagnostics have the correct code
    for diag in &diagnostics {
        assert_eq!(
            diag.code,
            Some(tower_lsp::lsp_types::NumberOrString::String(
                "yard-unknown-param".to_string()
            )),
            "YARD diagnostic should have 'yard-unknown-param' code"
        );
    }
}

/// Test that correctly matched YARD params don't generate diagnostics
#[tokio::test]
async fn test_yard_matched_params_no_diagnostics() {
    let harness = TestHarness::new().await;

    // Open the YARD types fixture
    harness.open_fixture_dir("yard_types.rb").await;

    let fixture_path = fixture_root().join("yard_types.rb");
    let uri = path_to_uri(&fixture_path);

    // Get YARD diagnostics
    let index = harness.server().index.lock();
    let diagnostics = generate_yard_diagnostics(&index, &uri);

    // Check that correctly documented methods (like initialize with name, age)
    // don't have diagnostics about their params
    let messages: Vec<&str> = diagnostics.iter().map(|d| d.message.as_str()).collect();

    // These are correctly matched params from the User#initialize method
    assert!(
        !messages.iter().any(|m| m.contains("'name'")),
        "Correctly matched 'name' param should not have diagnostic"
    );

    assert!(
        !messages.iter().any(|m| m.contains("'age'")),
        "Correctly matched 'age' param should not have diagnostic"
    );
}
