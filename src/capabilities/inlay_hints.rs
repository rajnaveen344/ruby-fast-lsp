use tower_lsp::lsp_types::{
    InlayHint, InlayHintKind, InlayHintLabel, InlayHintOptions, InlayHintParams,
    InlayHintServerCapabilities, InlayHintTooltip, Position, Range, WorkDoneProgressOptions,
};

use crate::indexer::entry::entry_kind::EntryKind;
use crate::inferrer::r#type::ruby::RubyType;
use crate::query::{IndexQuery, TypeQuery};
use crate::server::RubyLanguageServer;

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

    // Get document content and Arc safely
    let (content, doc_arc) = {
        let doc_guard = server.docs.lock();
        match doc_guard.get(&uri) {
            Some(doc_arc) => {
                let doc = doc_arc.read();
                (doc.content.clone(), doc_arc.clone())
            }
            None => return Vec::new(),
        }
    };

    // Create unified query context (owns index reference, so no direct server.index.lock needed)
    let query = IndexQuery::with_doc(server.index.clone(), doc_arc.clone());

    // infer_and_update_visible_types handles the "dirty" work of AST parsing
    // and updating the index with inferred return types.
    // This encapsulates the index access within the Query layer.
    query.infer_and_update_visible_types(&uri, &content, &requested_range);

    // Re-acquire document for structural hints (read lock)
    let document = doc_arc.read();

    // Get structural hints from the document (filtered to range)
    let mut all_hints: Vec<InlayHint> = document
        .get_inlay_hints()
        .into_iter()
        .filter(|h| is_position_in_range(&h.position, &requested_range))
        .collect();

    // Generate type hints from document.lvars for local variables
    let mut type_updates = Vec::new();

    eprintln!("Generating inlay hints for doc: {}", uri);
    let lvars = document.get_all_lvars();

    for (_scope_id, entries) in lvars {
        for entry in entries {
            // Skip entries outside the requested range
            if !is_position_in_range(&entry.location.range.start, &requested_range)
                && !is_position_in_range(&entry.location.range.end, &requested_range)
            {
                continue;
            }

            if let EntryKind::LocalVariable(data) = &entry.kind {
                // Get type from document (base type or persisted flow type)
                let from_lvar = data.assignments.last().map(|a| a.r#type.clone());

                // Try type narrowing
                let from_narrowing = match from_lvar.clone() {
                    Some(ty) if ty != RubyType::Unknown => Some(ty),
                    _ => {
                        let offset = position_to_offset(&content, entry.location.range.start);
                        server
                            .type_narrowing
                            .get_narrowed_type(&uri, offset, Some(&content))
                    }
                };

                // Resolve final type using Query layer logic
                let final_type = query.resolve_local_var_type(
                    &content,
                    &data.name,
                    from_lvar.as_ref(),
                    from_narrowing.clone(),
                );

                if let Some(ty) = final_type {
                    // Collect update if we inferred something new
                    // Check against what we already have in the document (from_lvar)
                    let matches_existing = from_lvar.as_ref() == Some(&ty);
                    if !matches_existing && ty != RubyType::Unknown {
                        type_updates.push((
                            data.scope_id,
                            data.name.clone(),
                            entry.location.range,
                            ty.clone(),
                        ));
                    }

                    let label = if ty == RubyType::Unknown {
                        ": ?".to_string()
                    } else {
                        format!(": {}", ty)
                    };

                    let end_position = entry.location.range.end;
                    let type_hint = InlayHint {
                        position: end_position,
                        label: InlayHintLabel::String(label),
                        kind: Some(InlayHintKind::TYPE),
                        text_edits: None,
                        tooltip: None,
                        padding_left: None,
                        padding_right: None,
                        data: None,
                    };
                    all_hints.push(type_hint);
                } else {
                    // If we have an entry, it's a variable. If we don't know the type, show ?
                    let label = ": ?".to_string();
                    let end_position = entry.location.range.end;
                    let type_hint = InlayHint {
                        position: end_position,
                        label: InlayHintLabel::String(label),
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
    drop(document);

    // Apply updates to the document (persistence)
    if !type_updates.is_empty() {
        let mut document = doc_arc.write(); // Apply updates to document
        for (scope_id, name, range, ty) in type_updates {
            document.update_local_var_type(scope_id, &name, range, ty);
        }
    }

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
        // Use query layer to get entries instead of raw lock if possible,
        // but for iteration we still need access.
        // Ideally we would move this filtering logic to Query too, but for now
        // we use index_ref() to access.
        let index_ref = query.index_ref();
        let index = index_ref.lock();
        let entries = index.file_entries(&uri);

        // Generate hints for non-method entries and collect method data
        // ... (rest of logic remains similar but using the index from query)
        // Note: The original code iterated over entries. We'll keep this structure
        // but use the index from query context.

        let mut local_hints = Vec::new(); // Temporary collection

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
                    local_hints.push(type_hint);
                }
            }
        }
        all_hints.extend(local_hints);

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
