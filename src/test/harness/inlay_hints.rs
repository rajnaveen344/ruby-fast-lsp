//! Inlay hints utilities for the check harness.

use tower_lsp::lsp_types::{InlayHint, InlayHintTooltip};

/// Get label string from an InlayHint
pub fn get_hint_label(hint: &InlayHint) -> String {
    match &hint.label {
        tower_lsp::lsp_types::InlayHintLabel::String(s) => s.clone(),
        tower_lsp::lsp_types::InlayHintLabel::LabelParts(parts) => {
            parts.iter().map(|p| p.value.clone()).collect::<String>()
        }
    }
}

/// Read the complete type separately from the compact inline label.
pub fn get_hint_tooltip(hint: &InlayHint) -> Option<&str> {
    hint.tooltip.as_ref().map(|tooltip| match tooltip {
        InlayHintTooltip::String(text) => text.as_str(),
        InlayHintTooltip::MarkupContent(markup) => markup.value.as_str(),
    })
}
