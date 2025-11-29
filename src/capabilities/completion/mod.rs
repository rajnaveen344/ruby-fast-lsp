pub mod completion_ranker;
pub mod constant;
pub mod constant_completion;
pub mod constant_matcher;
pub mod method;
pub mod scope_resolver;
pub mod snippets;
pub mod variable;

use tower_lsp::lsp_types::{
    CompletionContext, CompletionResponse, CompletionTriggerKind, Position, Url,
};

use crate::{
    analyzer_prism::{Identifier, ReceiverKind, RubyPrismAnalyzer},
    server::RubyLanguageServer,
};

pub use completion_ranker::CompletionRanker;
pub use constant_completion::{
    ConstantCompletionContext, ConstantCompletionEngine, ConstantCompletionItem,
};
pub use constant_matcher::ConstantMatcher;
pub use scope_resolver::ScopeResolver;
pub use snippets::RubySnippets;

pub async fn find_completion_at_position(
    server: &RubyLanguageServer,
    uri: Url,
    position: Position,
    context: Option<CompletionContext>,
) -> CompletionResponse {
    // Use unified document access to ensure we get the latest in-memory content
    let document = match server.get_doc(&uri) {
        Some(doc) => doc,
        None => {
            // Return empty completion response if document not found
            return CompletionResponse::Array(vec![]);
        }
    };
    let analyzer = RubyPrismAnalyzer::new(uri.clone(), document.content.clone());

    // Check if completion was triggered by a trigger character
    let is_trigger_character = context
        .as_ref()
        .map(|ctx| ctx.trigger_kind == CompletionTriggerKind::TRIGGER_CHARACTER)
        .unwrap_or(false);

    let trigger_character = context
        .as_ref()
        .and_then(|ctx| ctx.trigger_character.as_ref())
        .map(|s| s.as_str());

    let line_text = document
        .content
        .lines()
        .nth(position.line as usize)
        .unwrap_or("");

    let (partial_name, _, lv_stack_at_pos) = analyzer.get_identifier(position);

    // Check if we're in a :: (scope resolution) context
    let is_scope_resolution_context = if is_trigger_character && trigger_character == Some(":") {
        // Look at the text before the cursor to see if we have "::"
        let line_text = document
            .content
            .lines()
            .nth(position.line as usize)
            .unwrap_or("");
        let char_pos = position.character as usize;

        // Check if there's a ':' character immediately before the current position
        // This means we're completing after "::" (user typed :: and cursor is after the second :)
        char_pos >= 2
            && line_text.chars().nth(char_pos - 1) == Some(':')
            && line_text.chars().nth(char_pos - 2) == Some(':')
    } else {
        false
    };

    // Enhanced partial string extraction for better constant completion
    let partial_string = match &partial_name {
        Some(Identifier::RubyConstant { namespace: _, iden }) => {
            if is_scope_resolution_context {
                // For scope resolution context (A::), we need to pass the full qualified name
                // The 'iden' field contains the constant being referenced (A), which is what we want
                // as the namespace for finding nested modules
                let namespace_str = if iden.is_empty() {
                    String::new()
                } else {
                    iden.iter()
                        .map(|ns| ns.to_string())
                        .collect::<Vec<_>>()
                        .join("::")
                };

                if !namespace_str.is_empty() {
                    // Return "A::" so the engine can parse namespace "A" and partial ""
                    format!("{}::", namespace_str)
                } else {
                    // Top-level scope resolution (::)
                    "::".to_string()
                }
            } else {
                // For normal constant completion, we want just the last part being typed
                iden.last().map(|c| c.to_string()).unwrap_or_default()
            }
        }
        Some(Identifier::RubyMethod {
            namespace: _,
            receiver_kind: _,
            receiver: _,
            iden,
        }) => {
            // For method completion, extract the method name being typed
            iden.to_string()
        }
        None => {
            if is_scope_resolution_context {
                // For top-level scope resolution (::) or when analyzer doesn't detect a constant
                // Extract from line text as fallback
                let line_text = document
                    .content
                    .lines()
                    .nth(position.line as usize)
                    .unwrap_or("");
                let char_pos = position.character as usize;

                // Look backwards from the current position to find the namespace
                if char_pos >= 2 {
                    let before_colon = &line_text[..char_pos.saturating_sub(2)];
                    if let Some(start) =
                        before_colon.rfind(|c: char| !c.is_alphanumeric() && c != '_' && c != ':')
                    {
                        let namespace = &before_colon[start + 1..];
                        if !namespace.is_empty()
                            && namespace.chars().all(|c| c.is_alphanumeric() || c == '_')
                        {
                            format!("{}::", namespace)
                        } else {
                            "::".to_string()
                        }
                    } else {
                        // The namespace starts at the beginning of the line
                        let namespace = before_colon.trim();
                        if !namespace.is_empty()
                            && namespace.chars().all(|c| c.is_alphanumeric() || c == '_')
                        {
                            format!("{}::", namespace)
                        } else {
                            "::".to_string()
                        }
                    }
                } else {
                    "::".to_string()
                }
            } else {
                // Fallback: extract partial word from current line for snippet completion
                let line_text = document
                    .content
                    .lines()
                    .nth(position.line as usize)
                    .unwrap_or("");
                let char_pos = position.character as usize;

                // Look backwards from the current position to find the start of the current word
                let before_cursor = &line_text[..char_pos.min(line_text.len())];
                if let Some(start) = before_cursor.rfind(|c: char| !c.is_alphanumeric() && c != '_')
                {
                    before_cursor[start + 1..].to_string()
                } else {
                    before_cursor.trim().to_string()
                }
            }
        }
        _ => {
            if is_scope_resolution_context {
                "::".to_string()
            } else {
                String::new()
            }
        }
    };

    let mut completions = vec![];

    // Check if we're in a method call context (after a dot)
    let is_dot_trigger = is_trigger_character && trigger_character == Some(".");

    // Also detect method call context by looking for a dot before the cursor
    let line_has_dot = {
        let line = document
            .content
            .lines()
            .nth(position.line as usize)
            .unwrap_or("");
        let char_pos = position.character as usize;
        // Safely get substring before cursor
        let before_cursor = if char_pos <= line.len() {
            &line[..char_pos]
        } else {
            line
        };
        // Check if there's a dot followed by optional method name chars
        before_cursor.contains('.')
            && before_cursor
                .rfind('.')
                .map(|dot_pos| {
                    let after_dot = &before_cursor[dot_pos + 1..];
                    after_dot.chars().all(|c| c.is_alphanumeric() || c == '_')
                })
                .unwrap_or(false)
    };

    let is_method_call_context = is_dot_trigger
        || line_has_dot
        || matches!(
            &partial_name,
            Some(Identifier::RubyMethod {
                receiver_kind: ReceiverKind::Expr,
                ..
            })
        );

    // Prioritize constant completions when in scope resolution context (::)
    if is_scope_resolution_context {
        // Focus on constant completions for scope resolution
        let index_arc = server.index();
        let index_guard = index_arc.lock();
        let constant_completions =
            constant::find_constant_completions(&index_guard, &analyzer, position, partial_string);
        completions.extend(constant_completions);
    } else if is_method_call_context {
        // Method call context: provide type-aware method completions using CFG
        let index_arc = server.index();

        // Get receiver type using CFG-based type inference
        let receiver_type =
            get_receiver_type_from_cfg(server, &uri, &document.content, position, &partial_name);

        if let Some(receiver_type) = receiver_type {
            // Determine if this is a class method call (receiver is a constant)
            let is_class_method = matches!(
                &partial_name,
                Some(Identifier::RubyMethod {
                    receiver_kind: ReceiverKind::Constant,
                    ..
                })
            );

            let method_completions = method::find_method_completions(
                &index_arc,
                &receiver_type,
                &partial_string,
                is_class_method,
            );
            completions.extend(method_completions);
        }
    } else {
        // Normal completion: include variables, constants, and snippets

        // Add local variable completions
        let variable_completions = variable::find_variable_completions(&document, &lv_stack_at_pos);
        completions.extend(variable_completions);

        // Add constant completions
        let index_arc = server.index();
        let index_guard = index_arc.lock();
        let constant_completions = constant::find_constant_completions(
            &index_guard,
            &analyzer,
            position,
            partial_string.clone(),
        );
        completions.extend(constant_completions);

        // Add snippet completions with context awareness
        // Only include snippets if not triggered by a dot character
        if !is_dot_trigger {
            let snippet_context = snippets::RubySnippets::determine_context_with_position(
                &partial_name,
                line_text,
                position.character,
            );

            let snippet_completions =
                RubySnippets::get_matching_snippets_with_context(&partial_string, snippet_context);

            completions.extend(snippet_completions);
        }
    }

    CompletionResponse::Array(completions)
}

/// Get the receiver type using CFG-based type inference
///
/// This function determines the type of the receiver expression at a completion position.
/// It handles:
/// - Constant receivers (e.g., `User.find`) -> ClassReference
/// - Literal receivers (e.g., `"hello".`, `123.`) -> direct type
/// - Variable receivers (e.g., `name.`) -> CFG narrowed type (works for both methods and top-level)
fn get_receiver_type_from_cfg(
    server: &RubyLanguageServer,
    uri: &Url,
    content: &str,
    position: Position,
    identifier: &Option<Identifier>,
) -> Option<crate::type_inference::ruby_type::RubyType> {
    use crate::type_inference::ruby_type::RubyType;
    use crate::types::fully_qualified_name::FullyQualifiedName;
    use crate::types::ruby_namespace::RubyConstant;

    // If we have a method identifier with constant receiver, use it directly
    if let Some(Identifier::RubyMethod {
        receiver_kind: ReceiverKind::Constant,
        receiver: Some(recv_parts),
        ..
    }) = identifier
    {
        let fqn = FullyQualifiedName::Constant(recv_parts.clone());
        return Some(RubyType::ClassReference(fqn));
    }

    // Extract receiver text from the line
    let line = content.lines().nth(position.line as usize)?;
    let char_pos = position.character as usize;

    let before_cursor = if char_pos <= line.len() {
        &line[..char_pos]
    } else {
        line
    };

    let dot_pos = before_cursor.rfind('.')?;
    let before_dot = &before_cursor[..dot_pos];

    // Extract only the last token (word) before the dot
    // This handles cases like "puts b." where we want just "b"
    let receiver_text = before_dot
        .rsplit(|c: char| !c.is_alphanumeric() && c != '_' && c != '@' && c != '$')
        .next()
        .map(|s| s.trim())
        .unwrap_or("")
        .trim();

    if receiver_text.is_empty() {
        return None;
    }

    // Handle literals directly (no CFG needed - these are unambiguous)
    if let Some(literal_type) = infer_literal_type(receiver_text) {
        return Some(literal_type);
    }

    // Handle constant references (class/module names)
    if receiver_text
        .chars()
        .next()
        .map(|c| c.is_uppercase())
        .unwrap_or(false)
    {
        if let Ok(constant) = RubyConstant::new(receiver_text) {
            return Some(RubyType::ClassReference(FullyQualifiedName::Constant(
                vec![constant],
            )));
        }
    }

    // For variables, use CFG-based type narrowing
    // CFG handles both method-level and top-level code
    if is_variable_name(receiver_text) {
        let offset = position_to_offset(content, position);
        return server
            .type_narrowing
            .get_narrowed_type(uri, receiver_text, offset);
    }

    None
}

/// Infer type from a literal expression
fn infer_literal_type(text: &str) -> Option<crate::type_inference::ruby_type::RubyType> {
    use crate::type_inference::ruby_type::RubyType;

    // String literal
    if text.starts_with('"') || text.starts_with('\'') {
        return Some(RubyType::string());
    }

    // Symbol literal
    if text.starts_with(':') {
        return Some(RubyType::symbol());
    }

    // Array literal
    if text.starts_with('[') {
        return Some(RubyType::Array(vec![RubyType::Any]));
    }

    // Hash literal
    if text.starts_with('{') {
        return Some(RubyType::Hash(vec![RubyType::Any], vec![RubyType::Any]));
    }

    // Integer literal (must check before float)
    if !text.is_empty() && text.chars().all(|c| c.is_ascii_digit() || c == '_') {
        return Some(RubyType::integer());
    }

    // Float literal
    if text.contains('.')
        && text
            .chars()
            .all(|c| c.is_ascii_digit() || c == '_' || c == '.')
    {
        return Some(RubyType::float());
    }

    None
}

/// Check if text is a valid Ruby variable name (lowercase identifier)
fn is_variable_name(text: &str) -> bool {
    if text.is_empty() {
        return false;
    }

    let first_char = text.chars().next().unwrap();
    if !first_char.is_lowercase() && first_char != '_' {
        return false;
    }

    text.chars().all(|c| c.is_alphanumeric() || c == '_')
}

/// Convert LSP Position to byte offset
fn position_to_offset(content: &str, position: Position) -> usize {
    let mut offset = 0;
    for (line_num, line) in content.lines().enumerate() {
        if line_num == position.line as usize {
            offset += position.character as usize;
            break;
        }
        offset += line.len() + 1;
    }
    offset
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        indexer::entry::{entry_kind::EntryKind, Entry, MixinRef},
        server::RubyLanguageServer,
        types::{fully_qualified_name::FullyQualifiedName, ruby_namespace::RubyConstant},
    };
    use tower_lsp::{
        lsp_types::{
            CompletionItemKind, CompletionTriggerKind, DidOpenTextDocumentParams, InitializeParams,
            InsertTextFormat, Location, Range, TextDocumentItem, Url,
        },
        LanguageServer,
    };

    async fn create_test_server() -> RubyLanguageServer {
        let server = RubyLanguageServer::default();
        let _ = server.initialize(InitializeParams::default()).await;
        server
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

        // Explicitly call processing function to ensure local variables are indexed
        use crate::handlers::helpers::{process_definitions, DefinitionOptions};
        use crate::types::ruby_document::RubyDocument;
        use parking_lot::RwLock;
        use std::sync::Arc;

        // Create and store the document in cache first
        let document = RubyDocument::new(uri.clone(), content.to_string(), 1);
        let doc_arc = Arc::new(RwLock::new(document));
        server.docs.lock().insert(uri.clone(), doc_arc);

        // Process for definitions (which includes local variables)
        let _ = process_definitions(&server, uri.clone(), content, DefinitionOptions::default());

        // Give a small delay to ensure processing completes
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Test completion at position where "loc" is typed (should match "local_var")
        let position = Position {
            line: 3,
            character: 5,
        }; // After "loc"
        let response = find_completion_at_position(&server, uri, position, None).await;

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
                    superclass: Some(MixinRef {
                        parts: vec![RubyConstant::new("Object").unwrap()],
                        absolute: false,
                    }),
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
                    superclass: Some(MixinRef {
                        parts: vec![RubyConstant::new("Object").unwrap()],
                        absolute: false,
                    }),
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
        let response = find_completion_at_position(&server, uri, position, None).await;

        match response {
            CompletionResponse::Array(completions) => {
                assert!(!completions.is_empty(), "Should have completions");

                // Check if we have MY_CONSTANT completion
                let my_constant_completion = completions.iter().find(|c| c.label == "MY_CONSTANT");
                assert!(
                    my_constant_completion.is_some(),
                    "Should have MY_CONSTANT completion"
                );

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
        let response = find_completion_at_position(&server, uri, position, None).await;

        match response {
            CompletionResponse::Array(_) => {
                // Should still provide completions (local variables, constants, etc.)
                // The exact behavior depends on the implementation, but it shouldn't crash
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
                    superclass: Some(MixinRef {
                        parts: vec![RubyConstant::new("Object").unwrap()],
                        absolute: false,
                    }),
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
        let response = find_completion_at_position(&server, uri, position, None).await;

        match response {
            CompletionResponse::Array(completions) => {
                assert!(!completions.is_empty(), "Should have completions");

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

            // Use the same FQN structure as the indexed constant (TestClass::MY_CONSTANT)
            let fqn = FullyQualifiedName::try_from("TestClass::MY_CONSTANT").unwrap();

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
                        start: Position {
                            line: 1,
                            character: 2,
                        },
                        end: Position {
                            line: 1,
                            character: 15,
                        },
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
                        start: Position {
                            line: 1,
                            character: 2,
                        },
                        end: Position {
                            line: 1,
                            character: 15,
                        },
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
        let response = find_completion_at_position(&server, uri, position, None).await;

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

    #[tokio::test]
    async fn test_trigger_character_completion() {
        let server = create_test_server().await;
        let uri = Url::parse("file:///test.rb").unwrap();
        let content = r#"
class TestClass
  def test_method
    ::
  end
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

        // Add some top-level constants to the index after opening the document
        {
            let index_arc = server.index();
            let mut index_guard = index_arc.lock();

            // Create top-level entries
            let string_entry = Entry {
                fqn: FullyQualifiedName::try_from("String").unwrap(),
                kind: EntryKind::Class {
                    superclass: Some(MixinRef {
                        parts: vec![RubyConstant::new("Object").unwrap()],
                        absolute: false,
                    }),
                    includes: vec![],
                    extends: vec![],
                    prepends: vec![],
                },
                location: Location {
                    uri: uri.clone(),
                    range: Range::default(),
                },
            };
            index_guard.add_entry(string_entry);

            let array_entry = Entry {
                fqn: FullyQualifiedName::try_from("Array").unwrap(),
                kind: EntryKind::Class {
                    superclass: Some(MixinRef {
                        parts: vec![RubyConstant::new("Object").unwrap()],
                        absolute: false,
                    }),
                    includes: vec![],
                    extends: vec![],
                    prepends: vec![],
                },
                location: Location {
                    uri: uri.clone(),
                    range: Range::default(),
                },
            };
            index_guard.add_entry(array_entry);
        }

        // Test completion triggered by ":" character (for "::")
        let position = Position {
            line: 3,
            character: 6,
        }; // After "::" (line 3: "    ::" - position 6 is after the second colon)

        // Create completion context with trigger character
        let context = Some(tower_lsp::lsp_types::CompletionContext {
            trigger_kind: CompletionTriggerKind::TRIGGER_CHARACTER,
            trigger_character: Some(":".to_string()),
        });

        let response = find_completion_at_position(&server, uri, position, context).await;

        match response {
            CompletionResponse::Array(completions) => {
                assert!(
                    !completions.is_empty(),
                    "Should have completions when triggered by ':'"
                );

                // Should prioritize constant completions when triggered by ":"
                let constant_completions: Vec<_> = completions
                    .iter()
                    .filter(|c| {
                        matches!(
                            c.kind,
                            Some(CompletionItemKind::CLASS) | Some(CompletionItemKind::CONSTANT)
                        )
                    })
                    .collect();

                assert!(
                    !constant_completions.is_empty(),
                    "Should have constant/class completions when triggered by ':'"
                );

                // Verify we get top-level constants like String and Array
                let labels: Vec<&str> = completions.iter().map(|c| c.label.as_str()).collect();
                assert!(labels.contains(&"String"), "Should include String class");
                assert!(labels.contains(&"Array"), "Should include Array class");
            }
            _ => panic!("Expected array response"),
        }
    }

    #[tokio::test]
    async fn test_nested_module_scope_resolution() {
        let server = create_test_server().await;
        let uri = Url::parse("file:///nested_test.rb").unwrap();
        let content = r#"
module OuterModule
  module MiddleModule
    class InnerClass
      def test_method
        ::
      end
    end

    module DeepModule
      class DeepClass
      end
    end
  end

  class OuterClass
  end
end

module AnotherModule
  class AnotherClass
  end
end
"#;

        let params = DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: "ruby".into(),
                version: 1,
                text: content.to_string(),
            },
        };
        server.did_open(params).await;

        // Add complex nested module structure to the index
        {
            let index_arc = server.index();
            let mut index_guard = index_arc.lock();

            // Top-level modules and classes
            let outer_module_entry = Entry {
                fqn: FullyQualifiedName::try_from("OuterModule").unwrap(),
                kind: EntryKind::Module {
                    includes: vec![],
                    extends: vec![],
                    prepends: vec![],
                },
                location: Location {
                    uri: uri.clone(),
                    range: Range::default(),
                },
            };
            index_guard.add_entry(outer_module_entry);

            let another_module_entry = Entry {
                fqn: FullyQualifiedName::try_from("AnotherModule").unwrap(),
                kind: EntryKind::Module {
                    includes: vec![],
                    extends: vec![],
                    prepends: vec![],
                },
                location: Location {
                    uri: uri.clone(),
                    range: Range::default(),
                },
            };
            index_guard.add_entry(another_module_entry);

            // Nested modules and classes
            let middle_module_entry = Entry {
                fqn: FullyQualifiedName::try_from("OuterModule::MiddleModule").unwrap(),
                kind: EntryKind::Module {
                    includes: vec![],
                    extends: vec![],
                    prepends: vec![],
                },
                location: Location {
                    uri: uri.clone(),
                    range: Range::default(),
                },
            };
            index_guard.add_entry(middle_module_entry);

            let inner_class_entry = Entry {
                fqn: FullyQualifiedName::try_from("OuterModule::MiddleModule::InnerClass").unwrap(),
                kind: EntryKind::Class {
                    superclass: Some(MixinRef {
                        parts: vec![RubyConstant::new("Object").unwrap()],
                        absolute: false,
                    }),
                    includes: vec![],
                    extends: vec![],
                    prepends: vec![],
                },
                location: Location {
                    uri: uri.clone(),
                    range: Range::default(),
                },
            };
            index_guard.add_entry(inner_class_entry);

            // Add some built-in Ruby classes
            let string_entry = Entry {
                fqn: FullyQualifiedName::try_from("String").unwrap(),
                kind: EntryKind::Class {
                    superclass: Some(MixinRef {
                        parts: vec![RubyConstant::new("Object").unwrap()],
                        absolute: false,
                    }),
                    includes: vec![],
                    extends: vec![],
                    prepends: vec![],
                },
                location: Location {
                    uri: uri.clone(),
                    range: Range::default(),
                },
            };
            index_guard.add_entry(string_entry);

            let hash_entry = Entry {
                fqn: FullyQualifiedName::try_from("Hash").unwrap(),
                kind: EntryKind::Class {
                    superclass: Some(MixinRef {
                        parts: vec![RubyConstant::new("Object").unwrap()],
                        absolute: false,
                    }),
                    includes: vec![],
                    extends: vec![],
                    prepends: vec![],
                },
                location: Location {
                    uri: uri.clone(),
                    range: Range::default(),
                },
            };
            index_guard.add_entry(hash_entry);
        }

        // Test scope resolution from within a deeply nested class
        let position = Position {
            line: 5,
            character: 10,
        }; // After "::" inside InnerClass

        let context = Some(tower_lsp::lsp_types::CompletionContext {
            trigger_kind: CompletionTriggerKind::TRIGGER_CHARACTER,
            trigger_character: Some(":".to_string()),
        });

        let response = find_completion_at_position(&server, uri, position, context).await;

        match response {
            CompletionResponse::Array(completions) => {
                // Should include all top-level modules and classes when using ::
                let completion_labels: Vec<&str> =
                    completions.iter().map(|c| c.label.as_str()).collect();

                // Built-in classes should be available
                assert!(completion_labels.contains(&"String"));
                assert!(completion_labels.contains(&"Hash"));

                // Top-level modules should be available
                assert!(completion_labels.contains(&"OuterModule"));
                assert!(completion_labels.contains(&"AnotherModule"));
            }
            _ => panic!("Expected array response"),
        }
    }

    #[tokio::test]
    async fn test_scope_resolution_edge_cases() {
        let server = create_test_server().await;
        let uri = Url::parse("file:///edge_cases.rb").unwrap();

        // Test various edge cases for scope resolution
        let test_cases = [
            (
                r#"
class TestClass
  def method
::
  end
end
"#,
                Position {
                    line: 3,
                    character: 2,
                },
                "beginning of line",
            ),
            // Case 2: :: with spaces around
            (
                r#"
class TestClass
  def method
    ::
  end
end
"#,
                Position {
                    line: 3,
                    character: 6,
                },
                "with trailing space",
            ),
            // Case 3: :: in a complex expression
            (
                r#"
class TestClass
  def method
    result = ::
  end
end
"#,
                Position {
                    line: 3,
                    character: 15,
                },
                "in assignment",
            ),
            // Case 4: :: in method call chain
            (
                r#"
class TestClass
  def method
    obj.method.::
  end
end
"#,
                Position {
                    line: 3,
                    character: 17,
                },
                "in method chain",
            ),
            // Case 5: :: with partial constant name
            (
                r#"
class TestClass
  def method
    ::Str
  end
end
"#,
                Position {
                    line: 3,
                    character: 9,
                },
                "with partial constant",
            ),
        ];

        for (i, (content, position, description)) in test_cases.iter().enumerate() {
            let params = DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri: uri.clone(),
                    language_id: "ruby".into(),
                    version: i as i32 + 1,
                    text: content.to_string(),
                },
            };

            server.did_open(params).await;

            // Add some test entries to the index
            {
                let index_arc = server.index();
                let mut index_guard = index_arc.lock();

                let string_entry = Entry {
                    fqn: FullyQualifiedName::try_from("String").unwrap(),
                    kind: EntryKind::Class {
                        superclass: Some(MixinRef {
                            parts: vec![RubyConstant::new("Object").unwrap()],
                            absolute: false,
                        }),
                        includes: vec![],
                        extends: vec![],
                        prepends: vec![],
                    },
                    location: Location {
                        uri: uri.clone(),
                        range: Range::default(),
                    },
                };
                index_guard.add_entry(string_entry);

                let test_class_entry = Entry {
                    fqn: FullyQualifiedName::try_from("TestClass").unwrap(),
                    kind: EntryKind::Class {
                        superclass: Some(MixinRef {
                            parts: vec![RubyConstant::new("Object").unwrap()],
                            absolute: false,
                        }),
                        includes: vec![],
                        extends: vec![],
                        prepends: vec![],
                    },
                    location: Location {
                        uri: uri.clone(),
                        range: Range::default(),
                    },
                };
                index_guard.add_entry(test_class_entry);
            }

            let context = Some(tower_lsp::lsp_types::CompletionContext {
                trigger_kind: CompletionTriggerKind::TRIGGER_CHARACTER,
                trigger_character: Some(":".to_string()),
            });

            let response =
                find_completion_at_position(&server, uri.clone(), *position, context).await;

            match response {
                CompletionResponse::Array(completions) => {
                    // Should include constants in all edge cases
                    let completion_labels: Vec<&str> =
                        completions.iter().map(|c| c.label.as_str()).collect();
                    assert!(
                        completion_labels.contains(&"String")
                            || completion_labels.contains(&"TestClass"),
                        "Failed for case: {} - got completions: {:?}",
                        description,
                        completion_labels
                    );
                }
                _ => panic!("Expected array response for case: {}", description),
            }
        }
    }

    #[tokio::test]
    async fn test_simple_nested_module_scope_resolution() {
        let server = create_test_server().await;
        let uri = Url::parse("file:///simple_nested_test.rb").unwrap();
        let content = r#"
module A
  module B
  end
end

A::
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

        // Add the modules to the index manually to ensure they're available
        {
            let index_arc = server.index();
            let mut index_guard = index_arc.lock();

            let a_entry = create_test_entry(
                "A",
                EntryKind::Module {
                    includes: vec![],
                    extends: vec![],
                    prepends: vec![],
                },
            );
            index_guard.add_entry(a_entry);

            let b_entry = create_test_entry(
                "A::B",
                EntryKind::Module {
                    includes: vec![],
                    extends: vec![],
                    prepends: vec![],
                },
            );
            index_guard.add_entry(b_entry);
        }

        // Test completion at position after "A::" (line 6, character 3)
        let position = Position {
            line: 6,
            character: 3,
        };

        let context = Some(tower_lsp::lsp_types::CompletionContext {
            trigger_kind: CompletionTriggerKind::TRIGGER_CHARACTER,
            trigger_character: Some(":".to_string()),
        });

        let response = find_completion_at_position(&server, uri, position, context).await;

        match response {
            CompletionResponse::Array(completions) => {
                let completion_labels: Vec<&str> =
                    completions.iter().map(|c| c.label.as_str()).collect();

                // Should include module B as a direct child of A
                assert!(
                    completion_labels.contains(&"B"),
                    "Expected to find module B in A:: completion, but found: {:?}",
                    completion_labels
                );

                // Should not include A itself or unrelated constants
                assert!(!completion_labels.contains(&"A"));
            }
            _ => panic!("Expected array response"),
        }
    }

    #[tokio::test]
    async fn test_deeply_nested_namespace_completion() {
        let server = create_test_server().await;
        let uri = Url::parse("file:///deep_nested.rb").unwrap();
        let content = r#"
module A
  module B
    module C
      module D
        module E
          class DeepClass
            DEEP_CONSTANT = "deep"

            def deep_method
              ::
            end
          end
        end
      end
    end
  end
end

module X
  module Y
    class YClass
    end
  end
end
"#;

        let params = DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: "ruby".into(),
                version: 1,
                text: content.to_string(),
            },
        };
        server.did_open(params).await;

        // Add deeply nested structure to the index
        {
            let index_arc = server.index();
            let mut index_guard = index_arc.lock();

            // Create the full nested hierarchy
            let modules = vec![
                "A",
                "A::B",
                "A::B::C",
                "A::B::C::D",
                "A::B::C::D::E",
                "X",
                "X::Y",
            ];

            for module_fqn in modules {
                let entry = Entry {
                    fqn: FullyQualifiedName::try_from(module_fqn).unwrap(),
                    kind: EntryKind::Module {
                        includes: vec![],
                        extends: vec![],
                        prepends: vec![],
                    },
                    location: Location {
                        uri: uri.clone(),
                        range: Range::default(),
                    },
                };
                index_guard.add_entry(entry);
            }

            // Add classes
            let classes = vec!["A::B::C::D::E::DeepClass", "X::Y::YClass"];

            for class_fqn in classes {
                let entry = Entry {
                    fqn: FullyQualifiedName::try_from(class_fqn).unwrap(),
                    kind: EntryKind::Class {
                        superclass: Some(MixinRef {
                            parts: vec![RubyConstant::new("Object").unwrap()],
                            absolute: false,
                        }),
                        includes: vec![],
                        extends: vec![],
                        prepends: vec![],
                    },
                    location: Location {
                        uri: uri.clone(),
                        range: Range::default(),
                    },
                };
                index_guard.add_entry(entry);
            }

            // Add some built-in classes for comparison
            let builtin_entry = Entry {
                fqn: FullyQualifiedName::try_from("Array").unwrap(),
                kind: EntryKind::Class {
                    superclass: Some(MixinRef {
                        parts: vec![RubyConstant::new("Object").unwrap()],
                        absolute: false,
                    }),
                    includes: vec![],
                    extends: vec![],
                    prepends: vec![],
                },
                location: Location {
                    uri: uri.clone(),
                    range: Range::default(),
                },
            };
            index_guard.add_entry(builtin_entry);
        }

        // Test scope resolution from within the deeply nested class
        let position = Position {
            line: 10,
            character: 14,
        }; // After "::" inside DeepClass

        let context = Some(tower_lsp::lsp_types::CompletionContext {
            trigger_kind: CompletionTriggerKind::TRIGGER_CHARACTER,
            trigger_character: Some(":".to_string()),
        });

        let response = find_completion_at_position(&server, uri, position, context).await;

        match response {
            CompletionResponse::Array(completions) => {
                let completion_labels: Vec<&str> =
                    completions.iter().map(|c| c.label.as_str()).collect();

                // Should include top-level modules and classes
                assert!(completion_labels.contains(&"A"));
                assert!(completion_labels.contains(&"X"));
                assert!(completion_labels.contains(&"Array"));

                // Should include nested classes that are accessible at top level
                assert!(completion_labels.contains(&"DeepClass"));
                assert!(completion_labels.contains(&"YClass"));

                // Verify we have a reasonable number of completions
                assert!(
                    !completions.is_empty(),
                    "Should have at least some completions"
                );
            }
            _ => panic!("Expected array response"),
        }
    }

    #[tokio::test]
    async fn test_scope_resolution_with_partial_typing() {
        let server = create_test_server().await;
        let uri = Url::parse("file:///partial_test.rb").unwrap();
        let content = r#"
module MyModule
  class MyClass
  end

  module MySubModule
    class MySubClass
    end
  end
end

class MyTopClass
end

def test_method
  ::My
end
"#;

        let params = DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: "ruby".into(),
                version: 1,
                text: content.to_string(),
            },
        };
        server.did_open(params).await;

        // Add entries to the index
        {
            let index_arc = server.index();
            let mut index_guard = index_arc.lock();

            let my_module_entry = Entry {
                fqn: FullyQualifiedName::try_from("MyModule").unwrap(),
                kind: EntryKind::Module {
                    includes: vec![],
                    extends: vec![],
                    prepends: vec![],
                },
                location: Location {
                    uri: uri.clone(),
                    range: Range::default(),
                },
            };
            index_guard.add_entry(my_module_entry);

            let my_class_entry = Entry {
                fqn: FullyQualifiedName::try_from("MyModule::MyClass").unwrap(),
                kind: EntryKind::Class {
                    superclass: Some(MixinRef {
                        parts: vec![RubyConstant::new("Object").unwrap()],
                        absolute: false,
                    }),
                    includes: vec![],
                    extends: vec![],
                    prepends: vec![],
                },
                location: Location {
                    uri: uri.clone(),
                    range: Range::default(),
                },
            };
            index_guard.add_entry(my_class_entry);

            let my_top_class_entry = Entry {
                fqn: FullyQualifiedName::try_from("MyTopClass").unwrap(),
                kind: EntryKind::Class {
                    superclass: Some(MixinRef {
                        parts: vec![RubyConstant::new("Object").unwrap()],
                        absolute: false,
                    }),
                    includes: vec![],
                    extends: vec![],
                    prepends: vec![],
                },
                location: Location {
                    uri: uri.clone(),
                    range: Range::default(),
                },
            };
            index_guard.add_entry(my_top_class_entry);
        }

        // Test scope resolution with partial typing "::My"
        let position = Position {
            line: 15,
            character: 5,
        }; // After "::My"

        let context = Some(tower_lsp::lsp_types::CompletionContext {
            trigger_kind: CompletionTriggerKind::TRIGGER_CHARACTER,
            trigger_character: Some(":".to_string()),
        });

        let response = find_completion_at_position(&server, uri, position, context).await;

        match response {
            CompletionResponse::Array(completions) => {
                // Should include all classes/modules that start with "My"
                let completion_labels: Vec<&str> =
                    completions.iter().map(|c| c.label.as_str()).collect();

                // All "My*" classes and modules should be available
                assert!(completion_labels.contains(&"MyModule"));
                assert!(completion_labels.contains(&"MyClass"));
                assert!(completion_labels.contains(&"MyTopClass"));
            }
            _ => panic!("Expected array response"),
        }
    }

    #[tokio::test]
    async fn test_multi_level_namespace_completion() {
        let server = create_test_server().await;
        let uri = Url::parse("file:///test.rb").unwrap();
        let content = r#"module A
  module B
    module C
    end
  end
end

A::B::"#;

        let params = DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: "ruby".into(),
                version: 1,
                text: content.to_string(),
            },
        };
        server.did_open(params).await;

        // Add entries to the index
        {
            let index_arc = server.index();
            let mut index_guard = index_arc.lock();

            // Add the A module and its nested modules
            let a_module_entry = Entry {
                fqn: FullyQualifiedName::try_from("A").unwrap(),
                kind: EntryKind::Module {
                    includes: vec![],
                    extends: vec![],
                    prepends: vec![],
                },
                location: Location {
                    uri: uri.clone(),
                    range: Range::default(),
                },
            };
            index_guard.add_entry(a_module_entry);

            let a_b_module_entry = Entry {
                fqn: FullyQualifiedName::try_from("A::B").unwrap(),
                kind: EntryKind::Module {
                    includes: vec![],
                    extends: vec![],
                    prepends: vec![],
                },
                location: Location {
                    uri: uri.clone(),
                    range: Range::default(),
                },
            };
            index_guard.add_entry(a_b_module_entry);

            let a_b_c_module_entry = Entry {
                fqn: FullyQualifiedName::try_from("A::B::C").unwrap(),
                kind: EntryKind::Module {
                    includes: vec![],
                    extends: vec![],
                    prepends: vec![],
                },
                location: Location {
                    uri: uri.clone(),
                    range: Range::default(),
                },
            };
            index_guard.add_entry(a_b_c_module_entry);

            // Add some other modules for comparison
            let other_modules = vec!["Array", "ActionController"];

            for module_fqn in other_modules {
                let entry = Entry {
                    fqn: FullyQualifiedName::try_from(module_fqn).unwrap(),
                    kind: EntryKind::Class {
                        superclass: Some(MixinRef {
                            parts: vec![RubyConstant::new("Object").unwrap()],
                            absolute: false,
                        }),
                        includes: vec![],
                        extends: vec![],
                        prepends: vec![],
                    },
                    location: Location {
                        uri: uri.clone(),
                        range: Range::default(),
                    },
                };
                index_guard.add_entry(entry);
            }
        }

        // Test completion at position after "A::B::" (line 8, character 6)
        let position = Position {
            line: 7,      // 0-indexed, so line 8 in the editor
            character: 6, // After "A::B::"
        };

        let context = Some(tower_lsp::lsp_types::CompletionContext {
            trigger_kind: CompletionTriggerKind::TRIGGER_CHARACTER,
            trigger_character: Some(":".to_string()),
        });

        let response = find_completion_at_position(&server, uri, position, context).await;

        match response {
            CompletionResponse::Array(completions) => {
                let completion_labels: Vec<&str> =
                    completions.iter().map(|c| c.label.as_str()).collect();

                println!("Multi-level completions found: {:?}", completion_labels);

                // Should include nested modules within A::B
                assert!(
                    completion_labels.contains(&"C"),
                    "Should contain nested module C in A::B:: scope"
                );

                // Should NOT include top-level classes or modules from other namespaces
                assert!(
                    !completion_labels.contains(&"Array"),
                    "Should not contain top-level Array in A::B:: scope"
                );
                assert!(
                    !completion_labels.contains(&"ActionController"),
                    "Should not contain top-level ActionController in A::B:: scope"
                );
                assert!(
                    !completion_labels.contains(&"A"),
                    "Should not contain A in A::B:: scope"
                );
                assert!(
                    !completion_labels.contains(&"B"),
                    "Should not contain B in A::B:: scope"
                );

                // Verify we have the expected number of completions (should be just C)
                assert_eq!(
                    completions.len(),
                    1,
                    "Should have exactly 1 completion for A::B:: (C)"
                );
            }
            _ => panic!("Expected array response"),
        }
    }

    #[tokio::test]
    async fn test_exact_screenshot_scenario() {
        let server = create_test_server().await;
        let uri = Url::parse("file:///test.rb").unwrap();
        let content = r#"module A
  module B
  end
  module A
  end
end

A::"#;

        let params = DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: "ruby".into(),
                version: 1,
                text: content.to_string(),
            },
        };
        server.did_open(params).await;

        // Add entries to the index that match what would be available in a real Rails app
        {
            let index_arc = server.index();
            let mut index_guard = index_arc.lock();

            // Add the A module and its nested modules
            let a_module_entry = Entry {
                fqn: FullyQualifiedName::try_from("A").unwrap(),
                kind: EntryKind::Module {
                    includes: vec![],
                    extends: vec![],
                    prepends: vec![],
                },
                location: Location {
                    uri: uri.clone(),
                    range: Range::default(),
                },
            };
            index_guard.add_entry(a_module_entry);

            let a_b_module_entry = Entry {
                fqn: FullyQualifiedName::try_from("A::B").unwrap(),
                kind: EntryKind::Module {
                    includes: vec![],
                    extends: vec![],
                    prepends: vec![],
                },
                location: Location {
                    uri: uri.clone(),
                    range: Range::default(),
                },
            };
            index_guard.add_entry(a_b_module_entry);

            let a_a_module_entry = Entry {
                fqn: FullyQualifiedName::try_from("A::A").unwrap(),
                kind: EntryKind::Module {
                    includes: vec![],
                    extends: vec![],
                    prepends: vec![],
                },
                location: Location {
                    uri: uri.clone(),
                    range: Range::default(),
                },
            };
            index_guard.add_entry(a_a_module_entry);

            // Add some common Rails/Ruby classes that start with A
            let rails_classes = vec![
                "ABA",
                "ACH",
                "ActionCable",
                "ActionController",
                "ActionDispatch",
                "ActionMailer",
                "ActionPack",
                "ActionView",
                "ActiveJob",
                "ActiveModel",
                "ActiveRecord",
                "ActiveStorage",
                "ActiveSupport",
                "Array",
                "Arel",
            ];

            for class_fqn in rails_classes {
                let entry = Entry {
                    fqn: FullyQualifiedName::try_from(class_fqn).unwrap(),
                    kind: EntryKind::Class {
                        superclass: Some(MixinRef {
                            parts: vec![RubyConstant::new("Object").unwrap()],
                            absolute: false,
                        }),
                        includes: vec![],
                        extends: vec![],
                        prepends: vec![],
                    },
                    location: Location {
                        uri: uri.clone(),
                        range: Range::default(),
                    },
                };
                index_guard.add_entry(entry);
            }
        }

        // Test completion at position after "A::" (line 8, character 3)
        let position = Position {
            line: 7,      // 0-indexed, so line 8 in the editor
            character: 3, // After "A::"
        };

        let context = Some(tower_lsp::lsp_types::CompletionContext {
            trigger_kind: CompletionTriggerKind::TRIGGER_CHARACTER,
            trigger_character: Some(":".to_string()),
        });

        let response = find_completion_at_position(&server, uri, position, context).await;

        match response {
            CompletionResponse::Array(completions) => {
                let completion_labels: Vec<&str> =
                    completions.iter().map(|c| c.label.as_str()).collect();

                // Should include nested modules within A
                assert!(
                    completion_labels.contains(&"B"),
                    "Should contain nested module B"
                );
                assert!(
                    completion_labels.contains(&"A"),
                    "Should contain nested module A"
                );

                // Should NOT include top-level classes that start with A (since we're in A:: scope)
                assert!(
                    !completion_labels.contains(&"Array"),
                    "Should not contain top-level Array in A:: scope"
                );
                assert!(
                    !completion_labels.contains(&"ActionController"),
                    "Should not contain top-level ActionController in A:: scope"
                );

                // Verify we have the expected number of completions (should be just the nested modules)
                assert_eq!(
                    completions.len(),
                    2,
                    "Should have exactly 2 completions for A:: (A and B)"
                );
            }
            _ => panic!("Expected array response"),
        }
    }

    #[tokio::test]
    async fn test_snippet_completions() {
        let server = create_test_server().await;
        let uri = Url::parse("file:///test.rb").unwrap();
        let content = r#"
def test_method
  i
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

        // Test completion at position where "i" is typed (should match "if" snippet)
        let position = Position {
            line: 2,
            character: 3,
        }; // After "i"
        let response = find_completion_at_position(&server, uri, position, None).await;

        match response {
            CompletionResponse::Array(completions) => {
                assert!(!completions.is_empty(), "Should have completions");

                // Check if we have snippet completions
                let if_snippet = completions.iter().find(|c| c.label == "if");
                assert!(if_snippet.is_some(), "Should have 'if' snippet completion");

                if let Some(completion) = if_snippet {
                    assert_eq!(completion.kind, Some(CompletionItemKind::SNIPPET));
                    assert!(completion.insert_text.is_some(), "Should have insert text");
                    assert_eq!(
                        completion.insert_text_format,
                        Some(InsertTextFormat::SNIPPET)
                    );
                }

                // Check for other control structure snippets that contain "i"
                let while_snippet = completions.iter().find(|c| c.label == "while");
                assert!(
                    while_snippet.is_some(),
                    "Should have 'while' snippet completion"
                );

                let times_snippet = completions.iter().find(|c| c.label == "times");
                assert!(
                    times_snippet.is_some(),
                    "Should have 'times' snippet completion"
                );
            }
            _ => panic!("Expected array response"),
        }
    }

    #[tokio::test]
    async fn test_snippet_completions_partial_match() {
        let server = create_test_server().await;
        let uri = Url::parse("file:///test.rb").unwrap();
        let content = r#"
def test_method
  wh
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

        // Test completion at position where "wh" is typed (should match "while" snippet)
        let position = Position {
            line: 2,
            character: 4,
        }; // After "wh"
        let response = find_completion_at_position(&server, uri, position, None).await;

        match response {
            CompletionResponse::Array(completions) => {
                // Check if we have while snippet completion
                let while_snippet = completions.iter().find(|c| c.label == "while");
                assert!(
                    while_snippet.is_some(),
                    "Should have 'while' snippet completion for 'wh' prefix"
                );

                if let Some(completion) = while_snippet {
                    assert_eq!(completion.kind, Some(CompletionItemKind::SNIPPET));
                    assert!(completion.insert_text.is_some(), "Should have insert text");
                    assert_eq!(
                        completion.insert_text_format,
                        Some(InsertTextFormat::SNIPPET)
                    );
                }

                // Should not have 'if' snippet since it doesn't match 'wh'
                let if_snippet = completions.iter().find(|c| c.label == "if");
                assert!(
                    if_snippet.is_none(),
                    "Should not have 'if' snippet for 'wh' prefix"
                );
            }
            _ => panic!("Expected array response"),
        }
    }

    #[tokio::test]
    async fn test_context_aware_each_completion() {
        let server = create_test_server().await;
        let uri = Url::parse("file:///test.rb").unwrap();
        let content = r#"a = [1, 2, 3]
a.each"#;

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

        // Test completion at position where "a.each" is typed
        let position = Position {
            line: 1,
            character: 6,
        }; // After "each" in "a.each"
        let response = find_completion_at_position(&server, uri, position, None).await;

        match response {
            CompletionResponse::Array(completions) => {
                // Check if we have each completion (now from RBS, so it's a METHOD)
                let each_completion = completions.iter().find(|c| c.label == "each");
                assert!(
                    each_completion.is_some(),
                    "Should have each method completion"
                );

                if let Some(completion) = each_completion {
                    // Now returns METHOD from RBS type-aware completion
                    assert_eq!(completion.kind, Some(CompletionItemKind::METHOD));
                }
            }
            _ => panic!("Expected array response"),
        }
    }

    #[tokio::test]
    async fn test_enhanced_context_detection_after_dot() {
        let server = create_test_server().await;
        let uri = Url::parse("file:///test.rb").unwrap();
        let content = r#"a = [1, 2, 3]
a."#;

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

        // Test completion at position right after the dot
        let position = Position {
            line: 1,
            character: 2,
        }; // Right after "a."
        let response = find_completion_at_position(&server, uri, position, None).await;

        match response {
            CompletionResponse::Array(completions) => {
                // With CFG-based type inference, we now get proper method completions from RBS
                // Check if we have each method completion (from Array RBS)
                let each_completion = completions.iter().find(|c| c.label == "each");
                assert!(
                    each_completion.is_some(),
                    "Should have 'each' method completion from Array"
                );

                if let Some(completion) = each_completion {
                    // Now returns METHOD from RBS type-aware completion
                    assert_eq!(completion.kind, Some(CompletionItemKind::METHOD));
                }
            }
            _ => panic!("Expected array response"),
        }
    }

    #[tokio::test]
    async fn test_keyword_snippets_filtered_in_method_call_context() {
        let server = create_test_server().await;
        let uri = Url::parse("file:///test.rb").unwrap();
        let content = r#"a = [1, 2, 3]
a."#;

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

        // Test completion at position right after the dot
        let position = Position {
            line: 1,
            character: 2,
        }; // Right after "a."
        let response = find_completion_at_position(&server, uri, position, None).await;

        match response {
            CompletionResponse::Array(completions) => {
                // In method call context, keyword snippets should be filtered out
                let keyword_snippets = [
                    "if",
                    "unless",
                    "while",
                    "for",
                    "def",
                    "class",
                    "module",
                    "begin rescue",
                ];

                for keyword in keyword_snippets {
                    let keyword_completion = completions.iter().find(|c| c.label == keyword);
                    assert!(
                        keyword_completion.is_none(),
                        "Should not have '{}' keyword snippet in method call context",
                        keyword
                    );
                }

                // Method completions should be present (from RBS)
                // Note: some methods like 'map' and 'reject' are aliases or inherited from Enumerable
                // We check for methods that are directly defined in Array
                let method_names = ["each", "first", "last", "length"];
                for method in method_names {
                    let method_completion = completions.iter().find(|c| c.label == method);
                    assert!(
                        method_completion.is_some(),
                        "Should have '{}' method completion in method call context",
                        method
                    );

                    // Verify it's a METHOD, not a SNIPPET
                    if let Some(completion) = method_completion {
                        assert_eq!(completion.kind, Some(CompletionItemKind::METHOD));
                    }
                }
            }
            _ => panic!("Expected array response"),
        }
    }

    #[tokio::test]
    async fn test_snippets_not_shown_on_dot_trigger() {
        let server = create_test_server().await;
        let uri = Url::parse("file:///test.rb").unwrap();
        let content = r#"a = [1, 2, 3]
a."#;

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

        // Test completion at position right after the dot with dot trigger
        let position = Position {
            line: 1,
            character: 2,
        }; // Right after "a."

        let context = Some(CompletionContext {
            trigger_kind: CompletionTriggerKind::TRIGGER_CHARACTER,
            trigger_character: Some(".".to_string()),
        });

        let response = find_completion_at_position(&server, uri, position, context).await;

        match response {
            CompletionResponse::Array(completions) => {
                // When triggered by dot, snippets should NOT be included
                let snippet_completions: Vec<_> = completions
                    .iter()
                    .filter(|c| c.kind == Some(CompletionItemKind::SNIPPET))
                    .collect();

                assert!(
                    snippet_completions.is_empty(),
                    "Should not have any snippet completions when triggered by dot character. Found: {:?}",
                    snippet_completions.iter().map(|c| &c.label).collect::<Vec<_>>()
                );
            }
            _ => panic!("Expected array response"),
        }
    }

    #[tokio::test]
    async fn test_snippets_shown_on_character_typing() {
        let server = create_test_server().await;
        let uri = Url::parse("file:///test.rb").unwrap();
        let content = r#"
def test_method
  wh
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

        // Test completion when user types characters (not triggered by special character)
        let position = Position {
            line: 2,
            character: 4,
        }; // After "wh"

        // No trigger context (user is just typing)
        let response = find_completion_at_position(&server, uri, position, None).await;

        match response {
            CompletionResponse::Array(completions) => {
                // When not triggered by dot, snippets should be included
                let while_snippet = completions.iter().find(|c| c.label == "while");
                assert!(
                    while_snippet.is_some(),
                    "Should have 'while' snippet completion when user types characters"
                );

                if let Some(completion) = while_snippet {
                    assert_eq!(completion.kind, Some(CompletionItemKind::SNIPPET));
                }
            }
            _ => panic!("Expected array response"),
        }
    }

    #[tokio::test]
    async fn test_top_level_string_variable_completion() {
        let server = create_test_server().await;
        let uri = Url::parse("file:///test_string.rb").unwrap();
        let content = r#"name = "hello"
name."#;

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

        // Test completion at position right after "name."
        let position = Position {
            line: 1,
            character: 5,
        }; // Right after "name."

        let response = find_completion_at_position(&server, uri, position, None).await;

        match response {
            CompletionResponse::Array(completions) => {
                // Should have String methods like upcase, downcase, length
                let upcase_completion = completions.iter().find(|c| c.label == "upcase");
                assert!(
                    upcase_completion.is_some(),
                    "Should have 'upcase' method completion for String"
                );

                let length_completion = completions.iter().find(|c| c.label == "length");
                assert!(
                    length_completion.is_some(),
                    "Should have 'length' method completion for String"
                );
            }
            _ => panic!("Expected array response"),
        }
    }

    #[tokio::test]
    async fn test_variable_to_variable_assignment_completion() {
        let server = create_test_server().await;
        let uri = Url::parse("file:///test_var_assign.rb").unwrap();
        let content = r#"a = 'str'
b = a
puts b."#;

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

        // Test completion at position right after "b."
        let position = Position {
            line: 2,
            character: 7,
        }; // Right after "puts b."

        // Debug: check what type is inferred for b
        let response = find_completion_at_position(&server, uri, position, None).await;

        match response {
            CompletionResponse::Array(completions) => {
                // b should have String type (inherited from a)
                // So we should get String methods
                let upcase_completion = completions.iter().find(|c| c.label == "upcase");
                assert!(
                    upcase_completion.is_some(),
                    "Should have 'upcase' method completion for b (String via a)"
                );

                let length_completion = completions.iter().find(|c| c.label == "length");
                assert!(
                    length_completion.is_some(),
                    "Should have 'length' method completion for b (String via a)"
                );
            }
            _ => panic!("Expected array response"),
        }
    }
}
