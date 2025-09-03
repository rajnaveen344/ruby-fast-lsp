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

    let mut visitor = IndexVisitor::new(server, uri);
    visitor.visit(&parse_result.node());
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

    let mut visitor = if include_local_vars {
        ReferenceVisitor::new(server, uri)
    } else {
        ReferenceVisitor::with_options(server, uri, false)
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

    // First test with local vars enabled
    open_file_with_options(&server, code, &uri, true);
    let index = server.index();
    let index_guard = index.lock();

    // Should find local variable references
    let local_var_refs: Vec<_> = index_guard
        .references
        .iter()
        .filter(|(fqn, _)| fqn.to_string().contains("local_var"))
        .collect();

    assert!(
        !local_var_refs.is_empty(),
        "Should find local variable references when include_local_vars is true"
    );

    // Now test with local vars disabled
    let server = create_server().await;
    open_file_with_options(&server, code, &uri, false);
    let index = server.index();
    let index_guard = index.lock();

    // Should not find any local variable references
    let local_var_refs: Vec<_> = index_guard
        .references
        .iter()
        .filter(|(fqn, _)| fqn.to_string().contains("local_var"))
        .collect();

    assert!(
        local_var_refs.is_empty(),
        "Should not find local variable references when include_local_vars is false"
    );
}
