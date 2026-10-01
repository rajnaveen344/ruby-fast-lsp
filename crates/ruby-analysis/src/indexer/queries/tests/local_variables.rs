use super::*;

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
    let (identifier_opt, namespace, _lv_scope_stack) = get_identifier_for_test(&analyzer, position);

    let identifier = identifier_opt.expect("Expected to find variable identifier");
    assert_variable_identifier(&identifier, "local_var");
    assert_namespace_context(&namespace, &["TestClass"]);

    // Verify the variable is a local variable identifier
    assert!(
        matches!(identifier, Identifier::RubyLocalVariable { .. }),
        "Expected RubyLocalVariable identifier"
    );
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
    let (identifier_opt, namespace, _) = get_identifier_for_test(&analyzer, position);

    let identifier = identifier_opt.expect("Expected to find variable identifier");
    assert_variable_identifier(&identifier, "outer_var");
    assert_namespace_context(&namespace, &["TestClass"]);

    // Test access to inner_var within block
    let position = Position::new(8, 12); // Position at "inner_var" in block
    let (identifier_opt, namespace, _) = get_identifier_for_test(&analyzer, position);

    let identifier = identifier_opt.expect("Expected to find variable identifier");
    assert_variable_identifier(&identifier, "inner_var");
    assert_namespace_context(&namespace, &["TestClass"]);

    // Verify it is a local variable identifier
    assert!(
        matches!(identifier, Identifier::RubyLocalVariable { .. }),
        "Expected RubyLocalVariable identifier"
    );
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
    let (identifier_opt, namespace, _) = get_identifier_for_test(&analyzer, position);

    let identifier = identifier_opt.expect("Expected to find variable identifier");
    assert_variable_identifier(&identifier, "block_local");
    assert_namespace_context(&namespace, &["TestClass"]);

    // Verify it is a local variable identifier
    assert!(
        matches!(identifier, Identifier::RubyLocalVariable { .. }),
        "Expected RubyLocalVariable identifier"
    );
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
    let (identifier_opt, namespace, _) = get_identifier_for_test(&analyzer, position);

    let identifier = identifier_opt.expect("Expected to find variable identifier");
    assert_variable_identifier(&identifier, "error_var");
    assert_namespace_context(&namespace, &["TestClass"]);

    // Verify it is a local variable identifier
    assert!(
        matches!(identifier, Identifier::RubyLocalVariable { .. }),
        "Expected RubyLocalVariable identifier"
    );
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
    let (identifier_opt, namespace, _) = get_identifier_for_test(&analyzer, position);

    let identifier = identifier_opt.expect("Expected to find variable identifier");
    assert_variable_identifier(&identifier, "class_local");
    assert_namespace_context(&namespace, &["TestClass"]);

    // Verify it is a local variable identifier
    assert!(
        matches!(identifier, Identifier::RubyLocalVariable { .. }),
        "Expected RubyLocalVariable identifier"
    );
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
    let (identifier_opt, namespace, _) = get_identifier_for_test(&analyzer, position);

    let identifier = identifier_opt.expect("Expected to find variable identifier");
    assert_variable_identifier(&identifier, "module_local");
    assert_namespace_context(&namespace, &["TestModule"]);

    // Verify it is a local variable identifier
    assert!(
        matches!(identifier, Identifier::RubyLocalVariable { .. }),
        "Expected RubyLocalVariable identifier"
    );
}
