//! Convert identity-bearing type labels to native LSP navigation links.

use super::type_display::TypeDisplay;
use crate::query::analysis_location::location_for_range;
use ruby_analysis::core::RubyType;
use ruby_analysis::engine::{AnalysisEngine, AnalysisQuery};
use tower_lsp::lsp_types::{
    InlayHintLabel, InlayHintLabelPart, InlayHintLabelPartTooltip, InlayHintTooltip,
};

pub(super) fn type_hint_label(
    ruby_type: &RubyType,
    prefix: &str,
    engine: Option<&AnalysisEngine>,
) -> (InlayHintLabel, InlayHintTooltip) {
    let display = TypeDisplay::new(ruby_type);
    let mut parts = vec![InlayHintLabelPart {
        value: prefix.to_string(),
        ..Default::default()
    }];
    for part in display.parts {
        let location = engine
            .zip(part.target.as_ref())
            .and_then(|(engine, target)| {
                // Use the same exact symbol and source-preference query as ordinary
                // navigation. A missing declaration remains an unlinked type.
                AnalysisQuery::new(engine)
                    .type_name_definition_ranges(target)
                    .into_iter()
                    .find_map(|range| location_for_range(engine, range))
            });
        parts.push(InlayHintLabelPart {
            value: part.value,
            location,
            tooltip: Some(match &display.tooltip {
                InlayHintTooltip::String(text) => InlayHintLabelPartTooltip::String(text.clone()),
                InlayHintTooltip::MarkupContent(markup) => {
                    InlayHintLabelPartTooltip::MarkupContent(markup.clone())
                }
            }),
            command: None,
        });
    }
    (InlayHintLabel::LabelParts(parts), display.tooltip)
}
