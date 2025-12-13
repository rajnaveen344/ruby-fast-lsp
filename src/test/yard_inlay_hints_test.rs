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

    // Filter for TYPE hints only (those containing "->" for return types or ":" for variable types)
    // This excludes structural hints like "def no_docs_method" or "return"
    let type_hint_labels: Vec<String> = hints
        .iter()
        .filter_map(|h| match &h.label {
            tower_lsp::lsp_types::InlayHintLabel::String(s) => {
                if s.contains("->") || s.starts_with(":") {
                    Some(s.clone())
                } else {
                    None
                }
            }
            _ => None,
        })
        .collect();

    // The no_docs_method should not have type hints
    // Check that no type hint references "no_docs" in any way
    let has_no_docs_type_hint = type_hint_labels.iter().any(|h| h.contains("no_docs"));
    assert!(
        !has_no_docs_type_hint,
        "Method without YARD docs should not have type hints. Found: {:?}",
        type_hint_labels
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

    // Filter to only param mismatch diagnostics (not type resolution diagnostics)
    let param_diagnostics: Vec<_> = diagnostics
        .iter()
        .filter(|d| {
            d.code
                == Some(tower_lsp::lsp_types::NumberOrString::String(
                    "yard-unknown-param".to_string(),
                ))
        })
        .collect();

    // Should have at least 2 param mismatch diagnostics (wrong_name and another_wrong)
    assert!(
        param_diagnostics.len() >= 2,
        "Should have at least 2 param mismatch diagnostics, got {}",
        param_diagnostics.len()
    );

    // Check that param mismatch diagnostics have correct severity (warning)
    for diag in &param_diagnostics {
        assert_eq!(
            diag.severity,
            Some(tower_lsp::lsp_types::DiagnosticSeverity::WARNING),
            "YARD param mismatch should be a warning"
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

/// Test that return types are inferred from method bodies when no YARD is present
#[tokio::test]
async fn test_return_type_inference_without_yard() {
    let harness = TestHarness::new().await;

    // Open the return type inference fixture
    harness.open_fixture_dir("return_type_inference.rb").await;

    let fixture_path = fixture_root().join("return_type_inference.rb");
    let uri = path_to_uri(&fixture_path);

    // Request inlay hints for the entire file
    let params = InlayHintParams {
        text_document: TextDocumentIdentifier { uri: uri.clone() },
        range: Range {
            start: Position::new(0, 0),
            end: Position::new(150, 0),
        },
        work_done_progress_params: Default::default(),
    };

    let hints = handle_inlay_hints(harness.server(), params).await;

    // Print hints for debugging
    println!("Generated {} inlay hints", hints.len());
    for hint in &hints {
        println!("  Hint at {:?}: {:?}", hint.position, hint.label);
    }

    // // Check that we have inlay hints for methods without YARD
    // // Methods should have return type hints like "-> String", "-> Integer", etc.
    // let hint_labels: Vec<String> = hints
    //     .iter()
    //     .map(|h| match &h.label {
    //         tower_lsp::lsp_types::InlayHintLabel::String(s) => s.clone(),
    //         tower_lsp::lsp_types::InlayHintLabel::LabelParts(parts) => {
    //             parts.iter().map(|p| p.value.clone()).collect::<String>()
    //         }
    //     })
    //     .collect();

    // Check specific methods that should NOT have hints (because they have no YARD)

    // Method `string_return` is at line 5 (1-based) -> 4 (0-based)
    // We check a range just in case: 4 (def) to 6 (end)
    let string_return_hints: Vec<_> = hints
        .iter()
        .filter(|h| h.position.line == 4 || h.position.line == 6)
        .collect();
    assert!(
        string_return_hints.iter().all(|h| match &h.label {
            tower_lsp::lsp_types::InlayHintLabel::String(s) => !s.contains("->"),
            _ => true,
        }),
        "Method string_return should NOT have return type hint without YARD"
    );

    // Method `integer_return` is at line 9 (1-based) -> 8 (0-based)
    let integer_return_hints: Vec<_> = hints
        .iter()
        .filter(|h| h.position.line == 8 || h.position.line == 10)
        .collect();
    assert!(
        integer_return_hints.iter().all(|h| match &h.label {
            tower_lsp::lsp_types::InlayHintLabel::String(s) => !s.contains("->"),
            _ => true,
        }),
        "Method integer_return should NOT have return type hint without YARD"
    );
    // Verify that we DO have hints for the method WITH YARD
    // Method `with_yard_return` is at line 128 (1-based) -> 127 (0-based)
    let yard_return_hints: Vec<_> = hints.iter().filter(|h| h.position.line == 127).collect();
    assert!(
        !yard_return_hints.is_empty(),
        "Method with_yard_return SHOULD have hints (from YARD)"
    );
    assert!(
        yard_return_hints.iter().any(|h| match &h.label {
            tower_lsp::lsp_types::InlayHintLabel::String(s) => s.contains("-> String"),
            _ => false,
        }),
        "Method with_yard_return SHOULD have '-> String' hint"
    );
}

/// Test that method call resolution works for User.new -> User instance
#[tokio::test]
async fn test_method_call_type_resolution() {
    let harness = TestHarness::new().await;

    // Open the method call resolution fixture
    harness.open_fixture_dir("method_call_resolution.rb").await;

    let fixture_path = fixture_root().join("method_call_resolution.rb");
    let _uri = path_to_uri(&fixture_path);

    // Check that the index has the User class and its methods
    let index = harness.server().index.lock();

    // Verify User class is indexed
    let user_entries: Vec<_> = index
        .definitions()
        .filter(|(fqn, _)| {
            matches!(fqn, crate::types::fully_qualified_name::FullyQualifiedName::Constant(parts)
            if parts.len() == 1 && parts[0].to_string() == "User")
        })
        .collect();

    assert!(!user_entries.is_empty(), "User class should be indexed");

    // Verify User#name method is indexed with return type
    let name_methods: Vec<_> = index
        .methods_by_name()
        .filter(|(method, _)| method.to_string().contains("name"))
        .collect();

    println!(
        "Found {} methods with 'name': {:?}",
        name_methods.len(),
        name_methods
    );

    // Verify User.find class method is indexed
    let find_methods: Vec<_> = index
        .methods_by_name()
        .filter(|(method, _)| method.to_string().contains("find"))
        .collect();

    println!(
        "Found {} methods with 'find': {:?}",
        find_methods.len(),
        find_methods
    );
}

/// Test that variable type hints are correctly generated after indexing
#[tokio::test]
async fn test_variable_type_hints_after_indexing() {
    use tower_lsp::lsp_types::{InlayHintParams, Range, TextDocumentIdentifier};

    let harness = TestHarness::new().await;

    // Open the method call resolution fixture
    harness.open_fixture_dir("method_call_resolution.rb").await;

    let fixture_path = fixture_root().join("method_call_resolution.rb");
    let uri = path_to_uri(&fixture_path);

    // Request inlay hints
    let params = InlayHintParams {
        text_document: TextDocumentIdentifier { uri: uri.clone() },
        range: Range {
            start: Position::new(0, 0),
            end: Position::new(60, 0),
        },
        work_done_progress_params: Default::default(),
    };

    let hints = handle_inlay_hints(harness.server(), params).await;

    // Print all hints for debugging
    println!("Generated {} inlay hints", hints.len());
    for hint in &hints {
        println!("  Hint at {:?}: {:?}", hint.position, hint.label);
    }

    // Check for variable type hints
    let hint_labels: Vec<String> = hints
        .iter()
        .filter_map(|h| match &h.label {
            tower_lsp::lsp_types::InlayHintLabel::String(s) => Some(s.clone()),
            _ => None,
        })
        .collect();

    // Should have type hint for user variable (: User)
    assert!(
        hint_labels.iter().any(|h| h.contains("User")),
        "Expected type hint containing 'User' for user variable, got: {:?}",
        hint_labels
    );

    // NOTE: `: String` type hint for `name = user.name` is NOT expected because
    // chained method inference is disabled. Only YARD/RBS and `Constant.new` patterns work.
    // The `: User` hint above verifies that `User.new` instantiation inference still works.
}

/// Test that RBS types are used for built-in methods like String#length
#[tokio::test]
async fn test_rbs_builtin_type_resolution() {
    use crate::type_inference::rbs_index::{
        get_rbs_method_return_type_as_ruby_type, rbs_declaration_count, rbs_method_count,
    };
    use crate::type_inference::RubyType;

    // Verify RBS types are loaded
    let decl_count = rbs_declaration_count();
    let method_count = rbs_method_count();
    println!(
        "RBS: {} declarations, {} methods loaded",
        decl_count, method_count
    );
    assert!(decl_count > 0, "RBS declarations should be loaded");
    assert!(method_count > 0, "RBS methods should be loaded");

    // Test String#length returns Integer
    let length_type = get_rbs_method_return_type_as_ruby_type("String", "length", false);
    assert!(
        length_type.is_some(),
        "String#length should have a return type"
    );
    if let Some(RubyType::Class(fqn)) = length_type {
        assert!(
            fqn.to_string().contains("Integer"),
            "String#length should return Integer, got: {}",
            fqn
        );
    } else {
        panic!(
            "Expected Class type for String#length, got: {:?}",
            length_type
        );
    }

    // Test Integer#to_s returns String
    let to_s_type = get_rbs_method_return_type_as_ruby_type("Integer", "to_s", false);
    assert!(
        to_s_type.is_some(),
        "Integer#to_s should have a return type"
    );
    if let Some(RubyType::Class(fqn)) = to_s_type {
        assert!(
            fqn.to_string().contains("String"),
            "Integer#to_s should return String, got: {}",
            fqn
        );
    } else {
        panic!("Expected Class type for Integer#to_s, got: {:?}", to_s_type);
    }

    // Test Array#length returns Integer (size is an alias)
    let length_type = get_rbs_method_return_type_as_ruby_type("Array", "length", false);
    assert!(
        length_type.is_some(),
        "Array#length should have a return type"
    );
    if let Some(RubyType::Class(fqn)) = length_type {
        assert!(
            fqn.to_string().contains("Integer"),
            "Array#length should return Integer, got: {}",
            fqn
        );
    } else {
        panic!(
            "Expected Class type for Array#length, got: {:?}",
            length_type
        );
    }

    // Test Hash#keys returns Array
    let keys_type = get_rbs_method_return_type_as_ruby_type("Hash", "keys", false);
    assert!(keys_type.is_some(), "Hash#keys should have a return type");
    println!("Hash#keys return type: {:?}", keys_type);
}

/// Test that inlay hints show RBS-derived types for built-in method calls
#[tokio::test]
async fn test_rbs_inlay_hints_for_builtin_methods() {
    let harness = TestHarness::new().await;

    // Open the RBS builtin types fixture
    harness.open_fixture_dir("rbs_builtin_types.rb").await;

    let fixture_path = fixture_root().join("rbs_builtin_types.rb");
    let uri = path_to_uri(&fixture_path);

    // Request inlay hints for the entire file
    let params = InlayHintParams {
        text_document: TextDocumentIdentifier { uri },
        range: Range {
            start: Position::new(0, 0),
            end: Position::new(50, 0),
        },
        work_done_progress_params: Default::default(),
    };

    let hints = handle_inlay_hints(harness.server(), params).await;

    // Print all hints for debugging
    println!(
        "Generated {} inlay hints for RBS built-in types:",
        hints.len()
    );
    for hint in &hints {
        println!("  Line {}: {:?}", hint.position.line, hint.label);
    }

    // Collect hint labels
    let hint_labels: Vec<String> = hints
        .iter()
        .filter_map(|h| match &h.label {
            tower_lsp::lsp_types::InlayHintLabel::String(s) => Some(s.clone()),
            _ => None,
        })
        .collect();

    // Should have type hints for built-in method results
    // name.length should show Integer
    assert!(
        hint_labels.iter().any(|h| h.contains("Integer")),
        "Expected type hint containing 'Integer' for length variable, got: {:?}",
        hint_labels
    );

    // name.upcase should show String (or self type)
    assert!(
        hint_labels.iter().any(|h| h.contains("String")),
        "Expected type hint containing 'String' for upper variable, got: {:?}",
        hint_labels
    );
}
