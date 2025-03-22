#[cfg(test)]
mod tests {
    use lsp_types::{
        GotoDefinitionParams, GotoDefinitionResponse, Location, Position, ReferenceContext,
        ReferenceParams, TextDocumentIdentifier, TextDocumentPositionParams, Url,
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
}
