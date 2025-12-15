use crate::indexer::entry::entry_kind::EntryKind;
use crate::indexer::index::RubyIndex;
use crate::types::ruby_document::RubyDocument;
use crate::yard::YardTypeConverter;
use log::{debug, warn};
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Range, Url};

/// Generate diagnostics from a parse result
///
/// This extracts syntax errors and warnings from an existing parse result.
/// Used by process_file() to avoid re-parsing.
pub fn generate_diagnostics(
    parse_result: &ruby_prism::ParseResult<'_>,
    document: &RubyDocument,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    // Extend with syntax diagnostics (errors and warnings from parser)
    diagnostics.extend(extract_syntax_diagnostics(parse_result, document));

    diagnostics
}

/// Extract syntax errors and warnings from a parse result
fn extract_syntax_diagnostics(
    parse_result: &ruby_prism::ParseResult<'_>,
    document: &RubyDocument,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

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

    debug!(
        "Generated {} total diagnostics for document",
        diagnostics.len()
    );
    diagnostics
}

/// Generate diagnostics for YARD documentation issues
/// This checks for:
/// - @param tags that reference non-existent parameters
/// - Type references that don't exist in the index (classes/modules)
pub fn generate_yard_diagnostics(index: &RubyIndex, uri: &Url) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    let entries = index.file_entries(uri);
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

            // Check for unresolved types in @param tags
            for param in &yard_doc.params {
                let result =
                    YardTypeConverter::convert_multiple_with_validation(&param.types, Some(index));
                for unresolved in result.unresolved_types {
                    // Prefer types_range (just the [Type] portion) over range (entire line)
                    let diagnostic_range = param.types_range.or(param.range);
                    if let Some(range) = diagnostic_range {
                        let diagnostic = Diagnostic {
                            range,
                            severity: Some(DiagnosticSeverity::ERROR),
                            code: Some(tower_lsp::lsp_types::NumberOrString::String(
                                "yard-unknown-type".to_string(),
                            )),
                            code_description: None,
                            source: Some("ruby-fast-lsp".to_string()),
                            message: format!(
                                "Unknown type '{}' in YARD @param documentation",
                                unresolved.type_name
                            ),
                            related_information: None,
                            tags: None,
                            data: None,
                        };
                        diagnostics.push(diagnostic);
                    }
                }
            }

            // Check for unresolved types in @return tags
            for return_doc in &yard_doc.returns {
                let result = YardTypeConverter::convert_multiple_with_validation(
                    &return_doc.types,
                    Some(index),
                );
                for unresolved in result.unresolved_types {
                    // For return types, we don't have a specific range stored
                    // We could add range tracking to YardReturn in the future
                    debug!(
                        "Unresolved return type '{}' (no range available for diagnostic)",
                        unresolved.type_name
                    );
                }
            }
        }
    }

    diagnostics
}

/// Get diagnostics for unresolved entries (constants and methods) from the index
pub fn get_unresolved_diagnostics(
    server: &crate::server::RubyLanguageServer,
    uri: &Url,
) -> Vec<Diagnostic> {
    use crate::indexer::index::UnresolvedEntry;
    use tower_lsp::lsp_types::{DiagnosticSeverity, NumberOrString};

    let index_arc = server.index();
    let index = index_arc.lock();
    let unresolved_list = index.get_unresolved_entries(uri);

    unresolved_list
        .iter()
        .map(|entry| match entry {
            UnresolvedEntry::Constant { name, location, .. } => Diagnostic {
                range: location.range,
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(NumberOrString::String("unresolved-constant".to_string())),
                code_description: None,
                source: Some("ruby-fast-lsp".to_string()),
                message: format!("Unresolved constant `{}`", name),
                related_information: None,
                tags: None,
                data: None,
            },
            UnresolvedEntry::Method {
                name,
                receiver,
                location,
            } => {
                let message = match receiver {
                    Some(recv) => format!("Unresolved method `{}` on `{}`", name, recv),
                    None => format!("Unresolved method `{}`", name),
                };

                Diagnostic {
                    range: location.range,
                    severity: Some(DiagnosticSeverity::WARNING),
                    code: Some(NumberOrString::String("unresolved-method".to_string())),
                    code_description: None,
                    source: Some("ruby-fast-lsp".to_string()),
                    message,
                    related_information: None,
                    tags: None,
                    data: None,
                }
            }
        })
        .collect()
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

    fn parse_and_generate(content: &str) -> Vec<Diagnostic> {
        let uri = Url::parse("file:///test.rb").unwrap();
        let document = RubyDocument::new(uri, content.to_string(), 1);
        let parse_result = ruby_prism::parse(content.as_bytes());
        generate_diagnostics(&parse_result, &document)
    }

    #[test]
    fn test_generate_diagnostics_valid_ruby() {
        let content = r#"
class TestClass
  def test_method
    puts "Hello, World!"
  end
end
"#;
        let diagnostics = parse_and_generate(content);
        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_generate_diagnostics_syntax_error() {
        let content = r#"
class TestClass
  def test_method
    puts "Hello, World!"
  # Missing 'end' for method
end
"#;
        let diagnostics = parse_and_generate(content);
        assert!(!diagnostics.is_empty());

        let first_diagnostic = &diagnostics[0];
        assert_eq!(first_diagnostic.severity, Some(DiagnosticSeverity::ERROR));
        assert_eq!(first_diagnostic.source, Some("ruby-fast-lsp".to_string()));
    }

    #[test]
    fn test_generate_diagnostics_multiple_errors() {
        let content = r#"
class TestClass
  def test_method(
    puts "Hello, World!"
  # Missing closing parenthesis and 'end'
end
"#;
        let diagnostics = parse_and_generate(content);
        assert!(!diagnostics.is_empty());

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
        let content = "def incomplete_method(\n  puts 'hello'\n# missing closing paren and end";
        let diagnostics = parse_and_generate(content);

        assert!(!diagnostics.is_empty());
        assert_eq!(diagnostics.len(), 4);

        let messages: Vec<&str> = diagnostics.iter().map(|d| d.message.as_str()).collect();
        assert!(
            messages.contains(&"unexpected string literal; expected a `)` to close the parameters")
        );
        assert!(messages.contains(
            &"unexpected end-of-input, assuming it is closing the parent top level context"
        ));
        assert!(messages.contains(&"expected an `end` to close the `def` statement"));
        assert!(messages.contains(&"possibly useless use of a literal in void context"));

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
    }

    #[test]
    fn test_human_readable_messages_simple_syntax_error() {
        let content = "if true\n  puts 'hello'\n# missing end";
        let diagnostics = parse_and_generate(content);

        assert!(!diagnostics.is_empty());

        for diagnostic in &diagnostics {
            let message = &diagnostic.message;
            assert!(!message.contains("Diagnostic {"));
            assert!(!message.contains("0x"));
            assert!(!message.is_empty());
            assert!(message.len() > 5);
            assert_eq!(diagnostic.source, Some("ruby-fast-lsp".to_string()));
        }
    }
}
