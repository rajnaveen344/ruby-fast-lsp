use std::fmt;

use crate::types::ruby_method::RubyMethod;
use crate::types::ruby_namespace::RubyConstant;
use crate::types::ruby_variable::RubyVariable;
use crate::types::{ruby_document::RubyDocument, scope::LVScopeStack};
use lsp_types::{Position, Url};
use ruby_prism::Visit;
use visitors::identifier_visitor::IdentifierVisitor;

// Export the visitors module
pub mod scope_tracker;
pub mod utils;
pub mod visitors;

/// Enum to categorize method receiver types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReceiverKind {
    /// No receiver, e.g., "method_a"
    None,

    /// Self receiver, e.g., "self.method_a"
    SelfReceiver,

    /// Constant receiver, e.g., "Class.method_a" or "Module::Class.method_a"
    Constant,

    /// Expression receiver, e.g., "a.method_a" or "(a + b).method_a"
    Expr,
}

/// Enum to represent different types of identifiers at a specific position
#[derive(Debug, Clone)]
pub enum Identifier {
    /// Ruby constant with namespace context and identifier path
    /// - namespace: Current namespace stack (where the cursor is located)
    /// - iden: The constant path being referenced
    RubyConstant {
        namespace: Vec<RubyConstant>,
        iden: Vec<RubyConstant>,
    },

    /// Ruby method with comprehensive context
    /// - namespace: Current namespace stack (where the cursor is located)
    /// - receiver_kind: Type of method receiver
    /// - receiver: Receiver information for constant receivers
    /// - iden: The method being called
    RubyMethod {
        namespace: Vec<RubyConstant>,
        receiver_kind: ReceiverKind,
        receiver: Option<Vec<RubyConstant>>,
        iden: RubyMethod,
    },

    /// Ruby variable with appropriate scope context
    /// - iden: The variable information (includes its own scope context)
    RubyVariable { iden: RubyVariable },
}

impl fmt::Display for Identifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Identifier::RubyConstant { namespace: _, iden } => {
                let iden_str: Vec<String> = iden.iter().map(|c| c.to_string()).collect();
                write!(f, "{}", iden_str.join("::"))
            }
            Identifier::RubyMethod {
                namespace: _,
                receiver_kind: _,
                receiver: _,
                iden,
            } => {
                write!(f, "{}", iden)
            }
            Identifier::RubyVariable { iden } => {
                write!(f, "{}", iden)
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

        let (identifier, _, ns_stack_at_pos, lv_stack_at_pos) = iden_visitor.get_result();
        (identifier, ns_stack_at_pos, lv_stack_at_pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper function to parse content and create an analyzer
    fn create_analyzer(content: &str) -> RubyPrismAnalyzer {
        RubyPrismAnalyzer::new(Url::parse("file:///dummy.rb").unwrap(), content.to_string())
    }

    // Helper functions for test assertions with the new Identifier enum structure

    /// Assert that an identifier is a RubyConstant with the expected constant path
    pub fn assert_constant_identifier(identifier: &Identifier, expected_path: &[&str]) {
        match identifier {
            Identifier::RubyConstant { namespace: _, iden } => {
                assert_eq!(
                    iden.len(),
                    expected_path.len(),
                    "Expected constant path length {}, got {}",
                    expected_path.len(),
                    iden.len()
                );
                for (i, expected) in expected_path.iter().enumerate() {
                    assert_eq!(
                        iden[i].to_string(),
                        *expected,
                        "Expected constant at position {} to be '{}', got '{}'",
                        i,
                        expected,
                        iden[i]
                    );
                }
            }
            _ => panic!("Expected RubyConstant identifier, got {:?}", identifier),
        }
    }

    /// Assert that an identifier is a RubyMethod with the expected method name and receiver kind
    pub fn assert_method_identifier(
        identifier: &Identifier,
        expected_method: &str,
        expected_receiver_kind: ReceiverKind,
    ) {
        match identifier {
            Identifier::RubyMethod {
                namespace: _,
                receiver_kind,
                receiver: _,
                iden,
            } => {
                assert_eq!(
                    *receiver_kind, expected_receiver_kind,
                    "Expected receiver kind {:?}, got {:?}",
                    expected_receiver_kind, receiver_kind
                );
                assert_eq!(
                    iden.to_string(),
                    expected_method,
                    "Expected method name '{}', got '{}'",
                    expected_method,
                    iden
                );
            }
            _ => panic!("Expected RubyMethod identifier, got {:?}", identifier),
        }
    }

    /// Assert that an identifier is a RubyVariable with the expected variable name
    pub fn assert_variable_identifier(identifier: &Identifier, expected_name: &str) {
        match identifier {
            Identifier::RubyVariable { iden } => {
                assert_eq!(
                    iden.to_string(),
                    expected_name,
                    "Expected variable name '{}', got '{}'",
                    expected_name,
                    iden
                );
            }
            _ => panic!("Expected RubyVariable identifier, got {:?}", identifier),
        }
    }

    /// Assert that the namespace context has the expected length and contents
    pub fn assert_namespace_context(namespace: &[RubyConstant], expected_namespace: &[&str]) {
        assert_eq!(
            namespace.len(),
            expected_namespace.len(),
            "Expected namespace length {}, got {}",
            expected_namespace.len(),
            namespace.len()
        );
        for (i, expected) in expected_namespace.iter().enumerate() {
            assert_eq!(
                namespace[i].to_string(),
                *expected,
                "Expected namespace at position {} to be '{}', got '{}'",
                i,
                expected,
                namespace[i]
            );
        }
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

        // Use helper function for cleaner assertion
        assert_constant_identifier(&identifier, &["CONST_A"]);
        assert_namespace_context(&ancestors, &["Object"]);
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

        // Use helper functions for cleaner assertions
        assert_constant_identifier(&identifier, &["CONST_B"]);
        assert_namespace_context(&ancestors, &["Object", "Outer"]);
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
            Identifier::RubyConstant { namespace: _, iden } => {
                assert_eq!(iden.last().unwrap().to_string(), "CONST_A");
                assert_eq!(
                    iden.len(),
                    2,
                    "Identifier should have two entries: Inner and CONST_A"
                );
                assert_eq!(iden[0].to_string(), "Inner");
                assert_eq!(iden[1].to_string(), "CONST_A");
            }
            _ => panic!("Expected RubyConstant, got {:?}", identifier),
        }

        // Ancestor stack should be [Outer] because the lookup Inner::CONST_A happens within Outer
        assert_eq!(ancestors.len(), 2);
        assert_eq!(ancestors[0].to_string(), "Object");
        assert_eq!(ancestors[1].to_string(), "Outer");
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
            Identifier::RubyConstant { namespace: _, iden } => {
                assert_eq!(iden[0].to_string(), "CONST_C");
                assert_eq!(iden.len(), 1, "Identifier should have one entry: CONST_C");
            }
            _ => panic!("Expected RubyConstant, got {:?}", identifier),
        }

        // Namespace stack should be [Outer, Inner]
        assert_eq!(ancestors.len(), 3);
        assert_eq!(ancestors[0].to_string(), "Object");
        assert_eq!(ancestors[1].to_string(), "Outer");
        assert_eq!(ancestors[2].to_string(), "Inner");
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
            Identifier::RubyConstant { namespace: _, iden } => {
                assert_eq!(iden.last().unwrap().to_string(), "CONST_A");
                assert_eq!(iden.len(), 3);
                assert_eq!(iden[0].to_string(), "Outer");
                assert_eq!(iden[1].to_string(), "Inner");
                assert_eq!(iden[2].to_string(), "CONST_A");
            }
            _ => panic!("Expected RubyConstant, got {:?}", identifier),
        }

        assert_eq!(
            ancestors.len(),
            1,
            "Namespace stack should have one entry for absolute reference at global scope"
        );

        // Test position at "Inner" in the "::Outer::Inner::CONST_A" reference
        let position = Position::new(7, 18);
        let (identifier_opt, ancestors, _scope_stack) = analyzer.get_identifier(position);

        // Ensure we found an identifier
        let identifier = identifier_opt.expect("Expected to find an identifier at this position");

        match identifier {
            Identifier::RubyConstant { namespace: _, iden } => {
                assert_eq!(iden.len(), 2);
                assert_eq!(iden[0].to_string(), "Outer");
                assert_eq!(iden[1].to_string(), "Inner");
            }
            _ => panic!("Expected RubyConstant, got {:?}", identifier),
        }

        assert_eq!(
            ancestors.len(),
            1,
            "Namespace stack should have one entry for absolute reference at global scope"
        );

        // Test position at "Outer" in the "::Outer::Inner::CONST_A" reference
        let position = Position::new(7, 12);
        let (identifier_opt, ancestors, _scope_stack) = analyzer.get_identifier(position);

        // Ensure we found an identifier
        let identifier = identifier_opt.expect("Expected to find an identifier at this position");

        match identifier {
            Identifier::RubyConstant { namespace: _, iden } => {
                assert_eq!(iden.len(), 1);
                assert_eq!(iden[0].to_string(), "Outer");
            }
            _ => panic!("Expected RubyConstant, got {:?}", identifier),
        }

        assert_eq!(
            ancestors.len(),
            1,
            "Namespace stack should have one entry for absolute reference at global scope"
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
            Identifier::RubyConstant { namespace: _, iden } => {
                assert_eq!(iden.last().unwrap().to_string(), "TopLevelConst");
                assert_eq!(
                    iden.len(),
                    1,
                    "Identifier should have one entry for top-level constant"
                );
            }
            _ => panic!("Expected RubyConstant, got {:?}", identifier),
        }

        // There should be one namespace in the stack (Outer) as we're inside it
        assert_eq!(ancestors.len(), 2);
        assert_eq!(ancestors[0].to_string(), "Object");
        assert_eq!(ancestors[1].to_string(), "Outer");
    }

    #[test]
    fn test_helper_functions_demonstration() {
        // Test constant helper
        let content = "Foo::Bar::BAZ";
        let analyzer = create_analyzer(content);
        let position = Position::new(0, 10); // Position at "BAZ"
        let (identifier_opt, ancestors, _) = analyzer.get_identifier(position);
        let identifier = identifier_opt.expect("Expected identifier");

        // Demonstrate constant helper usage
        assert_constant_identifier(&identifier, &["Foo", "Bar", "BAZ"]);
        assert_namespace_context(&ancestors, &["Object"]);

        // Test method helper (this would need a method call context)
        let method_content = r#"
class TestClass
  def test_method
    some_method
  end
end
"#;
        let method_analyzer = create_analyzer(method_content);
        let method_position = Position::new(3, 6); // Position at "some_method"
        let (method_identifier_opt, method_ancestors, _) =
            method_analyzer.get_identifier(method_position);

        if let Some(method_identifier) = method_identifier_opt {
            // Demonstrate method helper usage
            assert_method_identifier(&method_identifier, "some_method", ReceiverKind::None);
            assert_namespace_context(&method_ancestors, &["Object", "TestClass"]);
        }

        // Test variable helper
        let variable_content = r#"
class TestClass
  def test_method
    local_var = 42
    local_var
  end
end
"#;
        let variable_analyzer = create_analyzer(variable_content);
        let variable_position = Position::new(4, 6); // Position at "local_var" usage
        let (variable_identifier_opt, _, _) = variable_analyzer.get_identifier(variable_position);

        if let Some(variable_identifier) = variable_identifier_opt {
            // Demonstrate variable helper usage
            assert_variable_identifier(&variable_identifier, "local_var");
        }
    }

    // Comprehensive tests for ReceiverKind classification

    #[test]
    fn test_receiver_kind_none_simple_method_call() {
        let content = r#"
class TestClass
  def test_method
    simple_method
  end
end
"#;
        let analyzer = create_analyzer(content);
        let position = Position::new(3, 8); // Position at "simple_method"
        let (identifier_opt, ancestors, _) = analyzer.get_identifier(position);

        let identifier = identifier_opt.expect("Expected to find method identifier");
        assert_method_identifier(&identifier, "simple_method", ReceiverKind::None);
        assert_namespace_context(&ancestors, &["Object", "TestClass"]);
    }

    #[test]
    fn test_receiver_kind_none_method_with_arguments() {
        let content = r#"
class TestClass
  def test_method
    method_with_args(1, 2, 3)
  end
end
"#;
        let analyzer = create_analyzer(content);
        let position = Position::new(3, 8); // Position at "method_with_args"
        let (identifier_opt, ancestors, _) = analyzer.get_identifier(position);

        let identifier = identifier_opt.expect("Expected to find method identifier");
        assert_method_identifier(&identifier, "method_with_args", ReceiverKind::None);
        assert_namespace_context(&ancestors, &["Object", "TestClass"]);
    }

    #[test]
    fn test_receiver_kind_none_method_with_block() {
        let content = r#"
class TestClass
  def test_method
    method_with_block { |x| x + 1 }
  end
end
"#;
        let analyzer = create_analyzer(content);
        let position = Position::new(3, 8); // Position at "method_with_block"
        let (identifier_opt, ancestors, _) = analyzer.get_identifier(position);

        let identifier = identifier_opt.expect("Expected to find method identifier");
        assert_method_identifier(&identifier, "method_with_block", ReceiverKind::None);
        assert_namespace_context(&ancestors, &["Object", "TestClass"]);
    }

    #[test]
    fn test_receiver_kind_none_top_level_method() {
        let content = r#"
def global_method
  puts "hello"
end

global_method
"#;
        let analyzer = create_analyzer(content);
        let position = Position::new(5, 5); // Position at "global_method"
        let (identifier_opt, ancestors, _) = analyzer.get_identifier(position);

        let identifier = identifier_opt.expect("Expected to find method identifier");
        assert_method_identifier(&identifier, "global_method", ReceiverKind::None);
        assert_namespace_context(&ancestors, &["Object"]);
    }

    #[test]
    fn test_receiver_kind_self_receiver_simple() {
        let content = r#"
class TestClass
  def test_method
    self.helper_method
  end
end
"#;
        let analyzer = create_analyzer(content);
        let position = Position::new(3, 12); // Position at "helper_method"
        let (identifier_opt, ancestors, _) = analyzer.get_identifier(position);

        let identifier = identifier_opt.expect("Expected to find method identifier");
        assert_method_identifier(&identifier, "helper_method", ReceiverKind::SelfReceiver);
        assert_namespace_context(&ancestors, &["Object", "TestClass"]);
    }

    #[test]
    fn test_receiver_kind_self_receiver_with_arguments() {
        let content = r#"
class TestClass
  def test_method
    self.helper_method(arg1, arg2)
  end
end
"#;
        let analyzer = create_analyzer(content);
        let position = Position::new(3, 12); // Position at "helper_method"
        let (identifier_opt, ancestors, _) = analyzer.get_identifier(position);

        let identifier = identifier_opt.expect("Expected to find method identifier");
        assert_method_identifier(&identifier, "helper_method", ReceiverKind::SelfReceiver);
        assert_namespace_context(&ancestors, &["Object", "TestClass"]);
    }

    #[test]
    fn test_receiver_kind_self_receiver_chained() {
        let content = r#"
class TestClass
  def test_method
    self.first_method.second_method
  end
end
"#;
        let analyzer = create_analyzer(content);
        let position = Position::new(3, 25); // Position at "second_method"
        let (identifier_opt, ancestors, _) = analyzer.get_identifier(position);

        let identifier = identifier_opt.expect("Expected to find method identifier");
        assert_method_identifier(&identifier, "second_method", ReceiverKind::Expr);
        assert_namespace_context(&ancestors, &["Object", "TestClass"]);
    }

    #[test]
    fn test_receiver_kind_constant_simple_class_method() {
        let content = r#"
class MyClass
  def self.class_method
    puts "class method"
  end
end

MyClass.class_method
"#;
        let analyzer = create_analyzer(content);
        let position = Position::new(7, 12); // Position at "class_method"
        let (identifier_opt, ancestors, _) = analyzer.get_identifier(position);

        let identifier = identifier_opt.expect("Expected to find method identifier");
        assert_method_identifier(&identifier, "class_method", ReceiverKind::Constant);
        assert_namespace_context(&ancestors, &["Object"]);
    }

    #[test]
    fn test_receiver_kind_constant_nested_class_method() {
        let content = r#"
module MyModule
  class MyClass
    def self.nested_method
      puts "nested method"
    end
  end
end

MyModule::MyClass.nested_method
"#;
        let analyzer = create_analyzer(content);
        let position = Position::new(9, 22); // Position at "nested_method"
        let (identifier_opt, ancestors, _) = analyzer.get_identifier(position);

        let identifier = identifier_opt.expect("Expected to find method identifier");
        assert_method_identifier(&identifier, "nested_method", ReceiverKind::Constant);
        assert_namespace_context(&ancestors, &["Object"]);
    }

    #[test]
    fn test_receiver_kind_constant_module_method() {
        let content = r#"
module MyModule
  def self.module_method
    puts "module method"
  end
end

MyModule.module_method
"#;
        let analyzer = create_analyzer(content);
        let position = Position::new(7, 15); // Position at "module_method"
        let (identifier_opt, ancestors, _) = analyzer.get_identifier(position);

        let identifier = identifier_opt.expect("Expected to find method identifier");
        assert_method_identifier(&identifier, "module_method", ReceiverKind::Constant);
        assert_namespace_context(&ancestors, &["Object"]);
    }

    #[test]
    fn test_receiver_kind_constant_deeply_nested() {
        let content = r#"
module A
  module B
    module C
      class D
        def self.deep_method
          puts "deep method"
        end
      end
    end
  end
end

A::B::C::D.deep_method
"#;
        let analyzer = create_analyzer(content);
        let position = Position::new(13, 17); // Position at "deep_method"
        let (identifier_opt, ancestors, _) = analyzer.get_identifier(position);

        let identifier = identifier_opt.expect("Expected to find method identifier");
        assert_method_identifier(&identifier, "deep_method", ReceiverKind::Constant);
        assert_namespace_context(&ancestors, &["Object"]);
    }

    #[test]
    fn test_receiver_kind_expr_variable_receiver() {
        let content = r#"
class TestClass
  def test_method
    obj = SomeClass.new
    obj.instance_method
  end
end
"#;
        let analyzer = create_analyzer(content);
        let position = Position::new(4, 12); // Position at "instance_method"
        let (identifier_opt, ancestors, _) = analyzer.get_identifier(position);

        let identifier = identifier_opt.expect("Expected to find method identifier");
        assert_method_identifier(&identifier, "instance_method", ReceiverKind::Expr);
        assert_namespace_context(&ancestors, &["Object", "TestClass"]);
    }

    #[test]
    fn test_receiver_kind_expr_method_chain() {
        let content = r#"
class TestClass
  def test_method
    obj.first_method.second_method
  end
end
"#;
        let analyzer = create_analyzer(content);
        let position = Position::new(3, 25); // Position at "second_method"
        let (identifier_opt, ancestors, _) = analyzer.get_identifier(position);

        let identifier = identifier_opt.expect("Expected to find method identifier");
        assert_method_identifier(&identifier, "second_method", ReceiverKind::Expr);
        assert_namespace_context(&ancestors, &["Object", "TestClass"]);
    }

    #[test]
    fn test_receiver_kind_expr_parenthesized_expression() {
        let content = r#"
class TestClass
  def test_method
    (a + b).result_method
  end
end
"#;
        let analyzer = create_analyzer(content);
        let position = Position::new(3, 16); // Position at "result_method"
        let (identifier_opt, ancestors, _) = analyzer.get_identifier(position);

        let identifier = identifier_opt.expect("Expected to find method identifier");
        assert_method_identifier(&identifier, "result_method", ReceiverKind::Expr);
        assert_namespace_context(&ancestors, &["Object", "TestClass"]);
    }

    #[test]
    fn test_receiver_kind_expr_array_access() {
        let content = r#"
class TestClass
  def test_method
    arr[0].array_element_method
  end
end
"#;
        let analyzer = create_analyzer(content);
        let position = Position::new(3, 19); // Position at "array_element_method"
        let (identifier_opt, ancestors, _) = analyzer.get_identifier(position);

        let identifier = identifier_opt.expect("Expected to find method identifier");
        assert_method_identifier(&identifier, "array_element_method", ReceiverKind::Expr);
        assert_namespace_context(&ancestors, &["Object", "TestClass"]);
    }

    #[test]
    fn test_receiver_kind_expr_hash_access() {
        let content = r#"
class TestClass
  def test_method
    hash[:key].hash_value_method
  end
end
"#;
        let analyzer = create_analyzer(content);
        let position = Position::new(3, 20); // Position at "hash_value_method"
        let (identifier_opt, ancestors, _) = analyzer.get_identifier(position);

        let identifier = identifier_opt.expect("Expected to find method identifier");
        assert_method_identifier(&identifier, "hash_value_method", ReceiverKind::Expr);
        assert_namespace_context(&ancestors, &["Object", "TestClass"]);
    }

    #[test]
    fn test_receiver_kind_expr_instance_variable() {
        let content = r#"
class TestClass
  def test_method
    @instance_var.instance_var_method
  end
end
"#;
        let analyzer = create_analyzer(content);
        let position = Position::new(3, 25); // Position at "instance_var_method"
        let (identifier_opt, ancestors, _) = analyzer.get_identifier(position);

        let identifier = identifier_opt.expect("Expected to find method identifier");
        assert_method_identifier(&identifier, "instance_var_method", ReceiverKind::Expr);
        assert_namespace_context(&ancestors, &["Object", "TestClass"]);
    }

    #[test]
    fn test_receiver_kind_expr_class_variable() {
        let content = r#"
class TestClass
  def test_method
    @@class_var.class_var_method
  end
end
"#;
        let analyzer = create_analyzer(content);
        let position = Position::new(3, 22); // Position at "class_var_method"
        let (identifier_opt, ancestors, _) = analyzer.get_identifier(position);

        let identifier = identifier_opt.expect("Expected to find method identifier");
        assert_method_identifier(&identifier, "class_var_method", ReceiverKind::Expr);
        assert_namespace_context(&ancestors, &["Object", "TestClass"]);
    }

    #[test]
    fn test_receiver_kind_expr_global_variable() {
        let content = r#"
class TestClass
  def test_method
    $global_var.global_var_method
  end
end
"#;
        let analyzer = create_analyzer(content);
        let position = Position::new(3, 22); // Position at "global_var_method"
        let (identifier_opt, ancestors, _) = analyzer.get_identifier(position);

        let identifier = identifier_opt.expect("Expected to find method identifier");
        assert_method_identifier(&identifier, "global_var_method", ReceiverKind::Expr);
        assert_namespace_context(&ancestors, &["Object", "TestClass"]);
    }

    // ===== Enhanced Constant Identifier Context Tests =====
    // These tests specifically address task 8 requirements

    #[test]
    fn test_constant_resolution_nested_modules_context() {
        let content = r#"
module Level1
  module Level2
    module Level3
      DEEP_CONST = "deep value"

      def self.access_constant
        DEEP_CONST
      end
    end

    def self.access_nested_constant
      Level3::DEEP_CONST
    end
  end

  def self.access_deeply_nested_constant
    Level2::Level3::DEEP_CONST
  end
end
"#;
        let analyzer = create_analyzer(content);

        // Test 1: DEEP_CONST accessed within Level3 (same namespace)
        let position = Position::new(7, 10);
        let (identifier_opt, ancestors, _) = analyzer.get_identifier(position);

        let identifier = identifier_opt.expect("Expected to find constant identifier");

        assert_constant_identifier(&identifier, &["DEEP_CONST"]);
        assert_namespace_context(&ancestors, &["Object", "Level1", "Level2", "Level3"]);

        // Test 2: Level3::DEEP_CONST accessed within Level2
        let position = Position::new(12, 20);
        let (identifier_opt, ancestors, _) = analyzer.get_identifier(position);

        let identifier = identifier_opt.expect("Expected to find constant identifier");

        assert_constant_identifier(&identifier, &["Level3", "DEEP_CONST"]);
        assert_namespace_context(&ancestors, &["Object", "Level1", "Level2"]);

        // Test 3: Level2::Level3::DEEP_CONST accessed within Level1
        let position = Position::new(17, 25); // Adjusted position for DEEP_CONST
        let (identifier_opt, ancestors, _) = analyzer.get_identifier(position);

        let identifier = identifier_opt.expect("Expected to find constant identifier");

        assert_constant_identifier(&identifier, &["Level2", "Level3", "DEEP_CONST"]);
        assert_namespace_context(&ancestors, &["Object", "Level1"]);
    }

    #[test]
    fn test_absolute_constant_path_resolution() {
        let content = r#"
module Outer
  class Inner
    CONST_VALUE = 42
  end

  module AnotherModule
    val1 = ::Outer::Inner::CONST_VALUE
    val2 = ::TopLevelConst
  end
end

TopLevelConst = "top level"
"#;
        let analyzer = create_analyzer(content);

        // Test 1: Absolute reference ::Outer::Inner::CONST_VALUE
        let position = Position::new(7, 35);
        let (identifier_opt, ancestors, _) = analyzer.get_identifier(position);
        let identifier = identifier_opt.expect("Expected to find constant identifier");

        assert_constant_identifier(&identifier, &["Outer", "Inner", "CONST_VALUE"]);
        assert_namespace_context(&ancestors, &["Object", "Outer", "AnotherModule"]);

        // Test 2: Absolute reference ::Outer::Inner
        let position = Position::new(7, 20);
        let (identifier_opt, ancestors, _) = analyzer.get_identifier(position);
        let identifier = identifier_opt.expect("Expected to find constant identifier");

        assert_constant_identifier(&identifier, &["Outer", "Inner"]);
        assert_namespace_context(&ancestors, &["Object", "Outer", "AnotherModule"]);

        // Test 3: Absolute reference ::Outer
        let position = Position::new(7, 14);
        let (identifier_opt, ancestors, _) = analyzer.get_identifier(position);
        let identifier = identifier_opt.expect("Expected to find constant identifier");

        assert_constant_identifier(&identifier, &["Outer"]);
        assert_namespace_context(&ancestors, &["Object", "Outer", "AnotherModule"]);

        // Test 4: Absolute reference to top-level constant ::TopLevelConst
        let position = Position::new(8, 18);
        let (identifier_opt, ancestors, _) = analyzer.get_identifier(position);
        let identifier = identifier_opt.expect("Expected to find constant identifier");

        assert_constant_identifier(&identifier, &["TopLevelConst"]);
        assert_namespace_context(&ancestors, &["Object", "Outer", "AnotherModule"]);
    }

    #[test]
    fn test_constant_path_precision_cursor_positions() {
        let content = r#"
module Alpha
  module Beta
    class Gamma
      DELTA = "value"
    end
  end
end

result = Alpha::Beta::Gamma::DELTA
"#;
        let analyzer = create_analyzer(content);

        // Test 1: Cursor on Alpha
        let position = Position::new(9, 11);
        let (identifier_opt, ancestors, _) = analyzer.get_identifier(position);
        let identifier = identifier_opt.expect("Expected to find constant identifier");

        assert_constant_identifier(&identifier, &["Alpha"]);
        assert_namespace_context(&ancestors, &["Object"]);

        // Test 2: Cursor on Beta
        let position = Position::new(9, 18);
        let (identifier_opt, ancestors, _) = analyzer.get_identifier(position);
        let identifier = identifier_opt.expect("Expected to find constant identifier");

        assert_constant_identifier(&identifier, &["Alpha", "Beta"]);
        assert_namespace_context(&ancestors, &["Object"]);

        // Test 3: Cursor on Gamma
        let position = Position::new(9, 25);
        let (identifier_opt, ancestors, _) = analyzer.get_identifier(position);
        let identifier = identifier_opt.expect("Expected to find constant identifier");

        assert_constant_identifier(&identifier, &["Alpha", "Beta", "Gamma"]);
        assert_namespace_context(&ancestors, &["Object"]);

        // Test 4: Cursor on DELTA
        let position = Position::new(9, 32);
        let (identifier_opt, ancestors, _) = analyzer.get_identifier(position);
        let identifier = identifier_opt.expect("Expected to find constant identifier");

        assert_constant_identifier(&identifier, &["Alpha", "Beta", "Gamma", "DELTA"]);
        assert_namespace_context(&ancestors, &["Object"]);
    }

    #[test]
    fn test_constant_identifiers_various_namespace_contexts() {
        let content = r#"
GLOBAL_CONST = "global"

module OuterModule
  OUTER_CONST = "outer"

  class OuterClass
    CLASS_CONST = "class"

    def instance_method
      local1 = GLOBAL_CONST
      local2 = OUTER_CONST
      local3 = CLASS_CONST
      local4 = OuterModule::OUTER_CONST
    end

    module InnerModule
      INNER_CONST = "inner"

      def self.module_method
        val1 = GLOBAL_CONST
        val2 = OUTER_CONST
        val3 = CLASS_CONST
        val4 = INNER_CONST
        val5 = OuterModule::OuterClass::CLASS_CONST
      end
    end
  end
end
"#;
        let analyzer = create_analyzer(content);

        // Test 1: GLOBAL_CONST accessed from instance method in OuterClass
        let position = Position::new(10, 18);
        let (identifier_opt, ancestors, _) = analyzer.get_identifier(position);
        let identifier = identifier_opt.expect("Expected to find constant identifier");

        assert_constant_identifier(&identifier, &["GLOBAL_CONST"]);
        assert_namespace_context(&ancestors, &["Object", "OuterModule", "OuterClass"]);

        // Test 2: OUTER_CONST accessed from instance method in OuterClass
        let position = Position::new(11, 18);
        let (identifier_opt, ancestors, _) = analyzer.get_identifier(position);
        let identifier = identifier_opt.expect("Expected to find constant identifier");

        assert_constant_identifier(&identifier, &["OUTER_CONST"]);
        assert_namespace_context(&ancestors, &["Object", "OuterModule", "OuterClass"]);

        // Test 3: CLASS_CONST accessed from instance method in OuterClass
        let position = Position::new(12, 18);
        let (identifier_opt, ancestors, _) = analyzer.get_identifier(position);
        let identifier = identifier_opt.expect("Expected to find constant identifier");

        assert_constant_identifier(&identifier, &["CLASS_CONST"]);
        assert_namespace_context(&ancestors, &["Object", "OuterModule", "OuterClass"]);

        // Test 4: Qualified access OuterModule::OUTER_CONST
        let position = Position::new(13, 35);
        let (identifier_opt, ancestors, _) = analyzer.get_identifier(position);
        let identifier = identifier_opt.expect("Expected to find constant identifier");

        assert_constant_identifier(&identifier, &["OuterModule", "OUTER_CONST"]);
        assert_namespace_context(&ancestors, &["Object", "OuterModule", "OuterClass"]);

        // Test 5: GLOBAL_CONST accessed from InnerModule
        let position = Position::new(20, 18);
        let (identifier_opt, ancestors, _) = analyzer.get_identifier(position);
        let identifier = identifier_opt.expect("Expected to find constant identifier");

        assert_constant_identifier(&identifier, &["GLOBAL_CONST"]);
        assert_namespace_context(
            &ancestors,
            &["Object", "OuterModule", "OuterClass", "InnerModule"],
        );

        // Test 6: INNER_CONST accessed from InnerModule
        let position = Position::new(23, 18);
        let (identifier_opt, ancestors, _) = analyzer.get_identifier(position);
        let identifier = identifier_opt.expect("Expected to find constant identifier");

        assert_constant_identifier(&identifier, &["INNER_CONST"]);
        assert_namespace_context(
            &ancestors,
            &["Object", "OuterModule", "OuterClass", "InnerModule"],
        );

        // Test 7: Fully qualified access OuterModule::OuterClass::CLASS_CONST
        let position = Position::new(24, 50);
        let (identifier_opt, ancestors, _) = analyzer.get_identifier(position);
        let identifier = identifier_opt.expect("Expected to find constant identifier");

        assert_constant_identifier(&identifier, &["OuterModule", "OuterClass", "CLASS_CONST"]);
        assert_namespace_context(
            &ancestors,
            &["Object", "OuterModule", "OuterClass", "InnerModule"],
        );
    }
}
