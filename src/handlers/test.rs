#[cfg(test)]
mod tests {
    use lsp_types::{
        GotoDefinitionParams, GotoDefinitionResponse, Location, PartialResultParams, Position,
        ReferenceContext, ReferenceParams, TextDocumentIdentifier, TextDocumentPositionParams, Url,
        WorkDoneProgressParams,
    };
    use std::path::PathBuf;

    use crate::handlers::request;
    use crate::indexer::RubyIndexer;
    use crate::server::RubyLanguageServer;

    // Helper function to create a test server with an indexer
    fn create_test_server() -> RubyLanguageServer {
        let indexer = RubyIndexer::new().expect("Failed to create indexer");
        let client = None;
        RubyLanguageServer {
            client,
            indexer: tokio::sync::Mutex::new(indexer),
        }
    }

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

    #[tokio::test]
    async fn test_goto_definition_handler() {
        // Create a server with a mock indexer
        let server = create_test_server();

        // Create the parameters for the goto definition request
        let uri = fixture_uri("class_declaration.rb");
        let position = Position {
            line: 12,      // Line with foo_instance.bar
            character: 16, // Position of 'Foo'
        };

        let params = GotoDefinitionParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                position,
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: lsp_types::PartialResultParams::default(),
        };

        // Mock an indexer that would return a definition
        {
            let mut indexer = server.indexer.lock().await;
            // The file needs to be indexed before we can find definitions
            // In a real scenario, this would happen on file open or initialize
            indexer.set_debug_mode(true);

            // Read the file content and manually trigger indexing
            let file_path = uri.to_file_path().unwrap();
            let content = std::fs::read_to_string(file_path).expect("Failed to read fixture file");
            let _ = indexer.process_file(uri.clone(), &content);
        }

        // Call the handler
        let result = request::handle_goto_definition(&server, params).await;

        // Verify the result
        assert!(result.is_ok(), "Definition handler should return Ok");

        // We don't necessarily expect to find a definition in this test setup
        // But the function should complete without errors
        println!("Definition test result: {:?}", result);
        match result {
            Ok(Some(GotoDefinitionResponse::Scalar(Location {
                uri: def_uri,
                range,
            }))) => {
                println!("Found definition at {:?}, range: {:?}", def_uri, range);
            }
            Ok(Some(_)) => {
                println!("Found definition with multiple locations");
            }
            Ok(None) => {
                println!("No definition found, but handler completed successfully");
            }
            Err(e) => {
                panic!("Handler returned error: {:?}", e);
            }
        }
    }

    #[tokio::test]
    async fn test_references_handler() {
        // Create a server with a mock indexer
        let server = create_test_server();

        // Create the parameters for the references request
        let uri = fixture_uri("class_declaration.rb");
        let position = Position {
            line: 1,      // Line with def bar
            character: 6, // Position of 'bar'
        };

        let params = ReferenceParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                position,
            },
            context: ReferenceContext {
                include_declaration: true,
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: lsp_types::PartialResultParams::default(),
        };

        // Mock an indexer that would return references
        {
            let mut indexer = server.indexer.lock().await;
            // The file needs to be indexed before we can find references
            indexer.set_debug_mode(true);

            // Read the file content and manually trigger indexing
            let file_path = uri.to_file_path().unwrap();
            let content = std::fs::read_to_string(file_path).expect("Failed to read fixture file");
            let _ = indexer.process_file(uri.clone(), &content);
        }

        // Call the handler
        let result = request::handle_references(&server, params).await;

        // Verify the result
        assert!(result.is_ok(), "References handler should return Ok");

        // We don't necessarily expect to find references in this test setup
        // But the function should complete without errors
        println!("References test result: {:?}", result);
        match result {
            Ok(Some(locations)) => {
                println!("Found {} references", locations.len());
                for (i, loc) in locations.iter().enumerate() {
                    println!("Reference {}: {:?}, range: {:?}", i, loc.uri, loc.range);
                }
            }
            Ok(None) => {
                println!("No references found, but handler completed successfully");
            }
            Err(e) => {
                panic!("Handler returned error: {:?}", e);
            }
        }
    }

    #[tokio::test]
    async fn test_multiple_definitions_handler() {
        // Create a test server
        let server = create_test_server();

        // Create file URIs for multiple class definitions
        let file1_uri = fixture_uri("class_file1.rb");
        let file2_uri = fixture_uri("class_file2.rb");

        // Create file content for both files
        let file1_content = "class MultiClassTest\n  def method1\n    puts 'method1'\n  end\nend";
        let file2_content = "class MultiClassTest\n  def method2\n    puts 'method2'\n  end\nend";

        // Get file paths
        let file1_path = file1_uri.to_file_path().unwrap();
        let file2_path = file2_uri.to_file_path().unwrap();

        // Ensure fixture files exist with correct content
        ensure_fixture_file_exists(&file1_path, file1_content);
        ensure_fixture_file_exists(&file2_path, file2_content);

        // Setup params for goto definition at the class name
        let params = GotoDefinitionParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: file1_uri.clone(),
                },
                position: Position {
                    line: 0,
                    character: 8, // Position at "MultiClassTest" in file1
                },
            },
            work_done_progress_params: WorkDoneProgressParams {
                work_done_token: None,
            },
            partial_result_params: PartialResultParams {
                partial_result_token: None,
            },
        };

        // Index both files
        {
            let mut indexer = server.indexer.lock().await;
            indexer
                .process_file(file1_uri.clone(), file1_content)
                .unwrap();
            indexer
                .process_file(file2_uri.clone(), file2_content)
                .unwrap();
        }

        // Call handler
        let result = request::handle_goto_definition(&server, params).await;

        // Verify result
        match result {
            Ok(Some(GotoDefinitionResponse::Array(locations))) => {
                // Should find exactly 2 definitions
                assert_eq!(
                    locations.len(),
                    2,
                    "Expected 2 locations for class defined in multiple files"
                );

                // Make sure both files are included
                let uris: Vec<&Url> = locations.iter().map(|loc| &loc.uri).collect();
                assert!(
                    uris.contains(&&file1_uri) && uris.contains(&&file2_uri),
                    "Results should include both file URIs"
                );
            }
            Ok(Some(GotoDefinitionResponse::Scalar(_))) => {
                panic!("Expected multiple definitions, but got a single location");
            }
            Ok(Some(GotoDefinitionResponse::Link(_))) => {
                panic!("Expected multiple definitions, but got a Link response");
            }
            Ok(None) => {
                panic!("Expected definitions to be found, but none were returned");
            }
            Err(err) => {
                panic!("Definition handler returned an error: {:?}", err);
            }
        }
    }

    /// Helper function to ensure a fixture file exists with the correct content
    fn ensure_fixture_file_exists(file_path: &std::path::Path, content: &str) {
        if !file_path.exists() {
            std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
            std::fs::write(file_path, content).unwrap();
        } else {
            let existing_content = std::fs::read_to_string(file_path).unwrap_or_default();
            if existing_content != content {
                std::fs::write(file_path, content).unwrap();
            }
        }
    }
}
