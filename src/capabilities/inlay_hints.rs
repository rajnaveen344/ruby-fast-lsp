use tower_lsp::lsp_types::{
    InlayHint, InlayHintKind, InlayHintLabel, InlayHintOptions, InlayHintParams,
    InlayHintServerCapabilities, InlayHintTooltip, Position, Range, WorkDoneProgressOptions,
};

use crate::indexer::entry::entry_kind::EntryKind;
use crate::inferrer::query::{infer_type_from_assignment, TypeQuery};
use crate::inferrer::r#type::ruby::RubyType;
use crate::inferrer::return_type::ReturnTypeInferrer;
use crate::server::RubyLanguageServer;
use tower_lsp::lsp_types::Url;

pub fn get_inlay_hints_capability() -> InlayHintServerCapabilities {
    InlayHintServerCapabilities::Options(InlayHintOptions {
        work_done_progress_options: WorkDoneProgressOptions::default(),
        resolve_provider: Some(false),
    })
}

use crate::utils::position_to_offset;

/// Check if a position is within a range
#[inline]
fn is_position_in_range(pos: &Position, range: &Range) -> bool {
    TypeQuery::is_in_range(pos, range)
}

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
                    server
                        .type_narrowing
                        .get_narrowed_type(&uri, offset, Some(&content))
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

    // Now process method hints - return types were already inferred by infer_methods_in_range
    for data in method_data {
        // Add parameter hints
        all_hints.extend(data.param_hints);

        // Priority: Inferred/RBS return type > YARD documentation
        let return_type_str = data
            .return_type
            .as_ref()
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

    // Parse the file ONCE and infer only the visible methods
    // Using new_with_content with URI enables on-demand inference of called methods
    // The URI is used to filter methods to only those in the SAME FILE, avoiding
    // the O(n) AST traversal problem where we used to search for methods from
    // other files in the current file's AST
    let parse_result = ruby_prism::parse(content.as_bytes());
    let node = parse_result.node();

    let inferrer =
        ReturnTypeInferrer::new_with_content(server.index.clone(), content.as_bytes(), uri);

    // Infer and cache (no lock held during inference)
    let inferred_types: Vec<(EntryId, RubyType)> = methods_needing_inference
        .iter()
        .filter_map(|(line, entry_id)| {
            let def_node = find_def_node_at_line(&node, *line, content)?;
            let inferred_ty = inferrer.infer_return_type(content.as_bytes(), &def_node)?;
            Some((*entry_id, inferred_ty))
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
