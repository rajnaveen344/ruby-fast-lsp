//! Shape tooltips preserve structured types in readable Markdown code blocks.

use crate::test::harness::{get_hint_label, FakeEditor};
use tower_lsp::lsp_types::{
    InlayHint, InlayHintLabel, InlayHintLabelPartTooltip, InlayHintTooltip, MarkupKind,
};

fn assert_tooltip(hint: &InlayHint, expected: &str) {
    let Some(InlayHintTooltip::MarkupContent(markup)) = &hint.tooltip else {
        panic!("shape tooltips must use Markdown so indentation is preserved");
    };
    assert_eq!(markup.kind, MarkupKind::Markdown);
    assert_eq!(markup.value, expected);
    let InlayHintLabel::LabelParts(parts) = &hint.label else {
        panic!("formatted tooltips must retain navigable label parts");
    };
    for part in parts.iter().skip(1) {
        assert_eq!(
            part.tooltip,
            Some(InlayHintLabelPartTooltip::MarkupContent(markup.clone())),
            "hovering a type component must show the same formatted shape"
        );
    }
}

#[tokio::test]
async fn formatted_shape_tooltip_lists_fields_and_keeps_navigation() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "types.rb",
            "class Hash; end\nclass Symbol; end\nclass Float; end\n",
        )
        .await;
    editor
        .open(
            "readings.rb",
            "readings = { north: 1.5, east: 2.5, west: 3.5 }\n",
        )
        .await;
    let hints = editor.inlay_hints("readings.rb").await;
    let hint = hints
        .iter()
        .find(|hint| get_hint_label(hint) == ": Hash<Symbol, Float>")
        .unwrap();
    assert_tooltip(
        hint,
        "```ruby\n{\n  east: Float,\n  north: Float,\n  west: Float\n}\n```",
    );
    let InlayHintLabel::LabelParts(parts) = &hint.label else {
        unreachable!()
    };
    assert_eq!(
        parts.iter().filter(|part| part.location.is_some()).count(),
        3
    );
}

#[tokio::test]
async fn formatted_shape_tooltip_indents_nested_fields_and_union_alternatives() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "snapshot.rb",
            r#"
def snapshot(flag)
  flag ? { profile: { active: true, name: "ready" }, rows: [{ id: 1 }] } : {}
end
"#,
        )
        .await;
    let hints = editor.inlay_hints("snapshot.rb").await;
    let hint = hints
        .iter()
        .find(|hint| get_hint_label(hint).starts_with(" -> "))
        .unwrap();
    assert_tooltip(hint, "```ruby\n(\n  { }\n  | {\n    profile: {\n      active: TrueClass,\n      name: String\n    },\n    rows: Array<{\n      id: Integer\n    }>\n  }\n)\n```");
}
