use tower_lsp::lsp_types::{
    InlayHint, InlayHintKind, InlayHintLabel, InlayHintOptions, InlayHintParams,
    InlayHintServerCapabilities, InlayHintTooltip, Position, Range, WorkDoneProgressOptions,
};

/// Check if a position is within a range
#[inline]
fn is_position_in_range(pos: &Position, range: &Range) -> bool {
    (pos.line > range.start.line
        || (pos.line == range.start.line && pos.character >= range.start.character))
        && (pos.line < range.end.line
            || (pos.line == range.end.line && pos.character <= range.end.character))
}

use crate::indexer::entry::entry_kind::EntryKind;
use crate::server::RubyLanguageServer;
use crate::type_inference::return_type_inferrer::ReturnTypeInferrer;
use crate::type_inference::ruby_type::RubyType;
use tower_lsp::lsp_types::Url;

pub fn get_inlay_hints_capability() -> InlayHintServerCapabilities {
    InlayHintServerCapabilities::Options(InlayHintOptions {
        work_done_progress_options: WorkDoneProgressOptions::default(),
        resolve_provider: Some(false),
    })
}

use crate::utils::position_to_offset;

pub async fn handle_inlay_hints(
    server: &RubyLanguageServer,
    params: InlayHintParams,
) -> Vec<InlayHint> {
    let uri = params.text_document.uri;
    let requested_range = params.range;

    // Only provide hints for open files (documents in memory)
    let document_guard = server.docs.lock();
    let doc_arc = match document_guard.get(&uri) {
        Some(doc) => doc.clone(),
        None => {
            // File is not open - no inlay hints available
            return Vec::new();
        }
    };
    drop(document_guard);

    let document = doc_arc.read();
    let content = document.content.clone();

    // Lazily infer return types for methods in the VISIBLE RANGE only
    // This keeps performance fast - only analyze methods the user can see
    drop(document); // Drop read lock before calling inference
    infer_methods_in_range(server, &uri, &content, &requested_range);

    // Re-acquire document for structural hints
    let document = doc_arc.read();

    // Get structural hints from the document (filtered to range)
    let mut all_hints: Vec<InlayHint> = document
        .get_inlay_hints()
        .into_iter()
        .filter(|h| is_position_in_range(&h.position, &requested_range))
        .collect();

    // Generate type hints from document.lvars for local variables
    for entries in document.get_all_lvars().values() {
        for entry in entries {
            // Skip entries outside the requested range
            if !is_position_in_range(&entry.location.range.start, &requested_range)
                && !is_position_in_range(&entry.location.range.end, &requested_range)
            {
                continue;
            }

            if let EntryKind::LocalVariable(data) = &entry.kind {
                let r#type = &data.r#type;
                let name = &data.name;
                let from_lvar = if r#type != &RubyType::Unknown {
                    Some(r#type.clone())
                } else {
                    None
                };

                // Try type narrowing if not from lvar
                let from_narrowing = from_lvar.clone().or_else(|| {
                    let offset = position_to_offset(&content, entry.location.range.start);
                    server.type_narrowing.get_narrowed_type(&uri, name, offset)
                });

                // Try method chain assignment inference if still unknown
                let index = server.index.lock();
                let final_type =
                    from_narrowing.or_else(|| infer_type_from_assignment(&content, name, &index));
                drop(index);

                if let Some(ty) = final_type {
                    if ty != RubyType::Unknown {
                        let end_position = entry.location.range.end;
                        let type_hint = InlayHint {
                            position: end_position,
                            label: InlayHintLabel::String(format!(": {}", ty)),
                            kind: Some(InlayHintKind::TYPE),
                            text_edits: None,
                            tooltip: None,
                            padding_left: None,
                            padding_right: None,
                            data: None,
                        };
                        all_hints.push(type_hint);
                    }
                }
            }
        }
    }
    drop(document);

    // Collect method entries that need return type inference
    // We need to do this in two passes to avoid holding the lock during inference
    struct MethodHintData {
        return_type: Option<RubyType>,
        return_type_position: Option<Position>,
        yard_return_str: Option<String>,
        yard_return_desc: Option<String>,
        param_hints: Vec<InlayHint>,
    }

    let method_data: Vec<MethodHintData>;
    {
        let index = server.index.lock();
        let entries = index.file_entries(&uri);

        // Generate hints for non-method entries and collect method data
        for entry in &entries {
            // Skip entries outside the requested range
            if !is_position_in_range(&entry.location.range.start, &requested_range)
                && !is_position_in_range(&entry.location.range.end, &requested_range)
            {
                continue;
            }

            // Extract type from variable entries if present
            let var_type = match &entry.kind {
                EntryKind::InstanceVariable(data) => Some(&data.r#type),
                EntryKind::ClassVariable(data) => Some(&data.r#type),
                EntryKind::GlobalVariable(data) => Some(&data.r#type),
                _ => None,
            };

            if let Some(r#type) = var_type {
                if *r#type != RubyType::Unknown {
                    let end_position = entry.location.range.end;
                    let type_hint = InlayHint {
                        position: end_position,
                        label: InlayHintLabel::String(format!(": {}", r#type)),
                        kind: Some(InlayHintKind::TYPE),
                        text_edits: None,
                        tooltip: None,
                        padding_left: None,
                        padding_right: None,
                        data: None,
                    };
                    all_hints.push(type_hint);
                }
            }
        }

        // Collect method data for second pass (lazy inference happens outside lock)
        method_data = entries
            .iter()
            .filter(|entry| {
                is_position_in_range(&entry.location.range.start, &requested_range)
                    || is_position_in_range(&entry.location.range.end, &requested_range)
            })
            .filter_map(|entry| {
                if let EntryKind::Method(data) = &entry.kind {
                    let mut param_hints = Vec::new();
                    if let Some(doc) = &data.yard_doc {
                        for param in &data.params {
                            if let Some(type_str) = doc.get_param_type_str(&param.name) {
                                let yard_param = doc.params.iter().find(|p| p.name == param.name);
                                let label = if param.has_colon() {
                                    format!(" {}", type_str)
                                } else {
                                    format!(": {}", type_str)
                                };
                                let hint = InlayHint {
                                    position: param.end_position,
                                    label: InlayHintLabel::String(label),
                                    kind: Some(InlayHintKind::TYPE),
                                    text_edits: None,
                                    tooltip: yard_param
                                        .and_then(|p| p.description.clone())
                                        .map(InlayHintTooltip::String),
                                    padding_left: None,
                                    padding_right: None,
                                    data: None,
                                };
                                param_hints.push(hint);
                            }
                        }
                    }

                    Some(MethodHintData {
                        return_type: data.return_type.clone(),
                        return_type_position: data.return_type_position,
                        yard_return_str: data
                            .yard_doc
                            .as_ref()
                            .and_then(|doc| doc.format_return_type()),
                        yard_return_desc: data
                            .yard_doc
                            .as_ref()
                            .and_then(|doc| doc.get_return_description().cloned()),
                        param_hints,
                    })
                } else {
                    None
                }
            })
            .collect();
    }
    // Index lock is dropped here

    // Now process method hints with lazy inference (no lock held)
    for data in method_data {
        // Add parameter hints
        all_hints.extend(data.param_hints);

        // Get return type - either from index or infer lazily
        let return_type_value: Option<RubyType> = if let Some(rt) = &data.return_type {
            if *rt != RubyType::Unknown {
                Some(rt.clone())
            } else {
                None
            }
        } else {
            // Lazy inference (now safe - no lock held)
            if let Some(pos) = data.return_type_position {
                infer_return_type_for_method(server, &uri, &content, pos.line)
            } else {
                None
            }
        };

        // Priority: Inferred/RBS return type > YARD documentation
        let return_type_str = return_type_value
            .map(|rt| rt.to_string())
            .or_else(|| data.yard_return_str.clone());

        if let (Some(type_str), Some(pos)) = (return_type_str, data.return_type_position) {
            let hint = InlayHint {
                position: pos,
                label: InlayHintLabel::String(format!(" -> {}", type_str)),
                kind: Some(InlayHintKind::TYPE),
                text_edits: None,
                tooltip: data.yard_return_desc.map(InlayHintTooltip::String),
                padding_left: None,
                padding_right: None,
                data: None,
            };
            all_hints.push(hint);
        }
    }

    all_hints
}

/// Lazily infer return type for a method at the given line.
/// This is called when inlay hints are requested and the method doesn't have an explicit return type.
fn infer_return_type_for_method(
    server: &RubyLanguageServer,
    _uri: &Url,
    content: &str,
    line: u32,
) -> Option<RubyType> {
    // Parse the content to get the AST
    let parse_result = ruby_prism::parse(content.as_bytes());
    let node = parse_result.node();

    // Find the DefNode at the given line
    let def_node = find_def_node_at_line(&node, line, content)?;

    // Create inferrer and infer return type
    let inferrer = ReturnTypeInferrer::new(server.index.clone());
    let inferred_ty = inferrer.infer_return_type(content.as_bytes(), &def_node)?;

    if inferred_ty != RubyType::Unknown {
        Some(inferred_ty)
    } else {
        None
    }
}

/// Infer return types for methods in the VISIBLE RANGE only.
/// This is called lazily when hints are requested for a specific viewport.
/// Only methods within the range are analyzed - keeps performance fast O(visible methods).
fn infer_methods_in_range(server: &RubyLanguageServer, uri: &Url, content: &str, range: &Range) {
    use crate::indexer::entry::entry_kind::EntryKind;
    use crate::indexer::index::EntryId;

    // Collect only method entries that:
    // 1. Are within the visible range
    // 2. Need inference (return_type is None)
    let methods_needing_inference: Vec<(u32, EntryId)> = {
        let index = server.index.lock();
        index
            .get_entry_ids_for_uri(uri)
            .iter()
            .filter_map(|&entry_id| {
                if let Some(entry) = index.get_entry(entry_id) {
                    if let EntryKind::Method(data) = &entry.kind {
                        // Check if method is within visible range
                        let method_line = entry.location.range.start.line;
                        if method_line >= range.start.line && method_line <= range.end.line {
                            // Only include if needs inference
                            if data.return_type.is_none() {
                                if let Some(pos) = data.return_type_position {
                                    return Some((pos.line, entry_id));
                                }
                            }
                        }
                    }
                }
                None
            })
            .collect()
    };

    // Fast path: nothing to infer
    if methods_needing_inference.is_empty() {
        return;
    }

    // Parse the file and infer only the visible methods
    // Using new_with_content enables on-demand inference of called methods
    let parse_result = ruby_prism::parse(content.as_bytes());
    let node = parse_result.node();
    let inferrer = ReturnTypeInferrer::new_with_content(server.index.clone(), content.as_bytes());

    // Infer and cache (no lock held during inference)
    let inferred_types: Vec<(EntryId, RubyType)> = methods_needing_inference
        .iter()
        .filter_map(|(line, entry_id)| {
            let def_node = find_def_node_at_line(&node, *line, content)?;
            let inferred_ty = inferrer.infer_return_type(content.as_bytes(), &def_node)?;
            if inferred_ty != RubyType::Unknown {
                Some((*entry_id, inferred_ty))
            } else {
                None
            }
        })
        .collect();

    // Update the index (brief lock)
    if !inferred_types.is_empty() {
        let mut index = server.index.lock();
        for (entry_id, inferred_ty) in inferred_types {
            index.update_method_return_type(entry_id, inferred_ty);
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::RubyLanguageServer;
    use tower_lsp::lsp_types::{
        DidOpenTextDocumentParams, InitializeParams, Range, TextDocumentIdentifier,
        TextDocumentItem, Url,
    };
    use tower_lsp::LanguageServer;

    async fn create_test_server() -> RubyLanguageServer {
        let server = RubyLanguageServer::default();
        let _ = server.initialize(InitializeParams::default()).await;
        server
    }

    #[tokio::test]
    async fn test_inlay_hints_for_variable_to_variable_assignment() {
        let server = create_test_server().await;
        let uri = Url::parse("file:///test_inlay.rb").unwrap();
        let content = r#"a = 'str'
b = a"#;

        // Open the document
        let params = DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: "ruby".into(),
                version: 1,
                text: content.to_string(),
            },
        };
        server.did_open(params).await;

        // Request inlay hints
        let inlay_params = InlayHintParams {
            work_done_progress_params: Default::default(),
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 2,
                    character: 0,
                },
            },
        };

        let hints = handle_inlay_hints(&server, inlay_params).await;

        // Should have hints for both 'a' and 'b'
        let a_hint = hints.iter().find(|h| {
            if let InlayHintLabel::String(s) = &h.label {
                s.contains("String") && h.position.line == 0
            } else {
                false
            }
        });
        assert!(a_hint.is_some(), "Should have type hint for 'a'");

        let b_hint = hints.iter().find(|h| {
            if let InlayHintLabel::String(s) = &h.label {
                s.contains("String") && h.position.line == 1
            } else {
                false
            }
        });
        assert!(
            b_hint.is_some(),
            "Should have type hint for 'b' (inherited from 'a' via CFG)"
        );
    }
}

/// Infer type from assignment patterns like `var = Class.new.method`.
/// Handles constructor calls and method chains.
fn infer_type_from_assignment(
    content: &str,
    var_name: &str,
    index: &crate::indexer::index::RubyIndex,
) -> Option<RubyType> {
    use crate::type_inference::method_resolver::MethodResolver;
    use crate::types::fully_qualified_name::FullyQualifiedName;
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
