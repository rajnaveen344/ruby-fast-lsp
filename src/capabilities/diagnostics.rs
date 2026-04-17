//! Diagnostics capability — syntax diagnostics from the parser.
//!
//! AST-only diagnostics (syntax errors/warnings) live here.
//! Index-dependent diagnostics (unresolved entries, YARD issues) are in the query layer.

use crate::analyzer_prism::control_flow;
use crate::types::ruby_document::RubyDocument;
use log::{debug, warn};
use ruby_prism::Visit;
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Range};

/// Prism emits "statement not reached" for return/break/next, but only flags the
/// first dead stmt and lacks a code. We own this diagnostic — filter Prism's version
/// out and let `extract_unreachable_diagnostics` produce our richer one.
const PRISM_UNREACHABLE_MSG: &str = "statement not reached";

/// Generate diagnostics from a parse result.
///
/// Extracts syntax errors and warnings from an existing parse result.
/// Used by process_file() to avoid re-parsing.
pub fn generate_diagnostics(
    parse_result: &ruby_prism::ParseResult<'_>,
    document: &RubyDocument,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    diagnostics.extend(extract_syntax_diagnostics(parse_result, document));
    diagnostics.extend(extract_unreachable_diagnostics(parse_result, document));
    diagnostics.extend(extract_inconsistent_return_diagnostics(parse_result, document));
    diagnostics
}

/// `inconsistent-return`: method body falls through, has at least one explicit
/// `return EXPR` with a non-nil value, AND the body's last statement is a
/// "void" call (`puts`/`print`/`p`/`pp`/`warn`) — strongly suggesting a forgotten
/// return on the fall-through path.
///
/// Conservative: silent when the last expression is a meaningful value.
fn extract_inconsistent_return_diagnostics(
    parse_result: &ruby_prism::ParseResult<'_>,
    document: &RubyDocument,
) -> Vec<Diagnostic> {
    let mut visitor = InconsistentReturnVisitor {
        document,
        diagnostics: Vec::new(),
    };
    visitor.visit(&parse_result.node());
    visitor.diagnostics
}

struct InconsistentReturnVisitor<'a> {
    document: &'a RubyDocument,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> InconsistentReturnVisitor<'a> {
    fn check_def(&mut self, def: &ruby_prism::DefNode<'_>) {
        let Some(body) = def.body() else { return };
        let Some(stmts) = body.as_statements_node() else {
            return;
        };
        let body_nodes: Vec<_> = stmts.body().iter().collect();
        if body_nodes.is_empty() {
            return;
        }

        // Body must fall through.
        if control_flow::analyze_statements(&stmts).is_diverges() {
            return;
        }

        // Last statement must be a "void" call.
        let last = body_nodes.last().unwrap();
        if !is_void_call(last) {
            return;
        }

        // Body must contain at least one explicit `return EXPR` with non-nil value
        // (excluding returns inside nested defs/lambdas).
        let mut finder = ValueReturnFinder { found: false };
        ruby_prism::visit_statements_node(&mut finder, &stmts);
        if !finder.found {
            return;
        }

        let loc = def.location();
        let start = self.document.offset_to_position(loc.start_offset());
        let end = self.document.offset_to_position(loc.end_offset());
        self.diagnostics.push(Diagnostic {
            range: Range::new(start, end),
            severity: Some(DiagnosticSeverity::WARNING),
            code: Some(NumberOrString::String("inconsistent-return".to_string())),
            code_description: None,
            source: Some("ruby-fast-lsp".to_string()),
            message: "Method has explicit `return` with a value on some paths but \
                      falls through to a side-effect call on another path — \
                      implicit `nil` return may be unintentional."
                .to_string(),
            related_information: None,
            tags: None,
            data: None,
        });
    }
}

impl<'a, 'pr> Visit<'pr> for InconsistentReturnVisitor<'a> {
    fn visit_def_node(&mut self, node: &ruby_prism::DefNode<'pr>) {
        self.check_def(node);
        // Recurse so nested defs (rare) are also checked.
        ruby_prism::visit_def_node(self, node);
    }
}

/// True if `node` is a fn-style call with no receiver, no block, and a name in the
/// canonical "void / side-effect" set. These calls return `nil` and signal intent.
fn is_void_call(node: &ruby_prism::Node<'_>) -> bool {
    let Some(call) = node.as_call_node() else {
        return false;
    };
    if call.receiver().is_some() || call.block().is_some() {
        return false;
    }
    let name = call.name();
    matches!(
        name.as_slice(),
        b"puts" | b"print" | b"p" | b"pp" | b"warn"
    )
}

/// Visitor that finds any `return EXPR` where EXPR is present and not `nil` literal.
/// Skips nested DefNode/LambdaNode/BlockNode bodies — returns there target a
/// different method or block context.
struct ValueReturnFinder {
    found: bool,
}

impl<'pr> Visit<'pr> for ValueReturnFinder {
    fn visit_return_node(&mut self, node: &ruby_prism::ReturnNode<'pr>) {
        if self.found {
            return;
        }
        if let Some(args) = node.arguments() {
            let arg_nodes: Vec<_> = args.arguments().iter().collect();
            for arg in &arg_nodes {
                if arg.as_nil_node().is_none() {
                    self.found = true;
                    return;
                }
            }
        }
    }

    fn visit_def_node(&mut self, _node: &ruby_prism::DefNode<'pr>) {}
    fn visit_lambda_node(&mut self, _node: &ruby_prism::LambdaNode<'pr>) {}
}

/// Walk every `StatementsNode` and flag every statement following a node that
/// always diverges (per `control_flow::analyze`) as unreachable.
///
/// V2: branch-aware via `control_flow` — `if`/`unless`/`case`/`begin` whose
/// branches all terminate are themselves treated as terminators.
fn extract_unreachable_diagnostics(
    parse_result: &ruby_prism::ParseResult<'_>,
    document: &RubyDocument,
) -> Vec<Diagnostic> {
    let mut visitor = UnreachableVisitor {
        document,
        diagnostics: Vec::new(),
    };
    visitor.visit(&parse_result.node());
    visitor.diagnostics
}

struct UnreachableVisitor<'a> {
    document: &'a RubyDocument,
    diagnostics: Vec<Diagnostic>,
}

impl<'a, 'pr> Visit<'pr> for UnreachableVisitor<'a> {
    fn visit_statements_node(&mut self, node: &ruby_prism::StatementsNode<'pr>) {
        let stmts: Vec<_> = node.body().iter().collect();
        let mut terminator_seen = false;
        for stmt in &stmts {
            if terminator_seen {
                let loc = stmt.location();
                let start = self.document.offset_to_position(loc.start_offset());
                let end = self.document.offset_to_position(loc.end_offset());
                self.diagnostics.push(Diagnostic {
                    range: Range::new(start, end),
                    severity: Some(DiagnosticSeverity::WARNING),
                    code: Some(NumberOrString::String("unreachable-code".to_string())),
                    code_description: None,
                    source: Some("ruby-fast-lsp".to_string()),
                    message: "Unreachable code: previous statement always exits this block."
                        .to_string(),
                    related_information: None,
                    tags: Some(vec![tower_lsp::lsp_types::DiagnosticTag::UNNECESSARY]),
                    data: None,
                });
            }
            if !terminator_seen && control_flow::diverges(stmt) {
                terminator_seen = true;
            }
        }
        ruby_prism::visit_statements_node(self, node);
    }
}

/// Extract syntax errors and warnings from a parse result.
fn extract_syntax_diagnostics(
    parse_result: &ruby_prism::ParseResult<'_>,
    document: &RubyDocument,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    let errors: Vec<_> = parse_result.errors().collect();
    if !errors.is_empty() {
        debug!("Found {} syntax errors in document", errors.len());

        for error in errors {
            let location = error.location();
            let start_pos = document.offset_to_position(location.start_offset());
            let end_pos = document.offset_to_position(location.end_offset());

            diagnostics.push(Diagnostic {
                range: Range::new(start_pos, end_pos),
                severity: Some(DiagnosticSeverity::ERROR),
                code: None,
                code_description: None,
                source: Some("ruby-fast-lsp".to_string()),
                message: error.message().to_string(),
                related_information: None,
                tags: None,
                data: None,
            });
        }
    }

    let warnings: Vec<_> = parse_result
        .warnings()
        .filter(|w| w.message() != PRISM_UNREACHABLE_MSG)
        .collect();
    if !warnings.is_empty() {
        debug!("Found {} warnings in document", warnings.len());

        for warning in warnings {
            let location = warning.location();
            let start_pos = document.offset_to_position(location.start_offset());
            let end_pos = document.offset_to_position(location.end_offset());

            diagnostics.push(Diagnostic {
                range: Range::new(start_pos, end_pos),
                severity: Some(DiagnosticSeverity::WARNING),
                code: None,
                code_description: None,
                source: Some("ruby-fast-lsp".to_string()),
                message: warning.message().to_string(),
                related_information: None,
                tags: None,
                data: None,
            });
        }
    }

    debug!(
        "Generated {} total diagnostics for document",
        diagnostics.len()
    );
    diagnostics
}

/// Validate that a diagnostic range is within document bounds.
///
/// Safety check to ensure we don't send invalid ranges to the client.
fn _validate_diagnostic_range(document: &RubyDocument, range: &Range) -> bool {
    let lines: Vec<&str> = document.content.lines().collect();
    let line_count = lines.len();

    if range.start.line as usize >= line_count {
        warn!(
            "Diagnostic start line {} exceeds document line count {}",
            range.start.line, line_count
        );
        return false;
    }

    if range.end.line as usize >= line_count {
        warn!(
            "Diagnostic end line {} exceeds document line count {}",
            range.end.line, line_count
        );
        return false;
    }

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
