//! Visitor to convert tree-sitter AST to our RBS types.

mod declarations;
mod members;
mod methods;
mod types;

use tree_sitter::Node;

use crate::types::{Declaration, Location, ParseError, Visibility};

/// Visitor that converts tree-sitter nodes to our AST types
pub struct Visitor<'a> {
    source: &'a str,
    pub declarations: Vec<Declaration>,
    current_visibility: Visibility,
}

impl<'a> Visitor<'a> {
    pub fn new(source: &'a str) -> Self {
        Self {
            source,
            declarations: Vec::new(),
            current_visibility: Visibility::Public,
        }
    }

    /// Get the text content of a node
    fn node_text(&self, node: &Node) -> &str {
        node.utf8_text(self.source.as_bytes()).unwrap_or("")
    }

    /// Get location from a tree-sitter node
    fn node_location(&self, node: &Node) -> Location {
        let start = node.start_position();
        let end = node.end_position();
        Location::new(start.row, start.column, end.row, end.column)
    }

    /// Visit the root program node
    pub fn visit_program(&mut self, node: Node) -> Result<(), ParseError> {
        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            self.visit_declaration(child)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::RbsType;
    use crate::Parser;

    #[test]
    fn test_visit_simple_class() {
        let mut parser = Parser::new();
        let source = "class Foo end";
        let result = parser.parse(source);
        assert!(result.is_ok());

        let decls = result.unwrap();
        assert_eq!(decls.len(), 1);

        if let Declaration::Class(class) = &decls[0] {
            assert_eq!(class.name, "Foo");
        } else {
            panic!("Expected class");
        }
    }

    #[test]
    fn test_visit_class_with_superclass() {
        let mut parser = Parser::new();
        let source = "class Foo < Bar end";
        let result = parser.parse(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_simple_type() {
        assert_eq!(
            Visitor::parse_simple_type("String"),
            RbsType::Class("String".to_string())
        );
        assert_eq!(Visitor::parse_simple_type("void"), RbsType::Void);
        assert_eq!(Visitor::parse_simple_type("nil"), RbsType::Nil);
        assert_eq!(Visitor::parse_simple_type("bool"), RbsType::Bool);
        assert_eq!(Visitor::parse_simple_type("self"), RbsType::SelfType);
    }

    #[test]
    fn test_parse_optional_type() {
        let result = Visitor::parse_simple_type("String?");
        assert_eq!(
            result,
            RbsType::optional(RbsType::Class("String".to_string()))
        );
    }

    #[test]
    fn test_parse_generic_type() {
        let result = Visitor::parse_simple_type("Array[String]");
        assert_eq!(
            result,
            RbsType::generic("Array", vec![RbsType::Class("String".to_string())])
        );
    }
}
