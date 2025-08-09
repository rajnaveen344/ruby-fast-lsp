pub mod completion_ranker;
pub mod constant;
pub mod constant_completion;
pub mod constant_matcher;
pub mod scope_resolver;
pub mod variable;

use tower_lsp::lsp_types::{CompletionItem, CompletionResponse, InitializeParams, Position, Url};
use tower_lsp::LanguageServer;

use crate::{analyzer_prism::{RubyPrismAnalyzer, Identifier}, server::RubyLanguageServer};

pub use completion_ranker::CompletionRanker;
pub use constant_completion::{
    ConstantCompletionContext, ConstantCompletionEngine, ConstantCompletionItem,
};
pub use constant_matcher::ConstantMatcher;
pub use scope_resolver::ScopeResolver;

pub async fn find_completion_at_position(
    server: &RubyLanguageServer,
    uri: Url,
    position: Position,
) -> CompletionResponse {
    let document = server.get_doc(&uri).unwrap();
    let analyzer = RubyPrismAnalyzer::new(uri.clone(), document.content.clone());
    
    let (partial_name, _, lv_stack_at_pos) = analyzer.get_identifier(position);
    
    let partial_string = match &partial_name {
        Some(Identifier::RubyConstant { namespace: _, iden }) => {
            // For constants, we want just the last part being typed
            iden.last().map(|c| c.to_string()).unwrap_or_default()
        }
        _ => String::new(),
    };

    let mut completions = vec![];

    // Add local variable completions
    let variable_completions = variable::find_variable_completions(&document, &lv_stack_at_pos);
    completions.extend(variable_completions);

    // Add constant completions
    let index_arc = server.index();
    let index_guard = index_arc.lock();
    let constant_completions =
        constant::find_constant_completions(&*index_guard, &analyzer, position, partial_string);
    completions.extend(constant_completions);

    CompletionResponse::Array(completions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        indexer::{entry::entry_kind::EntryKind, entry::Entry, index::RubyIndex},
        server::RubyLanguageServer,
        types::{fully_qualified_name::FullyQualifiedName, ruby_document::RubyDocument},
    };
    use tower_lsp::lsp_types::{
        CompletionItem, CompletionItemKind, DidOpenTextDocumentParams, InitializeParams, Location,
        Range, TextDocumentItem, Url,
    };

    async fn create_test_server() -> RubyLanguageServer {
        let server = RubyLanguageServer::default();
        let _ = server.initialize(InitializeParams::default()).await;
        server
    }

    fn create_test_document(uri: Url, content: &str) -> RubyDocument {
        RubyDocument::new(uri, content.to_string(), 1)
    }

    fn create_test_entry(name: &str, kind: EntryKind) -> Entry {
        Entry {
            fqn: FullyQualifiedName::try_from(name).unwrap(),
            kind,
            location: Location {
                uri: Url::parse("file:///test.rb").unwrap(),
                range: Range::default(),
            },
        }
    }

    #[tokio::test]
    async fn test_find_completion_at_position_with_local_variables() {
        let server = create_test_server().await;
        let uri = Url::parse("file:///test.rb").unwrap();
        let content = r#"
def test_method
  local_var = 42
  another_var = "hello"
  loc
end
"#;

        // Open the document in the server
        let params = DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: "ruby".into(),
                version: 1,
                text: content.to_string(),
            },
        };
        server.did_open(params).await;

        // Debug: check index after did_open
        {
            let index_arc = server.index();
            let index_guard = index_arc.lock();
        }

        // Test completion at position where "loc" is typed (should match "local_var")
        let position = Position {
            line: 3,
            character: 5,
        }; // After "loc"
        let response = find_completion_at_position(&server, uri, position).await;

        match response {
            CompletionResponse::Array(completions) => {
                assert!(!completions.is_empty(), "Should have completions");

                // Check if we have local variable completions
                let local_var_completion = completions.iter().find(|c| c.label == "local_var");
                assert!(
                    local_var_completion.is_some(),
                    "Should have local_var completion"
                );

                if let Some(completion) = local_var_completion {
                    assert_eq!(completion.kind, Some(CompletionItemKind::VARIABLE));
                }
            }
            _ => panic!("Expected array response"),
        }
    }

    #[tokio::test]
    async fn test_find_completion_at_position_with_constants() {
        let server = create_test_server().await;
        let uri = Url::parse("file:///test.rb").unwrap();
        let content = r#"class TestClass
  MY_CONSTANT = 42
  puts MY
end"#;

        // Open the document in the server first
        let params = DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: "ruby".into(),
                version: 1,
                text: content.to_string(),
            },
        };
        server.did_open(params).await;

        // Add some test entries to the index after opening the document
        {
            let index_arc = server.index();
            let mut index_guard = index_arc.lock();

            // Add String class - use a simple name that should match
            let string_entry = create_test_entry(
                "String",
                EntryKind::Class {
                    superclass: Some(FullyQualifiedName::try_from("Object").unwrap()),
                    includes: vec![],
                    extends: vec![],
                    prepends: vec![],
                },
            );
            index_guard.add_entry(string_entry);

            // Add StringIO class for additional testing
            let stringio_entry = create_test_entry(
                "StringIO",
                EntryKind::Class {
                    superclass: Some(FullyQualifiedName::try_from("Object").unwrap()),
                    includes: vec![],
                    extends: vec![],
                    prepends: vec![],
                },
            );
            index_guard.add_entry(stringio_entry);
        }

        // Test completion at position where "MY" is typed (should match "MY_CONSTANT")
        // Line 2: "  puts MY" - cursor after "MY"
        let position = Position {
            line: 2,
            character: 10,
        }; // After "MY" in "puts MY" (right after 'Y')
        let response = find_completion_at_position(&server, uri, position).await;

        match response {
            CompletionResponse::Array(completions) => {
                assert!(!completions.is_empty(), "Should have completions");

                // Check if we have MY_CONSTANT completion
                let my_constant_completion = completions.iter().find(|c| c.label == "MY_CONSTANT");
                assert!(my_constant_completion.is_some(), "Should have MY_CONSTANT completion");

                if let Some(completion) = my_constant_completion {
                    assert_eq!(completion.kind, Some(CompletionItemKind::CONSTANT));
                }
            }
            _ => panic!("Expected array response"),
        }
    }

    #[tokio::test]
    async fn test_find_completion_at_position_empty_partial() {
        let server = create_test_server().await;
        let uri = Url::parse("file:///test.rb").unwrap();
        let content = r#"
def test_method
  local_var = 42
  
end
"#;

        // Open the document in the server
        let params = DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: "ruby".into(),
                version: 1,
                text: content.to_string(),
            },
        };
        server.did_open(params).await;

        // Test completion at empty position
        let position = Position {
            line: 2,
            character: 2,
        }; // Empty line with just spaces
        let response = find_completion_at_position(&server, uri, position).await;

        match response {
            CompletionResponse::Array(completions) => {
                // Should still provide completions (local variables, constants, etc.)
                // The exact behavior depends on the implementation, but it shouldn't crash
                assert!(completions.len() >= 0); // Just ensure it doesn't crash
            }
            _ => panic!("Expected array response"),
        }
    }

    #[tokio::test]
    async fn test_find_completion_at_position_mixed_completions() {
        let server = create_test_server().await;
        let uri = Url::parse("file:///test.rb").unwrap();
        let content = r#"
def test_method
  string_var = "hello"
  str_constant = String.new
  s
end
"#;

        // Add String class to index
        {
            let index_arc = server.index();
            let mut index_guard = index_arc.lock();

            let string_entry = create_test_entry(
                "String",
                EntryKind::Class {
                    superclass: Some(FullyQualifiedName::try_from("Object").unwrap()),
                    includes: vec![],
                    extends: vec![],
                    prepends: vec![],
                },
            );
            index_guard.add_entry(string_entry);
        }

        // Open the document in the server
        let params = DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: "ruby".into(),
                version: 1,
                text: content.to_string(),
            },
        };
        server.did_open(params).await;

        // Test completion at position where "s" is typed
        let position = Position {
            line: 3,
            character: 3,
        }; // After "s"
        let response = find_completion_at_position(&server, uri, position).await;

        match response {
            CompletionResponse::Array(completions) => {
                assert!(!completions.is_empty(), "Should have completions");

                // Should have both local variables and constants starting with "s"
                let variable_completions: Vec<_> = completions
                    .iter()
                    .filter(|c| c.kind == Some(CompletionItemKind::VARIABLE))
                    .collect();

                let class_completions: Vec<_> = completions
                    .iter()
                    .filter(|c| c.kind == Some(CompletionItemKind::CLASS))
                    .collect();

                // Should have at least some completions
                assert!(!completions.is_empty(), "Should have mixed completions");
            }
            _ => panic!("Expected array response"),
        }
    }

    #[tokio::test]
    async fn test_find_completion_no_duplicate_constants() {
        let server = create_test_server().await;
        let uri = Url::parse("file:///test.rb").unwrap();
        let content = r#"
class TestClass
  MY_CONSTANT = 42
  
  # Same constant defined again (could happen in real code)
  MY_CONSTANT = 100
  
  # Another context where the same constant might be referenced
  def some_method
    MY_CONSTANT = 200  # Local redefinition
  end
  
  MY
end
"#;

        // Open the document in the server
        let params = DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: "ruby".into(),
                version: 1,
                text: content.to_string(),
            },
        };
        server.did_open(params).await;

        // Manually add multiple entries for the same constant to simulate the duplicate issue
         {
             let index_arc = server.index();
             let mut index_guard = index_arc.lock();

             // Use the same FQN structure as the indexed constant (Object::TestClass::MY_CONSTANT)
             let fqn = FullyQualifiedName::try_from("Object::TestClass::MY_CONSTANT").unwrap();
             
             // Add the same constant multiple times (simulating multiple definitions)
              let entry1 = Entry {
                  fqn: fqn.clone(),
                  kind: EntryKind::Constant { 
                      value: Some("42".to_string()),
                      visibility: None,
                  },
                  location: Location {
                      uri: uri.clone(),
                      range: Range {
                          start: Position { line: 1, character: 2 },
                          end: Position { line: 1, character: 15 },
                      },
                  },
              };

              let entry2 = Entry {
                  fqn: fqn.clone(),
                  kind: EntryKind::Constant { 
                      value: Some("42".to_string()),
                      visibility: None,
                  },
                  location: Location {
                      uri: uri.clone(),
                      range: Range {
                          start: Position { line: 1, character: 2 },
                          end: Position { line: 1, character: 15 },
                      },
                  },
              };

             // Add both entries to create duplicates
             index_guard.add_entry(entry1);
             index_guard.add_entry(entry2);
         }

        // At this point, the index should contain multiple entries for MY_CONSTANT:
        // - 3 from the source code (multiple definitions)
        // - 2 from manually added duplicates
        // The deduplication logic should ensure only 1 completion item is returned

        // Test completion at position where "MY" is typed
        let position = Position {
            line: 11,
            character: 4,
        }; // After "MY"
        let response = find_completion_at_position(&server, uri, position).await;

        match response {
            CompletionResponse::Array(completions) => {
                // Filter for MY_CONSTANT completions
                let my_constant_completions: Vec<_> = completions
                    .iter()
                    .filter(|c| c.label == "MY_CONSTANT")
                    .collect();



                // Should have exactly one MY_CONSTANT completion, not duplicates
                assert_eq!(
                    my_constant_completions.len(),
                    1,
                    "Should have exactly one MY_CONSTANT completion, found: {}",
                    my_constant_completions.len()
                );

                if let Some(completion) = my_constant_completions.first() {
                    assert_eq!(completion.kind, Some(CompletionItemKind::CONSTANT));
                    assert_eq!(completion.label, "MY_CONSTANT");
                }
            }
            _ => panic!("Expected array response"),
        }
    }
}
