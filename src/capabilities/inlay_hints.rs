use tower_lsp::lsp_types::{
    InlayHint, InlayHintLabel, InlayHintOptions, InlayHintParams, InlayHintServerCapabilities,
    WorkDoneProgressOptions,
};

use crate::indexer::entry::entry_kind::EntryKind;
use crate::server::RubyLanguageServer;

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

    // Generate type hints from indexed Variable entries
    let index = server.index.lock();

    if let Some(entries) = index.file_entries.get(&uri) {
        for entry in entries {
            if let EntryKind::Variable { ruby_type, .. } = &entry.kind {
                if let Some(type_info) = ruby_type {
                    // Create type hint at the end of the variable name
                    let end_position = entry.location.range.end;
                    let type_hint = InlayHint {
                        position: end_position,
                        label: InlayHintLabel::String(format!(": {}", type_info.to_string())),
                        kind: None,
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

    all_hints
}
