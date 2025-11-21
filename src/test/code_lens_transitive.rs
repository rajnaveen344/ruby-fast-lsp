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

    // Should have CodeLens for both module A and module B
    assert!(lenses.len() >= 2);

    // Find CodeLens for module A (should show it's used in B and transitively in MyClass)
    let module_a_lens = lenses.iter().find(|l| {
        l.range.start.line == 1 // module A is on line 1
    });

    assert!(module_a_lens.is_some());
    let lens = module_a_lens.unwrap();
    assert!(lens.command.is_some());
    let command = lens.command.as_ref().unwrap();

    // Should show: "1 include • 1 class"
    println!("Module A CodeLens: {}", command.title);
    assert!(command.title.contains("1 include"));
    assert!(command.title.contains("1 class"));

    // Find CodeLens for module B
    let module_b_lens = lenses.iter().find(|l| {
        l.range.start.line == 4 // module B is on line 4
    });

    assert!(module_b_lens.is_some());
    let lens_b = module_b_lens.unwrap();
    assert!(lens_b.command.is_some());
    let command_b = lens_b.command.as_ref().unwrap();

    // Should show: "1 include • 1 class"
    println!("Module B CodeLens: {}", command_b.title);
    assert!(command_b.title.contains("1 include"));
    assert!(command_b.title.contains("1 class"));
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
    let module_a_lens = lenses.iter().find(|l| {
        l.range.start.line == 1
    });

    assert!(module_a_lens.is_some());
    let lens = module_a_lens.unwrap();
    let command = lens.command.as_ref().unwrap();

    // Module A is used in B and Class3 directly, and transitively in Class1 and Class2
    // Should show: "2 include • 3 classes"
    println!("Module A CodeLens: {}", command.title);
    assert!(command.title.contains("2 include"));
    assert!(command.title.contains("3 classes"));
}

