use log::info;
use lsp_types::*;
use std::path::PathBuf;
use tower_lsp::LanguageServer;

use crate::analyzer::RubyAnalyzer;
use crate::handlers::request;
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
        workspace_folders: None,
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

/// Test goto definition functionality for class_declaration.rb
#[tokio::test]
async fn test_goto_definition_class_declaration() {
    let fixture_file = "class_declaration.rb";
    let server = init_and_open_file(fixture_file).await;

    info!("Index ready for testing");

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
}
