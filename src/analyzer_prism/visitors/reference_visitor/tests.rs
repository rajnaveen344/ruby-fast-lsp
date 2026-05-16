use crate::capabilities::references;
use crate::indexer::entry::EntryKind;
use crate::indexer::file_processor::{FileProcessor, ProcessingOptions};
use crate::server::RubyLanguageServer;
use crate::types::ruby_document::RubyDocument;
use tower_lsp::lsp_types::*;
use tower_lsp::LanguageServer;

async fn create_server() -> RubyLanguageServer {
    let server = RubyLanguageServer::default();
    let workspace_uri = Url::parse("file:///tmp/test_workspace").unwrap();
    let init_params = InitializeParams {
        root_uri: Some(workspace_uri),
        ..Default::default()
    };
    let _ = server.initialize(init_params).await;
    server
}

fn open_file(server: &RubyLanguageServer, content: &str, uri: &Url) -> RubyDocument {
    open_file_with_options(server, content, uri, true)
}

fn open_file_with_options(
    server: &RubyLanguageServer,
    content: &str,
    uri: &Url,
    include_local_vars: bool,
) -> RubyDocument {
    let processor = FileProcessor::with_extension_registry(
        server.index_for_uri(uri),
        server.extension_registry.clone(),
    );
    processor
        .process_file(
            uri,
            content,
            server,
            ProcessingOptions {
                index_definitions: true,
                index_references: true,
                resolve_mixins: true,
                include_local_vars,
            },
        )
        .expect(
            "INVARIANT VIOLATED: reference visitor test file failed to process. \
             This is a bug because test helpers must exercise the same file processor path as LSP. \
             Fix: keep test content parseable and FileProcessor usable in tests.",
        );
    server
        .docs
        .lock()
        .get(uri)
        .expect(
            "INVARIANT VIOLATED: processed test file was not stored in server docs. \
             This is a bug because FileProcessor must publish the updated RubyDocument. \
             Fix: keep FileProcessor document cache writes active.",
        )
        .read()
        .clone()
}

#[tokio::test]
async fn test_reference_visitor() {
    let code = r#"
module Core
    module Platform
        module API
            module Users; end
        end

        module Something
            include API::Users
        end
    end
end
        "#;
    let server = create_server().await;
    let uri = Url::parse("file:///dummy.rb").unwrap();
    open_file(&server, code, &uri);

    // Test finding references to Users constant at position (4, 19)
    let references =
        references::find_references_at_position(&server, &uri, Position::new(4, 19)).await;
    assert_eq!(references.unwrap().len(), 2);

    // Test finding references to API constant at position (3, 15)
    let references =
        references::find_references_at_position(&server, &uri, Position::new(3, 15)).await;
    assert_eq!(references.unwrap().len(), 2);
}

#[tokio::test]
async fn test_local_variable_references() {
    let code = r#"
def my_method
  local_var = 42
  puts local_var  # Reference to local_var

  local_var.times do |i|
    puts "Count: #{i}"
  end

  local_var  # Another reference
end

my_method
        "#;

    let server = create_server().await;
    let uri = Url::parse("file:///local_vars.rb").unwrap();

    // Test with local vars enabled - should store in VariableScopes (NOT global index)
    open_file_with_options(&server, code, &uri, true);

    // Verify LocalVariables are in VariableScopes
    let doc_guard = server.docs.lock();
    let doc = doc_guard.get(&uri).expect("Document should exist");
    let doc_read = doc.read();

    let found_local_var = doc_read
        .variable_scopes()
        .get_all_definitions()
        .iter()
        .any(|(_, var)| var.name == "local_var");
    drop(doc_read);
    drop(doc_guard);

    assert!(
        found_local_var,
        "Should find local_var in VariableScopes when include_local_vars is true"
    );

    // Verify LocalVariables are NOT in global index
    let index = server.index_for_uri(&uri).lock_arc();
    let entries = index.file_entries(&uri);
    let local_var_entries: Vec<_> = entries
        .iter()
        .filter(|e| matches!(e.kind, EntryKind::LocalVariable(_)))
        .collect();

    assert!(
        local_var_entries.is_empty(),
        "LocalVariables should NOT be in global index (they're file-local)"
    );
}
