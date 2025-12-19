//! Inlay hints check function using inline markers.
//!
//! Markers:
//! - `<hint:TYPE>` - expected hint at this exact position containing TYPE
//!
//! The marker should be placed WHERE the hint appears (after variable name).

use tower_lsp::lsp_types::{InlayHint, InlayHintParams, Position, Range, TextDocumentIdentifier};

use super::fixture::setup_with_fixture;
use crate::capabilities::inlay_hints::handle_inlay_hints;

/// Parsed hint marker with position and expected type.
#[derive(Debug)]
struct ExpectedHint {
    line: u32,
    character: u32,
    expected_type: String,
}

/// Extract hint markers from text.
/// Returns (expected_hints, clean_content)
///
/// Marker: `<hint:TYPE>` is placed where the hint should appear.
/// Example: `x<hint:Integer> = 42` expects `: Integer` hint after `x`.
fn extract_hint_markers(text: &str) -> (Vec<ExpectedHint>, String) {
    let mut hints = Vec::new();
    let mut clean_text = String::new();
    let mut current_line = 0u32;
    let mut current_char = 0u32;

    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        // Check for marker start
        if ch == '<' {
            // Look ahead for "hint:"
            let rest: String = chars.clone().take(5).collect();
            if rest == "hint:" {
                // Found marker, extract type
                for _ in 0..5 {
                    chars.next();
                }
                let mut hint_type = String::new();
                while let Some(c) = chars.next() {
                    if c == '>' {
                        break;
                    }
                    hint_type.push(c);
                }

                hints.push(ExpectedHint {
                    line: current_line,
                    character: current_char,
                    expected_type: hint_type,
                });
                continue;
            }
        }

        clean_text.push(ch);
        if ch == '\n' {
            current_line += 1;
            current_char = 0;
        } else {
            current_char += 1;
        }
    }

    (hints, clean_text)
}

/// Check that inlay hints match expected markers.
///
/// # Markers
/// - `<hint:TYPE>` - expected hint at this position containing TYPE
///
/// # Example
///
/// ```ignore
/// check_inlay_hints(r#"
/// x<hint:Integer> = 42
/// "#).await;
/// ```
pub async fn check_inlay_hints(fixture_text: &str) {
    let (expected_hints, content) = extract_hint_markers(fixture_text);
    let (server, uri) = setup_with_fixture(&content).await;

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
        let found = hints.iter().find(|hint| {
            // Check position matches (same line, character within range)
            if hint.position.line != expected.line {
                return false;
            }
            // Allow some tolerance for character position (hint might be at end of variable)
            let char_diff = (hint.position.character as i32 - expected.character as i32).abs();
            if char_diff > 2 {
                return false;
            }
            // Check label contains expected type
            let label = get_hint_label(hint);
            label.contains(&expected.expected_type)
        });

        assert!(
            found.is_some(),
            "Expected inlay hint containing '{}' at line {}:{}, got hints: {:?}",
            expected.expected_type,
            expected.line,
            expected.character,
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

/// Get inlay hints for content (no markers).
pub async fn get_inlay_hints(content: &str) -> Vec<InlayHint> {
    let (server, uri) = setup_with_fixture(content).await;

    let params = InlayHintParams {
        text_document: TextDocumentIdentifier { uri: uri.clone() },
        range: Range {
            start: Position::new(0, 0),
            end: Position::new(1000, 0),
        },
        work_done_progress_params: Default::default(),
    };

    handle_inlay_hints(&server, params).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_hint_markers() {
        let (hints, clean) = extract_hint_markers("x<hint:Integer> = 42");
        assert_eq!(hints.len(), 1);
        assert_eq!(hints[0].line, 0);
        assert_eq!(hints[0].character, 1); // after 'x'
        assert_eq!(hints[0].expected_type, "Integer");
        assert_eq!(clean, "x = 42");
    }

    #[test]
    fn test_extract_hint_markers_multiline() {
        let (hints, clean) = extract_hint_markers("a<hint:String> = \"hi\"\nb<hint:Integer> = 1");
        assert_eq!(hints.len(), 2);
        assert_eq!(hints[0].line, 0);
        assert_eq!(hints[0].expected_type, "String");
        assert_eq!(hints[1].line, 1);
        assert_eq!(hints[1].expected_type, "Integer");
        assert_eq!(clean, "a = \"hi\"\nb = 1");
    }

    #[tokio::test]
    async fn test_inlay_hints_string_literal() {
        check_inlay_hints(
            r#"
x<hint:String> = "hello"
"#,
        )
        .await;
    }

    #[tokio::test]
    async fn test_inlay_hints_integer_literal() {
        check_inlay_hints(
            r#"
x<hint:Integer> = 42
"#,
        )
        .await;
    }
}
