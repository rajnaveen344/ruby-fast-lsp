use std::sync::Arc;

use crate::capabilities::references;
use crate::handlers::helpers::{process_file_for_definitions, process_file_for_references};
use crate::server::RubyLanguageServer;
use crate::types::ruby_document::RubyDocument;
use tower_lsp::lsp_types::*;
use parking_lot::RwLock;

fn create_server() -> RubyLanguageServer {
    RubyLanguageServer::default()
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
    let _ = process_file_for_definitions(server, uri.clone());
    let _ = process_file_for_references(server, uri.clone(), include_local_vars);
    document
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
    let server = create_server();
    let uri = Url::parse("file:///dummy.rb").unwrap();
    open_file(&server, code, &uri);

    let references =
        references::find_references_at_position(&server, &uri, Position::new(4, 19)).await;

    assert_eq!(references.unwrap().len(), 2);

    // ConstantReadNode
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

    let server = create_server();
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
    let server = create_server();
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