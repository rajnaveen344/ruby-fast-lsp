//! Inlay hints utilities for the check harness.

use tower_lsp::lsp_types::InlayHint;

/// Get label string from an InlayHint
pub fn get_hint_label(hint: &InlayHint) -> String {
    match &hint.label {
        tower_lsp::lsp_types::InlayHintLabel::String(s) => s.clone(),
        tower_lsp::lsp_types::InlayHintLabel::LabelParts(parts) => {
            parts.iter().map(|p| p.value.clone()).collect::<String>()
        }
    }
}
