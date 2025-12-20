//! Hover capability for displaying type information.
//!
//! Provides hover information for:
//! - Local variables (shows inferred type)
//! - Methods (shows return type)
//! - Classes/Modules (shows class/module name)
//! - Constants (shows type or value info)

use tower_lsp::lsp_types::{
    Hover, HoverContents, HoverParams, HoverProviderCapability, MarkupContent, MarkupKind,
};

use crate::analyzer_prism::{Identifier, RubyPrismAnalyzer};
use crate::indexer::entry::entry_kind::EntryKind;
use crate::server::RubyLanguageServer;
use crate::type_inference::ruby_type::RubyType;
use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::utils::position_to_offset;

/// Return the hover capability.
pub fn get_hover_capability() -> HoverProviderCapability {
    HoverProviderCapability::Simple(true)
}

/// Handle hover request.
pub async fn handle_hover(server: &RubyLanguageServer, params: HoverParams) -> Option<Hover> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;

    // Get document content
    let content = {
        let docs = server.docs.lock();
        let doc_arc = docs.get(&uri)?;
        let doc = doc_arc.read();
        doc.content.clone()
    };

    // Get identifier at position
    let analyzer = RubyPrismAnalyzer::new(uri.clone(), content.clone());
    let (identifier_opt, _namespace, scope_id) = analyzer.get_identifier(position);
    let identifier = identifier_opt?;

    let hover_text = match &identifier {
        Identifier::RubyLocalVariable { name, .. } => {
            // Look up type from document lvars
            let offset = crate::utils::position_to_offset(&content, position);
            let type_str = get_local_variable_type(server, &uri, name, scope_id, &content, offset)
                .map(|t| t.to_string());

            // If not found in lvars, try type narrowing engine
            let type_from_narrowing = type_str.or_else(|| {
                let offset = position_to_offset(&content, position);
                server
                    .type_narrowing
                    .get_narrowed_type(&uri, name, offset)
                    .map(|t| t.to_string())
            });

            // If still not found, try inferring from constructor/method chain assignment
            let final_type = type_from_narrowing.or_else(|| {
                infer_type_from_assignment(&content, name, &server.index.lock())
                    .map(|t| t.to_string())
            });

            match final_type {
                Some(t) => format!("{}", t),
                None => name.clone(),
            }
        }

        Identifier::RubyConstant { iden, .. } => {
            // Build FQN and look up in index
            let fqn = FullyQualifiedName::namespace(iden.clone());
            let index = server.index.lock();

            if let Some(entries) = index.get(&fqn) {
                // Find if it's a class or module
                let entry_kind = entries.iter().find_map(|entry| match &entry.kind {
                    EntryKind::Class(_) => Some("class"),
                    EntryKind::Module(_) => Some("module"),
                    _ => None,
                });

                let fqn_str = iden
                    .iter()
                    .map(|c| c.to_string())
                    .collect::<Vec<_>>()
                    .join("::");

                match entry_kind {
                    Some("class") => format!("class {}", fqn_str),
                    Some("module") => format!("module {}", fqn_str),
                    _ => fqn_str,
                }
            } else {
                iden.iter()
                    .map(|c| c.to_string())
                    .collect::<Vec<_>>()
                    .join("::")
            }
        }

        Identifier::RubyMethod {
            iden,
            receiver,
            namespace,
        } => {
            let method_name = iden.to_string();

            // Special handling for .new - return the class instance type
            if method_name == "new" {
                if let crate::analyzer_prism::MethodReceiver::Constant(parts) = receiver {
                    let fqn_str = parts
                        .iter()
                        .map(|c| c.to_string())
                        .collect::<Vec<_>>()
                        .join("::");
                    return Some(Hover {
                        contents: HoverContents::Markup(MarkupContent {
                            kind: MarkupKind::Markdown,
                            value: format!("```ruby\n{}\n```", fqn_str),
                        }),
                        range: None,
                    });
                }
            }

            let index = server.index.lock();

            // Resolve receiver type
            let receiver_type = match receiver {
                crate::analyzer_prism::MethodReceiver::None
                | crate::analyzer_prism::MethodReceiver::SelfReceiver => {
                    // Implicit self or explicit self
                    if namespace.is_empty() {
                        RubyType::class("Object")
                    } else {
                        // Assume instance method context for now (TODO: Handle singleton methods)
                        let fqn = FullyQualifiedName::from(namespace.clone());
                        RubyType::Class(fqn)
                    }
                }
                crate::analyzer_prism::MethodReceiver::Constant(path) => {
                    // Constant receiver (e.g. valid class/module)
                    // Treat as ClassReference
                    let fqn = FullyQualifiedName::Constant(path.clone().into());
                    RubyType::ClassReference(fqn)
                }
                crate::analyzer_prism::MethodReceiver::LocalVariable(name) => {
                    let offset = crate::utils::position_to_offset(&content, position);
                    get_local_variable_type(server, &uri, name, scope_id, &content, offset)
                        .unwrap_or(RubyType::Unknown)
                }
                // TODO: Handle other receiver types (InstanceVar, chains, etc)
                _ => RubyType::Unknown,
            };

            // Use MethodResolver to find return type
            let return_type =
                crate::type_inference::method_resolver::MethodResolver::resolve_method_return_type(
                    &index,
                    &receiver_type,
                    &method_name,
                );

            // Fallback: Naive search in file if resolution fails (legacy behavior)
            // Now updated to collect ALL matches in the file and union them
            let return_type = return_type.or_else(|| {
                let local_types: Vec<RubyType> = index
                    .file_entries(&uri)
                    .iter()
                    .filter_map(|entry| {
                        if let EntryKind::Method(data) = &entry.kind {
                            if data.name.to_string() == method_name {
                                return data.return_type.clone();
                            }
                        }
                        None
                    })
                    .collect();

                if !local_types.is_empty() {
                    Some(RubyType::union(local_types))
                } else {
                    None
                }
            });

            match return_type {
                Some(t) => format!("```ruby\n{}\n```", t),
                None => format!("```ruby\ndef {}\n```", method_name),
            }
        }

        Identifier::RubyInstanceVariable { name, .. } => {
            let index = server.index.lock();
            let type_str = index.file_entries(&uri).iter().find_map(|entry| {
                if let EntryKind::InstanceVariable(data) = &entry.kind {
                    if &data.name == name && data.r#type != RubyType::Unknown {
                        return Some(data.r#type.to_string());
                    }
                }
                None
            });

            match type_str {
                Some(t) => format!("{}: {}", name, t),
                None => name.clone(),
            }
        }

        Identifier::RubyClassVariable { name, .. } => {
            let index = server.index.lock();
            let type_str = index.file_entries(&uri).iter().find_map(|entry| {
                if let EntryKind::ClassVariable(data) = &entry.kind {
                    if &data.name == name && data.r#type != RubyType::Unknown {
                        return Some(data.r#type.to_string());
                    }
                }
                None
            });

            match type_str {
                Some(t) => format!("{}: {}", name, t),
                None => name.clone(),
            }
        }

        Identifier::RubyGlobalVariable { name, .. } => {
            let index = server.index.lock();
            let type_str = index.file_entries(&uri).iter().find_map(|entry| {
                if let EntryKind::GlobalVariable(data) = &entry.kind {
                    if &data.name == name && data.r#type != RubyType::Unknown {
                        return Some(data.r#type.to_string());
                    }
                }
                None
            });

            match type_str {
                Some(t) => format!("{}: {}", name, t),
                None => name.clone(),
            }
        }

        Identifier::YardType { type_name, .. } => type_name.clone(),
    };

    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: hover_text,
        }),
        range: None,
    })
}

/// Infer type from assignment patterns like `var = Class.new.method`.
/// Handles constructor calls and method chains.
fn infer_type_from_assignment(
    content: &str,
    var_name: &str,
    index: &crate::indexer::index::RubyIndex,
) -> Option<RubyType> {
    use crate::type_inference::method_resolver::MethodResolver;
    use crate::types::ruby_namespace::RubyConstant;

    // Look for assignment pattern: `var_name = ...`
    for line in content.lines() {
        let trimmed = line.trim();

        // Look for assignment pattern: `var = ...`
        if let Some(rest) = trimmed.strip_prefix(var_name) {
            // Make sure we matched the whole variable name (not just a prefix)
            let next_char = rest.chars().next();
            if !matches!(next_char, Some(' ') | Some('\t') | Some('=')) {
                continue;
            }

            let rest = rest.trim();
            if let Some(rest) = rest.strip_prefix('=') {
                let rhs = rest.trim();

                // Look for .new somewhere in the chain
                if let Some(new_pos) = rhs.find(".new") {
                    // Extract the class name before .new
                    let class_part = rhs[..new_pos].trim();

                    // Validate it's a constant (starts with uppercase)
                    if !class_part
                        .chars()
                        .next()
                        .map(|c| c.is_uppercase())
                        .unwrap_or(false)
                    {
                        continue;
                    }

                    // Parse the constant path
                    let parts: Vec<_> = class_part
                        .split("::")
                        .filter_map(|s| RubyConstant::new(s.trim()).ok())
                        .collect();

                    if parts.is_empty() {
                        continue;
                    }

                    let class_fqn = FullyQualifiedName::Constant(parts.into());
                    let mut current_type = RubyType::Class(class_fqn);

                    // Check for method chain after .new
                    let after_new = &rhs[new_pos + 4..]; // Skip ".new"

                    // Skip any arguments after .new
                    let after_new = if after_new.starts_with('(') {
                        if let Some(close_paren) = after_new.find(')') {
                            &after_new[close_paren + 1..]
                        } else {
                            after_new
                        }
                    } else {
                        after_new
                    };

                    // Parse method chain: .method1.method2.method3
                    for method_call in after_new.split('.') {
                        let method_name = method_call
                            .split(|c: char| c == '(' || c.is_whitespace())
                            .next()
                            .unwrap_or("")
                            .trim();

                        if method_name.is_empty() {
                            continue;
                        }

                        // Look up the method's return type
                        if let Some(return_type) = MethodResolver::resolve_method_return_type(
                            index,
                            &current_type,
                            method_name,
                        ) {
                            current_type = return_type;
                        } else {
                            // Can't resolve this method, stop the chain
                            break;
                        }
                    }

                    return Some(current_type);
                }
            }
        }
    }

    None
}

/// Helper to resolve local variable type
fn get_local_variable_type(
    server: &RubyLanguageServer,
    uri: &tower_lsp::lsp_types::Url,
    name: &str,
    scope_id: crate::types::scope::LVScopeId,
    content: &str,
    offset: usize,
) -> Option<RubyType> {
    use crate::indexer::entry::entry_kind::EntryKind;

    // 1. Check document lvars
    {
        let docs = server.docs.lock();
        if let Some(doc_arc) = docs.get(uri) {
            let doc = doc_arc.read();
            if let Some(entries) = doc.get_local_var_entries(scope_id) {
                let found_type = entries.iter().find_map(|entry| {
                    if let EntryKind::LocalVariable(data) = &entry.kind {
                        if &data.name == name && data.r#type != RubyType::Unknown {
                            return Some(data.r#type.clone());
                        }
                    }
                    None
                });
                if found_type.is_some() {
                    return found_type;
                }
            }
        }
    }

    // 2. Try type narrowing engine
    if let Some(type_from_narrowing) = server.type_narrowing.get_narrowed_type(uri, name, offset) {
        return Some(type_from_narrowing);
    }

    // 3. Try inferring from assignment
    if let Some(inferred) = infer_type_from_assignment(content, name, &server.index.lock()) {
        return Some(inferred);
    }

    None
}
