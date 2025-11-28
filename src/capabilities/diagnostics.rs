use crate::indexer::entry::entry_kind::EntryKind;
use crate::indexer::index::RubyIndex;
use crate::types::ruby_document::RubyDocument;
use log::{debug, warn};
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Range, Url};

/// Generate diagnostics for a Ruby document using ruby-prism
///
/// This function parses the document content and returns a vector of LSP diagnostics
/// for syntax errors, warnings, and other issues found in the code.
pub fn generate_diagnostics(document: &RubyDocument) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    debug!("Generating diagnostics for document: {}", document.uri);

    // Parse the document content using ruby-prism
    let parse_result = ruby_prism::parse(document.content.as_bytes());

    // Check for syntax errors
    let errors: Vec<_> = parse_result.errors().collect();

    if !errors.is_empty() {
        debug!("Found {} syntax errors in document", errors.len());

        for error in errors {
            let location = error.location();
            let start_offset = location.start_offset();
            let end_offset = location.end_offset();

            // Convert byte offsets to LSP positions
            let start_pos = document.offset_to_position(start_offset);
            let end_pos = document.offset_to_position(end_offset);

            // Create diagnostic for syntax error
            let diagnostic = Diagnostic {
                range: Range::new(start_pos, end_pos),
                severity: Some(DiagnosticSeverity::ERROR),
                code: None,
                code_description: None,
                source: Some("ruby-fast-lsp".to_string()),
                message: error.message().to_string(),
                related_information: None,
                tags: None,
                data: None,
            };

            diagnostics.push(diagnostic);
        }
    }

    // Check for warnings from the parser
    let warnings: Vec<_> = parse_result.warnings().collect();

    if !warnings.is_empty() {
        debug!("Found {} warnings in document", warnings.len());

        for warning in warnings {
            let location = warning.location();
            let start_offset = location.start_offset();
            let end_offset = location.end_offset();

            // Convert byte offsets to LSP positions
            let start_pos = document.offset_to_position(start_offset);
            let end_pos = document.offset_to_position(end_offset);

            // Create diagnostic for warning
            let diagnostic = Diagnostic {
                range: Range::new(start_pos, end_pos),
                severity: Some(DiagnosticSeverity::WARNING),
                code: None,
                code_description: None,
                source: Some("ruby-fast-lsp".to_string()),
                message: warning.message().to_string(),
                related_information: None,
                tags: None,
                data: None,
            };

            diagnostics.push(diagnostic);
        }
    }

    // Additional linting checks can be added here in the future
    // For example:
    // - Unused variables
    // - Unreachable code
    // - Style violations
    // - Performance suggestions

    debug!(
        "Generated {} total diagnostics for document",
        diagnostics.len()
    );
    diagnostics
}

/// Generate diagnostics for YARD documentation issues
/// This checks for @param tags that reference non-existent parameters
pub fn generate_yard_diagnostics(index: &RubyIndex, uri: &Url) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    if let Some(entries) = index.file_entries.get(uri) {
        for entry in entries {
            if let EntryKind::Method {
                yard_doc: Some(yard_doc),
                params: method_params,
                ..
            } = &entry.kind
            {
                // Get actual parameter names from the method
                let actual_param_names: Vec<&str> =
                    method_params.iter().map(|p| p.name.as_str()).collect();

                // Find YARD @param tags that don't match any actual parameter
                let unmatched = yard_doc.find_unmatched_params(&actual_param_names);

                for (yard_param, range) in unmatched {
                    let diagnostic = Diagnostic {
                        range,
                        severity: Some(DiagnosticSeverity::WARNING),
                        code: Some(tower_lsp::lsp_types::NumberOrString::String(
                            "yard-unknown-param".to_string(),
                        )),
                        code_description: None,
                        source: Some("ruby-fast-lsp".to_string()),
                        message: format!(
                            "YARD @param '{}' does not match any method parameter",
                            yard_param.name
                        ),
                        related_information: None,
                        tags: None,
                        data: None,
                    };
                    diagnostics.push(diagnostic);
                }
            }
        }
    }

    diagnostics
}

/// Validate that a diagnostic range is within document bounds
///
/// This is a safety check to ensure we don't send invalid ranges to the client
fn _validate_diagnostic_range(document: &RubyDocument, range: &Range) -> bool {
    let lines: Vec<&str> = document.content.lines().collect();
    let line_count = lines.len();

    // Check if start position is valid
    if range.start.line as usize >= line_count {
        warn!(
            "Diagnostic start line {} exceeds document line count {}",
            range.start.line, line_count
        );
        return false;
    }

    // Check if end position is valid
    if range.end.line as usize >= line_count {
        warn!(
            "Diagnostic end line {} exceeds document line count {}",
            range.end.line, line_count
        );
        return false;
    }

    // Check if character positions are valid for their respective lines
    if let Some(start_line) = lines.get(range.start.line as usize) {
        if range.start.character as usize > start_line.len() {
            warn!(
                "Diagnostic start character {} exceeds line length {}",
                range.start.character,
                start_line.len()
            );
            return false;
        }
    }

    if let Some(end_line) = lines.get(range.end.line as usize) {
        if range.end.character as usize > end_line.len() {
            warn!(
                "Diagnostic end character {} exceeds line length {}",
                range.end.character,
                end_line.len()
            );
            return false;
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::Url;

    #[test]
    fn test_generate_diagnostics_valid_ruby() {
        let uri = Url::parse("file:///test.rb").unwrap();
        let content = r#"
class TestClass
  def test_method
    puts "Hello, World!"
  end
end
"#
        .to_string();

        let document = RubyDocument::new(uri, content, 1);
        let diagnostics = generate_diagnostics(&document);

        // Valid Ruby code should have no diagnostics
        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_generate_diagnostics_syntax_error() {
        let uri = Url::parse("file:///test.rb").unwrap();
        let content = r#"
class TestClass
  def test_method
    puts "Hello, World!"
  # Missing 'end' for method
end
"#
        .to_string();

        let document = RubyDocument::new(uri, content, 1);
        let diagnostics = generate_diagnostics(&document);

        // Should have at least one syntax error diagnostic
        assert!(!diagnostics.is_empty());

        // Check that the diagnostic is an error
        let first_diagnostic = &diagnostics[0];
        assert_eq!(first_diagnostic.severity, Some(DiagnosticSeverity::ERROR));
        assert_eq!(first_diagnostic.source, Some("ruby-fast-lsp".to_string()));
    }

    #[test]
    fn test_generate_diagnostics_multiple_errors() {
        let uri = Url::parse("file:///test.rb").unwrap();
        let content = r#"
class TestClass
  def test_method(
    puts "Hello, World!"
  # Missing closing parenthesis and 'end'
end
"#
        .to_string();

        let document = RubyDocument::new(uri, content, 1);
        let diagnostics = generate_diagnostics(&document);

        // Should have multiple syntax diagnostics
        assert!(!diagnostics.is_empty());

        // All diagnostics should be either errors or warnings
        for diagnostic in &diagnostics {
            assert!(
                diagnostic.severity == Some(DiagnosticSeverity::ERROR)
                    || diagnostic.severity == Some(DiagnosticSeverity::WARNING)
            );
            assert_eq!(diagnostic.source, Some("ruby-fast-lsp".to_string()));
        }
    }

    #[test]
    fn test_human_readable_diagnostic_messages() {
        let uri = Url::parse("file:///test.rb").unwrap();
        let content =
            "def incomplete_method(\n  puts 'hello'\n# missing closing paren and end".to_string();
        let document = RubyDocument::new(uri, content, 1);

        let diagnostics = generate_diagnostics(&document);

        assert!(!diagnostics.is_empty());
        assert_eq!(diagnostics.len(), 4);

        // Check specific human-readable messages
        let messages: Vec<&str> = diagnostics.iter().map(|d| d.message.as_str()).collect();

        // Verify we get the expected human-readable error messages
        assert!(
            messages.contains(&"unexpected string literal; expected a `)` to close the parameters")
        );
        assert!(messages.contains(
            &"unexpected end-of-input, assuming it is closing the parent top level context"
        ));
        assert!(messages.contains(&"expected an `end` to close the `def` statement"));
        assert!(messages.contains(&"possibly useless use of a literal in void context"));

        // Verify severities are correct
        let error_count = diagnostics
            .iter()
            .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
            .count();
        let warning_count = diagnostics
            .iter()
            .filter(|d| d.severity == Some(DiagnosticSeverity::WARNING))
            .count();

        assert_eq!(error_count, 3);
        assert_eq!(warning_count, 1);

        // Verify all diagnostics have the correct source
        for diagnostic in &diagnostics {
            assert_eq!(diagnostic.source, Some("ruby-fast-lsp".to_string()));
        }
    }

    #[test]
    fn test_human_readable_messages_simple_syntax_error() {
        let uri = Url::parse("file:///test.rb").unwrap();
        let content = "if true\n  puts 'hello'\n# missing end".to_string();
        let document = RubyDocument::new(uri, content, 1);

        let diagnostics = generate_diagnostics(&document);

        assert!(!diagnostics.is_empty());

        // Verify all messages are human-readable (no debug artifacts)
        for diagnostic in &diagnostics {
            let message = &diagnostic.message;

            // Should not contain Rust debug artifacts
            assert!(!message.contains("Diagnostic {"));
            assert!(!message.contains("0x"));
            assert!(!message.contains("PhantomData"));
            assert!(!message.contains("parser:"));
            assert!(!message.contains("marker:"));

            // Should be a proper sentence or phrase
            assert!(!message.is_empty());
            assert!(message.len() > 5); // Reasonable minimum length for a diagnostic message

            // Verify source is set correctly
            assert_eq!(diagnostic.source, Some("ruby-fast-lsp".to_string()));
        }
    }
}
