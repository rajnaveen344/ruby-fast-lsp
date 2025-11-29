//! RBS parser using tree-sitter-rbs grammar.

use tree_sitter::{Node, Tree};

use crate::types::*;
use crate::visitor::Visitor;

/// RBS parser using tree-sitter
pub struct Parser {
    parser: tree_sitter::Parser,
}

impl Parser {
    /// Create a new RBS parser
    pub fn new() -> Self {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_rbs::LANGUAGE.into())
            .expect("Error loading RBS grammar");
        Self { parser }
    }

    /// Parse RBS source code and return declarations
    pub fn parse(&mut self, source: &str) -> Result<Vec<Declaration>, ParseError> {
        let tree = self.parse_to_tree(source)?;
        let mut visitor = Visitor::new(source);
        visitor.visit_program(tree.root_node())?;
        Ok(visitor.declarations)
    }

    /// Parse a single type expression
    pub fn parse_type(&mut self, source: &str) -> Result<RbsType, ParseError> {
        // Wrap the type in a minimal declaration to parse it
        let wrapped = format!("type _t = {}", source);
        let tree = self.parse_to_tree(&wrapped)?;

        let root = tree.root_node();
        let mut cursor = root.walk();

        // Find the type alias declaration
        for child in root.children(&mut cursor) {
            if child.kind() == "type_alias_declaration" {
                let visitor = Visitor::new(&wrapped);
                if let Some(type_node) = child.child_by_field_name("type") {
                    return visitor.visit_type(type_node);
                }
            }
        }

        Err(ParseError::new("Failed to parse type expression"))
    }

    /// Parse source to a tree-sitter tree
    fn parse_to_tree(&mut self, source: &str) -> Result<Tree, ParseError> {
        self.parser
            .parse(source, None)
            .ok_or_else(|| ParseError::new("Failed to parse RBS source"))
    }

    /// Get the raw tree-sitter tree for advanced use cases
    pub fn parse_raw(&mut self, source: &str) -> Result<Tree, ParseError> {
        self.parse_to_tree(source)
    }
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

/// Debug helper to print the tree-sitter AST
#[allow(dead_code)]
pub fn debug_print_tree(source: &str) {
    let mut parser = Parser::new();
    if let Ok(tree) = parser.parse_raw(source) {
        print_node(&tree.root_node(), source, 0);
    }
}

#[allow(dead_code)]
fn print_node(node: &Node, source: &str, indent: usize) {
    let indent_str = "  ".repeat(indent);
    let text = node.utf8_text(source.as_bytes()).unwrap_or("");
    let text_preview = if text.len() > 50 {
        format!("{}...", &text[..50])
    } else {
        text.to_string()
    };

    if node.is_named() {
        println!(
            "{}{} [{}-{}] {:?}",
            indent_str,
            node.kind(),
            node.start_position().row,
            node.end_position().row,
            text_preview.replace('\n', "\\n")
        );
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        print_node(&child, source, indent + 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parser_creation() {
        let parser = Parser::new();
        assert!(parser.parser.language().is_some());
    }

    #[test]
    fn test_parse_empty() {
        let mut parser = Parser::new();
        let result = parser.parse("");
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_parse_comment() {
        let mut parser = Parser::new();
        let result = parser.parse("# This is a comment");
        assert!(result.is_ok());
    }

    #[test]
    fn test_debug_print() {
        let source = r#"
class String
  def length: () -> Integer
end
"#;
        debug_print_tree(source);
    }

    #[test]
    fn test_debug_print_method_types() {
        let source = r#"
class String
  def length: () -> Integer
  def each_char: () -> Enumerator[String, self]
               | () { (String char) -> void } -> self
end
"#;
        debug_print_tree(source);
    }

    #[test]
    fn test_parse_raw_tree() {
        let mut parser = Parser::new();
        let source = "class Foo end";
        let result = parser.parse_raw(source);
        assert!(result.is_ok());

        let tree = result.unwrap();
        let root = tree.root_node();
        assert_eq!(root.kind(), "program");
    }
}
