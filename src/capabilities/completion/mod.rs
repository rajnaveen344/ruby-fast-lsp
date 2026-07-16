pub mod snippets;
pub mod variable;

use ruby_analysis::core::{FullyQualifiedName, NamespaceKind, RubyMethod, SourceFileId};
use tower_lsp::lsp_types::{
    CompletionContext, CompletionResponse, CompletionTriggerKind, Position, Url,
};

use ruby_analysis::indexer::{Identifier, MethodReceiver, RubyPrismAnalyzer};
use ruby_analysis::inference::{
    completion::{CompletionSemanticQuery, CompletionVariableKind},
    RubyType,
};

use crate::{
    query::{analyzer_for_document, EngineQuery},
    server::RubyLanguageServer,
    utils::{ast::is_in_statement_position, position_to_offset},
};

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
    if !document.is_ruby_position(position) {
        return CompletionResponse::Array(Vec::new());
    }
    let analyzer = analyzer_for_document(
        RubyPrismAnalyzer::new(uri.clone(), document.content.clone()),
        &document,
        &server.analysis_engine_for_uri(&uri),
        position,
    );

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

    let (partial_name, _, _, _lv_scope_id, _namespace_kind) = analyzer.get_identifier(position);

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
        let query = EngineQuery::with_engine(server.analysis_engine_for_uri(&uri));
        let constant_completions =
            query.find_constant_completions(&analyzer, position, partial_string);
        completions.extend(constant_completions);
    } else if is_method_call_context {
        // Method call context: provide type-aware method completions

        // Get receiver type using type snapshots
        let semantic_query = ServerCompletionSemanticQuery {
            analysis_engine: server.analysis_engine_for_uri(&uri),
        };
        let receiver_type = ruby_analysis::inference::completion::receiver_type_from_context(
            &semantic_query,
            &document,
            &document.content,
            position,
            &partial_name,
        );

        if let Some(receiver_type) = receiver_type {
            // Determine namespace kind from the receiver
            // Constant receivers (Foo.bar) use singleton methods
            // Variable/expression receivers (obj.bar) use instance methods
            let kind = if let Some(Identifier::RubyMethod { receiver, .. }) = &partial_name {
                match receiver {
                    MethodReceiver::Constant(_) => NamespaceKind::Singleton,
                    _ => NamespaceKind::Instance,
                }
            } else if matches!(
                receiver_type,
                ruby_analysis::inference::RubyType::ClassReference(_)
            ) {
                // Dot-trigger on a constant (e.g., "UserA.") — partial_name is None
                // but the text-based receiver detection found a ClassReference
                NamespaceKind::Singleton
            } else {
                NamespaceKind::Instance
            };

            let query = EngineQuery::with_engine(server.analysis_engine_for_uri(&uri));
            let method_completions =
                query.find_method_completions(&receiver_type, &partial_string, kind);
            completions.extend(method_completions);
        }
    } else {
        // Normal completion: include variables, constants, methods, and snippets

        // Add local variable completions
        let variable_completions = variable::find_variable_completions(&document, position);
        completions.extend(variable_completions);

        // Add constant completions
        let query = EngineQuery::with_engine(server.analysis_engine_for_uri(&uri));
        let constant_completions =
            query.find_constant_completions(&analyzer, position, partial_string.clone());
        completions.extend(constant_completions);

        // Add top-level method completions (methods defined outside any class/module).
        let top_level_methods = query.find_top_level_method_completions(&partial_string);
        completions.extend(top_level_methods);

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
struct ServerCompletionSemanticQuery {
    analysis_engine: std::sync::Arc<parking_lot::RwLock<ruby_analysis::engine::AnalysisEngine>>,
}

impl CompletionSemanticQuery for ServerCompletionSemanticQuery {
    fn method_return_type_for_receiver(
        &self,
        namespace: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Option<RubyType> {
        let engine = self.analysis_engine.read();
        ruby_analysis::engine::AnalysisQuery::new(&engine)
            .method_return_type_for_receiver(namespace, method)
    }

    fn variable_type_in_file(
        &self,
        kind: CompletionVariableKind,
        name: &str,
        file_id: SourceFileId,
    ) -> Option<RubyType> {
        let kind = match kind {
            CompletionVariableKind::Instance => ruby_analysis::engine::VariableTypeKind::Instance,
            CompletionVariableKind::Class => ruby_analysis::engine::VariableTypeKind::Class,
            CompletionVariableKind::Global => ruby_analysis::engine::VariableTypeKind::Global,
        };
        let engine = self.analysis_engine.read();
        ruby_analysis::engine::AnalysisQuery::new(&engine)
            .variable_type_in_file(kind, name, file_id)
    }

    fn implicit_receiver_at(
        &self,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> Option<FullyQualifiedName> {
        let engine = self.analysis_engine.read();
        ruby_analysis::engine::AnalysisQuery::new(&engine)
            .execution_context_at(file_id, byte_offset)
            .map(|context| context.implicit_receiver.clone())
    }
}
