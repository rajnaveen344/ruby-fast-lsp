//! Inlay hints check function using inline markers.
//!
//! Markers:
//! - `<hint:TYPE>` - expected hint at this exact position containing TYPE
//!
//! The marker should be placed WHERE the hint appears (after variable name).

use tower_lsp::lsp_types::{InlayHint, InlayHintParams, Position, Range, TextDocumentIdentifier};

use super::fixture::{extract_tags_with_attributes, setup_with_fixture};
use crate::capabilities::inlay_hints::handle_inlay_hints;

/// Check that inlay hints match expected markers.
///
/// # Markers
/// - `<hint label="TYPE">` - expected hint at this position containing TYPE
///
/// # Example
///
/// ```ignore
/// check_inlay_hints(r#"
/// x<hint label="Integer"> = 42
/// "#).await;
/// ```
pub async fn check_inlay_hints(fixture_text: &str) {
    let (expected_hints, clean_text) = extract_tags_with_attributes(fixture_text, &["hint"]);
    let (server, uri) = setup_with_fixture(&clean_text).await;

    let params = InlayHintParams {
        text_document: TextDocumentIdentifier { uri: uri.clone() },
        range: Range {
            start: Position::new(0, 0),
            end: Position::new(1000, 0),
        },
        work_done_progress_params: Default::default(),
    };

    let hints = handle_inlay_hints(&server, params).await;

    for expected in &expected_hints {
        let expected_label = expected
            .attributes
            .get("label")
            .expect("hint tag missing 'label' attribute");

        let found = hints.iter().find(|hint| {
            // Check position matches (same line, character within range)
            if hint.position.line != expected.range.start.line {
                return false;
            }
            // Allow some tolerance for character position (hint might be at end of variable)
            let char_diff =
                (hint.position.character as i32 - expected.range.start.character as i32).abs();
            if char_diff > 2 {
                return false;
            }
            // Check label contains expected type
            let label = get_hint_label(hint);
            label.contains(expected_label)
        });

        assert!(
            found.is_some(),
            "Expected inlay hint containing '{}' at line {}:{}, got hints: {:?}",
            expected_label,
            expected.range.start.line,
            expected.range.start.character,
            hints
                .iter()
                .map(|h| {
                    format!(
                        "{}:{} '{}'",
                        h.position.line,
                        h.position.character,
                        get_hint_label(h)
                    )
                })
                .collect::<Vec<_>>()
        );
    }
}

/// Get label string from an InlayHint
pub fn get_hint_label(hint: &InlayHint) -> String {
    match &hint.label {
        tower_lsp::lsp_types::InlayHintLabel::String(s) => s.clone(),
        tower_lsp::lsp_types::InlayHintLabel::LabelParts(parts) => {
            parts.iter().map(|p| p.value.clone()).collect::<String>()
        }
    }
}

/// Check that no inlay hints are generated for the content.
pub async fn check_no_inlay_hints(content: &str) {
    let (server, uri) = setup_with_fixture(content).await;

    let params = InlayHintParams {
        text_document: TextDocumentIdentifier { uri: uri.clone() },
        range: Range {
            start: Position::new(0, 0),
            end: Position::new(1000, 0),
        },
        work_done_progress_params: Default::default(),
    };

    let hints = handle_inlay_hints(&server, params).await;

    assert!(
        hints.is_empty(),
        "Expected no inlay hints, got: {:?}",
        hints
            .iter()
            .map(|h| format!(
                "{}:{} '{}'",
                h.position.line,
                h.position.character,
                get_hint_label(h)
            ))
            .collect::<Vec<_>>()
    );
}

/// Check that no inlay hints containing `part` are generated.
pub async fn check_no_inlay_hints_containing(content: &str, part: &str) {
    let (server, uri) = setup_with_fixture(content).await;

    let params = InlayHintParams {
        text_document: TextDocumentIdentifier { uri: uri.clone() },
        range: Range {
            start: Position::new(0, 0),
            end: Position::new(1000, 0),
        },
        work_done_progress_params: Default::default(),
    };

    let hints = handle_inlay_hints(&server, params).await;

    let matching: Vec<_> = hints
        .iter()
        .filter(|h| get_hint_label(h).contains(part))
        .collect();

    assert!(
        matching.is_empty(),
        "Expected no inlay hints containing '{}', got: {:?}",
        part,
        matching
            .iter()
            .map(|h| format!(
                "{}:{} '{}'",
                h.position.line,
                h.position.character,
                get_hint_label(h)
            ))
            .collect::<Vec<_>>()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_inlay_hints_string_literal() {
        check_inlay_hints(
            r#"
x<hint label="String"> = "hello"
"#,
        )
        .await;
    }

    #[tokio::test]
    async fn test_inlay_hints_integer_literal() {
        check_inlay_hints(
            r#"
x<hint label="Integer"> = 42
"#,
        )
        .await;
    }
}
