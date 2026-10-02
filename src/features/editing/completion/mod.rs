//! Completion: shape keys, constants, methods, variables, and snippets at the
//! cursor, read after the document's current semantic commit.

mod candidates;
pub mod snippets;
pub mod variable;

use log::debug;
use ruby_analysis::core::NamespaceKind;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::{
    CompletionContext, CompletionItem, CompletionItemKind, CompletionParams, CompletionResponse,
    CompletionTextEdit, CompletionTriggerKind, Position, Range, TextEdit, Url,
};

use ruby_analysis::core::MethodReceiver;
use ruby_analysis::indexer::Identifier;

use crate::features::cursor::{Cursor, EngineQuery};
use crate::server::RubyLanguageServer;
use crate::utils::ast::is_in_statement_position;
use crate::utils::lsp::{lsp_position, source_position};
use crate::utils::parser::position_to_offset;

pub use snippets::RubySnippets;

/// Handle `textDocument/completion`.
pub async fn handle(
    server: &RubyLanguageServer,
    params: CompletionParams,
) -> LspResult<Option<CompletionResponse>> {
    let uri = params.text_document_position.text_document.uri.clone();
    let position = params.text_document_position.position;
    debug!("Completion request received with params {:?}", params);
    Ok(Some(
        find_completion_at_position(server, uri, position, params.context).await,
    ))
}

/// Handle `completionItem/resolve`: items are complete when listed.
pub async fn handle_resolve(
    _server: &RubyLanguageServer,
    item: CompletionItem,
) -> LspResult<CompletionItem> {
    Ok(item)
}

pub async fn find_completion_at_position(
    server: &RubyLanguageServer,
    uri: Url,
    position: Position,
    context: Option<CompletionContext>,
) -> CompletionResponse {
    // An accepted edit replaces the source and local scopes in separate steps.
    // Wait for that transaction before returning a list the editor may reuse
    // while the user continues typing the same identifier.
    let semantic_lock = server.document_semantic_lock(&uri);
    let _semantic_guard = semantic_lock.lock().await;

    let Some(document) = server.documents.read().get(&uri).cloned() else {
        return CompletionResponse::Array(Vec::new());
    };
    EngineQuery::with_doc_and_engine(document, server.analysis_engine_for_uri(&uri))
        .with_view(|cursor| completion_at(cursor, &uri, position, context.as_ref()))
}

/// Completions at `position`: shape keys, then `::` constants, receiver
/// methods, or variables, constants, top-level methods, and snippets.
pub fn completion_at(
    cursor: Cursor<'_>,
    uri: &Url,
    position: Position,
    context: Option<&CompletionContext>,
) -> CompletionResponse {
    let Some(document) = cursor.document else {
        return CompletionResponse::Array(Vec::new());
    };
    if !document.is_ruby_position(source_position(position)) {
        return CompletionResponse::Array(Vec::new());
    }
    let analyzer = cursor.analyzer(uri, &document.content, position);
    let byte_offset = document.position_to_analysis_offset(source_position(position));
    let (
        (partial_name, _, _, _lv_scope_id, namespace_kind),
        shape_key_completion_target,
        completion_receiver_target,
    ) = analyzer.get_completion_context(byte_offset);
    if let Some(target) = shape_key_completion_target {
        let shape_keys = ruby_analysis::engine::completion::shape_key_completions_for_target(
            cursor.view,
            document,
            &target,
        );
        let replacement_range = Range::new(
            lsp_position(document.offset_to_position(shape_keys.replacement_start as usize)),
            lsp_position(document.offset_to_position(shape_keys.replacement_end as usize)),
        );
        let items = shape_keys
            .keys
            .into_iter()
            .map(|key| {
                let label = match key {
                    ruby_analysis::core::LiteralKey::Symbol(value)
                    | ruby_analysis::core::LiteralKey::String(value) => value,
                };
                CompletionItem {
                    label: label.clone(),
                    kind: Some(CompletionItemKind::FIELD),
                    detail: Some("Structural Hash key".to_string()),
                    filter_text: Some(label.clone()),
                    text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                        range: replacement_range,
                        new_text: label,
                    })),
                    ..Default::default()
                }
            })
            .collect::<Vec<_>>();
        return CompletionResponse::Array(items);
    }

    // Check if completion was triggered by a trigger character
    let is_trigger_character = context
        .map(|ctx| ctx.trigger_kind == CompletionTriggerKind::TRIGGER_CHARACTER)
        .unwrap_or(false);

    let trigger_character = context
        .and_then(|ctx| ctx.trigger_character.as_ref())
        .map(|s| s.as_str());

    let line_text = document
        .content
        .lines()
        .nth(position.line as usize)
        .unwrap_or("");

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
        Some(Identifier::RubyMethod { iden, .. }) => {
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
                receiver: MethodReceiver::LocalVariable(_)
                    | MethodReceiver::InstanceVariable(_)
                    | MethodReceiver::ClassVariable(_)
                    | MethodReceiver::GlobalVariable(_)
                    | MethodReceiver::MethodCall { .. }
                    | MethodReceiver::Literal(_)
                    | MethodReceiver::Expression,
                ..
            })
        );

    // Prioritize constant completions when in scope resolution context (::)
    if is_scope_resolution_context {
        // Focus on constant completions for scope resolution
        completions.extend(candidates::constant_completions(
            cursor.view,
            &partial_string,
        ));
    } else if is_method_call_context {
        // Method call context: provide type-aware method completions

        // Get receiver type using type snapshots
        let receiver_type = ruby_analysis::engine::completion::receiver_type_from_context(
            cursor.view,
            document,
            &document.content,
            byte_offset,
            namespace_kind,
            &partial_name,
            completion_receiver_target.as_ref(),
        );

        if let Some(receiver_type) = receiver_type {
            // Ruby constants may contain either a class/module object or an ordinary
            // value. The resolved type, not the syntactic capitalization, owns the
            // singleton-vs-instance completion decision.
            let kind = if matches!(
                receiver_type,
                ruby_analysis::core::RubyType::ClassReference(_)
                    | ruby_analysis::core::RubyType::ModuleReference(_)
            ) {
                NamespaceKind::Singleton
            } else {
                NamespaceKind::Instance
            };

            completions.extend(candidates::method_completions(
                cursor.view,
                &receiver_type,
                &partial_string,
                kind,
            ));
        }
    } else {
        // Normal completion: include variables, constants, methods, and snippets

        // Add local variable completions
        let variable_completions = variable::find_variable_completions(document, position);
        completions.extend(variable_completions);

        // Add constant completions
        completions.extend(candidates::constant_completions(
            cursor.view,
            &partial_string,
        ));

        // Add top-level method completions (methods defined outside any class/module).
        completions.extend(candidates::top_level_method_completions(
            cursor.view,
            &partial_string,
        ));

        // Add snippet completions with context awareness
        // Only include snippets in statement positions (not in value positions like
        // arguments, array elements, hash values, string interpolations, etc.)
        if !is_dot_trigger {
            let byte_offset = position_to_offset(&document.content, position);
            let parse_result = document.parse();
            let root = parse_result.node();

            if is_in_statement_position(&root, byte_offset) {
                let snippet_context = snippets::RubySnippets::determine_context_with_position(
                    &partial_name,
                    line_text,
                    position.character,
                );

                let snippet_completions = RubySnippets::get_matching_snippets_with_context(
                    &partial_string,
                    snippet_context,
                );

                completions.extend(snippet_completions);
            }
        }
    }

    CompletionResponse::Array(completions)
}
