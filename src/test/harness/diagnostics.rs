//! Diagnostics check function using inline markers.
//!
//! Markers:
//! - `<err>...</err>` - expected error at this range
//! - `<warn>...</warn>` - expected warning at this range

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Url};

use super::fixture::extract_tags;
use crate::capabilities::diagnostics::generate_diagnostics;
use crate::types::ruby_document::RubyDocument;

/// Check that diagnostics match expected markers.
///
/// # Markers
/// - `<err>...</err>` - expected error range
/// - `<warn>...</warn>` - expected warning range
///
/// # Example
///
/// ```ignore
/// check_diagnostics(r#"
/// <err>def foo(</err>
/// "#);
/// ```
pub fn check_diagnostics(fixture_text: &str) {
    // Extract error and warning markers
    let (err_ranges, text_no_errs) = extract_tags(fixture_text, "err");
    let (warn_ranges, content) = extract_tags(&text_no_errs, "warn");

    let uri = Url::parse("file:///test.rb").unwrap();
    let document = RubyDocument::new(uri, content.clone(), 1);
    let parse_result = ruby_prism::parse(content.as_bytes());
    let diagnostics = generate_diagnostics(&parse_result, &document);

    // Check errors
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        .collect();

    assert_eq!(
        errors.len(),
        err_ranges.len(),
        "Expected {} errors, got {}.\nExpected ranges: {:?}\nActual errors: {:?}",
        err_ranges.len(),
        errors.len(),
        err_ranges,
        errors
            .iter()
            .map(|e| (&e.range, &e.message))
            .collect::<Vec<_>>()
    );

    // Check warnings
    let warnings: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::WARNING))
        .collect();

    assert_eq!(
        warnings.len(),
        warn_ranges.len(),
        "Expected {} warnings, got {}.\nExpected ranges: {:?}\nActual warnings: {:?}",
        warn_ranges.len(),
        warnings.len(),
        warn_ranges,
        warnings
            .iter()
            .map(|w| (&w.range, &w.message))
            .collect::<Vec<_>>()
    );
}

/// Get diagnostics for content (no markers).
pub fn get_diagnostics(content: &str) -> Vec<Diagnostic> {
    let uri = Url::parse("file:///test.rb").unwrap();
    let document = RubyDocument::new(uri, content.to_string(), 1);
    let parse_result = ruby_prism::parse(content.as_bytes());
    generate_diagnostics(&parse_result, &document)
}

/// Check that no diagnostics are reported (valid code).
pub fn check_no_diagnostics(content: &str) {
    let diagnostics = get_diagnostics(content);
    assert!(
        diagnostics.is_empty(),
        "Expected no diagnostics, got: {:?}",
        diagnostics.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_syntax_error_detected() {
        let diagnostics = get_diagnostics("def foo(");
        assert!(
            !diagnostics.is_empty(),
            "Expected syntax error for incomplete method definition"
        );
    }

    #[test]
    fn test_valid_code_no_errors() {
        check_no_diagnostics(
            r#"
class Foo
  def bar
    "hello"
  end
end
"#,
        );
    }

    #[test]
    fn test_unclosed_string() {
        let diagnostics = get_diagnostics(r#"x = "unclosed"#);
        assert!(
            !diagnostics.is_empty(),
            "Expected syntax error for unclosed string"
        );
    }
}
