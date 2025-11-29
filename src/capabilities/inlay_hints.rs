use tower_lsp::lsp_types::{
    InlayHint, InlayHintKind, InlayHintLabel, InlayHintOptions, InlayHintParams,
    InlayHintServerCapabilities, InlayHintTooltip, WorkDoneProgressOptions,
};

use crate::indexer::entry::entry_kind::EntryKind;
use crate::server::RubyLanguageServer;
use crate::type_inference::ruby_type::RubyType;

pub fn get_inlay_hints_capability() -> InlayHintServerCapabilities {
    InlayHintServerCapabilities::Options(InlayHintOptions {
        work_done_progress_options: WorkDoneProgressOptions::default(),
        resolve_provider: Some(false),
    })
}

pub async fn handle_inlay_hints(
    server: &RubyLanguageServer,
    params: InlayHintParams,
) -> Vec<InlayHint> {
    let uri = params.text_document.uri;
    let document_guard = server.docs.lock();
    let document = document_guard.get(&uri).unwrap().read();

    // Get structural hints from the document
    let mut all_hints = document.get_inlay_hints();

    // Generate type hints from indexed entries
    let index = server.index.lock();

    if let Some(entries) = index.file_entries.get(&uri) {
        for entry in entries {
            match &entry.kind {
                EntryKind::LocalVariable { r#type, .. }
                | EntryKind::InstanceVariable { r#type, .. }
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
