pub mod constant_completion;
pub mod method;
pub mod snippets;
pub mod variable;

use ruby_analysis_core::NamespaceKind;
use tower_lsp::lsp_types::{
    CompletionContext, CompletionResponse, CompletionTriggerKind, Position, Url,
};

use crate::{
    analyzer_prism::{Identifier, MethodReceiver, RubyPrismAnalyzer},
    query::EngineQuery,
    server::RubyLanguageServer,
    types::fully_qualified_name::FullyQualifiedName,
    utils::{ast::is_in_statement_position, position_to_offset},
};

pub use constant_completion::ConstantCompletionContext;
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
        let query = EngineQuery::with_engine(server.analysis_engine.clone());
        let constant_completions =
            query.find_constant_completions(&analyzer, position, partial_string);
        completions.extend(constant_completions);
    } else if is_method_call_context {
        // Method call context: provide type-aware method completions

        // Get receiver type using type snapshots
        let receiver_type = get_receiver_type_from_snapshots(
            server,
            &uri,
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
                ruby_analysis_inference::RubyType::ClassReference(_)
            ) {
                // Dot-trigger on a constant (e.g., "UserA.") — partial_name is None
                // but the text-based receiver detection found a ClassReference
                NamespaceKind::Singleton
            } else {
                NamespaceKind::Instance
            };

            let query = EngineQuery::with_engine(server.analysis_engine.clone());
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
        let query = EngineQuery::with_engine(server.analysis_engine.clone());
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
            let parse_result = ruby_prism::parse(document.content.as_bytes());
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

/// Get the receiver type using type snapshots from TypeTracker
///
/// This function determines the type of the receiver expression at a completion position.
/// It handles:
/// - Constant receivers (e.g., `User.find`) -> ClassReference
/// - Literal receivers (e.g., `"hello".`, `123.`) -> direct type
/// - Variable receivers (e.g., `name.`) -> type from snapshots
fn get_receiver_type_from_snapshots(
    server: &RubyLanguageServer,
    uri: &Url,
    content: &str,
    position: Position,
    identifier: &Option<Identifier>,
) -> Option<ruby_analysis_inference::RubyType> {
    use crate::types::fully_qualified_name::FullyQualifiedName;
    use crate::types::ruby_namespace::RubyConstant;
    use ruby_analysis_inference::RubyType;

    // If we have a method identifier with constant receiver, use it directly
    if let Some(Identifier::RubyMethod {
        receiver: MethodReceiver::Constant(recv_parts),
        ..
    }) = identifier
    {
        let fqn = FullyQualifiedName::Constant(recv_parts.clone());
        return Some(RubyType::ClassReference(fqn));
    }

    // Handle self receiver — resolve to the enclosing class/module
    if let Some(Identifier::RubyMethod {
        receiver: MethodReceiver::SelfReceiver,
        namespace,
        ..
    }) = identifier
    {
        if !namespace.is_empty() {
            let fqn = FullyQualifiedName::from(namespace.clone());
            return Some(RubyType::Class(fqn));
        }
    }

    // Handle method call chains — resolve inner receiver, then infer return type
    // e.g., User.new.name -> Constant(User) + "new" -> Class(User) instance, then lookup "name"
    // e.g., user.name.upcase -> Variable("user") + "name" -> infer return type of name
    if let Some(Identifier::RubyMethod {
        receiver:
            MethodReceiver::MethodCall {
                inner_receiver,
                method_name,
            },
        ..
    }) = identifier
    {
        let inner_type =
            resolve_method_receiver_type(server, uri, content, position, inner_receiver);
        if let Some(inner_type) = inner_type {
            // Special case: .new on a ClassReference returns an instance of that class
            if method_name == "new" {
                if let RubyType::ClassReference(fqn) = &inner_type {
                    return Some(RubyType::Class(fqn.clone()));
                }
            }

            // General case: look up the method's return type
            if let Some(rt) =
                infer_method_call_return_type_from_analysis(server, &inner_type, method_name)
            {
                return Some(rt);
            }
        }
    }

    // Handle literal receivers — type is already known from the AST
    if let Some(Identifier::RubyMethod {
        receiver: MethodReceiver::Literal(ty),
        ..
    }) = identifier
    {
        return Some(ty.clone());
    }

    // Handle instance/class/global variable receivers — look up type from index
    if let Some(Identifier::RubyMethod { receiver, .. }) = identifier {
        let var_type = match receiver {
            MethodReceiver::InstanceVariable(name)
            | MethodReceiver::ClassVariable(name)
            | MethodReceiver::GlobalVariable(name) => {
                lookup_variable_type_from_engine(server, uri, name, receiver)
            }
            _ => None,
        };
        if let Some(ty) = var_type {
            return Some(ty);
        }
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
    let before_dot = before_cursor[..dot_pos].trim_end();

    // Try literal detection on the raw text before the dot first,
    // before we strip to just the last word token. This handles cases
    // like `"hello".upcase` and `[1,2,3].first` where the literal
    // expression contains non-alphanumeric chars.
    if let Some(literal_type) = infer_literal_type_from_expression(before_dot) {
        return Some(literal_type);
    }

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

    // Handle literals from single-word text (e.g., integer `42.abs`)
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

    // For variables, use VariableScopes tree for type resolution
    if is_variable_name(receiver_text) {
        let receiver_position = Position {
            line: position.line,
            character: (dot_pos - receiver_text.len()) as u32,
        };

        // Get type from VariableScopes tree
        if let Some(doc_arc) = server.docs.lock().get(uri) {
            let doc = doc_arc.read();
            if let Some(scope_id) = doc
                .variable_scopes()
                .find_scope_for_variable_at(receiver_text, receiver_position)
                .or_else(|| doc.variable_scopes().scope_at_position(receiver_position))
            {
                if let Some(ty) = doc.variable_scopes().get_type_at_position(
                    receiver_text,
                    scope_id,
                    receiver_position,
                ) {
                    if *ty != RubyType::Unknown {
                        return Some(ty.clone());
                    }
                }
            }
        }

        // Fallback: Look for constructor assignment pattern (var = ClassName.new)
        if let Some(ty) = infer_type_from_constructor_assignment(content, receiver_text) {
            return Some(ty);
        }
    }

    if let Some(return_type) =
        infer_bare_method_return_type_from_analysis(server, receiver_text, identifier)
    {
        return Some(return_type);
    }

    None
}

/// Resolve a `MethodReceiver` to a `RubyType` for method chain resolution.
///
/// This handles the base cases (Constant, LocalVariable, SelfReceiver) that
/// `get_receiver_type_from_snapshots` handles for Identifier, but operates
/// on the recursive `MethodReceiver` structure used in chained calls.
fn resolve_method_receiver_type(
    server: &RubyLanguageServer,
    uri: &Url,
    content: &str,
    position: Position,
    receiver: &MethodReceiver,
) -> Option<ruby_analysis_inference::RubyType> {
    use crate::types::fully_qualified_name::FullyQualifiedName;
    use ruby_analysis_inference::RubyType;

    match receiver {
        MethodReceiver::Constant(parts) => {
            let fqn = FullyQualifiedName::Constant(parts.clone());
            Some(RubyType::ClassReference(fqn))
        }
        MethodReceiver::LocalVariable(name) => {
            // Look up variable type from VariableScopes
            if let Some(doc_arc) = server.docs.lock().get(uri) {
                let doc = doc_arc.read();
                if let Some(scope_id) = doc
                    .variable_scopes()
                    .find_scope_for_variable_at(name, position)
                    .or_else(|| doc.variable_scopes().scope_at_position(position))
                {
                    if let Some(ty) = doc
                        .variable_scopes()
                        .get_type_at_position(name, scope_id, position)
                    {
                        if *ty != RubyType::Unknown {
                            return Some(ty.clone());
                        }
                    }
                }
            }
            // Fallback to constructor pattern
            infer_type_from_constructor_assignment(content, name)
        }
        MethodReceiver::SelfReceiver => {
            // Would need namespace context — not available here
            None
        }
        MethodReceiver::InstanceVariable(name)
        | MethodReceiver::ClassVariable(name)
        | MethodReceiver::GlobalVariable(name) => {
            lookup_variable_type_from_engine(server, uri, name, receiver)
        }
        MethodReceiver::MethodCall {
            inner_receiver,
            method_name,
        } => {
            let inner_type =
                resolve_method_receiver_type(server, uri, content, position, inner_receiver)?;
            // Special case: .new on a ClassReference returns an instance
            if method_name == "new" {
                if let RubyType::ClassReference(fqn) = &inner_type {
                    return Some(RubyType::Class(fqn.clone()));
                }
            }
            infer_method_call_return_type_from_analysis(server, &inner_type, method_name)
        }
        MethodReceiver::Literal(ty) => Some(ty.clone()),
        _ => None,
    }
}

fn infer_method_call_return_type_from_analysis(
    server: &RubyLanguageServer,
    receiver_type: &ruby_analysis_inference::RubyType,
    method_name: &str,
) -> Option<ruby_analysis_inference::RubyType> {
    use crate::types::ruby_method::RubyMethod;
    use ruby_analysis_inference::RubyType;

    if method_name == "new" {
        if let RubyType::ClassReference(fqn) = receiver_type {
            return Some(RubyType::Class(fqn.clone()));
        }
    }

    if let Some(return_type) = infer_generic_rbs_method_return_type(receiver_type, method_name) {
        return Some(return_type);
    }

    let method = RubyMethod::new(method_name).ok()?;
    let engine = server.analysis_engine.lock();
    let query = ruby_analysis_engine::AnalysisQuery::new(&engine);
    for namespace in receiver_type_to_analysis_namespaces(receiver_type) {
        if let Some(return_type) = query.method_return_type_for_receiver(&namespace, &method) {
            return Some(return_type);
        }
    }

    infer_rbs_method_return_type(receiver_type, method_name)
}

fn infer_generic_rbs_method_return_type(
    receiver_type: &ruby_analysis_inference::RubyType,
    method_name: &str,
) -> Option<ruby_analysis_inference::RubyType> {
    use ruby_analysis_inference::RubyType;

    match receiver_type {
        RubyType::Array(element_types) => {
            ruby_analysis_inference::rbs::get_rbs_method_return_type_with_type_args(
                "Array",
                method_name,
                false,
                element_types,
            )
        }
        RubyType::Hash(key_types, value_types) => {
            let type_args = vec![
                RubyType::union(key_types.clone()),
                RubyType::union(value_types.clone()),
            ];
            ruby_analysis_inference::rbs::get_rbs_method_return_type_with_type_args(
                "Hash",
                method_name,
                false,
                &type_args,
            )
        }
        RubyType::Class(_)
        | RubyType::Module(_)
        | RubyType::ClassReference(_)
        | RubyType::ModuleReference(_)
        | RubyType::Union(_)
        | RubyType::Unknown => None,
    }
}

fn infer_rbs_method_return_type(
    receiver_type: &ruby_analysis_inference::RubyType,
    method_name: &str,
) -> Option<ruby_analysis_inference::RubyType> {
    use ruby_analysis_inference::RubyType;

    match receiver_type {
        RubyType::Class(fqn) | RubyType::Module(fqn) => {
            rbs_method_return_for_fqn(fqn, method_name, false)
        }
        RubyType::ClassReference(fqn) | RubyType::ModuleReference(fqn) => {
            rbs_method_return_for_fqn(fqn, method_name, true)
        }
        RubyType::Array(_) | RubyType::Hash(_, _) => {
            infer_generic_rbs_method_return_type(receiver_type, method_name)
        }
        RubyType::Union(types) => {
            let mut return_types = types
                .iter()
                .filter_map(|ty| {
                    infer_method_call_return_type_from_analysis_fallback(ty, method_name)
                })
                .collect::<Vec<_>>();
            return_types.sort_by_key(|ty| ty.to_string());
            return_types.dedup();
            match return_types.len() {
                0 => None,
                1 => return_types.pop(),
                _ => Some(RubyType::union(return_types)),
            }
        }
        RubyType::Unknown => None,
    }
}

fn infer_method_call_return_type_from_analysis_fallback(
    receiver_type: &ruby_analysis_inference::RubyType,
    method_name: &str,
) -> Option<ruby_analysis_inference::RubyType> {
    infer_generic_rbs_method_return_type(receiver_type, method_name)
        .or_else(|| infer_rbs_method_return_type(receiver_type, method_name))
}

fn rbs_method_return_for_fqn(
    fqn: &FullyQualifiedName,
    method_name: &str,
    is_singleton: bool,
) -> Option<ruby_analysis_inference::RubyType> {
    for class_name in class_names_for_fqn(fqn) {
        if let Some(return_type) =
            ruby_analysis_inference::rbs::get_rbs_method_return_type_as_ruby_type(
                &class_name,
                method_name,
                is_singleton,
            )
        {
            return Some(return_type);
        }
    }
    None
}

fn class_names_for_fqn(fqn: &FullyQualifiedName) -> Vec<String> {
    let parts = fqn.namespace_parts();
    let fqn_name = parts
        .iter()
        .map(|part| part.to_string())
        .collect::<Vec<_>>()
        .join("::");
    let simple_name = parts.last().map(|part| part.to_string());

    let mut names = Vec::new();
    if !fqn_name.is_empty() {
        names.push(fqn_name);
    }
    if let Some(simple_name) = simple_name {
        if !names.contains(&simple_name) {
            names.push(simple_name);
        }
    }
    names
}

fn receiver_type_to_analysis_namespaces(
    receiver_type: &ruby_analysis_inference::RubyType,
) -> Vec<FullyQualifiedName> {
    use crate::types::fully_qualified_name::NamespaceKind;
    use ruby_analysis_inference::RubyType;

    match receiver_type {
        RubyType::Class(fqn) | RubyType::Module(fqn) => {
            vec![FullyQualifiedName::namespace_with_kind(
                fqn.namespace_parts(),
                NamespaceKind::Instance,
            )]
        }
        RubyType::ClassReference(fqn) | RubyType::ModuleReference(fqn) => {
            vec![FullyQualifiedName::namespace_with_kind(
                fqn.namespace_parts(),
                NamespaceKind::Singleton,
            )]
        }
        RubyType::Union(types) => types
            .iter()
            .flat_map(receiver_type_to_analysis_namespaces)
            .collect(),
        RubyType::Array(_) | RubyType::Hash(_, _) | RubyType::Unknown => Vec::new(),
    }
}

fn infer_bare_method_return_type_from_analysis(
    server: &RubyLanguageServer,
    method_name: &str,
    identifier: &Option<Identifier>,
) -> Option<ruby_analysis_inference::RubyType> {
    use crate::types::fully_qualified_name::NamespaceKind;
    use crate::types::ruby_method::RubyMethod;

    let method = RubyMethod::new(method_name).ok()?;
    let mut namespaces = Vec::new();
    if let Some(Identifier::RubyMethod { namespace, .. }) = identifier {
        namespaces.push(FullyQualifiedName::namespace_with_kind(
            namespace.clone(),
            NamespaceKind::Instance,
        ));
    }
    namespaces.push(FullyQualifiedName::namespace_with_kind(
        Vec::new(),
        NamespaceKind::Instance,
    ));

    let engine = server.analysis_engine.lock();
    let query = ruby_analysis_engine::AnalysisQuery::new(&engine);
    for namespace in namespaces {
        if let Some(return_type) = query.method_return_type_for_receiver(&namespace, &method) {
            return Some(return_type);
        }
    }
    None
}

/// Look up the type of an instance/class/global variable from analysis type facts.
fn lookup_variable_type_from_engine(
    server: &RubyLanguageServer,
    uri: &Url,
    name: &str,
    receiver: &MethodReceiver,
) -> Option<ruby_analysis_inference::RubyType> {
    use ruby_analysis_engine::VariableTypeKind;

    let file_id = {
        let docs = server.docs.lock();
        let file_id = docs.get(uri)?.read().analysis_file_id();
        file_id
    };

    let kind = match receiver {
        MethodReceiver::InstanceVariable(_) => VariableTypeKind::Instance,
        MethodReceiver::ClassVariable(_) => VariableTypeKind::Class,
        MethodReceiver::GlobalVariable(_) => VariableTypeKind::Global,
        MethodReceiver::None
        | MethodReceiver::SelfReceiver
        | MethodReceiver::Constant(_)
        | MethodReceiver::LocalVariable(_)
        | MethodReceiver::MethodCall { .. }
        | MethodReceiver::Literal(_)
        | MethodReceiver::Expression => return None,
    };

    let engine = server.analysis_engine.lock();
    ruby_analysis_engine::AnalysisQuery::new(&engine).variable_type_in_file(kind, name, file_id)
}

fn infer_type_from_constructor_assignment(
    content: &str,
    var_name: &str,
) -> Option<ruby_analysis_inference::RubyType> {
    use crate::types::fully_qualified_name::FullyQualifiedName;
    use crate::types::ruby_namespace::RubyConstant;
    use ruby_analysis_inference::RubyType;

    // Pattern: `var_name = SomeClass.new` or `var_name = Some::Namespaced::Class.new`
    // We search for assignments to this variable
    for line in content.lines() {
        let trimmed = line.trim();

        // Look for assignment pattern: `var = ...`
        if let Some(rest) = trimmed.strip_prefix(var_name) {
            // Make sure we matched the whole variable name (not just a prefix)
            // The next character should be whitespace or '='
            let next_char = rest.chars().next();
            if !matches!(next_char, Some(' ') | Some('\t') | Some('=')) {
                continue;
            }

            let rest = rest.trim();
            if let Some(rest) = rest.strip_prefix('=') {
                let rhs = rest.trim();

                // Check for `.new` pattern
                if rhs.ends_with(".new") || rhs.contains(".new(") || rhs.contains(".new ") {
                    // Extract the class name before .new
                    let new_pos = rhs.find(".new")?;
                    let class_part = rhs[..new_pos].trim();

                    // Validate it's a constant (starts with uppercase)
                    if class_part
                        .chars()
                        .next()
                        .map(|c| c.is_uppercase())
                        .unwrap_or(false)
                    {
                        // Parse the constant path (e.g., "Some::Namespaced::Class")
                        let parts: Vec<_> = class_part
                            .split("::")
                            .filter_map(|s| RubyConstant::new(s.trim()).ok())
                            .collect();

                        if !parts.is_empty() {
                            let fqn = FullyQualifiedName::Constant(parts);
                            // RubyType::Class represents an instance of the class
                            return Some(RubyType::Class(fqn));
                        }
                    }
                }
            }
        }
    }

    None
}

/// Infer type from a literal expression at the end of text before a dot.
///
/// Unlike `infer_literal_type` which works on a single token, this handles
/// full expressions like `"hello"`, `[1, 2, 3]`, `{ a: 1 }` by looking
/// at the trailing expression in the text.
fn infer_literal_type_from_expression(text: &str) -> Option<ruby_analysis_inference::RubyType> {
    use ruby_analysis_inference::RubyType;

    let trimmed = text.trim();

    // String literal ending: ..." or ...'
    if trimmed.ends_with('"') || trimmed.ends_with('\'') {
        return Some(RubyType::string());
    }

    // Array literal ending: ...]
    if trimmed.ends_with(']') && trimmed.starts_with('[') {
        let inner = &trimmed[1..trimmed.len() - 1];
        let element_types = infer_array_element_types(inner);
        return Some(RubyType::Array(element_types));
    }

    // Hash literal ending: ...}
    if trimmed.ends_with('}') {
        return Some(RubyType::Hash(
            vec![RubyType::Unknown],
            vec![RubyType::Unknown],
        ));
    }

    // Symbol literal: check for :word pattern at end
    if let Some(rest) = trimmed.rsplit_once(|c: char| c.is_whitespace() || c == '(' || c == ',') {
        if rest.1.starts_with(':') {
            return Some(RubyType::symbol());
        }
    } else if trimmed.starts_with(':') {
        return Some(RubyType::symbol());
    }

    None
}

/// Infer element types from array literal content (e.g., "1, 2, 3" → [Integer])
fn infer_array_element_types(inner: &str) -> Vec<ruby_analysis_inference::RubyType> {
    use ruby_analysis_inference::RubyType;

    let mut types = Vec::new();
    for element in inner.split(',') {
        let el = element.trim();
        if el.is_empty() {
            continue;
        }
        let ty = if el.starts_with('"') || el.starts_with('\'') {
            RubyType::string()
        } else if el.starts_with(':') {
            RubyType::symbol()
        } else if el.parse::<i64>().is_ok() {
            RubyType::integer()
        } else if el.parse::<f64>().is_ok() {
            RubyType::float()
        } else if el == "true" || el == "false" {
            RubyType::true_class()
        } else if el == "nil" {
            RubyType::nil_class()
        } else {
            RubyType::Unknown
        };
        if ty != RubyType::Unknown && !types.contains(&ty) {
            types.push(ty);
        }
    }
    if types.is_empty() {
        vec![RubyType::Unknown]
    } else {
        types
    }
}

/// Infer type from a literal expression
fn infer_literal_type(text: &str) -> Option<ruby_analysis_inference::RubyType> {
    use ruby_analysis_inference::RubyType;

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
        return Some(RubyType::Array(vec![RubyType::Unknown]));
    }

    // Hash literal
    if text.starts_with('{') {
        return Some(RubyType::Hash(
            vec![RubyType::Unknown],
            vec![RubyType::Unknown],
        ));
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
