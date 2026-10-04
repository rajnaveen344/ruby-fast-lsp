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
    /// Qualified name of the class or module whose body is being visited.
    namespace: Option<String>,
}

impl<'a> Visitor<'a> {
    pub fn new(source: &'a str) -> Self {
        Self {
            source,
            declarations: Vec::new(),
            current_visibility: Visibility::Public,
            namespace: None,
        }
    }

    /// The declared name resolved against the enclosing namespace. A leading
    /// `::` names a top-level declaration.
    fn qualified_name(&self, node: &Node) -> String {
        let name = self.node_text(node).trim();
        if let Some(absolute) = name.strip_prefix("::") {
            return absolute.to_string();
        }
        match &self.namespace {
            Some(namespace) => format!("{namespace}::{name}"),
            None => name.to_string(),
        }
    }

    /// Enter a declaration body, whose members start public. Returns the
    /// outer state for [`Self::leave_namespace`].
    fn enter_namespace(&mut self, name: &str) -> (Option<String>, Visibility) {
        let namespace = self.namespace.replace(name.to_string());
        let visibility = std::mem::replace(&mut self.current_visibility, Visibility::Public);
        (namespace, visibility)
    }

    fn leave_namespace(&mut self, (namespace, visibility): (Option<String>, Visibility)) {
        self.namespace = namespace;
        self.current_visibility = visibility;
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
    fn qualifies_namespaced_and_nested_declarations() {
        let source = r#"
class Outer::Named < Outer::Base
end
module Outer
  class Inner
    LIMIT: Integer
    private
    def hidden: () -> void
  end
  def after: () -> void
  module Deep::Path
  end
  type alias_name = Integer
end
module ::Top
end
"#;
        let declarations = Parser::new().parse(source).unwrap();
        let names = declarations
            .iter()
            .map(|declaration| match declaration {
                Declaration::Class(class) => class.name.as_str(),
                Declaration::Module(module) => module.name.as_str(),
                Declaration::Constant(constant) => constant.name.as_str(),
                Declaration::TypeAlias(alias) => alias.name.as_str(),
                other => panic!("unexpected declaration {other:?}"),
            })
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![
                "Outer::Named",
                "Outer",
                "Outer::Inner",
                "Outer::Inner::LIMIT",
                "Outer::Deep::Path",
                "Outer::alias_name",
                "Top",
            ]
        );
        let Declaration::Class(named) = &declarations[0] else {
            panic!("expected a class");
        };
        assert_eq!(
            named.superclass,
            Some(RbsType::Class("Outer::Base".to_string()))
        );
        let Declaration::Module(outer) = &declarations[1] else {
            panic!("expected a module");
        };
        let after = outer.methods.iter().find(|method| method.name == "after");
        assert_eq!(
            after.map(|method| method.visibility),
            Some(crate::types::Visibility::Public),
            "a nested class's private section does not leak into its namespace"
        );
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
