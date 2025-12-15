use std::sync::Arc;

use crate::capabilities::references;
use crate::server::RubyLanguageServer;
use crate::types::ruby_document::RubyDocument;
use parking_lot::RwLock;
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
    let document = RubyDocument::new(uri.clone(), content.to_string(), 0);
    server
        .docs
        .lock()
        .insert(uri.clone(), Arc::new(RwLock::new(document.clone())));

    // Process content directly instead of reading from filesystem
    process_content_for_definitions(server, uri.clone(), content);
    process_content_for_references(server, uri.clone(), content, include_local_vars);
    document
}

// Helper function to process content for definitions without reading from filesystem
fn process_content_for_definitions(server: &RubyLanguageServer, uri: Url, content: &str) {
    use crate::analyzer_prism::visitors::index_visitor::IndexVisitor;
    use ruby_prism::{parse, Visit};

    let parse_result = parse(content.as_bytes());
    let errors_count = parse_result.errors().count();
    if errors_count > 0 {
        println!("Parse errors in content: {} errors", errors_count);
        return;
    }

    let mut comment_ranges = Vec::new();
    for comment in parse_result.comments() {
        let loc = comment.location();
        comment_ranges.push((loc.start_offset(), loc.end_offset()));
    }
    let document = RubyDocument::new(uri.clone(), content.to_string(), 0);
    let mut visitor = IndexVisitor::new(server.index(), document, comment_ranges);
    visitor.visit(&parse_result.node());

    // Persist the document with LocalVariable entries back to server.docs
    server
        .docs
        .lock()
        .insert(uri, Arc::new(RwLock::new(visitor.document)));
}

// Helper function to process content for references without reading from filesystem
fn process_content_for_references(
    server: &RubyLanguageServer,
    uri: Url,
    content: &str,
    include_local_vars: bool,
) {
    use crate::analyzer_prism::visitors::reference_visitor::ReferenceVisitor;
    use ruby_prism::{parse, Visit};

    let parse_result = parse(content.as_bytes());
    let errors_count = parse_result.errors().count();
    if errors_count > 0 {
        println!("Parse errors in content: {} errors", errors_count);
        return;
    }

    // Create a temporary document since we're processing content directly
    let document = RubyDocument::new(uri.clone(), content.to_string(), 0);

    let mut visitor = if include_local_vars {
        ReferenceVisitor::new(server.index(), document)
    } else {
        ReferenceVisitor::with_options(server.index(), document, false)
    };
    visitor.visit(&parse_result.node());
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

    // Test with local vars enabled - should store in document.lvars (NOT global index)
    open_file_with_options(&server, code, &uri, true);

    // Verify LocalVariables are in document.lvars
    let doc_guard = server.docs.lock();
    let doc = doc_guard.get(&uri).expect("Document should exist");
    let doc_read = doc.read();

    // Check that the document has local variable entries
    // Method body scope starts at a specific offset
    let mut found_local_var = false;
    for scope_id in 0..100 {
        if let Some(entries) = doc_read.get_local_var_entries(scope_id) {
            for entry in entries {
                if let crate::indexer::entry::EntryKind::LocalVariable { name, .. } = &entry.kind {
                    if name == "local_var" {
                        found_local_var = true;
                        break;
                    }
                }
            }
        }
    }
    drop(doc_read);
    drop(doc_guard);

    assert!(
        found_local_var,
        "Should find local_var in document.lvars when include_local_vars is true"
    );

    // Verify LocalVariables are NOT in global index
    let index = server.index();
    let index_guard = index.lock();
    let entries = index_guard.get_entries_for_uri(&uri);
    let local_var_entries: Vec<_> = entries
        .iter()
        .filter(|e| {
            matches!(
                e.kind,
                crate::indexer::entry::EntryKind::LocalVariable { .. }
            )
        })
        .collect();

    assert!(
        local_var_entries.is_empty(),
        "LocalVariables should NOT be in global index (they're file-local)"
    );
}
