use super::*;

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
    let (identifier_opt, ancestors, _) = get_identifier_for_test(&analyzer, position);

    let identifier = identifier_opt.expect("Expected to find method identifier");
    assert_method_identifier(&identifier, "simple_method", MethodReceiver::None);
    assert_namespace_context(&ancestors, &["TestClass"]);
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
    let (identifier_opt, ancestors, _) = get_identifier_for_test(&analyzer, position);

    let identifier = identifier_opt.expect("Expected to find method identifier");
    assert_method_identifier(&identifier, "method_with_args", MethodReceiver::None);
    assert_namespace_context(&ancestors, &["TestClass"]);
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
    let (identifier_opt, ancestors, _) = get_identifier_for_test(&analyzer, position);

    let identifier = identifier_opt.expect("Expected to find method identifier");
    assert_method_identifier(&identifier, "method_with_block", MethodReceiver::None);
    assert_namespace_context(&ancestors, &["TestClass"]);
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
    let (identifier_opt, ancestors, _) = get_identifier_for_test(&analyzer, position);

    let identifier = identifier_opt.expect("Expected to find method identifier");
    assert_method_identifier(&identifier, "global_method", MethodReceiver::None);
    assert_namespace_context(&ancestors, &[]);
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
    let (identifier_opt, ancestors, _) = get_identifier_for_test(&analyzer, position);

    let identifier = identifier_opt.expect("Expected to find method identifier");
    assert_method_identifier(&identifier, "helper_method", MethodReceiver::SelfReceiver);
    assert_namespace_context(&ancestors, &["TestClass"]);
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
    let (identifier_opt, ancestors, _) = get_identifier_for_test(&analyzer, position);

    let identifier = identifier_opt.expect("Expected to find method identifier");
    assert_method_identifier(&identifier, "helper_method", MethodReceiver::SelfReceiver);
    assert_namespace_context(&ancestors, &["TestClass"]);
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
    let (identifier_opt, ancestors, _) = get_identifier_for_test(&analyzer, position);

    let identifier = identifier_opt.expect("Expected to find method identifier");
    // Now correctly identifies as MethodCall with inner receiver being self.first_method
    assert_method_identifier(
        &identifier,
        "second_method",
        MethodReceiver::MethodCall {
            inner_receiver: Box::new(MethodReceiver::SelfReceiver),
            method_name: "first_method".to_string(),
        },
    );
    assert_namespace_context(&ancestors, &["TestClass"]);
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
    let (identifier_opt, ancestors, _) = get_identifier_for_test(&analyzer, position);

    let identifier = identifier_opt.expect("Expected to find method identifier");
    assert_method_identifier(
        &identifier,
        "class_method",
        MethodReceiver::Constant(vec![]),
    );
    assert_namespace_context(&ancestors, &[]);
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
    let (identifier_opt, ancestors, _) = get_identifier_for_test(&analyzer, position);

    let identifier = identifier_opt.expect("Expected to find method identifier");
    assert_method_identifier(
        &identifier,
        "nested_method",
        MethodReceiver::Constant(vec![]),
    );
    assert_namespace_context(&ancestors, &[]);
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
    let (identifier_opt, ancestors, _) = get_identifier_for_test(&analyzer, position);

    let identifier = identifier_opt.expect("Expected to find method identifier");
    assert_method_identifier(
        &identifier,
        "module_method",
        MethodReceiver::Constant(vec![]),
    );
    assert_namespace_context(&ancestors, &[]);
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
    let (identifier_opt, ancestors, _) = get_identifier_for_test(&analyzer, position);

    let identifier = identifier_opt.expect("Expected to find method identifier");
    assert_method_identifier(&identifier, "deep_method", MethodReceiver::Constant(vec![]));
    assert_namespace_context(&ancestors, &[]);
}

#[test]
fn test_receiver_kind_local_variable_receiver() {
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
    let (identifier_opt, ancestors, _) = get_identifier_for_test(&analyzer, position);

    let identifier = identifier_opt.expect("Expected to find method identifier");
    assert_method_identifier(
        &identifier,
        "instance_method",
        MethodReceiver::LocalVariable("obj".to_string()),
    );
    assert_namespace_context(&ancestors, &["TestClass"]);
}

#[test]
fn test_receiver_kind_method_chain() {
    let content = r#"
class TestClass
  def test_method
    obj.first_method.second_method
  end
end
"#;
    let analyzer = create_analyzer(content);
    let position = Position::new(3, 25); // Position at "second_method"
    let (identifier_opt, ancestors, _) = get_identifier_for_test(&analyzer, position);

    let identifier = identifier_opt.expect("Expected to find method identifier");
    // Now correctly identifies as MethodCall with nested structure
    assert_method_identifier(
        &identifier,
        "second_method",
        MethodReceiver::MethodCall {
            inner_receiver: Box::new(MethodReceiver::LocalVariable("obj".to_string())),
            method_name: "first_method".to_string(),
        },
    );
    assert_namespace_context(&ancestors, &["TestClass"]);
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
    let (identifier_opt, ancestors, _) = get_identifier_for_test(&analyzer, position);

    let identifier = identifier_opt.expect("Expected to find method identifier");
    assert_method_identifier(&identifier, "result_method", MethodReceiver::Expression);
    assert_namespace_context(&ancestors, &["TestClass"]);
}

#[test]
fn test_receiver_kind_array_access() {
    let content = r#"
class TestClass
  def test_method
    arr[0].array_element_method
  end
end
"#;
    let analyzer = create_analyzer(content);
    let position = Position::new(3, 19); // Position at "array_element_method"
    let (identifier_opt, ancestors, _) = get_identifier_for_test(&analyzer, position);

    let identifier = identifier_opt.expect("Expected to find method identifier");
    // arr[0] is a method call ([] method) on arr
    assert_method_identifier(
        &identifier,
        "array_element_method",
        MethodReceiver::MethodCall {
            inner_receiver: Box::new(MethodReceiver::LocalVariable("arr".to_string())),
            method_name: "[]".to_string(),
        },
    );
    assert_namespace_context(&ancestors, &["TestClass"]);
}

#[test]
fn test_receiver_kind_hash_access() {
    let content = r#"
class TestClass
  def test_method
    hash[:key].hash_value_method
  end
end
"#;
    let analyzer = create_analyzer(content);
    let position = Position::new(3, 20); // Position at "hash_value_method"
    let (identifier_opt, ancestors, _) = get_identifier_for_test(&analyzer, position);

    let identifier = identifier_opt.expect("Expected to find method identifier");
    // hash[:key] is a method call ([] method) on hash
    assert_method_identifier(
        &identifier,
        "hash_value_method",
        MethodReceiver::MethodCall {
            inner_receiver: Box::new(MethodReceiver::LocalVariable("hash".to_string())),
            method_name: "[]".to_string(),
        },
    );
    assert_namespace_context(&ancestors, &["TestClass"]);
}

#[test]
fn test_receiver_kind_instance_variable() {
    let content = r#"
class TestClass
  def test_method
    @instance_var.instance_var_method
  end
end
"#;
    let analyzer = create_analyzer(content);
    let position = Position::new(3, 25); // Position at "instance_var_method"
    let (identifier_opt, ancestors, _) = get_identifier_for_test(&analyzer, position);

    let identifier = identifier_opt.expect("Expected to find method identifier");
    assert_method_identifier(
        &identifier,
        "instance_var_method",
        MethodReceiver::InstanceVariable("@instance_var".to_string()),
    );
    assert_namespace_context(&ancestors, &["TestClass"]);
}

#[test]
fn test_receiver_kind_class_variable() {
    let content = r#"
class TestClass
  def test_method
    @@class_var.class_var_method
  end
end
"#;
    let analyzer = create_analyzer(content);
    let position = Position::new(3, 22); // Position at "class_var_method"
    let (identifier_opt, ancestors, _) = get_identifier_for_test(&analyzer, position);

    let identifier = identifier_opt.expect("Expected to find method identifier");
    assert_method_identifier(
        &identifier,
        "class_var_method",
        MethodReceiver::ClassVariable("@@class_var".to_string()),
    );
    assert_namespace_context(&ancestors, &["TestClass"]);
}

#[test]
fn test_receiver_kind_global_variable() {
    let content = r#"
class TestClass
  def test_method
    $global_var.global_var_method
  end
end
"#;
    let analyzer = create_analyzer(content);
    let position = Position::new(3, 22); // Position at "global_var_method"
    let (identifier_opt, ancestors, _) = get_identifier_for_test(&analyzer, position);

    let identifier = identifier_opt.expect("Expected to find method identifier");
    assert_method_identifier(
        &identifier,
        "global_var_method",
        MethodReceiver::GlobalVariable("$global_var".to_string()),
    );
    assert_namespace_context(&ancestors, &["TestClass"]);
}
