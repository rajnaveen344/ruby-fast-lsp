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

use crate::analyzer_prism::{Identifier, IdentifierType, RubyPrismAnalyzer};
use crate::indexer::entry::entry_kind::EntryKind;
use crate::indexer::index::RubyIndex;
use crate::inferrer::query::infer_type_from_assignment;
use crate::inferrer::r#type::ruby::RubyType;
use crate::inferrer::return_type::ReturnTypeInferrer;
use crate::server::RubyLanguageServer;
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
    let (identifier_opt, identifier_type, _namespace, scope_id) = analyzer.get_identifier(position);
    let identifier = identifier_opt?;

    let hover_text = match &identifier {
        Identifier::RubyLocalVariable { name, .. } => {
            // Look up type from document lvars
            let offset = crate::utils::position_to_offset(&content, position);
            let index = server.index.lock();
            let type_str =
                get_local_variable_type(server, &uri, name, scope_id, &content, offset, &index)
                    .map(|t| t.to_string());

            // If not found in lvars, try type narrowing engine
            let type_from_narrowing = type_str.or_else(|| {
                let offset = position_to_offset(&content, position);
                server
                    .type_narrowing
                    .get_narrowed_type(&uri, offset, Some(&content))
                    .map(|t| t.to_string())
            });

            // If still not found, try inferring from constructor/method chain assignment
            let final_type = type_from_narrowing.or_else(|| {
                infer_type_from_assignment(&content, name, &index).map(|t| t.to_string())
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
            let is_method_definition = identifier_type == Some(IdentifierType::MethodDef);

            // Special handling for .new - return the class instance type
            if method_name == "new" && !is_method_definition {
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

            // For method definitions, look up directly in index by method name at position
            if is_method_definition {
                return handle_method_definition_hover(
                    server,
                    &uri,
                    &content,
                    position,
                    &method_name,
                );
            }

            let index = server.index.lock();

            // Resolve receiver type - distinguish between Class and Module for proper method resolution
            let receiver_type = match receiver {
                crate::analyzer_prism::MethodReceiver::None
                | crate::analyzer_prism::MethodReceiver::SelfReceiver => {
                    // Implicit self or explicit self
                    if namespace.is_empty() {
                        RubyType::class("Object")
                    } else {
                        let fqn = FullyQualifiedName::from(namespace.clone());
                        // Check if this is a module (not a class) for proper method resolution
                        let is_module = index.get(&fqn).map_or(false, |entries| {
                            entries
                                .iter()
                                .any(|e| matches!(e.kind, EntryKind::Module(_)))
                        });
                        if is_module {
                            RubyType::Module(fqn)
                        } else {
                            RubyType::Class(fqn)
                        }
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
                    // Pass &index (already locked) - no double-lock possible
                    get_local_variable_type(server, &uri, name, scope_id, &content, offset, &index)
                        .unwrap_or(RubyType::Unknown)
                }
                // TODO: Handle other receiver types (InstanceVar, chains, etc)
                _ => RubyType::Unknown,
            };

            // Use MethodResolver to find return type
            // This now handles both class and module contexts, including ancestor chain traversal
            // and searching through classes that include a module
            let return_type =
                crate::inferrer::method::resolver::MethodResolver::resolve_method_return_type(
                    &index,
                    &receiver_type,
                    &method_name,
                );

            // Fallback: Search in file if resolution fails (legacy behavior)
            // Now updated to collect ALL matches in the file and union them
            // Also triggers on-demand inference for methods without return types
            let return_type = return_type.or_else(|| {
                let mut local_types: Vec<RubyType> = Vec::new();
                let mut methods_needing_inference: Vec<(u32, crate::indexer::index::EntryId)> =
                    Vec::new();

                for entry_id in index.get_entry_ids_for_uri(&uri) {
                    if let Some(entry) = index.get_entry(entry_id) {
                        if let EntryKind::Method(data) = &entry.kind {
                            if data.name.to_string() == method_name {
                                if let Some(rt) = &data.return_type {
                                    if *rt != RubyType::Unknown {
                                        local_types.push(rt.clone());
                                    }
                                } else if let Some(pos) = data.return_type_position {
                                    // Collect for on-demand inference
                                    methods_needing_inference.push((pos.line, entry_id));
                                }
                            }
                        }
                    }
                }

                // If we found types, return them
                if !local_types.is_empty() {
                    return Some(RubyType::union(local_types));
                }

                // Otherwise try on-demand inference for methods in this file
                if !methods_needing_inference.is_empty() {
                    drop(index); // Drop lock before inference

                    // Parse and infer
                    let parse_result = ruby_prism::parse(content.as_bytes());
                    let node = parse_result.node();
                    let inferrer = ReturnTypeInferrer::new_with_content(
                        server.index.clone(),
                        content.as_bytes(),
                        &uri,
                    );

                    for (line, entry_id) in methods_needing_inference {
                        if let Some(def_node) = find_def_node_at_line(&node, line, &content) {
                            if let Some(inferred_ty) =
                                inferrer.infer_return_type(content.as_bytes(), &def_node)
                            {
                                if inferred_ty != RubyType::Unknown {
                                    // Cache in index
                                    server
                                        .index
                                        .lock()
                                        .update_method_return_type(entry_id, inferred_ty.clone());
                                    local_types.push(inferred_ty);
                                }
                            }
                        }
                    }

                    if !local_types.is_empty() {
                        return Some(RubyType::union(local_types));
                    }
                }

                None
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

/// Handle hover for method definitions - shows inferred/documented return type
fn handle_method_definition_hover(
    server: &RubyLanguageServer,
    uri: &tower_lsp::lsp_types::Url,
    content: &str,
    position: tower_lsp::lsp_types::Position,
    method_name: &str,
) -> Option<Hover> {
    // Find the method entry at this position
    let index = server.index.lock();

    // Find method entry at position
    let method_entry = index.file_entries(uri).into_iter().find(|entry| {
        if let EntryKind::Method(data) = &entry.kind {
            if data.name.to_string() == method_name {
                // Check if position is within the method's range
                let range = &entry.location.range;
                return position.line >= range.start.line && position.line <= range.end.line;
            }
        }
        false
    });

    if let Some(entry) = method_entry {
        if let EntryKind::Method(data) = &entry.kind {
            // Check if we already have a return type
            if let Some(rt) = &data.return_type {
                if *rt != RubyType::Unknown {
                    return Some(Hover {
                        contents: HoverContents::Markup(MarkupContent {
                            kind: MarkupKind::Markdown,
                            value: format!("```ruby\ndef {} -> {}\n```", method_name, rt),
                        }),
                        range: None,
                    });
                }
            }

            // Check YARD docs
            if let Some(yard_doc) = &data.yard_doc {
                if let Some(return_type) = yard_doc.format_return_type() {
                    return Some(Hover {
                        contents: HoverContents::Markup(MarkupContent {
                            kind: MarkupKind::Markdown,
                            value: format!("```ruby\ndef {} -> {}\n```", method_name, return_type),
                        }),
                        range: None,
                    });
                }
            }

            // Try on-demand inference
            if let Some(pos) = data.return_type_position {
                let entry_id_opt = index.get_entry_ids_for_uri(uri).into_iter().find(|eid| {
                    if let Some(e) = index.get_entry(*eid) {
                        if let EntryKind::Method(d) = &e.kind {
                            return d.name.to_string() == method_name
                                && d.return_type_position == Some(pos);
                        }
                    }
                    false
                });

                if let Some(entry_id) = entry_id_opt {
                    drop(index); // Release lock before parsing

                    // Parse and infer
                    let parse_result = ruby_prism::parse(content.as_bytes());
                    let node = parse_result.node();
                    let inferrer = ReturnTypeInferrer::new_with_content(
                        server.index.clone(),
                        content.as_bytes(),
                        uri,
                    );

                    if let Some(def_node) = find_def_node_at_line(&node, pos.line, content) {
                        if let Some(inferred_ty) =
                            inferrer.infer_return_type(content.as_bytes(), &def_node)
                        {
                            if inferred_ty != RubyType::Unknown {
                                // Cache in index
                                server
                                    .index
                                    .lock()
                                    .update_method_return_type(entry_id, inferred_ty.clone());

                                return Some(Hover {
                                    contents: HoverContents::Markup(MarkupContent {
                                        kind: MarkupKind::Markdown,
                                        value: format!(
                                            "```ruby\ndef {} -> {}\n```",
                                            method_name, inferred_ty
                                        ),
                                    }),
                                    range: None,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    // Fallback - just show the method name
    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: format!("```ruby\ndef {}\n```", method_name),
        }),
        range: None,
    })
}

/// Helper to resolve local variable type.
///
/// Takes `&RubyIndex` directly - caller must have already locked.
/// This prevents double-locking at compile time.
fn get_local_variable_type(
    server: &RubyLanguageServer,
    uri: &tower_lsp::lsp_types::Url,
    name: &str,
    scope_id: crate::types::scope::LVScopeId,
    content: &str,
    offset: usize,
    index: &RubyIndex,
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
    if let Some(type_from_narrowing) =
        server
            .type_narrowing
            .get_narrowed_type(uri, offset, Some(content))
    {
        return Some(type_from_narrowing);
    }

    // 3. Try inferring from assignment (index already locked - no double-lock risk)
    infer_type_from_assignment(content, name, index)
}

/// Find a DefNode at the given line in the AST.
fn find_def_node_at_line<'a>(
    node: &ruby_prism::Node<'a>,
    target_line: u32,
    content: &str,
) -> Option<ruby_prism::DefNode<'a>> {
    // Try to match DefNode
    if let Some(def_node) = node.as_def_node() {
        let offset = def_node.location().start_offset();
        // Calculate line from byte offset (count newlines before this offset)
        let line = content.as_bytes()[..offset]
            .iter()
            .filter(|&&b| b == b'\n')
            .count() as u32;
        if line == target_line {
            return Some(def_node);
        }
    }

    // Recurse into child nodes
    if let Some(program) = node.as_program_node() {
        for stmt in program.statements().body().iter() {
            if let Some(found) = find_def_node_at_line(&stmt, target_line, content) {
                return Some(found);
            }
        }
    }

    if let Some(class_node) = node.as_class_node() {
        if let Some(body) = class_node.body() {
            if let Some(stmts) = body.as_statements_node() {
                for stmt in stmts.body().iter() {
                    if let Some(found) = find_def_node_at_line(&stmt, target_line, content) {
                        return Some(found);
                    }
                }
            }
        }
    }

    if let Some(module_node) = node.as_module_node() {
        if let Some(body) = module_node.body() {
            if let Some(stmts) = body.as_statements_node() {
                for stmt in stmts.body().iter() {
                    if let Some(found) = find_def_node_at_line(&stmt, target_line, content) {
                        return Some(found);
                    }
                }
            }
        }
    }

    if let Some(stmts) = node.as_statements_node() {
        for stmt in stmts.body().iter() {
            if let Some(found) = find_def_node_at_line(&stmt, target_line, content) {
                return Some(found);
            }
        }
    }

    None
}
