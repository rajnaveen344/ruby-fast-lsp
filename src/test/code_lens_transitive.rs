use crate::capabilities::code_lens;
use crate::server::RubyLanguageServer;
use tower_lsp::lsp_types::*;
use tower_lsp::LanguageServer;

#[tokio::test]
async fn test_transitive_module_usage() {
    let content = r#"
module A
end

module B
  include A
end

class MyClass
  include B
end
"#;

    let server = RubyLanguageServer::default();
    let _ = server.initialize(InitializeParams::default()).await;

    let uri = Url::parse("file:///test.rb").unwrap();

    let params = DidOpenTextDocumentParams {
        text_document: TextDocumentItem {
            uri: uri.clone(),
            language_id: "ruby".into(),
            version: 1,
            text: content.to_string(),
        },
    };

    server.did_open(params).await;

    // Request CodeLens for the file
    let params = CodeLensParams {
        text_document: TextDocumentIdentifier { uri: uri.clone() },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    };

    let result = code_lens::handle_code_lens(&server, params).await;

    assert!(result.is_some());
    let lenses = result.unwrap();

    // Should have CodeLens for both module A and module B (2 modules × 2 CodeLens each = 4)
    assert!(lenses.len() >= 4);

    // Find CodeLens for module A (should show it's used in B and transitively in MyClass)
    let module_a_lenses: Vec<_> = lenses.iter().filter(|l| {
        l.range.start.line == 1 // module A is on line 1
    }).collect();

    assert!(module_a_lenses.len() >= 2); // Should have "include" and "classes" CodeLens

    // Get all titles for module A
    let module_a_titles: Vec<String> = module_a_lenses
        .iter()
        .filter_map(|l| l.command.as_ref().map(|c| c.title.clone()))
        .collect();

    println!("Module A CodeLens: {:?}", module_a_titles);

    // Should have separate CodeLens for "1 include" and "1 class"
    assert!(module_a_titles.iter().any(|t| t == "1 include"));
    assert!(module_a_titles.iter().any(|t| t == "1 class"));

    // Find CodeLens for module B
    let module_b_lenses: Vec<_> = lenses.iter().filter(|l| {
        l.range.start.line == 4 // module B is on line 4
    }).collect();

    assert!(module_b_lenses.len() >= 2); // Should have "include" and "classes" CodeLens

    let module_b_titles: Vec<String> = module_b_lenses
        .iter()
        .filter_map(|l| l.command.as_ref().map(|c| c.title.clone()))
        .collect();

    println!("Module B CodeLens: {:?}", module_b_titles);

    // Should have separate CodeLens for "1 include" and "1 class"
    assert!(module_b_titles.iter().any(|t| t == "1 include"));
    assert!(module_b_titles.iter().any(|t| t == "1 class"));
}

#[tokio::test]
async fn test_multiple_transitive_classes() {
    let content = r#"
module A
end

module B
  include A
end

class Class1
  include B
end

class Class2
  include B
end

class Class3
  include A
end
"#;

    let server = RubyLanguageServer::default();
    let _ = server.initialize(InitializeParams::default()).await;

    let uri = Url::parse("file:///test.rb").unwrap();

    let params = DidOpenTextDocumentParams {
        text_document: TextDocumentItem {
            uri: uri.clone(),
            language_id: "ruby".into(),
            version: 1,
            text: content.to_string(),
        },
    };

    server.did_open(params).await;

    let params = CodeLensParams {
        text_document: TextDocumentIdentifier { uri: uri.clone() },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    };

    let result = code_lens::handle_code_lens(&server, params).await;

    assert!(result.is_some());
    let lenses = result.unwrap();

    // Find CodeLens for module A
    let module_a_lenses: Vec<_> = lenses.iter().filter(|l| {
        l.range.start.line == 1
    }).collect();

    assert!(module_a_lenses.len() >= 2); // Should have "include" and "classes" CodeLens

    let module_a_titles: Vec<String> = module_a_lenses
        .iter()
        .filter_map(|l| l.command.as_ref().map(|c| c.title.clone()))
        .collect();

    // Module A is used in B and Class3 directly, and transitively in Class1 and Class2
    // Should have separate CodeLens for "2 include" and "3 classes"
    println!("Module A CodeLens: {:?}", module_a_titles);
    assert!(module_a_titles.iter().any(|t| t == "2 include"));
    assert!(module_a_titles.iter().any(|t| t == "3 classes"));
}

