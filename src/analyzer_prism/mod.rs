use crate::indexer::types::ruby_constant::RubyConstant;
use crate::indexer::types::ruby_method::RubyMethod;
use crate::indexer::types::ruby_namespace::RubyNamespace;
use lsp_types::Position;
use ruby_prism::Visit;
use visitors::identifier_visitor::IdentifierVisitor;

// Export the visitors module
pub mod position;
pub mod visitors;

// Enum to represent different types of identifiers at a specific position
#[derive(Debug, Clone)]
pub enum Identifier {
    // Eg. Foo::Bar::Baz | Can be class/module name
    //               ^     -> ([Foo, Bar, Baz])
    //          ^          -> ([Foo, Bar])
    RubyNamespace(Vec<RubyNamespace>),

    // Eg. Foo::Bar::Baz::CONST
    RubyConstant(Vec<RubyNamespace>, RubyConstant),

    // Eg. foo; foo.bar;
    //              ^    -> ([], bar)
    // Eg. Foo::Bar.baz;
    //              ^    -> ([Foo, Bar], baz)
    RubyMethod(Vec<RubyNamespace>, RubyMethod),

    // Eg. foo = 1; foo;
    //              ^    -> (foo)
    RubyLocalVariable(String),

    // Eg. @foo = 1; @foo;
    //               ^    -> ([], @foo)
    RubyInstanceVariable(Vec<RubyNamespace>, String),

    // Eg. @@foo = 1; @@foo;
    //                ^    -> ([], @@foo)
    RubyClassVariable(Vec<RubyNamespace>, String),
}

/// Main analyzer for Ruby code using Prism
pub struct RubyPrismAnalyzer {
    pub code: String,
    pub namespace_stack: Vec<RubyNamespace>,
}

impl RubyPrismAnalyzer {
    pub fn new(code: String) -> Self {
        Self {
            code,
            namespace_stack: Vec::new(),
        }
    }

    pub fn get_namespace_stack(&self) -> Vec<RubyNamespace> {
        self.namespace_stack.clone()
    }

    /// Returns the identifier and the ancestors stack at the time of the lookup.
    pub fn get_identifier(&self, position: Position) -> (Option<Identifier>, Vec<RubyNamespace>) {
        let parse_result = ruby_prism::parse(self.code.as_bytes());
        let mut visitor = IdentifierVisitor::new(self.code.clone(), position);
        let root_node = parse_result.node();
        visitor.visit(&root_node);
        (visitor.identifier, visitor.ancestors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper function to parse content and create an analyzer
    fn create_analyzer(content: &str) -> RubyPrismAnalyzer {
        RubyPrismAnalyzer::new(content.to_string())
    }

    #[test]
    fn test_get_identifier_simple_constant() {
        let content = "CONST_A";
        let analyzer = create_analyzer(content);

        // Position cursor at "CONST_A"
        let position = Position::new(0, 2);
        let (identifier_opt, ancestors) = analyzer.get_identifier(position);

        // Ensure we found an identifier
        let identifier = identifier_opt.expect("Expected to find an identifier at this position");

        match identifier {
            Identifier::RubyConstant(ns, constant) => {
                assert_eq!(constant.to_string(), "CONST_A");
                assert!(
                    ns.is_empty(),
                    "Namespace should be empty for top-level constant"
                );
            }
            _ => panic!("Expected RubyConstant, got {:?}", identifier),
        }

        assert!(
            ancestors.is_empty(),
            "Namespace stack should be empty for top-level constant"
        );
    }

    #[test]
    fn test_get_identifier_nested_constant() {
        let content = r#"
module Outer
  CONST_B = 20
end
"#;
        let analyzer = create_analyzer(content);

        // Position cursor at "CONST_B"
        let position = Position::new(2, 5);
        let (identifier_opt, ancestors) = analyzer.get_identifier(position);

        // Ensure we found an identifier
        let identifier = identifier_opt.expect("Expected to find an identifier at this position");

        match identifier {
            Identifier::RubyConstant(ns, constant) => {
                assert_eq!(constant.to_string(), "CONST_B");
                assert_eq!(ns.len(), 1, "Namespace should have one entry: Outer");
                assert_eq!(ns[0].to_string(), "Outer");
            }
            _ => panic!("Expected RubyConstant, got {:?}", identifier),
        }

        // There should be one namespace in the stack (Outer) as we're inside it
        assert_eq!(ancestors.len(), 1);
        assert_eq!(ancestors[0].to_string(), "Outer");
    }

    #[test]
    fn test_get_identifier_with_parent_namespace() {
        let content = r#"
module Outer
  module Inner
    CONST_A = 1
  end

  CONST_B = Inner::CONST_A
end
"#;
        let analyzer = create_analyzer(content);

        // Test position at "CONST_A" in the "Inner::CONST_A" reference (relative reference)
        let position = Position::new(6, 19);
        let (identifier_opt, ancestors) = analyzer.get_identifier(position);

        // Ensure we found an identifier
        let identifier = identifier_opt.expect("Expected to find an identifier at this position");

        match identifier {
            Identifier::RubyConstant(ns, constant) => {
                assert_eq!(constant.to_string(), "CONST_A");
                assert_eq!(ns.len(), 1, "Namespace for CONST_A should be Inner");
                assert_eq!(ns[0].to_string(), "Inner");
            }
            _ => panic!("Expected RubyConstant, got {:?}", identifier),
        }

        // Ancestor stack should be [Outer] because the lookup Inner::CONST_A happens within Outer
        assert_eq!(ancestors.len(), 1);
        assert_eq!(ancestors[0].to_string(), "Outer");
    }

    #[test]
    fn test_get_identifier_deeply_nested_constant() {
        let content = r#"
module Outer
  module Inner
    CONST_C = 30
  end
end
"#;
        let analyzer = create_analyzer(content);

        // Position cursor at "CONST_C"
        let position = Position::new(3, 9);
        let (identifier_opt, ancestors) = analyzer.get_identifier(position);

        // Ensure we found an identifier
        let identifier = identifier_opt.expect("Expected to find an identifier at this position");

        match identifier {
            Identifier::RubyConstant(ns, constant) => {
                assert_eq!(constant.to_string(), "CONST_C");
                assert_eq!(
                    ns.len(),
                    2,
                    "Namespace should have two entries: Outer, Inner"
                );
                assert_eq!(ns[0].to_string(), "Outer");
                assert_eq!(ns[1].to_string(), "Inner");
            }
            _ => panic!("Expected RubyConstant, got {:?}", identifier),
        }

        // Namespace stack should be [Outer, Inner]
        assert_eq!(ancestors.len(), 2);
        assert_eq!(ancestors[0].to_string(), "Outer");
        assert_eq!(ancestors[1].to_string(), "Inner");
    }

    #[test]
    fn test_get_identifier_absolute_reference_constant() {
        let content = r#"
module Outer
  module Inner
    CONST_A = 10
  end
end

val = ::Outer::Inner::CONST_A
"#;
        let analyzer = create_analyzer(content);

        // Test position at "CONST_A" in the "::Outer::Inner::CONST_A" reference
        let position = Position::new(7, 25);
        let (identifier_opt, ancestors) = analyzer.get_identifier(position);

        // Ensure we found an identifier
        let identifier = identifier_opt.expect("Expected to find an identifier at this position");

        match identifier {
            Identifier::RubyConstant(ns, constant) => {
                assert_eq!(constant.to_string(), "CONST_A");
                assert_eq!(ns.len(), 2);
                assert_eq!(ns[0].to_string(), "Outer");
                assert_eq!(ns[1].to_string(), "Inner");
            }
            _ => panic!("Expected RubyConstant, got {:?}", identifier),
        }

        assert!(
            ancestors.is_empty(),
            "Namespace stack should be empty for absolute reference at global scope"
        );

        // Test position at "Inner" in the "::Outer::Inner::CONST_A" reference
        let position = Position::new(7, 18);
        let (identifier_opt, ancestors) = analyzer.get_identifier(position);

        // Ensure we found an identifier
        let identifier = identifier_opt.expect("Expected to find an identifier at this position");

        match identifier {
            Identifier::RubyNamespace(ns) => {
                assert_eq!(ns.len(), 2);
                assert_eq!(ns[0].to_string(), "Outer");
                assert_eq!(ns[1].to_string(), "Inner");
            }
            _ => panic!("Expected RubyNamespace, got {:?}", identifier),
        }

        assert!(
            ancestors.is_empty(),
            "Namespace stack should be empty for absolute reference at global scope"
        );

        // Test position at "Outer" in the "::Outer::Inner::CONST_A" reference
        let position = Position::new(7, 12);
        let (identifier_opt, ancestors) = analyzer.get_identifier(position);

        // Ensure we found an identifier
        let identifier = identifier_opt.expect("Expected to find an identifier at this position");

        match identifier {
            Identifier::RubyNamespace(ns) => {
                assert_eq!(ns.len(), 1);
                assert_eq!(ns[0].to_string(), "Outer");
            }
            _ => panic!("Expected RubyNamespace, got {:?}", identifier),
        }

        assert!(
            ancestors.is_empty(),
            "Namespace stack should be empty for absolute reference at global scope"
        );
    }

    #[test]
    fn test_get_identifier_top_level_constant() {
        let content = r#"
TopLevelConst = 10
module Outer
  val = TopLevelConst
end
"#;
        let analyzer = create_analyzer(content);

        // Test position at "TopLevelConst" in the "val = TopLevelConst" reference
        let position = Position::new(3, 10);
        let (identifier_opt, ancestors) = analyzer.get_identifier(position);

        // Ensure we found an identifier
        let identifier = identifier_opt.expect("Expected to find an identifier at this position");

        match identifier {
            Identifier::RubyConstant(ns, constant) => {
                assert_eq!(constant.to_string(), "TopLevelConst");
                assert!(
                    ns.is_empty(),
                    "Namespace should be empty for top-level constant"
                );
            }
            _ => panic!("Expected RubyConstant, got {:?}", identifier),
        }

        // There should be one namespace in the stack (Outer) as we're inside it
        assert_eq!(ancestors.len(), 1);
        assert_eq!(ancestors[0].to_string(), "Outer");
    }
}
