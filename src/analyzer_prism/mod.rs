use std::fmt;

use crate::types::ruby_method::RubyMethod;
use crate::types::ruby_namespace::RubyConstant;
use crate::types::ruby_variable::RubyVariable;
use crate::types::{ruby_document::RubyDocument, scope::LVScopeStack};
use lsp_types::{Position, Url};
use ruby_prism::Visit;
use visitors::{identifier_visitor::IdentifierVisitor, scope_visitor::ScopeVisitor};

// Export the visitors module
pub mod utils;
pub mod visitors;

// Enum to represent different types of identifiers at a specific position
#[derive(Debug, Clone)]
pub enum Identifier {
    // Eg. Foo::Bar::BAZ | Can be class/module/constant name
    //               ^     -> ([Foo, Bar, BAZ])
    //          ^          -> ([Foo, Bar])
    RubyConstant(Vec<RubyConstant>),

    // Eg. foo; foo.bar;
    //              ^    -> ([], bar)
    // Eg. Foo::Bar.baz;
    //              ^    -> ([Foo, Bar], baz)
    RubyMethod(Vec<RubyConstant>, RubyMethod),

    // Eg. foo = 1; foo;
    //              ^    -> (foo)
    // Eg. @foo = 1; @foo;
    //               ^    -> ([], @foo)
    // Eg. @@foo = 1; @@foo;
    //                ^    -> ([], @@foo)
    RubyVariable(RubyVariable),
}

impl fmt::Display for Identifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Identifier::RubyConstant(ns) => {
                let ns_str: Vec<String> = ns.iter().map(|c| c.to_string()).collect();
                write!(f, "{}", ns_str.join("::"))
            }
            Identifier::RubyMethod(ns, method) => {
                let ns_str: Vec<String> = ns.iter().map(|c| c.to_string()).collect();
                write!(f, "{}#{}", ns_str.join("::"), method)
            }
            Identifier::RubyVariable(variable) => {
                write!(f, "{}", variable)
            }
        }
    }
}

/// Main analyzer for Ruby code using Prism
pub struct RubyPrismAnalyzer {
    pub uri: Url,
    pub code: String,
}

impl RubyPrismAnalyzer {
    pub fn new(uri: Url, code: String) -> Self {
        Self { uri, code }
    }

    /// Returns the identifier and the ancestors stack at the time of the lookup.
    pub fn get_identifier(
        &self,
        position: Position,
    ) -> (Option<Identifier>, Vec<RubyConstant>, LVScopeStack) {
        let parse_result = ruby_prism::parse(self.code.as_bytes());
        // Create a RubyDocument with a dummy URI since we only need it for position handling
        let document = RubyDocument::new(self.uri.clone(), self.code.clone(), 0);
        let root_node = parse_result.node();

        let mut iden_visitor = IdentifierVisitor::new(document.clone(), position);
        iden_visitor.visit(&root_node);

        let mut scope_visitor = ScopeVisitor::new(document.clone(), position);
        scope_visitor.visit(&root_node);

        (
            iden_visitor.identifier,
            iden_visitor.ancestors,
            scope_visitor.scope_stack,
        )
    }

    pub fn get_scope_stack(&self, position: Position) -> LVScopeStack {
        let parse_result = ruby_prism::parse(self.code.as_bytes());
        let document = RubyDocument::new(self.uri.clone(), self.code.clone(), 0);
        let mut visitor = ScopeVisitor::new(document, position);
        let root_node = parse_result.node();
        visitor.visit(&root_node);
        visitor.scope_stack
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper function to parse content and create an analyzer
    fn create_analyzer(content: &str) -> RubyPrismAnalyzer {
        RubyPrismAnalyzer::new(Url::parse("file:///dummy.rb").unwrap(), content.to_string())
    }

    #[test]
    fn test_get_identifier_simple_constant() {
        let content = "CONST_A";
        let analyzer = create_analyzer(content);

        // Position cursor at "CONST_A"
        let position = Position::new(0, 2);
        let (identifier_opt, ancestors, _scope_stack) = analyzer.get_identifier(position);

        // Ensure we found an identifier
        let identifier = identifier_opt.expect("Expected to find an identifier at this position");

        match identifier {
            Identifier::RubyConstant(ns) => {
                assert_eq!(ns.len(), 1, "Should have one constant");
                assert_eq!(ns[0].to_string(), "CONST_A");
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
        let (identifier_opt, ancestors, _scope_stack) = analyzer.get_identifier(position);

        // Ensure we found an identifier
        let identifier = identifier_opt.expect("Expected to find an identifier at this position");

        match identifier {
            Identifier::RubyConstant(ns) => {
                assert_eq!(ns.last().unwrap().to_string(), "CONST_B");
                assert_eq!(
                    ns.len(),
                    2,
                    "Namespace should have two entries: Outer and CONST_B"
                );
                assert_eq!(ns[0].to_string(), "Outer");
                assert_eq!(ns[1].to_string(), "CONST_B");
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
        let (identifier_opt, ancestors, _scope_stack) = analyzer.get_identifier(position);

        // Ensure we found an identifier
        let identifier = identifier_opt.expect("Expected to find an identifier at this position");

        match identifier {
            Identifier::RubyConstant(ns) => {
                assert_eq!(ns.last().unwrap().to_string(), "CONST_A");
                assert_eq!(
                    ns.len(),
                    2,
                    "Namespace should have two entries: Inner and CONST_A"
                );
                assert_eq!(ns[0].to_string(), "Inner");
                assert_eq!(ns[1].to_string(), "CONST_A");
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
        let (identifier_opt, ancestors, _scope_stack) = analyzer.get_identifier(position);

        // Ensure we found an identifier
        let identifier = identifier_opt.expect("Expected to find an identifier at this position");

        match identifier {
            Identifier::RubyConstant(ns) => {
                assert_eq!(ns.last().unwrap().to_string(), "CONST_C");
                assert_eq!(
                    ns.len(),
                    3,
                    "Namespace should have three entries: Outer, Inner, CONST_C"
                );
                assert_eq!(ns[0].to_string(), "Outer");
                assert_eq!(ns[1].to_string(), "Inner");
                assert_eq!(ns[2].to_string(), "CONST_C");
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
        let (identifier_opt, ancestors, _scope_stack) = analyzer.get_identifier(position);

        // Ensure we found an identifier
        let identifier = identifier_opt.expect("Expected to find an identifier at this position");

        match identifier {
            Identifier::RubyConstant(ns) => {
                assert_eq!(ns.last().unwrap().to_string(), "CONST_A");
                assert_eq!(ns.len(), 3);
                assert_eq!(ns[0].to_string(), "Outer");
                assert_eq!(ns[1].to_string(), "Inner");
                assert_eq!(ns[2].to_string(), "CONST_A");
            }
            _ => panic!("Expected RubyConstant, got {:?}", identifier),
        }

        assert!(
            ancestors.is_empty(),
            "Namespace stack should be empty for absolute reference at global scope"
        );

        // Test position at "Inner" in the "::Outer::Inner::CONST_A" reference
        let position = Position::new(7, 18);
        let (identifier_opt, ancestors, _scope_stack) = analyzer.get_identifier(position);

        // Ensure we found an identifier
        let identifier = identifier_opt.expect("Expected to find an identifier at this position");

        match identifier {
            Identifier::RubyConstant(ns) => {
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
        let (identifier_opt, ancestors, _scope_stack) = analyzer.get_identifier(position);

        // Ensure we found an identifier
        let identifier = identifier_opt.expect("Expected to find an identifier at this position");

        match identifier {
            Identifier::RubyConstant(ns) => {
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
        let (identifier_opt, ancestors, _scope_stack) = analyzer.get_identifier(position);

        // Ensure we found an identifier
        let identifier = identifier_opt.expect("Expected to find an identifier at this position");

        match identifier {
            Identifier::RubyConstant(ns) => {
                assert_eq!(ns.last().unwrap().to_string(), "TopLevelConst");
                assert_eq!(
                    ns.len(),
                    1,
                    "Namespace should have one entry for top-level constant"
                );
            }
            _ => panic!("Expected RubyConstant, got {:?}", identifier),
        }

        // There should be one namespace in the stack (Outer) as we're inside it
        assert_eq!(ancestors.len(), 1);
        assert_eq!(ancestors[0].to_string(), "Outer");
    }
}
