//! RBS parser using tree-sitter-rbs grammar.

use tree_sitter::Tree;

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
        for declaration in self.parse(&wrapped)? {
            if let Declaration::TypeAlias(alias) = declaration {
                return Ok(alias.r#type);
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
    fn test_parse_constant_declaration() {
        let source = "ARGV: Array[String]";
        let declarations = Parser::new()
            .parse(source)
            .expect("constant RBS must parse");
        assert_eq!(declarations.len(), 1);
        let Declaration::Constant(constant) = &declarations[0] else {
            panic!("ARGV RBS must produce a constant declaration");
        };
        assert_eq!(constant.name, "ARGV");
        assert_eq!(
            constant.r#type,
            RbsType::ClassInstance {
                name: "Array".to_string(),
                args: vec![RbsType::Class("String".to_string())],
            }
        );
    }

    fn assert_parses_without_syntax_errors(source: &str) {
        let tree = Parser::new()
            .parse_raw(source)
            .expect("tree-sitter must produce a tree for RBS source");
        assert!(
            !tree.root_node().has_error(),
            "valid RBS must parse without tree-sitter error nodes"
        );
    }

    #[test]
    fn test_raw_parse_class_method() {
        let source = r#"
class String
  def length: () -> Integer
end
"#;
        assert_parses_without_syntax_errors(source);
    }

    #[test]
    fn test_raw_parse_overloaded_method_types() {
        let source = r#"
class String
  def length: () -> Integer
  def each_char: () -> Enumerator[String, self]
               | () { (String char) -> void } -> self
end
"#;
        assert_parses_without_syntax_errors(source);
    }

    #[test]
    fn test_parse_named_union_method_parameters() {
        let source = r#"
class String
  def sub: (Regexp | string pattern, string | hash[String, _ToS] replacement) -> String
end
"#;
        let mut parser = Parser::new();
        let declarations = parser.parse(source).expect("RBS method must parse");
        let Declaration::Class(class) = &declarations[0] else {
            panic!("expected class declaration");
        };
        let parameters = &class.methods[0].overloads[0].params;
        assert_eq!(parameters.len(), 2);
        assert_eq!(parameters[0].name.as_deref(), Some("pattern"));
        assert_eq!(parameters[1].name.as_deref(), Some("replacement"));
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
