use log::info;
use lsp_types::*;
use std::path::PathBuf;
use tower_lsp::LanguageServer;

use crate::analyzer::RubyAnalyzer;
use crate::handlers::request;
use crate::indexer::traverser::RubyIndexer;
use crate::server::RubyLanguageServer;
use tower_lsp::lsp_types::Position;

/// Helper function to create absolute paths for test fixtures
fn fixture_dir(relative_path: &str) -> PathBuf {
    let root = std::env::current_dir().expect("Failed to get current directory");
    root.join("src")
        .join("test")
        .join("fixtures")
        .join(relative_path)
}

fn fixture_uri(file_name: &str) -> Url {
    Url::from_file_path(fixture_dir(file_name)).unwrap()
}

/// Helper function to initialize the logger once
fn init_logger() {
    use std::sync::Once;
    static INIT: Once = Once::new();

    INIT.call_once(|| {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    });
}

/// Initialize the server and open a fixture file
async fn init_and_open_file(fixture_file: &str) -> RubyLanguageServer {
    init_logger();
    let server = RubyLanguageServer::default();

    // Enable debug mode for the indexer
    {
        let mut indexer = server.indexer.lock().await;
        indexer.set_debug_mode(true);
    }

    let params = InitializeParams {
        root_uri: Some(fixture_uri(fixture_file)),
        ..Default::default()
    };
    let _ = server.initialize(params).await;

    // Also need to trigger a didOpen to properly index the file
    let file_uri = fixture_uri(fixture_file);
    let file_path = file_uri.to_file_path().unwrap();
    let content = std::fs::read_to_string(file_path).expect("Failed to read fixture file");

    let did_open_params = DidOpenTextDocumentParams {
        text_document: TextDocumentItem {
            uri: file_uri,
            language_id: "ruby".to_string(),
            version: 1,
            text: content,
        },
    };

    server.did_open(did_open_params).await;
    server
}

async fn _init_and_open_folder(folder_name: &str) -> RubyLanguageServer {
    init_logger();
    let server = RubyLanguageServer::default();
    let folder_uri = fixture_uri(folder_name);

    let params = InitializeParams {
        root_uri: Some(folder_uri.clone()),
        workspace_folders: Some(vec![WorkspaceFolder {
            uri: folder_uri,
            name: folder_name.to_string(),
        }]),
        ..Default::default()
    };

    let _ = server.initialize(params).await;
    let initialized_params = InitializedParams {};
    server.initialized(initialized_params).await;

    server
}

/// Test goto definition functionality for class_declaration.rb
#[tokio::test]
async fn test_goto_definition_class_declaration() {
    let fixture_file = "class_declaration.rb";
    let server = init_and_open_file(fixture_file).await;

    info!(
        "Index: {:#?}",
        server.indexer.lock().await.index().uri_to_entries
    );

    let res = request::handle_goto_definition(
        &server,
        GotoDefinitionParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: fixture_uri(fixture_file),
                },
                position: Position {
                    line: 12,      // Line with foo_instance.bar, reference to Foo
                    character: 16, // Position of 'Foo'
                },
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        },
    )
    .await;

    assert!(res.is_ok());
    let definition = res.unwrap();
    assert!(definition.is_some());

    // Verify the location points to the Foo class declaration
    if let Some(GotoDefinitionResponse::Scalar(location)) = definition {
        assert_eq!(location.uri, fixture_uri(fixture_file));
        assert_eq!(location.range.start.line, 0); // Class Foo starts at line 0
    } else {
        panic!("Expected scalar response for goto definition");
    }
}

/// Test goto definition functionality for the bar method
#[tokio::test]
async fn test_goto_definition_method() {
    let fixture_file = "class_declaration.rb";
    let server = init_and_open_file(fixture_file).await;

    // Try to go to definition of 'bar' method call inside another_method
    let res = request::handle_goto_definition(
        &server,
        GotoDefinitionParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: fixture_uri(fixture_file),
                },
                position: Position {
                    line: 7,      // Line with the 'bar' method call
                    character: 4, // Position within 'bar'
                },
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        },
    )
    .await;

    assert!(res.is_ok());
    let definition = res.unwrap();
    assert!(definition.is_some());

    // Verify the location points to the bar method definition
    if let Some(GotoDefinitionResponse::Scalar(location)) = definition {
        assert_eq!(location.uri, fixture_uri(fixture_file));
        assert_eq!(location.range.start.line, 1); // 'def bar' starts at line 1
    } else {
        panic!("Expected scalar response for goto definition");
    }
}

/// Test goto definition without explicitly opening a file
#[tokio::test]
async fn test_goto_definition_without_did_open() {
    let fixture_file = "class_declaration.rb";

    // Create server and initialize but don't call did_open
    init_logger();
    let server = RubyLanguageServer::default();
    let params = InitializeParams {
        root_uri: Some(fixture_uri(fixture_file)),
        ..Default::default()
    };
    let _ = server.initialize(params).await;

    // Try goto definition directly
    let res = request::handle_goto_definition(
        &server,
        GotoDefinitionParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: fixture_uri(fixture_file),
                },
                position: Position {
                    line: 12,
                    character: 16,
                },
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        },
    )
    .await;

    assert!(res.is_ok());
    assert!(res.unwrap().is_some());
}

/// Test find references functionality for a method
#[tokio::test]
async fn test_find_references_method() {
    let fixture_file = "class_declaration.rb";
    let server = init_and_open_file(fixture_file).await;

    // Get the file content to manually check the identifier
    let file_uri = fixture_uri(fixture_file);
    let file_path = file_uri.to_file_path().unwrap();
    let content = std::fs::read_to_string(file_path).expect("Failed to read fixture file");

    // Try different positions to find the identifier
    let mut analyzer = RubyAnalyzer::new();

    // Find position where the 'bar' method is called in line 7
    let pos = Position {
        line: 1,
        character: 6,
    };
    let identifier = analyzer.find_identifier_at_position(&content, pos);
    info!("Identifier found at position {:?}: {:?}", pos, identifier);

    // Find references to 'bar' method - use the position where we found the identifier
    let res = request::handle_references(
        &server,
        ReferenceParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: fixture_uri(fixture_file),
                },
                position: pos,
            },
            context: ReferenceContext {
                include_declaration: true,
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        },
    )
    .await;

    assert!(res.is_ok());
    let references = res.unwrap();
    assert!(
        references.is_some(),
        "Expected to find references to 'bar' method"
    );

    // Should find at least 2 references: the declaration and the call in another_method
    let references = references.unwrap();
    assert!(
        references.len() >= 2,
        "Expected at least 2 references to 'bar'"
    );

    // Debug log to see what references we're actually getting
    info!("Found references:");
    for (i, loc) in references.iter().enumerate() {
        info!(
            "Reference {}: line {}-{}, char {}-{}",
            i,
            loc.range.start.line,
            loc.range.end.line,
            loc.range.start.character,
            loc.range.end.character
        );
    }

    // Validate that we found references at the expected locations
    let expected_references = vec![
        // One reference is our declaration and one is the usage
        (6, 8, 6, 20),   // Declaration
        (22, 6, 22, 18), // Usage in inner.inner_method
    ];

    for expected in expected_references {
        let found = references.iter().any(|loc| {
            loc.range.start.line == expected.0
                && (loc.range.start.character >= expected.1 - 2
                    && loc.range.start.character <= expected.1 + 2)
                && loc.range.end.line == expected.2
                && (loc.range.end.character >= expected.3 - 2
                    && loc.range.end.character <= expected.3 + 2)
        });
        assert!(
            found,
            "Expected to find reference at line {}, character {}",
            expected.0, expected.1
        );
    }
}

/// Test find references functionality for a class
#[tokio::test]
async fn test_find_references_class() {
    let fixture_file = "class_declaration.rb";
    let server = init_and_open_file(fixture_file).await;

    // Use a position that will identify the Foo class
    let pos = Position {
        line: 0,
        character: 6,
    }; // Within "class Foo"

    // Find references to 'Foo' class
    let res = request::handle_references(
        &server,
        ReferenceParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: fixture_uri(fixture_file),
                },
                position: pos,
            },
            context: ReferenceContext {
                include_declaration: true,
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        },
    )
    .await;

    assert!(res.is_ok());
    let references = res.unwrap();
    assert!(
        references.is_some(),
        "Expected to find references to 'Foo' class"
    );

    // Should find at least 2 references: the declaration and the use in "foo_instance = Foo.new"
    let references = references.unwrap();
    assert!(
        references.len() >= 2,
        "Expected at least 2 references to 'Foo'"
    );

    // Validate that we found references at the expected locations
    let expected_references = vec![
        // The class declaration (line 0)
        (0, 0, 9, 3),
        // The usage with Foo.new (line 12)
        (12, 14, 12, 17),
    ];

    for expected in expected_references {
        let found = references.iter().any(|loc| {
            (loc.range.start.line == expected.0 &&
             loc.range.start.character <= expected.1 &&
             loc.range.end.line == expected.2 &&
             loc.range.end.character >= expected.3) ||
            // Also allow for just the "Foo" identifier
            (loc.range.start.line == expected.0 &&
             loc.range.end.line == expected.0 &&
             (expected.1 < 2 || loc.range.start.character >= expected.1 - 2) &&
             loc.range.end.character <= expected.1 + 5)
        });
        assert!(
            found,
            "Expected to find reference at line {}, character {}",
            expected.0, expected.1
        );
    }
}

/// Test find references functionality for nested classes
#[tokio::test]
async fn test_find_references_nested_class() {
    let fixture_file = "nested_classes.rb";
    let server = init_and_open_file(fixture_file).await;

    // Test finding references to Inner class
    let pos = Position {
        line: 5,
        character: 8,
    }; // Within "class Inner"

    // Find references to 'Inner' class
    let res = request::handle_references(
        &server,
        ReferenceParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: fixture_uri(fixture_file),
                },
                position: pos,
            },
            context: ReferenceContext {
                include_declaration: true,
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        },
    )
    .await;

    assert!(res.is_ok());
    let references = res.unwrap();
    assert!(
        references.is_some(),
        "Expected to find references to 'Inner' class"
    );

    // Should find references: the declaration and the use in "inner = Outer::Inner.new"
    let references = references.unwrap();
    info!("Found {} references to Inner class", references.len());

    // Debug log to see what references we're actually getting
    info!("Found references to Inner class:");
    for (i, loc) in references.iter().enumerate() {
        info!(
            "Reference {}: line {}-{}, char {}-{}",
            i,
            loc.range.start.line,
            loc.range.end.line,
            loc.range.start.character,
            loc.range.end.character
        );
    }

    // Validate that we found references at the expected locations
    let expected_references = vec![
        // The class declaration (line 5)
        (5, 8, 5, 13),
        // The usage in Outer::Inner.new (line 20)
        (21, 15, 21, 20),
        // The usage in Outer::Inner::VeryInner (line 23)
        (24, 20, 24, 25),
    ];

    for expected in expected_references {
        let found = references.iter().any(|loc| {
            loc.range.start.line == expected.0
                && loc.range.start.character == expected.1
                && loc.range.end.line == expected.2
                && loc.range.end.character == expected.3
        });
        assert!(
            found,
            "Expected to find reference at line {}, character {}",
            expected.0, expected.1
        );
    }

    // Now test finding references to VeryInner class
    let pos = Position {
        line: 10,
        character: 10,
    }; // Within "class VeryInner"

    // Find references to 'VeryInner' class
    let res = request::handle_references(
        &server,
        ReferenceParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: fixture_uri(fixture_file),
                },
                position: pos,
            },
            context: ReferenceContext {
                include_declaration: true,
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        },
    )
    .await;

    let references = res.unwrap();
    println!("VeryInner references: {:?}", references);

    // VeryInner occurs at:
    // 1. Declaration - line 10: class VeryInner
    // 2. Usage - line 24: very_inner = Outer::Inner::VeryInner.new
    let expected_references = vec![
        // The class declaration (line 10)
        (10, 10, 10, 19),
        // The usage in Outer::Inner::VeryInner.new (line 23)
        (24, 27, 24, 36),
    ];

    // Ensure we have references
    assert!(
        references.is_some(),
        "Expected to find references to VeryInner"
    );
    let references = references.unwrap();

    for expected in expected_references {
        let found = references.iter().any(|loc| {
            loc.range.start.line == expected.0
                && loc.range.start.character == expected.1
                && loc.range.end.line == expected.2
                && loc.range.end.character == expected.3
        });
        assert!(
            found,
            "Expected to find reference at line {}, character {}",
            expected.0, expected.1
        );
    }
}

/// Test find references functionality for methods in nested classes
#[tokio::test]
async fn test_find_references_nested_class_method() {
    let fixture_file = "nested_classes.rb";
    let server = init_and_open_file(fixture_file).await;

    // Test finding references to inner_method
    let pos = Position {
        line: 6,
        character: 12,
    }; // Within "def inner_method"

    // Find references to 'inner_method'
    let res = request::handle_references(
        &server,
        ReferenceParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: fixture_uri(fixture_file),
                },
                position: pos,
            },
            context: ReferenceContext {
                include_declaration: true,
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        },
    )
    .await;

    assert!(res.is_ok());
    let references = res.unwrap();
    assert!(
        references.is_some(),
        "Expected to find references to 'inner_method'"
    );

    // Should find references: the declaration and the call in "inner.inner_method"
    let references = references.unwrap();
    info!("Found {} references to inner_method", references.len());

    // Debug log to see what references we're actually getting
    info!("Found references to inner_method:");
    for (i, loc) in references.iter().enumerate() {
        info!(
            "Reference {}: line {}-{}, char {}-{}",
            i,
            loc.range.start.line,
            loc.range.end.line,
            loc.range.start.character,
            loc.range.end.character
        );
    }

    // Validate that we found references at the expected locations
    let expected_references = vec![
        // The method declaration (line 6)
        (6, 8, 6, 20),
        // The usage in inner.inner_method (line 21)
        (22, 6, 22, 18),
    ];

    // Just check that we have at least one match for each expected reference
    for expected in expected_references {
        let found = references.iter().any(|loc| {
            loc.range.start.line == expected.0
                && loc.range.start.character == expected.1
                && loc.range.end.line == expected.2
                && loc.range.end.character == expected.3
        });
        assert!(
            found,
            "Expected to find reference at line {}, character {}",
            expected.0, expected.1
        );
    }

    // Now test finding references to very_inner_method
    let pos = Position {
        line: 12,
        character: 10,
    }; // Within "def very_inner_method"

    // Find references to 'very_inner_method'
    let res = request::handle_references(
        &server,
        ReferenceParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: fixture_uri(fixture_file),
                },
                position: pos,
            },
            context: ReferenceContext {
                include_declaration: true,
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        },
    )
    .await;

    assert!(res.is_ok());
    let references = res.unwrap();
    assert!(
        references.is_some(),
        "Expected to find references to 'very_inner_method'"
    );

    // Should find references: the declaration and the call in "very_inner.very_inner_method"
    let references = references.unwrap();
    info!("Found {} references to very_inner_method", references.len());

    // Debug log to see what references we're actually getting
    info!("Found references to very_inner_method:");
    for (i, loc) in references.iter().enumerate() {
        info!(
            "Reference {}: line {}-{}, char {}-{}",
            i,
            loc.range.start.line,
            loc.range.end.line,
            loc.range.start.character,
            loc.range.end.character
        );
    }

    // Validate that we found references at the expected locations
    let expected_references = vec![
        // The method declaration (line 11)
        (11, 10, 11, 27),
        // The usage in very_inner.very_inner_method (line 24)
        (25, 11, 25, 28),
    ];

    // Just check that we have at least one match for each expected reference
    for expected in expected_references {
        let found = references.iter().any(|loc| {
            loc.range.start.line == expected.0
                && (loc.range.start.character >= expected.1 - 2
                    && loc.range.start.character <= expected.1 + 2)
                && loc.range.end.line == expected.2
                && (loc.range.end.character >= expected.3 - 2
                    && loc.range.end.character <= expected.3 + 2)
        });
        assert!(
            found,
            "Expected to find reference at line {}, character {}",
            expected.0, expected.1
        );
    }
}

/// Test semantic tokens functionality
#[tokio::test]
async fn test_semantic_tokens_full() {
    let fixture_file = "class_declaration.rb";
    let server = init_and_open_file(fixture_file).await;

    // Get semantic tokens for the entire file
    let res = request::handle_semantic_tokens_full(
        &server,
        SemanticTokensParams {
            text_document: TextDocumentIdentifier {
                uri: fixture_uri(fixture_file),
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        },
    )
    .await;

    assert!(res.is_ok());
    let tokens = res.unwrap();
    assert!(tokens.is_some());

    // There should be at least some tokens for classes, methods, and method calls
    if let Some(SemanticTokensResult::Tokens(tokens)) = tokens {
        assert!(
            !tokens.data.is_empty(),
            "Expected non-empty semantic tokens data"
        );
    }
}

/// Test semantic tokens range functionality
#[tokio::test]
async fn test_semantic_tokens_range() {
    let fixture_file = "class_declaration.rb";
    let server = init_and_open_file(fixture_file).await;

    // Get semantic tokens for just the Foo class definition range
    let res = request::handle_semantic_tokens_range(
        &server,
        SemanticTokensRangeParams {
            text_document: TextDocumentIdentifier {
                uri: fixture_uri(fixture_file),
            },
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                }, // Start of file
                end: Position {
                    line: 9,
                    character: 3,
                }, // End of class definition
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        },
    )
    .await;

    assert!(res.is_ok());
    let tokens = res.unwrap();
    assert!(tokens.is_some());

    // There should be at least some tokens for the class and methods
    if let Some(SemanticTokensRangeResult::Tokens(tokens)) = tokens {
        assert!(
            !tokens.data.is_empty(),
            "Expected non-empty semantic tokens data"
        );
    }
}

#[test]
fn test_identifier_at_method_name_position() {
    let mut analyzer = RubyAnalyzer::new();
    let _indexer = RubyIndexer::new();

    // Get fixture path
    let mut file_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    file_path.push("src/test/fixtures/class_declaration.rb");

    let _uri = Url::from_file_path(&file_path).unwrap();
    let content = std::fs::read_to_string(&file_path).expect("Failed to read fixture file");

    let method_name_position = Position {
        line: 1,
        character: 7, // Position of "bar" in "def bar"
    };

    let identifier = analyzer.find_identifier_at_position(&content, method_name_position);
    assert!(
        identifier.is_some(),
        "Identifier should be found at method name position"
    );
    assert_eq!(
        identifier.unwrap(),
        "Foo#bar",
        "Identifier at method name position should be 'Foo#bar'"
    );

    let another_method_position = Position {
        line: 5,
        character: 14, // Position of "another_method" in "def another_method"
    };

    let identifier = analyzer.find_identifier_at_position(&content, another_method_position);
    assert!(
        identifier.is_some(),
        "Identifier should be found at another method name position"
    );
    assert_eq!(
        identifier.unwrap(),
        "Foo#another_method",
        "Identifier at method name position should be 'Foo#another_method'"
    );
}

/// Test goto definition functionality for Inner class nested in Outer
#[tokio::test]
async fn test_goto_definition_inner_class() {
    let fixture_file = "nested_classes.rb";
    let server = init_and_open_file(fixture_file).await;

    info!(
        "Index: {:#?}",
        server.indexer.lock().await.index().uri_to_entries
    );

    // Test goto definition for Inner class from its usage
    let res = request::handle_goto_definition(
        &server,
        GotoDefinitionParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: fixture_uri(fixture_file),
                },
                position: Position {
                    line: 21,      // Line with inner = Outer::Inner.new
                    character: 17, // Position within 'Inner'
                },
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        },
    )
    .await;

    assert!(res.is_ok());
    let definition = res.unwrap();
    assert!(definition.is_some());

    // Verify the location points to the Inner class declaration
    if let Some(GotoDefinitionResponse::Scalar(location)) = definition {
        assert_eq!(location.uri, fixture_uri(fixture_file));
        assert_eq!(location.range.start.line, 5); // Inner class declaration starts at line 5
    } else {
        panic!("Expected scalar response for goto definition of Inner class");
    }
}

/// Test goto definition functionality for VeryInner class deeply nested
/// This test is expected to fail until support for deeply nested classes is fixed
#[tokio::test]
async fn test_goto_definition_very_inner_class() {
    let fixture_file = "nested_classes.rb";
    let server = init_and_open_file(fixture_file).await;

    // Test goto definition for VeryInner class from its usage
    let res = request::handle_goto_definition(
        &server,
        GotoDefinitionParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: fixture_uri(fixture_file),
                },
                position: Position {
                    line: 24,      // Line with very_inner = Outer::Inner::VeryInner.new
                    character: 31, // Position within 'VeryInner'
                },
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        },
    )
    .await;

    let definition = match res {
        Ok(Some(GotoDefinitionResponse::Scalar(loc))) => Some(loc),
        Ok(Some(GotoDefinitionResponse::Array(locs))) if !locs.is_empty() => Some(locs[0].clone()),
        Ok(Some(GotoDefinitionResponse::Link(links))) if !links.is_empty() => Some(Location::new(
            links[0].target_uri.clone(),
            links[0].target_range,
        )),
        _ => None,
    };

    assert!(
        definition.is_some(),
        "Definition should be found for VeryInner class"
    );

    // Verify the definition points to the correct location
    let definition = definition.unwrap();
    assert_eq!(definition.uri, fixture_uri(fixture_file));

    // The VeryInner class is defined at line 10
    assert_eq!(definition.range.start.line, 10);
}

/// Test goto definition functionality for methods in nested classes
#[tokio::test]
async fn test_goto_definition_nested_methods() {
    let fixture_file = "nested_classes.rb";
    let server = init_and_open_file(fixture_file).await;

    info!(
        "Index: {:#?}",
        server.indexer.lock().await.index().uri_to_entries
    );

    // Test goto definition for inner_method
    let res = request::handle_goto_definition(
        &server,
        GotoDefinitionParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: fixture_uri(fixture_file),
                },
                position: Position {
                    line: 22,      // Line with inner.inner_method
                    character: 12, // Position within 'inner_method' (exact position from regex search)
                },
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        },
    )
    .await;

    assert!(res.is_ok());
    let definition = res.unwrap();
    assert!(definition.is_some());

    // Verify the location points to the inner_method definition
    if let Some(GotoDefinitionResponse::Scalar(location)) = definition {
        assert_eq!(location.uri, fixture_uri(fixture_file));
        assert_eq!(location.range.start.line, 6); // inner_method starts at line 6 (confirmed by regex search)
        assert_eq!(location.range.start.character, 4); // Position of 'def inner_method' (confirmed by regex search)
    } else {
        panic!("Expected scalar response for goto definition of inner_method");
    }

    // Test goto definition for very_inner_method
    let res = request::handle_goto_definition(
        &server,
        GotoDefinitionParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: fixture_uri(fixture_file),
                },
                position: Position {
                    line: 25,      // Line with very_inner.very_inner_method
                    character: 20, // Position within 'very_inner_method' (exact position from regex search)
                },
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        },
    )
    .await;

    assert!(res.is_ok());
    let definition = res.unwrap();
    assert!(definition.is_some());

    // Verify the location points to the very_inner_method definition
    if let Some(GotoDefinitionResponse::Scalar(location)) = definition {
        assert_eq!(location.uri, fixture_uri(fixture_file));
        assert_eq!(location.range.start.line, 11); // very_inner_method starts at line 11 (confirmed by regex search)
        assert_eq!(location.range.start.character, 6); // Position of 'def very_inner_method' (confirmed by regex search)
    } else {
        panic!("Expected scalar response for goto definition of very_inner_method");
    }
}

/// Test finding identifiers in nested classes
#[test]
fn test_identifier_in_nested_classes() {
    let mut analyzer = RubyAnalyzer::new();
    let _indexer = RubyIndexer::new();

    // Get fixture path
    let mut file_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    file_path.push("src/test/fixtures/nested_classes.rb");

    let _uri = Url::from_file_path(&file_path).unwrap();
    let content = std::fs::read_to_string(&file_path).expect("Failed to read fixture file");

    // Test inner_method identifier
    let inner_method_position = Position {
        line: 6,
        character: 10, // Position of "inner_method" in "def inner_method"
    };

    let identifier = analyzer.find_identifier_at_position(&content, inner_method_position);
    assert!(
        identifier.is_some(),
        "Identifier should be found at inner_method position"
    );
    assert_eq!(
        identifier.unwrap(),
        "Outer::Inner#inner_method",
        "Identifier should reflect nested class structure"
    );

    // Test very_inner_method identifier
    let very_inner_method_position = Position {
        line: 11,
        character: 15, // Position of "very_inner_method" in "def very_inner_method"
    };

    let identifier = analyzer.find_identifier_at_position(&content, very_inner_method_position);
    assert!(
        identifier.is_some(),
        "Identifier should be found at very_inner_method position"
    );
    assert_eq!(
        identifier.unwrap(),
        "Outer::Inner::VeryInner#very_inner_method",
        "Identifier should reflect deeply nested class structure"
    );
}

/// Test goto definition for local variables
#[tokio::test]
async fn test_goto_definition_local_variables() {
    let fixture_file = "nested_classes.rb";
    let server = init_and_open_file(fixture_file).await;

    info!(
        "Index: {:#?}",
        server.indexer.lock().await.index().uri_to_entries
    );

    // Find definition of local variable 'outer'
    let res = request::handle_goto_definition(
        &server,
        GotoDefinitionParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: fixture_uri(fixture_file),
                },
                position: Position {
                    line: 19,     // Line with outer.outer_method, reference to 'outer'
                    character: 3, // Position of 'outer'
                },
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        },
    )
    .await;

    assert!(res.is_ok());
    let definition = res.unwrap();
    assert!(definition.is_some());

    // Verify the location points to the outer variable declaration
    if let Some(GotoDefinitionResponse::Scalar(location)) = definition {
        assert_eq!(location.uri, fixture_uri(fixture_file));
        assert_eq!(location.range.start.line, 18); // outer = Outer.new declaration
    } else {
        panic!("Expected scalar response for goto definition");
    }

    // Find definition of local variable 'inner'
    let res = request::handle_goto_definition(
        &server,
        GotoDefinitionParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: fixture_uri(fixture_file),
                },
                position: Position {
                    line: 22,     // Line with inner.inner_method, reference to 'inner'
                    character: 3, // Position of 'inner'
                },
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        },
    )
    .await;

    assert!(res.is_ok());
    let definition = res.unwrap();
    assert!(definition.is_some());

    // Verify the location points to the inner variable declaration
    if let Some(GotoDefinitionResponse::Scalar(location)) = definition {
        assert_eq!(location.uri, fixture_uri(fixture_file));
        assert_eq!(location.range.start.line, 21); // inner = Outer::Inner.new declaration
    } else {
        panic!("Expected scalar response for goto definition");
    }
}

/// Test goto definition for unscoped nested class references
#[tokio::test]
async fn test_goto_definition_unscoped_nested_class() {
    let fixture_file = "nested_classes.rb";
    let server = init_and_open_file(fixture_file).await;

    // The fixture doesn't have unscoped references, so let's test if our implementation
    // can find the definitions by name. We'll search for "Inner" in a position where it exists
    // as a fully qualified name Outer::Inner
    let res = request::handle_goto_definition(
        &server,
        GotoDefinitionParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: fixture_uri(fixture_file),
                },
                position: Position {
                    line: 21,      // Line with inner = Outer::Inner.new
                    character: 16, // Position of 'Inner' in Outer::Inner
                },
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        },
    )
    .await;

    assert!(res.is_ok());
    let definition = res.unwrap();
    assert!(definition.is_some());

    // Verify the location points to the Inner class declaration
    if let Some(GotoDefinitionResponse::Scalar(location)) = definition {
        assert_eq!(location.uri, fixture_uri(fixture_file));
        assert_eq!(location.range.start.line, 5); // class Inner starts at line 5
    } else {
        panic!("Expected scalar response for goto definition");
    }

    // Test finding VeryInner by name in a fully qualified context
    let res = request::handle_goto_definition(
        &server,
        GotoDefinitionParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: fixture_uri(fixture_file),
                },
                position: Position {
                    line: 24,      // Line with very_inner = Outer::Inner::VeryInner.new
                    character: 30, // Position of 'VeryInner' in Outer::Inner::VeryInner
                },
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        },
    )
    .await;

    assert!(res.is_ok());
    let definition = res.unwrap();
    assert!(definition.is_some());

    // Verify the location points to the VeryInner class declaration
    if let Some(GotoDefinitionResponse::Scalar(location)) = definition {
        assert_eq!(location.uri, fixture_uri(fixture_file));
        assert_eq!(location.range.start.line, 10); // class VeryInner starts at line 10
    } else {
        panic!("Expected scalar response for goto definition");
    }
}
