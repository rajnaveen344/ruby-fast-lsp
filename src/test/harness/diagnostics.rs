//! Diagnostics check function using inline markers.
//!
//! Markers:
//! - `<err>...</err>` - expected error at this range
//! - `<warn>...</warn>` - expected warning at this range

use tower_lsp::lsp_types::DiagnosticSeverity;

use super::fixture::{extract_tags_with_attributes, setup_with_fixture};
use crate::capabilities::diagnostics::{generate_diagnostics, generate_yard_diagnostics};

/// Check that diagnostics match expected markers.
///
/// Use `<err>...</err>` or `<warn>...</warn>` to verify diagnostics.
/// Supports optional `message` attribute: `<warn message="foo">...</warn>`
pub async fn check_diagnostics(fixture_text: &str) {
    // Extract error and warning markers with attributes in one pass
    let (all_tags, content) = extract_tags_with_attributes(fixture_text, &["err", "warn"]);
    let err_tags: Vec<_> = all_tags.iter().filter(|t| t.kind == "err").collect();
    let warn_tags: Vec<_> = all_tags.iter().filter(|t| t.kind == "warn").collect();

    let (server, uri) = setup_with_fixture(&content).await;
    let document = server.docs.lock().get(&uri).unwrap().read().clone();
    let parse_result = ruby_prism::parse(content.as_bytes());

    // Collect all diagnostics
    let mut diagnostics = generate_diagnostics(&parse_result, &document);
    let index = server.index.lock();
    diagnostics.extend(generate_yard_diagnostics(&index, &uri));

    // Check errors
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        .collect();

    assert_eq!(
        errors.len(),
        err_tags.len(),
        "Expected {} errors, got {}.\nExpected tags: {:?}\nActual errors: {:?}",
        err_tags.len(),
        errors.len(),
        err_tags,
        errors
            .iter()
            .map(|e| (&e.range, &e.message))
            .collect::<Vec<_>>()
    );

    for (error, expected_tag) in errors.iter().zip(err_tags.iter()) {
        assert_eq!(
            error.range, expected_tag.range,
            "Range mismatch for error: {:?}",
            error.message
        );
        if let Some(expected_msg) = expected_tag.message() {
            assert!(
                error.message.contains(&expected_msg),
                "Message mismatch.\nExpected to contain: '{}'\nActual: '{}'",
                expected_msg,
                error.message
            );
        }
    }

    // Check warnings
    let warnings: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::WARNING))
        .collect();

    assert_eq!(
        warnings.len(),
        warn_tags.len(),
        "Expected {} warnings, got {}.\nExpected tags: {:?}\nActual warnings: {:?}",
        warn_tags.len(),
        warnings.len(),
        warn_tags,
        warnings
            .iter()
            .map(|w| (&w.range, &w.message))
            .collect::<Vec<_>>()
    );

    for (warning, expected_tag) in warnings.iter().zip(warn_tags.iter()) {
        assert_eq!(
            warning.range, expected_tag.range,
            "Range mismatch for warning: {:?}",
            warning.message
        );
        if let Some(expected_msg) = expected_tag.message() {
            assert!(
                warning.message.contains(&expected_msg),
                "Message mismatch.\nExpected to contain: '{}'\nActual: '{}'",
                expected_msg,
                warning.message
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // FIXME: The ruby parser often produces multiple errors for a single syntax issue.
    // This makes testing exact error counts unreliable. Skipping until we have a better strategy.
    // #[tokio::test]
    // async fn test_syntax_error_detected() {
    //     check_diagnostics("...").await;
    // }

    #[tokio::test]
    async fn test_valid_code_no_errors() {
        check_diagnostics(
            r#"
class Foo
  def bar
    "hello"
  end
end
"#,
        )
        .await;
    }

    #[tokio::test]
    async fn test_unclosed_string() {
        // "def foo; 1 +; end" -> Error at ";" + Warning at "+"
        check_diagnostics("def foo; 1 <warn>+</warn><err>;</err> end").await;
    }
}
