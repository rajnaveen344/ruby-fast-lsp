use crate::indexer::types::fully_qualified_name::FullyQualifiedName;
use crate::indexer::types::ruby_namespace::RubyNamespace;
use lsp_types::Position;
use ruby_prism::Visit;
use visitors::identifier_visitor::IdentifierVisitor;

// Export the visitors module
pub mod position;
pub mod visitors;

/// Main analyzer for Ruby code using Prism
pub struct RubyPrismAnalyzer {
    code: String,
    namespace_stack: Vec<RubyNamespace>,
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
    pub fn get_identifier(
        &self,
        position: Position,
    ) -> (Option<FullyQualifiedName>, Vec<RubyNamespace>) {
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
        let (fqn_opt, ancestors) = analyzer.get_identifier(position);

        // Ensure we found an identifier
        let fqn = fqn_opt.expect("Expected to find an identifier at this position");

        match fqn {
            FullyQualifiedName::Constant(ns, constant) => {
                assert_eq!(constant.to_string(), "CONST_A");
                assert!(
                    ns.is_empty(),
                    "Namespace should be empty for top-level constant"
                );
            }
            _ => panic!("Expected Constant FQN, got {:?}", fqn),
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
  module Inner
    CONST_A = 1
  end
end

CONST_B = Outer::Inner::CONST_A
"#;
        let analyzer = create_analyzer(content);

        // Test position at "CONST_A" in the "Outer::Inner::CONST_A" reference
        let position = Position::new(7, 24);
        let (fqn_opt, ancestors) = analyzer.get_identifier(position);

        // Ensure we found an identifier
        let fqn = fqn_opt.expect("Expected to find an identifier at this position");

        match fqn {
            FullyQualifiedName::Constant(ns, constant) => {
                assert_eq!(constant.to_string(), "CONST_A");
                assert_eq!(ns.len(), 2);
                assert_eq!(ns[0].to_string(), "Outer");
                assert_eq!(ns[1].to_string(), "Inner");
            }
            _ => panic!("Expected Constant FQN, got {:?}", fqn),
        }

        assert!(
            ancestors.is_empty(),
            "Namespace stack should be empty as lookup is absolute"
        );

        // Test position at "Inner" in the "Outer::Inner::CONST_A" reference
        let position = Position::new(7, 17);
        let (fqn_opt, ancestors) = analyzer.get_identifier(position);

        // Ensure we found an identifier
        let fqn = fqn_opt.expect("Expected to find an identifier at this position");

        match fqn {
            FullyQualifiedName::Namespace(ns) => {
                assert_eq!(ns.len(), 2);
                assert_eq!(ns[0].to_string(), "Outer");
                assert_eq!(ns[1].to_string(), "Inner");
            }
            _ => panic!("Expected Namespace FQN, got {:?}", fqn),
        }

        assert!(
            ancestors.is_empty(),
            "Namespace stack should be empty for absolute reference"
        );
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
        let (fqn_opt, ancestors) = analyzer.get_identifier(position);

        // Ensure we found an identifier
        let fqn = fqn_opt.expect("Expected to find an identifier at this position");

        match fqn {
            FullyQualifiedName::Constant(ns, constant) => {
                assert_eq!(constant.to_string(), "CONST_A");
                assert_eq!(ns.len(), 1);
                assert_eq!(ns[0].to_string(), "Inner");
            }
            _ => panic!("Expected Constant FQN, got {:?}", fqn),
        }

        // There should be one namespace in the stack (Outer) as we're inside it
        assert_eq!(ancestors.len(), 1);
        assert_eq!(ancestors[0].to_string(), "Outer");
    }

    #[test]
    fn test_get_identifier_absolute_reference() {
        let content = r#"
module Outer
  module Inner
    CONST_A = 1
  end
end

CONST_C = ::Outer::Inner::CONST_A
"#;
        let analyzer = create_analyzer(content);

        // Test position at "CONST_A" in the "::Outer::Inner::CONST_A" reference (absolute reference)
        let position = Position::new(7, 27);
        let (fqn_opt, ancestors) = analyzer.get_identifier(position);

        // Ensure we found an identifier
        let fqn = fqn_opt.expect("Expected to find an identifier at this position");

        match fqn {
            FullyQualifiedName::Constant(ns, constant) => {
                assert_eq!(constant.to_string(), "CONST_A");
                assert_eq!(ns.len(), 2);
                assert_eq!(ns[0].to_string(), "Outer");
                assert_eq!(ns[1].to_string(), "Inner");
            }
            _ => panic!("Expected Constant FQN, got {:?}", fqn),
        }

        assert!(
            ancestors.is_empty(),
            "Namespace stack should be empty for absolute reference"
        );

        // Test position at "Outer" in the "::Outer::Inner::CONST_A" reference
        let position = Position::new(7, 12);
        let (fqn_opt, ancestors) = analyzer.get_identifier(position);

        // Ensure we found an identifier
        let fqn = fqn_opt.expect("Expected to find an identifier at this position");

        match fqn {
            FullyQualifiedName::Namespace(ns) => {
                assert_eq!(ns.len(), 1);
                assert_eq!(ns[0].to_string(), "Outer");
            }
            _ => panic!("Expected Namespace FQN, got {:?}", fqn),
        }

        assert!(
            ancestors.is_empty(),
            "Namespace stack should be empty for absolute reference"
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
        let (fqn_opt, ancestors) = analyzer.get_identifier(position);

        // Ensure we found an identifier
        let fqn = fqn_opt.expect("Expected to find an identifier at this position");

        match fqn {
            FullyQualifiedName::Constant(ns, constant) => {
                assert_eq!(constant.to_string(), "TopLevelConst");
                assert!(
                    ns.is_empty(),
                    "Namespace should be empty for top-level constant"
                );
            }
            _ => panic!("Expected Constant FQN, got {:?}", fqn),
        }

        // There should be one namespace in the stack (Outer) as we're inside it
        assert_eq!(ancestors.len(), 1);
        assert_eq!(ancestors[0].to_string(), "Outer");
    }
}
