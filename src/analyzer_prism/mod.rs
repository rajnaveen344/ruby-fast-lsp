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

    // ===== Variable Identifier Scope Context Tests =====

    #[test]
    fn test_local_variable_resolution_simple_method_scope() {
        let content = r#"
class TestClass
  def test_method
    local_var = 42
    puts local_var
  end
end
"#;
        let analyzer = create_analyzer(content);
        let position = Position::new(4, 10); // Position at "local_var" usage
        let (identifier_opt, namespace, _lv_scope_stack) = analyzer.get_identifier(position);

        let identifier = identifier_opt.expect("Expected to find variable identifier");
        assert_variable_identifier(&identifier, "local_var");
        assert_namespace_context(&namespace, &["Object", "TestClass"]);

        // Verify the variable has proper local variable scope context
        match identifier {
            Identifier::RubyVariable { iden } => {
                match iden.variable_type() {
                    crate::types::ruby_variable::RubyVariableType::Local(scope_stack) => {
                        assert!(
                            !scope_stack.is_empty(),
                            "Local variable should have scope stack"
                        );
                        // Should have at least one method scope
                        assert!(scope_stack.iter().any(|scope| matches!(
                            scope.kind(),
                            crate::types::scope::LVScopeKind::InstanceMethod | crate::types::scope::LVScopeKind::ClassMethod
                        )));
                    }
                    _ => panic!(
                        "Expected Local variable type, got {:?}",
                        iden.variable_type()
                    ),
                }
            }
            _ => panic!("Expected RubyVariable identifier"),
        }
    }

    #[test]
    fn test_local_variable_resolution_nested_scopes() {
        let content = r#"
class TestClass
  def outer_method
    outer_var = "outer"

    [1, 2, 3].each do |item|
      inner_var = "inner"
      puts outer_var  # Can access outer scope
      puts inner_var  # Local to block
    end
  end
end
"#;
        let analyzer = create_analyzer(content);

        // Test access to outer_var from within block
        let position = Position::new(7, 12); // Position at "outer_var" in block
        let (identifier_opt, namespace, _) = analyzer.get_identifier(position);

        let identifier = identifier_opt.expect("Expected to find variable identifier");
        assert_variable_identifier(&identifier, "outer_var");
        assert_namespace_context(&namespace, &["Object", "TestClass"]);

        // Test access to inner_var within block
        let position = Position::new(8, 12); // Position at "inner_var" in block
        let (identifier_opt, namespace, _) = analyzer.get_identifier(position);

        let identifier = identifier_opt.expect("Expected to find variable identifier");
        assert_variable_identifier(&identifier, "inner_var");
        assert_namespace_context(&namespace, &["Object", "TestClass"]);

        // Verify both have proper local variable scope context
        match identifier {
            Identifier::RubyVariable { iden } => {
                match iden.variable_type() {
                    crate::types::ruby_variable::RubyVariableType::Local(scope_stack) => {
                        assert!(
                            !scope_stack.is_empty(),
                            "Local variable should have scope stack"
                        );
                        // Should have method scope and potentially block scope
                        assert!(scope_stack.iter().any(|scope| matches!(
                            scope.kind(),
                            crate::types::scope::LVScopeKind::InstanceMethod | crate::types::scope::LVScopeKind::ClassMethod
                        )));
                    }
                    _ => panic!("Expected Local variable type"),
                }
            }
            _ => panic!("Expected RubyVariable identifier"),
        }
    }

    #[test]
    fn test_local_variable_resolution_block_local_variables() {
        let content = r#"
class TestClass
  def test_method
    outer_var = "outer"

    [1, 2, 3].each do |item; block_local|
      block_local = "block local"
      puts block_local
    end
  end
end
"#;
        let analyzer = create_analyzer(content);

        // Test access to explicitly declared block-local variable
        let position = Position::new(7, 12); // Position at "block_local" usage
        let (identifier_opt, namespace, _) = analyzer.get_identifier(position);

        let identifier = identifier_opt.expect("Expected to find variable identifier");
        assert_variable_identifier(&identifier, "block_local");
        assert_namespace_context(&namespace, &["Object", "TestClass"]);

        // Verify it has proper local variable scope context with explicit block local scope
        match identifier {
            Identifier::RubyVariable { iden } => {
                match iden.variable_type() {
                    crate::types::ruby_variable::RubyVariableType::Local(scope_stack) => {
                        assert!(
                            !scope_stack.is_empty(),
                            "Local variable should have scope stack"
                        );
                        // Should have method scope and explicit block local scope
                        assert!(scope_stack.iter().any(|scope| matches!(
                            scope.kind(),
                            crate::types::scope::LVScopeKind::InstanceMethod | crate::types::scope::LVScopeKind::ClassMethod
                        )));
                    }
                    _ => panic!("Expected Local variable type"),
                }
            }
            _ => panic!("Expected RubyVariable identifier"),
        }
    }

    #[test]
    fn test_local_variable_resolution_rescue_scope() {
        let content = r#"
class TestClass
  def test_method
    begin
      risky_operation
    rescue StandardError => error_var
      puts error_var.message
    end
  end
end
"#;
        let analyzer = create_analyzer(content);

        // Test access to rescue variable
        let position = Position::new(6, 12); // Position at "error_var" usage
        let (identifier_opt, namespace, _) = analyzer.get_identifier(position);

        let identifier = identifier_opt.expect("Expected to find variable identifier");
        assert_variable_identifier(&identifier, "error_var");
        assert_namespace_context(&namespace, &["Object", "TestClass"]);

        // Verify it has proper local variable scope context with rescue scope
        match identifier {
            Identifier::RubyVariable { iden } => {
                match iden.variable_type() {
                    crate::types::ruby_variable::RubyVariableType::Local(scope_stack) => {
                        assert!(
                            !scope_stack.is_empty(),
                            "Local variable should have scope stack"
                        );
                        // Should have method scope and potentially rescue scope
                        assert!(scope_stack.iter().any(|scope| matches!(
                            scope.kind(),
                            crate::types::scope::LVScopeKind::InstanceMethod | crate::types::scope::LVScopeKind::ClassMethod
                        )));
                    }
                    _ => panic!("Expected Local variable type"),
                }
            }
            _ => panic!("Expected RubyVariable identifier"),
        }
    }

    #[test]
    fn test_local_variable_resolution_class_body_scope() {
        let content = r#"
class TestClass
  class_local = "class local variable"

  def instance_method
    # class_local is not accessible here
    method_local = "method local"
    puts method_local
  end

  puts class_local
end
"#;
        let analyzer = create_analyzer(content);

        // Test access to class body local variable
        let position = Position::new(10, 8); // Position at "class_local" usage in class body
        let (identifier_opt, namespace, _) = analyzer.get_identifier(position);

        let identifier = identifier_opt.expect("Expected to find variable identifier");
        assert_variable_identifier(&identifier, "class_local");
        assert_namespace_context(&namespace, &["Object", "TestClass"]);

        // Verify it has proper local variable scope context with constant scope
        match identifier {
            Identifier::RubyVariable { iden } => {
                match iden.variable_type() {
                    crate::types::ruby_variable::RubyVariableType::Local(scope_stack) => {
                        assert!(
                            !scope_stack.is_empty(),
                            "Local variable should have scope stack"
                        );
                        // Should have constant scope (class body)
                        assert!(scope_stack.iter().any(|scope| matches!(
                            scope.kind(),
                            crate::types::scope::LVScopeKind::Constant
                        )));
                    }
                    _ => panic!("Expected Local variable type"),
                }
            }
            _ => panic!("Expected RubyVariable identifier"),
        }
    }

    #[test]
    fn test_local_variable_resolution_module_body_scope() {
        let content = r#"
module TestModule
  module_local = "module local variable"

  def self.module_method
    method_local = "method local"
    puts method_local
  end

  puts module_local
end
"#;
        let analyzer = create_analyzer(content);

        // Test access to module body local variable
        let position = Position::new(9, 8); // Position at "module_local" usage in module body
        let (identifier_opt, namespace, _) = analyzer.get_identifier(position);

        let identifier = identifier_opt.expect("Expected to find variable identifier");
        assert_variable_identifier(&identifier, "module_local");
        assert_namespace_context(&namespace, &["Object", "TestModule"]);

        // Verify it has proper local variable scope context with constant scope
        match identifier {
            Identifier::RubyVariable { iden } => {
                match iden.variable_type() {
                    crate::types::ruby_variable::RubyVariableType::Local(scope_stack) => {
                        assert!(
                            !scope_stack.is_empty(),
                            "Local variable should have scope stack"
                        );
                        // Should have constant scope (module body)
                        assert!(scope_stack.iter().any(|scope| matches!(
                            scope.kind(),
                            crate::types::scope::LVScopeKind::Constant
                        )));
                    }
                    _ => panic!("Expected Local variable type"),
                }
            }
            _ => panic!("Expected RubyVariable identifier"),
        }
    }

    #[test]
    fn test_instance_variable_resolution_with_namespace_context() {
        let content = r#"
class TestClass
  def initialize
    @instance_var = "instance value"
  end

  def access_instance_var
    puts @instance_var
  end
end
"#;
        let analyzer = create_analyzer(content);

        // Test access to instance variable
        let position = Position::new(7, 10); // Position at "@instance_var" usage
        let (identifier_opt, namespace, _) = analyzer.get_identifier(position);

        let identifier = identifier_opt.expect("Expected to find variable identifier");
        assert_variable_identifier(&identifier, "@instance_var");
        assert_namespace_context(&namespace, &["Object", "TestClass"]);

        // Verify it has proper instance variable type (no additional scope context needed)
        match identifier {
            Identifier::RubyVariable { iden } => {
                match iden.variable_type() {
                    crate::types::ruby_variable::RubyVariableType::Instance => {
                        // Instance variables don't need additional scope context
                        // Their scope is determined by the namespace context
                    }
                    _ => panic!(
                        "Expected Instance variable type, got {:?}",
                        iden.variable_type()
                    ),
                }
            }
            _ => panic!("Expected RubyVariable identifier"),
        }
    }

    #[test]
    fn test_instance_variable_resolution_nested_classes() {
        let content = r#"
class OuterClass
  def initialize
    @outer_instance = "outer"
  end

  class InnerClass
    def initialize
      @inner_instance = "inner"
    end

    def access_vars
      puts @inner_instance  # Can access own instance var
      # puts @outer_instance  # Cannot access outer class instance var
    end
  end
end
"#;
        let analyzer = create_analyzer(content);

        // Test access to inner class instance variable
        let position = Position::new(12, 12); // Position at "@inner_instance" usage
        let (identifier_opt, namespace, _) = analyzer.get_identifier(position);

        let identifier = identifier_opt.expect("Expected to find variable identifier");
        assert_variable_identifier(&identifier, "@inner_instance");
        assert_namespace_context(&namespace, &["Object", "OuterClass", "InnerClass"]);

        // Verify it has proper instance variable type with correct namespace context
        match identifier {
            Identifier::RubyVariable { iden } => {
                match iden.variable_type() {
                    crate::types::ruby_variable::RubyVariableType::Instance => {
                        // Instance variable scope is determined by namespace context
                    }
                    _ => panic!("Expected Instance variable type"),
                }
            }
            _ => panic!("Expected RubyVariable identifier"),
        }
    }

    #[test]
    fn test_class_variable_resolution_with_namespace_context() {
        let content = r#"
class TestClass
  @@class_var = "class value"

  def self.class_method
    puts @@class_var
  end

  def instance_method
    puts @@class_var
  end
end
"#;
        let analyzer = create_analyzer(content);

        // Test access to class variable from class method
        let position = Position::new(5, 10); // Position at "@@class_var" in class method
        let (identifier_opt, namespace, _) = analyzer.get_identifier(position);

        let identifier = identifier_opt.expect("Expected to find variable identifier");
        assert_variable_identifier(&identifier, "@@class_var");
        assert_namespace_context(&namespace, &["Object", "TestClass"]);

        // Test access to class variable from instance method
        let position = Position::new(9, 10); // Position at "@@class_var" in instance method
        let (identifier_opt, namespace, _) = analyzer.get_identifier(position);

        let identifier = identifier_opt.expect("Expected to find variable identifier");
        assert_variable_identifier(&identifier, "@@class_var");
        assert_namespace_context(&namespace, &["Object", "TestClass"]);

        // Verify it has proper class variable type (namespace context determines scope)
        match identifier {
            Identifier::RubyVariable { iden } => {
                match iden.variable_type() {
                    crate::types::ruby_variable::RubyVariableType::Class => {
                        // Class variables scope is determined by namespace context
                    }
                    _ => panic!(
                        "Expected Class variable type, got {:?}",
                        iden.variable_type()
                    ),
                }
            }
            _ => panic!("Expected RubyVariable identifier"),
        }
    }

    #[test]
    fn test_class_variable_resolution_inheritance() {
        let content = r#"
class ParentClass
  @@shared_var = "shared"
end

class ChildClass < ParentClass
  def access_shared
    puts @@shared_var  # Can access parent's class variable
  end

  def set_shared
    @@shared_var = "modified"
  end
end
"#;
        let analyzer = create_analyzer(content);

        // Test access to inherited class variable
        let position = Position::new(7, 10); // Position at "@@shared_var" in child class
        let (identifier_opt, namespace, _) = analyzer.get_identifier(position);

        let identifier = identifier_opt.expect("Expected to find variable identifier");
        assert_variable_identifier(&identifier, "@@shared_var");
        assert_namespace_context(&namespace, &["Object", "ChildClass"]);

        // Verify it has proper class variable type
        match identifier {
            Identifier::RubyVariable { iden } => {
                match iden.variable_type() {
                    crate::types::ruby_variable::RubyVariableType::Class => {
                        // Class variables are shared across inheritance hierarchy
                    }
                    _ => panic!("Expected Class variable type"),
                }
            }
            _ => panic!("Expected RubyVariable identifier"),
        }
    }

    #[test]
    fn test_global_variable_resolution_no_additional_context() {
        let content = r#"
$global_var = "global value"

class TestClass
  def test_method
    puts $global_var
  end
end

module TestModule
  def self.module_method
    puts $global_var
  end
end

puts $global_var
"#;
        let analyzer = create_analyzer(content);

        // Test access to global variable from class method
        let position = Position::new(5, 10); // Position at "$global_var" in class
        let (identifier_opt, namespace, _) = analyzer.get_identifier(position);

        let identifier = identifier_opt.expect("Expected to find variable identifier");
        assert_variable_identifier(&identifier, "$global_var");
        assert_namespace_context(&namespace, &["Object", "TestClass"]);

        // Test access to global variable from module method
        let position = Position::new(11, 10); // Position at "$global_var" in module
        let (identifier_opt, namespace, _) = analyzer.get_identifier(position);

        let identifier = identifier_opt.expect("Expected to find variable identifier");
        assert_variable_identifier(&identifier, "$global_var");
        assert_namespace_context(&namespace, &["Object", "TestModule"]);

        // Test access to global variable from top level
        let position = Position::new(15, 5); // Position at "$global_var" at top level
        let (identifier_opt, namespace, _) = analyzer.get_identifier(position);

        let identifier = identifier_opt.expect("Expected to find variable identifier");
        assert_variable_identifier(&identifier, "$global_var");
        assert_namespace_context(&namespace, &["Object"]);

        // Verify it has proper global variable type (no additional context)
        match identifier {
            Identifier::RubyVariable { iden } => {
                match iden.variable_type() {
                    crate::types::ruby_variable::RubyVariableType::Global => {
                        // Global variables have no additional scope context
                        // They are accessible from anywhere
                    }
                    _ => panic!(
                        "Expected Global variable type, got {:?}",
                        iden.variable_type()
                    ),
                }
            }
            _ => panic!("Expected RubyVariable identifier"),
        }
    }

    #[test]
    fn test_global_variable_special_variables() {
        let content = r#"
def test_method
  puts $1  # Regex capture group
  puts $_  # Last input line
  puts $!  # Last exception
  puts $$  # Process ID
end
"#;
        let analyzer = create_analyzer(content);

        // Test special global variables
        let test_cases = vec![
            (2, 8, "$1"), // Regex capture group
            (3, 8, "$_"), // Last input line
            (4, 8, "$!"), // Last exception
            (5, 8, "$$"), // Process ID
        ];

        for (line, col, expected_name) in test_cases {
            let position = Position::new(line, col);
            let (identifier_opt, namespace, _) = analyzer.get_identifier(position);

            let identifier = identifier_opt.expect(&format!(
                "Expected to find variable identifier for {}",
                expected_name
            ));
            assert_variable_identifier(&identifier, expected_name);
            assert_namespace_context(&namespace, &["Object"]);

            // Verify it has proper global variable type
            match identifier {
                Identifier::RubyVariable { iden } => {
                    match iden.variable_type() {
                        crate::types::ruby_variable::RubyVariableType::Global => {
                            // Special global variables are still global
                        }
                        _ => panic!("Expected Global variable type for {}", expected_name),
                    }
                }
                _ => panic!("Expected RubyVariable identifier for {}", expected_name),
            }
        }
    }

    #[test]
    fn test_debug_special_global_parsing() {
        let content = r#"
def test_method
  puts $&   # Last match
  puts $~   # Last match info
  puts $*   # Command line arguments
  puts $0   # Program name
end
"#;
        let parse_result = ruby_prism::parse(content.as_bytes());
        println!("Parse result: {:#?}", parse_result);
        
        let analyzer = create_analyzer(content);
        
        let test_cases = vec![
            (2, 8, "$&"),  // Last match
            (3, 8, "$~"),  // Last match info
            (4, 8, "$*"), // Command line arguments
            (5, 8, "$0"), // Program name
        ];

        for (line, col, expected_name) in test_cases {
            let position = Position::new(line, col);
            let (identifier_opt, namespace, _) = analyzer.get_identifier(position);
            println!("Position ({}, {}): Expected {}, Found: {:?}", line, col, expected_name, identifier_opt);
        }
    }

    #[test]
    fn test_global_variable_comprehensive_special_variables() {
        let content = r#"
def test_method
  puts $1   # Numbered reference
  puts $2   # Numbered reference
  puts $_   # Last input line
  puts $!   # Last exception
  puts $$   # Process ID
  puts $?   # Exit status
  puts $&   # Last match
  puts $~   # Last match info
  puts $*   # Command line arguments
  puts $0   # Program name
end
"#;
        let analyzer = create_analyzer(content);

        // Test comprehensive set of special global variables
        let test_cases = vec![
            (2, 8, "$1"),  // Numbered reference
            (3, 8, "$2"),  // Numbered reference
            (4, 8, "$_"),  // Last input line
            (5, 8, "$!"),  // Last exception
            (6, 8, "$$"),  // Process ID
            (7, 8, "$?"),  // Exit status
            (8, 8, "$&"),  // Last match
            (9, 8, "$~"),  // Last match info
            (10, 8, "$*"), // Command line arguments
            (11, 8, "$0"), // Program name
        ];

        for (line, col, expected_name) in test_cases {
            let position = Position::new(line, col);
            let (identifier_opt, namespace, _) = analyzer.get_identifier(position);

            let identifier = identifier_opt.expect(&format!(
                "Expected to find variable identifier for {}",
                expected_name
            ));
            assert_variable_identifier(&identifier, expected_name);
            assert_namespace_context(&namespace, &["Object"]);

            // Verify it has proper global variable type
            match identifier {
                Identifier::RubyVariable { iden } => {
                    match iden.variable_type() {
                        crate::types::ruby_variable::RubyVariableType::Global => {
                            // All special global variables should be Global type
                        }
                        _ => panic!("Expected Global variable type for {}", expected_name),
                    }
                }
                _ => panic!("Expected RubyVariable identifier for {}", expected_name),
            }
        }
    }

    #[test]
    fn test_global_variable_regular_variables() {
        let content = r#"
$global_var = "global value"
$LOAD_PATH = []

def test_method
  puts $global_var
  puts $LOAD_PATH
end
"#;
        let analyzer = create_analyzer(content);

        // Test regular global variable
        let position = Position::new(5, 8); // Position at "$global_var"
        let (identifier_opt, namespace, _) = analyzer.get_identifier(position);

        if let Some(identifier) = identifier_opt {
            assert_variable_identifier(&identifier, "$global_var");
            assert_namespace_context(&namespace, &["Object"]);

            // Verify it has proper global variable type
            match identifier {
                Identifier::RubyVariable { iden } => {
                    match iden.variable_type() {
                        crate::types::ruby_variable::RubyVariableType::Global => {
                            // Global variables have no additional scope context
                        }
                        _ => panic!("Expected Global variable type for $global_var"),
                    }
                }
                _ => panic!("Expected RubyVariable identifier for $global_var"),
            }
        }

        // Test special global variable
        let position = Position::new(6, 8); // Position at "$LOAD_PATH"
        let (identifier_opt, namespace, _) = analyzer.get_identifier(position);

        if let Some(identifier) = identifier_opt {
            assert_variable_identifier(&identifier, "$LOAD_PATH");
            assert_namespace_context(&namespace, &["Object"]);

            // Verify it has proper global variable type
            match identifier {
                Identifier::RubyVariable { iden } => {
                    match iden.variable_type() {
                        crate::types::ruby_variable::RubyVariableType::Global => {
                            // Special global variables are still global
                        }
                        _ => panic!("Expected Global variable type for $LOAD_PATH"),
                    }
                }
                _ => panic!("Expected RubyVariable identifier for $LOAD_PATH"),
            }
        }
    }

    #[test]
    fn test_variable_resolution_complex_nested_scenarios() {
        let content = r#"
$global_counter = 0

class ComplexClass
  @@class_counter = 0

  def initialize(name)
    @name = name
    @instance_id = generate_id
    @@class_counter += 1
    $global_counter += 1
  end

  def process_items(items)
    result = []

    items.each_with_index do |item, index|
      local_result = process_single_item(item)

      if local_result.valid?
        result << {
          item: item,
          index: index,
          result: local_result,
          instance_name: @name,
          class_count: @@class_counter,
          global_count: $global_counter
        }
      end
    end

    result
  end

  private

  def generate_id
    Time.now.to_i
  end
end
"#;
        let analyzer = create_analyzer(content);

        // Test variables within class context
        let class_context_test_cases = vec![
            // Local variables
            (14, 5, "result", &["Object", "ComplexClass"]),
            (17, 7, "local_result", &["Object", "ComplexClass"]),
            (16, 30, "item", &["Object", "ComplexClass"]),
            (16, 36, "index", &["Object", "ComplexClass"]),
            // Instance variables
            (7, 4, "@name", &["Object", "ComplexClass"]),
            (24, 25, "@name", &["Object", "ComplexClass"]),
            (8, 4, "@instance_id", &["Object", "ComplexClass"]),
            // Class variables
            (4, 2, "@@class_counter", &["Object", "ComplexClass"]),
            (25, 23, "@@class_counter", &["Object", "ComplexClass"]),
            // Global variables within class
            (26, 24, "$global_counter", &["Object", "ComplexClass"]),
        ];

        for (line, col, expected_name, expected_namespace) in class_context_test_cases {
            let position = Position::new(line, col);
            let (identifier_opt, namespace, _) = analyzer.get_identifier(position);

            if let Some(identifier) = identifier_opt {
                assert_variable_identifier(&identifier, expected_name);
                assert_namespace_context(&namespace, expected_namespace);

                // Verify proper variable type based on name
                match identifier {
                    Identifier::RubyVariable { iden } => {
                        let expected_type = if expected_name.starts_with("@@") {
                            "Class"
                        } else if expected_name.starts_with("@") {
                            "Instance"
                        } else if expected_name.starts_with("$") {
                            "Global"
                        } else {
                            "Local"
                        };

                        match (iden.variable_type(), expected_type) {
                            (crate::types::ruby_variable::RubyVariableType::Local(_), "Local") => {}
                            (
                                crate::types::ruby_variable::RubyVariableType::Instance,
                                "Instance",
                            ) => {}
                            (crate::types::ruby_variable::RubyVariableType::Class, "Class") => {}
                            (crate::types::ruby_variable::RubyVariableType::Global, "Global") => {}
                            _ => panic!(
                                "Variable type mismatch for {}: expected {}, got {:?}",
                                expected_name,
                                expected_type,
                                iden.variable_type()
                            ),
                        }
                    }
                    _ => panic!("Expected RubyVariable identifier for {}", expected_name),
                }
            } else {
                // Some positions might not resolve to identifiers, which is okay
                println!(
                    "No identifier found at {}:{} for {}",
                    line, col, expected_name
                );
            }
        }

        // Test top-level global variable (Object namespace only)
        let top_level_test_cases = vec![
            (1, 0, "$global_counter", &["Object"]),
        ];

        for (line, col, expected_name, expected_namespace) in top_level_test_cases {
            let position = Position::new(line, col);
            let (identifier_opt, namespace, _) = analyzer.get_identifier(position);

            if let Some(identifier) = identifier_opt {
                assert_variable_identifier(&identifier, expected_name);
                assert_namespace_context(&namespace, expected_namespace);
            } else {
                println!(
                    "No identifier found at {}:{} for {}",
                    line, col, expected_name
                );
            }
        }
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

    // ===== Method Identifier Context and Receiver Kind Tests =====
    // These tests specifically address task 9 requirements

    #[test]
    fn test_method_calls_different_receivers_nested_contexts() {
        let content = r#"
module OuterModule
  class OuterClass
    def instance_method
      # No receiver - should capture namespace context
      helper_method

      # Self receiver - should capture namespace context
      self.instance_helper

      # Constant receiver - should capture namespace context
      OuterClass.class_method

      # Expression receiver - should capture namespace context
      obj.expression_method
    end

    def self.class_method
      puts "class method"
    end

    def instance_helper
      puts "instance helper"
    end

    module InnerModule
      def self.module_method
        # No receiver within nested module
        nested_helper

        # Self receiver within nested module
        self.module_helper

        # Constant receiver within nested module
        OuterModule::OuterClass.class_method

        # Expression receiver within nested module
        var.nested_expression_method
      end

      def self.nested_helper
        puts "nested helper"
      end

      def self.module_helper
        puts "module helper"
      end
    end
  end
end
"#;
        let analyzer = create_analyzer(content);

        // Test 1: No receiver method call within OuterClass
        let position = Position::new(5, 8); // Position at "helper_method"
        let (identifier_opt, ancestors, _) = analyzer.get_identifier(position);
        let identifier = identifier_opt.expect("Expected to find method identifier");

        assert_method_identifier(&identifier, "helper_method", ReceiverKind::None);
        assert_namespace_context(&ancestors, &["Object", "OuterModule", "OuterClass"]);

        // Test 2: Self receiver method call within OuterClass
        let position = Position::new(8, 17); // Position at "instance_helper"
        let (identifier_opt, ancestors, _) = analyzer.get_identifier(position);
        let identifier = identifier_opt.expect("Expected to find method identifier");

        assert_method_identifier(&identifier, "instance_helper", ReceiverKind::SelfReceiver);
        assert_namespace_context(&ancestors, &["Object", "OuterModule", "OuterClass"]);

        // Test 3: Constant receiver method call within OuterClass
        let position = Position::new(11, 22); // Position at "class_method"
        let (identifier_opt, ancestors, _) = analyzer.get_identifier(position);
        let identifier = identifier_opt.expect("Expected to find method identifier");

        assert_method_identifier(&identifier, "class_method", ReceiverKind::Constant);
        assert_namespace_context(&ancestors, &["Object", "OuterModule", "OuterClass"]);

        // Test 4: Expression receiver method call within OuterClass
        let position = Position::new(14, 12); // Position at "expression_method"
        let (identifier_opt, ancestors, _) = analyzer.get_identifier(position);
        let identifier = identifier_opt.expect("Expected to find method identifier");

        assert_method_identifier(&identifier, "expression_method", ReceiverKind::Expr);
        assert_namespace_context(&ancestors, &["Object", "OuterModule", "OuterClass"]);

        // Test 5: No receiver method call within InnerModule
        let position = Position::new(28, 10); // Position at "nested_helper"
        let (identifier_opt, ancestors, _) = analyzer.get_identifier(position);
        let identifier = identifier_opt.expect("Expected to find method identifier");

        assert_method_identifier(&identifier, "nested_helper", ReceiverKind::None);
        assert_namespace_context(
            &ancestors,
            &["Object", "OuterModule", "OuterClass", "InnerModule"],
        );

        // Test 6: Self receiver method call within InnerModule
        let position = Position::new(31, 17); // Position at "module_helper"
        let (identifier_opt, ancestors, _) = analyzer.get_identifier(position);
        let identifier = identifier_opt.expect("Expected to find method identifier");

        assert_method_identifier(&identifier, "module_helper", ReceiverKind::SelfReceiver);
        assert_namespace_context(
            &ancestors,
            &["Object", "OuterModule", "OuterClass", "InnerModule"],
        );

        // Test 7: Complex constant receiver method call within InnerModule
        let position = Position::new(34, 42); // Position at "class_method"
        let (identifier_opt, ancestors, _) = analyzer.get_identifier(position);
        let identifier = identifier_opt.expect("Expected to find method identifier");

        assert_method_identifier(&identifier, "class_method", ReceiverKind::Constant);
        assert_namespace_context(
            &ancestors,
            &["Object", "OuterModule", "OuterClass", "InnerModule"],
        );

        // Test 8: Expression receiver method call within InnerModule
        let position = Position::new(37, 12); // Position at "nested_expression_method"
        let (identifier_opt, ancestors, _) = analyzer.get_identifier(position);
        let identifier = identifier_opt.expect("Expected to find method identifier");

        assert_method_identifier(&identifier, "nested_expression_method", ReceiverKind::Expr);
        assert_namespace_context(
            &ancestors,
            &["Object", "OuterModule", "OuterClass", "InnerModule"],
        );
    }

    #[test]
    fn test_method_resolution_nested_classes_modules() {
        let content = r#"
module Level1
  class Level1Class
    def self.level1_class_method
      puts "level1 class method"
    end

    def level1_instance_method
      puts "level1 instance method"
    end

    module Level2
      class Level2Class
        def self.level2_class_method
          puts "level2 class method"
        end

        def level2_instance_method
          # Method calls at different nesting levels
          level1_instance_method
          self.level2_instance_method
          Level1Class.level1_class_method
          Level2Class.level2_class_method
          Level1::Level1Class.level1_class_method
        end

        module Level3
          def self.level3_method
            # Deep nesting method calls
            nested_call
            self.level3_helper
            Level1::Level1Class.level1_class_method
            Level2Class.level2_class_method
          end

          def self.nested_call
            puts "nested call"
          end

          def self.level3_helper
            puts "level3 helper"
          end
        end
      end
    end
  end
end
"#;
        let analyzer = create_analyzer(content);

        // Test 1: Method call within Level2Class instance method
        let position = Position::new(19, 12); // Position at "level1_instance_method"
        let (identifier_opt, ancestors, _) = analyzer.get_identifier(position);
        let identifier = identifier_opt.expect("Expected to find method identifier");

        assert_method_identifier(&identifier, "level1_instance_method", ReceiverKind::None);
        assert_namespace_context(
            &ancestors,
            &["Object", "Level1", "Level1Class", "Level2", "Level2Class"],
        );

        // Test 2: Self method call within Level2Class
        let position = Position::new(20, 22); // Position at "level2_instance_method"
        let (identifier_opt, ancestors, _) = analyzer.get_identifier(position);
        let identifier = identifier_opt.expect("Expected to find method identifier");

        assert_method_identifier(
            &identifier,
            "level2_instance_method",
            ReceiverKind::SelfReceiver,
        );
        assert_namespace_context(
            &ancestors,
            &["Object", "Level1", "Level1Class", "Level2", "Level2Class"],
        );

        // Test 3: Constant receiver method call to parent class
        let position = Position::new(21, 30); // Position at "level1_class_method"
        let (identifier_opt, ancestors, _) = analyzer.get_identifier(position);
        let identifier = identifier_opt.expect("Expected to find method identifier");

        assert_method_identifier(&identifier, "level1_class_method", ReceiverKind::Constant);
        assert_namespace_context(
            &ancestors,
            &["Object", "Level1", "Level1Class", "Level2", "Level2Class"],
        );

        // Test 4: Constant receiver method call to same level class
        let position = Position::new(22, 30); // Position at "level2_class_method"
        let (identifier_opt, ancestors, _) = analyzer.get_identifier(position);
        let identifier = identifier_opt.expect("Expected to find method identifier");

        assert_method_identifier(&identifier, "level2_class_method", ReceiverKind::Constant);
        assert_namespace_context(
            &ancestors,
            &["Object", "Level1", "Level1Class", "Level2", "Level2Class"],
        );

        // Test 5: Fully qualified constant receiver method call
        let position = Position::new(23, 40); // Position at "level1_class_method"
        let (identifier_opt, ancestors, _) = analyzer.get_identifier(position);
        let identifier = identifier_opt.expect("Expected to find method identifier");

        assert_method_identifier(&identifier, "level1_class_method", ReceiverKind::Constant);
        assert_namespace_context(
            &ancestors,
            &["Object", "Level1", "Level1Class", "Level2", "Level2Class"],
        );

        // Test 6: Method call within Level3 module (deeply nested)
        let position = Position::new(29, 14); // Position at "nested_call"
        let (identifier_opt, ancestors, _) = analyzer.get_identifier(position);
        let identifier = identifier_opt.expect("Expected to find method identifier");

        assert_method_identifier(&identifier, "nested_call", ReceiverKind::None);
        assert_namespace_context(
            &ancestors,
            &[
                "Object",
                "Level1",
                "Level1Class",
                "Level2",
                "Level2Class",
                "Level3",
            ],
        );

        // Test 7: Self method call within Level3 module
        let position = Position::new(30, 19); // Position at "level3_helper"
        let (identifier_opt, ancestors, _) = analyzer.get_identifier(position);
        let identifier = identifier_opt.expect("Expected to find method identifier");

        assert_method_identifier(&identifier, "level3_helper", ReceiverKind::SelfReceiver);
        assert_namespace_context(
            &ancestors,
            &[
                "Object",
                "Level1",
                "Level1Class",
                "Level2",
                "Level2Class",
                "Level3",
            ],
        );

        // Test 8: Fully qualified method call from deeply nested context
        let position = Position::new(31, 40); // Position at "level1_class_method"
        let (identifier_opt, ancestors, _) = analyzer.get_identifier(position);
        let identifier = identifier_opt.expect("Expected to find method identifier");

        assert_method_identifier(&identifier, "level1_class_method", ReceiverKind::Constant);
        assert_namespace_context(
            &ancestors,
            &[
                "Object",
                "Level1",
                "Level1Class",
                "Level2",
                "Level2Class",
                "Level3",
            ],
        );

        // Test 9: Relative constant receiver from deeply nested context
        let position = Position::new(32, 30); // Position at "level2_class_method"
        let (identifier_opt, ancestors, _) = analyzer.get_identifier(position);
        let identifier = identifier_opt.expect("Expected to find method identifier");

        assert_method_identifier(&identifier, "level2_class_method", ReceiverKind::Constant);
        assert_namespace_context(
            &ancestors,
            &[
                "Object",
                "Level1",
                "Level1Class",
                "Level2",
                "Level2Class",
                "Level3",
            ],
        );
    }

    #[test]
    fn test_method_namespace_context_simple() {
        // Simple test to verify namespace context is captured correctly
        let content = r#"
module TestModule
  class TestClass
    def instance_method
      helper_method
      self.other_method
    end

    def helper_method
      puts "helper"
    end

    def other_method
      puts "other"
    end
  end
end
"#;
        let analyzer = create_analyzer(content);

        // Test 1: No receiver method call
        let position = Position::new(4, 8); // Position at "helper_method"
        let (identifier_opt, ancestors, _) = analyzer.get_identifier(position);
        let identifier = identifier_opt.expect("Expected to find method identifier");

        assert_method_identifier(&identifier, "helper_method", ReceiverKind::None);
        assert_namespace_context(&ancestors, &["Object", "TestModule", "TestClass"]);

        // Test 2: Self receiver method call
        let position = Position::new(5, 17); // Position at "other_method"
        let (identifier_opt, ancestors, _) = analyzer.get_identifier(position);
        let identifier = identifier_opt.expect("Expected to find method identifier");

        assert_method_identifier(&identifier, "other_method", ReceiverKind::SelfReceiver);
        assert_namespace_context(&ancestors, &["Object", "TestModule", "TestClass"]);
    }
}
