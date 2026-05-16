//! Diagnostics capability — syntax diagnostics from the parser.
//!
//! AST-only diagnostics (syntax errors/warnings) live here.
//! Index-dependent diagnostics (unresolved entries, YARD issues) are in the query layer.

use crate::analyzer_prism::control_flow;
use crate::inferrer::r#type::ruby::RubyType;
use crate::types::ruby_document::RubyDocument;
use log::debug;
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
    diagnostics.extend(extract_inconsistent_return_diagnostics(
        parse_result,
        document,
    ));
    diagnostics.extend(extract_nil_call_diagnostics(parse_result, document));
    diagnostics
}

/// `nil-call`: method invocation on a local variable whose tracked type at
/// the call position is `NilClass`. V1 fires only on definitive nil — nilable
/// unions stay silent (see module note in tests/diagnostics/nil_call.rs).
fn extract_nil_call_diagnostics(
    parse_result: &ruby_prism::ParseResult<'_>,
    document: &RubyDocument,
) -> Vec<Diagnostic> {
    // VariableScopes are populated by IndexVisitor at file_processor time. Bail
    // when no scopes exist (e.g. tests using bare `generate_diagnostics` before
    // indexing has run).
    if document.variable_scopes().scope_count() == 0 {
        return Vec::new();
    }
    let mut visitor = NilCallVisitor {
        document,
        diagnostics: Vec::new(),
    };
    visitor.visit(&parse_result.node());
    visitor.diagnostics
}

struct NilCallVisitor<'a> {
    document: &'a RubyDocument,
    diagnostics: Vec<Diagnostic>,
}

impl<'a, 'pr> Visit<'pr> for NilCallVisitor<'a> {
    fn visit_call_node(&mut self, node: &ruby_prism::CallNode<'pr>) {
        if let Some(receiver) = node.receiver() {
            if let Some(local) = receiver.as_local_variable_read_node() {
                let var_name = String::from_utf8_lossy(local.name().as_slice()).to_string();
                let recv_loc = receiver.location();
                let recv_pos = self.document.offset_to_position(recv_loc.start_offset());
                let scopes = self.document.variable_scopes();
                let scope_id = scopes
                    .find_scope_for_variable_at(&var_name, recv_pos)
                    .or_else(|| scopes.scope_at_position(recv_pos));
                if let Some(sid) = scope_id {
                    if let Some(ty) = scopes.get_type_at_position(&var_name, sid, recv_pos) {
                        if matches!(ty, RubyType::Class(fqn) if fqn.to_string() == "NilClass") {
                            // Flag the method-name location only, mirroring how
                            // other call diagnostics underline the message.
                            let msg_loc = node.message_loc().unwrap_or(node.location());
                            let start = self.document.offset_to_position(msg_loc.start_offset());
                            let end = self.document.offset_to_position(msg_loc.end_offset());
                            self.diagnostics.push(Diagnostic {
                                range: Range::new(start, end),
                                severity: Some(DiagnosticSeverity::WARNING),
                                code: Some(NumberOrString::String("nil-call".to_string())),
                                code_description: None,
                                source: Some("ruby-fast-lsp".to_string()),
                                message: format!(
                                    "Calling `{}` on `{}` which is `nil` here.",
                                    String::from_utf8_lossy(node.name().as_slice()),
                                    var_name
                                ),
                                related_information: None,
                                tags: None,
                                data: None,
                            });
                            // Suppress downstream chain noise — don't recurse into
                            // this node's children. The chained `.something` after
                            // this call would have an unknown receiver type.
                            return;
                        }
                    }
                }
            }
        }
        ruby_prism::visit_call_node(self, node);
    }
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
    matches!(name.as_slice(), b"puts" | b"print" | b"p" | b"pp" | b"warn")
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
