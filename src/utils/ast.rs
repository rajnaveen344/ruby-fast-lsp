//! AST utility functions shared across the codebase.
//!
//! This module contains common AST traversal and search functions
//! to avoid duplication across hover, inlay_hints, and type inference.

use ruby_prism::{DefNode, Node};

/// Find a DefNode at the given line in the AST.
///
/// This recursively searches the AST for a method definition (`def`) node
/// whose start position matches the target line.
///
/// # Arguments
/// * `node` - The AST node to search within
/// * `target_line` - The line number to search for (0-indexed)
/// * `content` - The source file content (used to calculate line numbers)
///
/// # Returns
/// * `Some(DefNode)` if a matching method definition is found
/// * `None` if no match is found
pub fn find_def_node_at_line<'a>(
    node: &Node<'a>,
    target_line: u32,
    content: &str,
) -> Option<DefNode<'a>> {
    // Try to match DefNode
    if let Some(def_node) = node.as_def_node() {
        let offset = def_node.location().start_offset();
        // Calculate line from byte offset (count newlines before this offset)
        let line = content.as_bytes()[..offset]
            .iter()
            .filter(|&&b| b == b'\n')
            .count() as u32;
        if line == target_line {
            return Some(def_node);
        }
    }

    // Recurse into child nodes (Program, Class, Module, Statements)
    if let Some(program) = node.as_program_node() {
        for stmt in program.statements().body().iter() {
            if let Some(found) = find_def_node_at_line(&stmt, target_line, content) {
                return Some(found);
            }
        }
    }

    if let Some(class_node) = node.as_class_node() {
        if let Some(body) = class_node.body() {
            if let Some(stmts) = body.as_statements_node() {
                for stmt in stmts.body().iter() {
                    if let Some(found) = find_def_node_at_line(&stmt, target_line, content) {
                        return Some(found);
                    }
                }
            }
        }
    }

    if let Some(module_node) = node.as_module_node() {
        if let Some(body) = module_node.body() {
            if let Some(stmts) = body.as_statements_node() {
                for stmt in stmts.body().iter() {
                    if let Some(found) = find_def_node_at_line(&stmt, target_line, content) {
                        return Some(found);
                    }
                }
            }
        }
    }

    if let Some(stmts) = node.as_statements_node() {
        for stmt in stmts.body().iter() {
            if let Some(found) = find_def_node_at_line(&stmt, target_line, content) {
                return Some(found);
            }
        }
    }

    None
}

/// Check if a byte offset is in a statement position (inside a `StatementsNode`).
///
/// Snippets (if, def, class, etc.) are statements — they only make sense where Ruby
/// expects statements, not where it expects value expressions. This function walks
/// the AST using the visitor pattern, tracking whether the deepest node containing
/// the offset was entered via a `StatementsNode`.
///
/// Value positions (method arguments, array elements, hash values, string
/// interpolations, etc.) return `false`.
pub fn is_in_statement_position(node: &Node, offset: usize) -> bool {
    use ruby_prism::Visit;

    struct StatementPositionVisitor {
        offset: usize,
        /// Tracks the deepest determination: true = statement position, false = value position
        in_statements: bool,
    }

    /// Helper macro: if this node contains the offset, mark as value position and recurse.
    macro_rules! visit_value_container {
        ($self:ident, $node:expr, $visit_fn:path) => {{
            let loc = $node.location();
            if $self.offset >= loc.start_offset() && $self.offset <= loc.end_offset() {
                $self.in_statements = false;
            }
            $visit_fn($self, $node);
        }};
    }

    impl<'pr> Visit<'pr> for StatementPositionVisitor {
        fn visit_statements_node(&mut self, node: &ruby_prism::StatementsNode<'pr>) {
            let loc = node.location();
            if self.offset >= loc.start_offset() && self.offset <= loc.end_offset() {
                self.in_statements = true;
            }
            ruby_prism::visit_statements_node(self, node);
        }

        fn visit_arguments_node(&mut self, node: &ruby_prism::ArgumentsNode<'pr>) {
            visit_value_container!(self, node, ruby_prism::visit_arguments_node);
        }

        fn visit_array_node(&mut self, node: &ruby_prism::ArrayNode<'pr>) {
            visit_value_container!(self, node, ruby_prism::visit_array_node);
        }

        fn visit_hash_node(&mut self, node: &ruby_prism::HashNode<'pr>) {
            visit_value_container!(self, node, ruby_prism::visit_hash_node);
        }

        fn visit_interpolated_string_node(
            &mut self,
            node: &ruby_prism::InterpolatedStringNode<'pr>,
        ) {
            visit_value_container!(self, node, ruby_prism::visit_interpolated_string_node);
        }

        fn visit_interpolated_symbol_node(
            &mut self,
            node: &ruby_prism::InterpolatedSymbolNode<'pr>,
        ) {
            visit_value_container!(self, node, ruby_prism::visit_interpolated_symbol_node);
        }

        fn visit_embedded_statements_node(
            &mut self,
            node: &ruby_prism::EmbeddedStatementsNode<'pr>,
        ) {
            // #{...} inside strings — value position.
            // Don't recurse: the inner StatementsNode would incorrectly flip in_statements back.
            let loc = node.location();
            if self.offset >= loc.start_offset() && self.offset <= loc.end_offset() {
                self.in_statements = false;
            }
        }

        fn visit_parameters_node(&mut self, node: &ruby_prism::ParametersNode<'pr>) {
            visit_value_container!(self, node, ruby_prism::visit_parameters_node);
        }

        fn visit_assoc_node(&mut self, node: &ruby_prism::AssocNode<'pr>) {
            visit_value_container!(self, node, ruby_prism::visit_assoc_node);
        }
    }

    let mut visitor = StatementPositionVisitor {
        offset,
        in_statements: true, // top-level is a statement position
    };
    visitor.visit(node);
    visitor.in_statements
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_def_node_at_line() {
        let content = "def foo\n  42\nend";
        let parse_result = ruby_prism::parse(content.as_bytes());
        let root = parse_result.node();

        let def_node = find_def_node_at_line(&root, 0, content);
        assert!(def_node.is_some());
        assert_eq!(
            String::from_utf8_lossy(def_node.unwrap().name().as_slice()),
            "foo"
        );
    }

    #[test]
    fn test_find_def_node_in_class() {
        let content = "class Foo\n  def bar\n    123\n  end\nend";
        let parse_result = ruby_prism::parse(content.as_bytes());
        let root = parse_result.node();

        let def_node = find_def_node_at_line(&root, 1, content);
        assert!(def_node.is_some());
        assert_eq!(
            String::from_utf8_lossy(def_node.unwrap().name().as_slice()),
            "bar"
        );
    }

    #[test]
    fn test_find_def_node_not_found() {
        let content = "x = 42";
        let parse_result = ruby_prism::parse(content.as_bytes());
        let root = parse_result.node();

        let def_node = find_def_node_at_line(&root, 0, content);
        assert!(def_node.is_none());
    }

    #[test]
    fn test_statement_position_in_method_body() {
        // "def foo\n  i\nend"
        // offset of 'i' = 10 (d=0,e=1,f=2,' '=3,f=4,o=5,o=6,\n=7,' '=8,' '=9,i=10)
        let content = "def foo\n  i\nend";
        let parse_result = ruby_prism::parse(content.as_bytes());
        let root = parse_result.node();
        assert!(
            is_in_statement_position(&root, 10),
            "offset 10 ('i' in method body) should be a statement position"
        );
    }

    #[test]
    fn test_statement_position_at_top_level() {
        let content = "d";
        let parse_result = ruby_prism::parse(content.as_bytes());
        let root = parse_result.node();
        assert!(
            is_in_statement_position(&root, 0),
            "offset 0 ('d' at top level) should be a statement position"
        );
    }

    #[test]
    fn test_value_position_in_array() {
        // "x = 1\na = [x]"
        // offset of 'x' inside array = 11 (x=0, ' '=1, ==2, ' '=3, 1=4, \n=5, a=6, ' '=7, ==8, ' '=9, [=10, x=11)
        let content = "x = 1\na = [x]";
        let parse_result = ruby_prism::parse(content.as_bytes());
        let root = parse_result.node();
        assert!(
            !is_in_statement_position(&root, 11),
            "offset 11 ('x' inside array literal) should NOT be a statement position"
        );
    }

    #[test]
    fn test_value_position_in_method_args() {
        let content = "y = 1\nfoo(y)";
        // y=0,' '=1,==2,' '=3,1=4,\n=5,f=6,o=7,o=8,(=9,y=10,)=11
        let parse_result = ruby_prism::parse(content.as_bytes());
        let root = parse_result.node();
        assert!(
            !is_in_statement_position(&root, 10),
            "offset 10 ('y' inside method call args) should NOT be a statement position"
        );
    }
}
