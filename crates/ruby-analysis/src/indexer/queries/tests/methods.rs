use super::*;

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
    let (identifier_opt, ancestors, _) = get_identifier_for_test(&analyzer, position);
    let identifier = identifier_opt.expect("Expected to find method identifier");

    assert_method_identifier(&identifier, "helper_method", MethodReceiver::None);
    assert_namespace_context(&ancestors, &["OuterModule", "OuterClass"]);

    // Test 2: Self receiver method call within OuterClass
    let position = Position::new(8, 17); // Position at "instance_helper"
    let (identifier_opt, ancestors, _) = get_identifier_for_test(&analyzer, position);
    let identifier = identifier_opt.expect("Expected to find method identifier");

    assert_method_identifier(&identifier, "instance_helper", MethodReceiver::SelfReceiver);
    assert_namespace_context(&ancestors, &["OuterModule", "OuterClass"]);

    // Test 3: Constant receiver method call within OuterClass
    let position = Position::new(11, 22); // Position at "class_method"
    let (identifier_opt, ancestors, _) = get_identifier_for_test(&analyzer, position);
    let identifier = identifier_opt.expect("Expected to find method identifier");

    assert_method_identifier(
        &identifier,
        "class_method",
        MethodReceiver::Constant(vec![]),
    );
    assert_namespace_context(&ancestors, &["OuterModule", "OuterClass"]);

    // Test 4: Method call receiver (obj is parsed as method call since it's not defined)
    let position = Position::new(14, 12); // Position at "expression_method"
    let (identifier_opt, ancestors, _) = get_identifier_for_test(&analyzer, position);
    let identifier = identifier_opt.expect("Expected to find method identifier");

    assert_method_identifier(
        &identifier,
        "expression_method",
        MethodReceiver::MethodCall {
            inner_receiver: Box::new(MethodReceiver::None),
            method_name: "obj".to_string(),
        },
    );
    assert_namespace_context(&ancestors, &["OuterModule", "OuterClass"]);

    // Test 5: No receiver method call within InnerModule
    let position = Position::new(28, 10); // Position at "nested_helper"
    let (identifier_opt, ancestors, _) = get_identifier_for_test(&analyzer, position);
    let identifier = identifier_opt.expect("Expected to find method identifier");

    assert_method_identifier(&identifier, "nested_helper", MethodReceiver::None);
    assert_namespace_context(&ancestors, &["OuterModule", "OuterClass", "InnerModule"]);

    // Test 6: Self receiver method call within InnerModule
    let position = Position::new(31, 17); // Position at "module_helper"
    let (identifier_opt, ancestors, _) = get_identifier_for_test(&analyzer, position);
    let identifier = identifier_opt.expect("Expected to find method identifier");

    assert_method_identifier(&identifier, "module_helper", MethodReceiver::SelfReceiver);
    assert_namespace_context(&ancestors, &["OuterModule", "OuterClass", "InnerModule"]);

    // Test 7: Complex constant receiver method call within InnerModule
    let position = Position::new(34, 42); // Position at "class_method"
    let (identifier_opt, ancestors, _) = get_identifier_for_test(&analyzer, position);
    let identifier = identifier_opt.expect("Expected to find method identifier");

    assert_method_identifier(
        &identifier,
        "class_method",
        MethodReceiver::Constant(vec![]),
    );
    assert_namespace_context(&ancestors, &["OuterModule", "OuterClass", "InnerModule"]);

    // Test 8: Method call receiver (var is parsed as method call since it's not defined)
    let position = Position::new(37, 12); // Position at "nested_expression_method"
    let (identifier_opt, ancestors, _) = get_identifier_for_test(&analyzer, position);
    let identifier = identifier_opt.expect("Expected to find method identifier");

    assert_method_identifier(
        &identifier,
        "nested_expression_method",
        MethodReceiver::MethodCall {
            inner_receiver: Box::new(MethodReceiver::None),
            method_name: "var".to_string(),
        },
    );
    assert_namespace_context(&ancestors, &["OuterModule", "OuterClass", "InnerModule"]);
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
    let (identifier_opt, ancestors, _) = get_identifier_for_test(&analyzer, position);
    let identifier = identifier_opt.expect("Expected to find method identifier");

    assert_method_identifier(&identifier, "level1_instance_method", MethodReceiver::None);
    assert_namespace_context(
        &ancestors,
        &["Level1", "Level1Class", "Level2", "Level2Class"],
    );

    // Test 2: Self method call within Level2Class
    let position = Position::new(20, 22); // Position at "level2_instance_method"
    let (identifier_opt, ancestors, _) = get_identifier_for_test(&analyzer, position);
    let identifier = identifier_opt.expect("Expected to find method identifier");

    assert_method_identifier(
        &identifier,
        "level2_instance_method",
        MethodReceiver::SelfReceiver,
    );
    assert_namespace_context(
        &ancestors,
        &["Level1", "Level1Class", "Level2", "Level2Class"],
    );

    // Test 3: Constant receiver method call to parent class
    let position = Position::new(21, 30); // Position at "level1_class_method"
    let (identifier_opt, ancestors, _) = get_identifier_for_test(&analyzer, position);
    let identifier = identifier_opt.expect("Expected to find method identifier");

    assert_method_identifier(
        &identifier,
        "level1_class_method",
        MethodReceiver::Constant(vec![]),
    );
    assert_namespace_context(
        &ancestors,
        &["Level1", "Level1Class", "Level2", "Level2Class"],
    );

    // Test 4: Constant receiver method call to same level class
    let position = Position::new(22, 30); // Position at "level2_class_method"
    let (identifier_opt, ancestors, _) = get_identifier_for_test(&analyzer, position);
    let identifier = identifier_opt.expect("Expected to find method identifier");

    assert_method_identifier(
        &identifier,
        "level2_class_method",
        MethodReceiver::Constant(vec![]),
    );
    assert_namespace_context(
        &ancestors,
        &["Level1", "Level1Class", "Level2", "Level2Class"],
    );

    // Test 5: Fully qualified constant receiver method call
    let position = Position::new(23, 40); // Position at "level1_class_method"
    let (identifier_opt, ancestors, _) = get_identifier_for_test(&analyzer, position);
    let identifier = identifier_opt.expect("Expected to find method identifier");

    assert_method_identifier(
        &identifier,
        "level1_class_method",
        MethodReceiver::Constant(vec![]),
    );
    assert_namespace_context(
        &ancestors,
        &["Level1", "Level1Class", "Level2", "Level2Class"],
    );

    // Test 6: Method call within Level3 module (deeply nested)
    let position = Position::new(29, 14); // Position at "nested_call"
    let (identifier_opt, ancestors, _) = get_identifier_for_test(&analyzer, position);
    let identifier = identifier_opt.expect("Expected to find method identifier");

    assert_method_identifier(&identifier, "nested_call", MethodReceiver::None);
    assert_namespace_context(
        &ancestors,
        &["Level1", "Level1Class", "Level2", "Level2Class", "Level3"],
    );

    // Test 7: Self method call within Level3 module
    let position = Position::new(30, 19); // Position at "level3_helper"
    let (identifier_opt, ancestors, _) = get_identifier_for_test(&analyzer, position);
    let identifier = identifier_opt.expect("Expected to find method identifier");

    assert_method_identifier(&identifier, "level3_helper", MethodReceiver::SelfReceiver);
    assert_namespace_context(
        &ancestors,
        &["Level1", "Level1Class", "Level2", "Level2Class", "Level3"],
    );

    // Test 8: Fully qualified method call from deeply nested context
    let position = Position::new(31, 40); // Position at "level1_class_method"
    let (identifier_opt, ancestors, _) = get_identifier_for_test(&analyzer, position);
    let identifier = identifier_opt.expect("Expected to find method identifier");

    assert_method_identifier(
        &identifier,
        "level1_class_method",
        MethodReceiver::Constant(vec![]),
    );
    assert_namespace_context(
        &ancestors,
        &["Level1", "Level1Class", "Level2", "Level2Class", "Level3"],
    );

    // Test 9: Relative constant receiver from deeply nested context
    let position = Position::new(32, 30); // Position at "level2_class_method"
    let (identifier_opt, ancestors, _) = get_identifier_for_test(&analyzer, position);
    let identifier = identifier_opt.expect("Expected to find method identifier");

    assert_method_identifier(
        &identifier,
        "level2_class_method",
        MethodReceiver::Constant(vec![]),
    );
    assert_namespace_context(
        &ancestors,
        &["Level1", "Level1Class", "Level2", "Level2Class", "Level3"],
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
    let (identifier_opt, ancestors, _) = get_identifier_for_test(&analyzer, position);
    let identifier = identifier_opt.expect("Expected to find method identifier");

    assert_method_identifier(&identifier, "helper_method", MethodReceiver::None);
    assert_namespace_context(&ancestors, &["TestModule", "TestClass"]);

    // Test 2: Self receiver method call
    let position = Position::new(5, 17); // Position at "other_method"
    let (identifier_opt, ancestors, _) = get_identifier_for_test(&analyzer, position);
    let identifier = identifier_opt.expect("Expected to find method identifier");

    assert_method_identifier(&identifier, "other_method", MethodReceiver::SelfReceiver);
    assert_namespace_context(&ancestors, &["TestModule", "TestClass"]);
}
