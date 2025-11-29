use crate::capabilities::code_lens;
use crate::server::RubyLanguageServer;
use tower_lsp::lsp_types::*;
use tower_lsp::LanguageServer;

/// Helper to create a test server and process a file
async fn create_test_server_with_content(content: &str) -> (RubyLanguageServer, Url) {
    let server = RubyLanguageServer::default();
    let _ = server.initialize(InitializeParams::default()).await;

    // Create a test URI
    let uri = Url::parse("file:///test.rb").unwrap();

    // Open the document
    let params = DidOpenTextDocumentParams {
        text_document: TextDocumentItem {
            uri: uri.clone(),
            language_id: "ruby".into(),
            version: 1,
            text: content.to_string(),
        },
    };

    server.did_open(params).await;

    (server, uri)
}

#[tokio::test]
async fn test_basic_include() {
    // Create a test file with a module and a class that includes it
    let content = r#"
module MyModule
end

class MyClass
  include MyModule
end
"#;

    let (server, uri) = create_test_server_with_content(content).await;

    // Request CodeLens for the file
    let params = CodeLensParams {
        text_document: TextDocumentIdentifier { uri: uri.clone() },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    };

    let result = code_lens::handle_code_lens(&server, params).await;

    // Verify we got CodeLens results
    assert!(result.is_some());
    let lenses = result.unwrap();

    // Should have two CodeLens for MyModule (include + classes)
    assert_eq!(lenses.len(), 2);

    // Verify the labels
    let titles: Vec<String> = lenses
        .iter()
        .filter_map(|l| l.command.as_ref().map(|c| c.title.clone()))
        .collect();

    assert!(titles.contains(&"1 include".to_string()));
    assert!(titles.contains(&"1 class".to_string()));
}

#[tokio::test]
async fn test_basic_prepend() {
    let content = r#"
module MyModule
end

class MyClass
  prepend MyModule
end
"#;

    let (server, uri) = create_test_server_with_content(content).await;

    let params = CodeLensParams {
        text_document: TextDocumentIdentifier { uri: uri.clone() },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    };

    let result = code_lens::handle_code_lens(&server, params).await;

    assert!(result.is_some());
    let lenses = result.unwrap();
    // Should have two CodeLens for MyModule (prepend + classes)
    assert_eq!(lenses.len(), 2);

    let titles: Vec<String> = lenses
        .iter()
        .filter_map(|l| l.command.as_ref().map(|c| c.title.clone()))
        .collect();

    assert!(titles.contains(&"1 prepend".to_string()));
    assert!(titles.contains(&"1 class".to_string()));
}

#[tokio::test]
async fn test_basic_extend() {
    let content = r#"
module MyModule
end

class MyClass
  extend MyModule
end
"#;

    let (server, uri) = create_test_server_with_content(content).await;

    let params = CodeLensParams {
        text_document: TextDocumentIdentifier { uri: uri.clone() },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    };

    let result = code_lens::handle_code_lens(&server, params).await;

    assert!(result.is_some());
    let lenses = result.unwrap();
    // Should have two CodeLens for MyModule (extend + classes)
    assert_eq!(lenses.len(), 2);

    let titles: Vec<String> = lenses
        .iter()
        .filter_map(|l| l.command.as_ref().map(|c| c.title.clone()))
        .collect();

    assert!(titles.contains(&"1 extend".to_string()));
    assert!(titles.contains(&"1 class".to_string()));
}

#[tokio::test]
async fn test_multiple_categories() {
    let content = r#"
module MyModule
end

class MyClass
  include MyModule
end

class AnotherClass
  extend MyModule
end

module AnotherModule
  prepend MyModule
end
"#;

    let (server, uri) = create_test_server_with_content(content).await;

    let params = CodeLensParams {
        text_document: TextDocumentIdentifier { uri: uri.clone() },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    };

    let result = code_lens::handle_code_lens(&server, params).await;

    assert!(result.is_some());
    let lenses = result.unwrap();
    // Should have 4 CodeLens items for MyModule (include, prepend, extend, classes)
    assert_eq!(lenses.len(), 4);

    // Verify each CodeLens
    let titles: Vec<String> = lenses
        .iter()
        .filter_map(|l| l.command.as_ref().map(|c| c.title.clone()))
        .collect();

    // Check that we have all four types
    assert!(titles.contains(&"1 include".to_string()));
    assert!(titles.contains(&"1 prepend".to_string()));
    assert!(titles.contains(&"1 extend".to_string()));
    assert!(titles.contains(&"2 classes".to_string()));
}

#[tokio::test]
async fn test_no_usages() {
    let content = r#"
module MyModule
end

class MyClass
end
"#;

    let (server, uri) = create_test_server_with_content(content).await;

    let params = CodeLensParams {
        text_document: TextDocumentIdentifier { uri: uri.clone() },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    };

    let result = code_lens::handle_code_lens(&server, params).await;

    // Should have no CodeLens when there are no usages
    assert!(result.is_some());
    let lenses = result.unwrap();
    assert_eq!(lenses.len(), 0);
}

#[tokio::test]
async fn test_nested_module() {
    let content = r#"
module Outer
  module Inner
  end
end

class MyClass
  include Outer::Inner
end
"#;

    let (server, uri) = create_test_server_with_content(content).await;

    let params = CodeLensParams {
        text_document: TextDocumentIdentifier { uri: uri.clone() },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    };

    let result = code_lens::handle_code_lens(&server, params).await;

    assert!(result.is_some());
    let lenses = result.unwrap();

    // Should have CodeLens for Inner module
    assert!(lenses.len() >= 1);

    // Find the CodeLens for Inner module
    let inner_lens = lenses.iter().find(|l| {
        l.command
            .as_ref()
            .map(|c| c.title.contains("include"))
            .unwrap_or(false)
    });

    assert!(inner_lens.is_some());
}
