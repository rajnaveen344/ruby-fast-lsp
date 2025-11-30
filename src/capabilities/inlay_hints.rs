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
use crate::type_inference::ruby_type::RubyType;

pub fn get_inlay_hints_capability() -> InlayHintServerCapabilities {
    InlayHintServerCapabilities::Options(InlayHintOptions {
        work_done_progress_options: WorkDoneProgressOptions::default(),
        resolve_provider: Some(false),
    })
}

use super::utils::position_to_offset;

pub async fn handle_inlay_hints(
    server: &RubyLanguageServer,
    params: InlayHintParams,
) -> Vec<InlayHint> {
    let uri = params.text_document.uri;
    let requested_range = params.range;

    let document_guard = server.docs.lock();
    let document = document_guard.get(&uri).unwrap().read();
    let content = document.content.clone();
    drop(document);
    drop(document_guard);

    // Get structural hints from the document (filtered to range)
    let document_guard = server.docs.lock();
    let document = document_guard.get(&uri).unwrap().read();
    let mut all_hints: Vec<InlayHint> = document
        .get_inlay_hints()
        .into_iter()
        .filter(|h| is_position_in_range(&h.position, &requested_range))
        .collect();
    drop(document);
    drop(document_guard);

    // Generate type hints from indexed entries - ONLY for entries in the requested range
    let index = server.index.lock();

    if let Some(entries) = index.file_entries.get(&uri) {
        for entry in entries {
            // Skip entries outside the requested range
            if !is_position_in_range(&entry.location.range.start, &requested_range)
                && !is_position_in_range(&entry.location.range.end, &requested_range)
            {
                continue;
            }

            match &entry.kind {
                EntryKind::LocalVariable { r#type, name, .. } => {
                    // Prefer CFG-based type inference (more accurate for ||, &&, etc.)
                    // Fall back to indexed type for method call results
                    let offset = position_to_offset(&content, entry.location.range.start);
                    let final_type = server
                        .type_narrowing
                        .get_narrowed_type(&uri, name, offset)
                        .or_else(|| {
                            // Fallback to indexed type (for method call results)
                            if *r#type != RubyType::Unknown {
                                Some(r#type.clone())
                            } else {
                                None
                            }
                        });

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
                EntryKind::InstanceVariable { r#type, .. }
                | EntryKind::ClassVariable { r#type, .. }
                | EntryKind::GlobalVariable { r#type, .. } => {
                    if *r#type != RubyType::Unknown {
                        // Create type hint at the end of the variable name
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
                // Generate inlay hints for methods
                EntryKind::Method {
                    yard_doc,
                    params,
                    return_type_position,
                    return_type,
                    ..
                } => {
                    // Generate individual type hints for each parameter (from YARD only for now)
                    if let Some(doc) = yard_doc {
                        for param in params {
                            if let Some(type_str) = doc.get_param_type_str(&param.name) {
                                let yard_param = doc.params.iter().find(|p| p.name == param.name);
                                // Keyword params already have a colon, so just add space + type
                                // Other params need ": type"
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
                                all_hints.push(hint);
                            }
                        }
                    }

                    // Generate return type hint at the end of the method signature
                    // Priority: YARD documentation > inferred return type
                    let return_type_str = yard_doc
                        .as_ref()
                        .and_then(|doc| doc.format_return_type())
                        .or_else(|| {
                            return_type.as_ref().and_then(|rt| {
                                if *rt != RubyType::Unknown && *rt != RubyType::Any {
                                    Some(rt.to_string())
                                } else {
                                    None
                                }
                            })
                        });

                    if let (Some(type_str), Some(pos)) = (return_type_str, return_type_position) {
                        let hint = InlayHint {
                            position: *pos,
                            label: InlayHintLabel::String(format!(" -> {}", type_str)),
                            kind: Some(InlayHintKind::TYPE),
                            text_edits: None,
                            tooltip: yard_doc
                                .as_ref()
                                .and_then(|doc| doc.get_return_description().cloned())
                                .map(InlayHintTooltip::String),
                            padding_left: None,
                            padding_right: None,
                            data: None,
                        };
                        all_hints.push(hint);
                    }
                }
                _ => {}
            }
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
