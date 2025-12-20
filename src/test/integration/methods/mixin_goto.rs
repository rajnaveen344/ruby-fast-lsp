use crate::test::harness::setup_with_fixture;
use tower_lsp::lsp_types::Url;
use tower_lsp::lsp_types::{Position, TextDocumentIdentifier, TextDocumentPositionParams};

#[tokio::test]
async fn test_goto_method_in_mixin_includers_ordering() {
    // 1. Index ClassA first (includes M_A, but M_A unknown)
    let class_a_source = "
class ClassA
  include M_A
  def services
  end
end
";
    let (server, uri) = setup_with_fixture(class_a_source).await;

    // 2. Index M_A (M_A is now defined)
    let m_a_source = "
module M_A
  def foo
    services
  end
end
";
    let m_a_uri = Url::parse("file:///m_a.rb").unwrap();
    let item = tower_lsp::lsp_types::TextDocumentItem {
        uri: m_a_uri.clone(),
        language_id: "ruby".to_string(),
        version: 1,
        text: m_a_source.to_string(),
    };

    use tower_lsp::LanguageServer;
    server
        .did_open(tower_lsp::lsp_types::DidOpenTextDocumentParams {
            text_document: item,
        })
        .await;

    // 3. Trigger Goto Definition in M_A on 'services'
    // Line 3 (0-indexed), "services" is at column 4
    let position = Position {
        line: 3,
        character: 4,
    };

    let params = tower_lsp::lsp_types::GotoDefinitionParams {
        text_document_position_params: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier {
                uri: m_a_uri.clone(),
            },
            position,
        },
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
    };

    let result = server.goto_definition(params).await.unwrap();

    // 4. Verify we found ClassA#services
    match result {
        Some(tower_lsp::lsp_types::GotoDefinitionResponse::Array(locations)) => {
            assert!(!locations.is_empty(), "Should find definition in ClassA");
            let found = locations.iter().any(|loc| loc.uri == uri);
            assert!(found, "Should point to ClassA file");
        }
        _ => panic!("Expected Array of locations, got {:?}", result),
    }
}
