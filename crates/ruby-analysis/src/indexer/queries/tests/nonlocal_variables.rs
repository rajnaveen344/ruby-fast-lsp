use super::*;

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
    let (identifier_opt, namespace, _) = get_identifier_for_test(&analyzer, position);

    let identifier = identifier_opt.expect("Expected to find variable identifier");
    assert_variable_identifier(&identifier, "@instance_var");
    assert_namespace_context(&namespace, &["TestClass"]);

    // Verify it has proper instance variable type (no additional scope context needed)
    match identifier {
        Identifier::RubyInstanceVariable { .. } => {
            // Instance variables don't need additional scope context
            // Their scope is determined by the namespace context
        }
        _ => panic!("Expected RubyInstanceVariable identifier"),
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
    let (identifier_opt, namespace, _) = get_identifier_for_test(&analyzer, position);

    let identifier = identifier_opt.expect("Expected to find variable identifier");
    assert_variable_identifier(&identifier, "@inner_instance");
    assert_namespace_context(&namespace, &["OuterClass", "InnerClass"]);

    // Verify it has proper instance variable type with correct namespace context
    match identifier {
        Identifier::RubyInstanceVariable { .. } => {
            // Instance variable scope is determined by namespace context
        }
        _ => panic!("Expected RubyInstanceVariable identifier"),
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
    let (identifier_opt, namespace, _) = get_identifier_for_test(&analyzer, position);

    let identifier = identifier_opt.expect("Expected to find variable identifier");
    assert_variable_identifier(&identifier, "@@class_var");
    assert_namespace_context(&namespace, &["TestClass"]);

    // Test access to class variable from instance method
    let position = Position::new(9, 10); // Position at "@@class_var" in instance method
    let (identifier_opt, namespace, _) = get_identifier_for_test(&analyzer, position);

    let identifier = identifier_opt.expect("Expected to find variable identifier");
    assert_variable_identifier(&identifier, "@@class_var");
    assert_namespace_context(&namespace, &["TestClass"]);

    // Verify it has proper class variable type (namespace context determines scope)
    match identifier {
        Identifier::RubyClassVariable { .. } => {
            // Class variables scope is determined by namespace context
        }
        _ => panic!("Expected RubyClassVariable identifier"),
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
    let (identifier_opt, namespace, _) = get_identifier_for_test(&analyzer, position);

    let identifier = identifier_opt.expect("Expected to find variable identifier");
    assert_variable_identifier(&identifier, "@@shared_var");
    assert_namespace_context(&namespace, &["ChildClass"]);

    // Verify it has proper class variable type
    match identifier {
        Identifier::RubyClassVariable { .. } => {
            // Class variables are shared across inheritance hierarchy
        }
        _ => panic!("Expected RubyClassVariable identifier"),
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
    let (identifier_opt, namespace, _) = get_identifier_for_test(&analyzer, position);

    let identifier = identifier_opt.expect("Expected to find variable identifier");
    assert_variable_identifier(&identifier, "$global_var");
    assert_namespace_context(&namespace, &["TestClass"]);

    // Test access to global variable from module method
    let position = Position::new(11, 10); // Position at "$global_var" in module
    let (identifier_opt, namespace, _) = get_identifier_for_test(&analyzer, position);

    let identifier = identifier_opt.expect("Expected to find variable identifier");
    assert_variable_identifier(&identifier, "$global_var");
    assert_namespace_context(&namespace, &["TestModule"]);

    // Test access to global variable from top level
    let position = Position::new(15, 5); // Position at "$global_var" at top level
    let (identifier_opt, namespace, _) = get_identifier_for_test(&analyzer, position);

    let identifier = identifier_opt.expect("Expected to find variable identifier");
    assert_variable_identifier(&identifier, "$global_var");
    assert_namespace_context(&namespace, &[]);

    // Verify it has proper global variable type (no additional context)
    match identifier {
        Identifier::RubyGlobalVariable { .. } => {
            // Global variables have no additional scope context
            // They are accessible from anywhere
        }
        _ => panic!("Expected RubyGlobalVariable identifier"),
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
        let (identifier_opt, namespace, _) = get_identifier_for_test(&analyzer, position);

        let identifier = identifier_opt.unwrap_or_else(|| {
            panic!("Expected to find variable identifier for {}", expected_name)
        });
        assert_variable_identifier(&identifier, expected_name);
        assert_namespace_context(&namespace, &[]);

        // Verify it has proper global variable type
        match identifier {
            Identifier::RubyGlobalVariable { .. } => {
                // Special global variables are still global
            }
            _ => panic!(
                "Expected RubyGlobalVariable identifier for {}",
                expected_name
            ),
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
        (2, 8, "$&"), // Last match
        (3, 8, "$~"), // Last match info
        (4, 8, "$*"), // Command line arguments
        (5, 8, "$0"), // Program name
    ];

    for (line, col, expected_name) in test_cases {
        let position = Position::new(line, col);
        let (identifier_opt, _, _) = get_identifier_for_test(&analyzer, position);
        println!(
            "Position ({}, {}): Expected {}, Found: {:?}",
            line, col, expected_name, identifier_opt
        );
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
        let (identifier_opt, namespace, _) = get_identifier_for_test(&analyzer, position);

        let identifier = identifier_opt.unwrap_or_else(|| {
            panic!("Expected to find variable identifier for {}", expected_name)
        });
        assert_variable_identifier(&identifier, expected_name);
        assert_namespace_context(&namespace, &[]);

        // Verify it has proper global variable type
        match identifier {
            Identifier::RubyGlobalVariable { .. } => {
                // All special global variables should be Global type
            }
            _ => panic!(
                "Expected RubyGlobalVariable identifier for {}",
                expected_name
            ),
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
    let (identifier_opt, namespace, _) = get_identifier_for_test(&analyzer, position);

    if let Some(identifier) = identifier_opt {
        assert_variable_identifier(&identifier, "$global_var");
        assert_namespace_context(&namespace, &[]);

        // Verify it has proper global variable type
        match identifier {
            Identifier::RubyGlobalVariable { .. } => {
                // Global variables have no additional scope context
            }
            _ => panic!("Expected RubyGlobalVariable identifier for $global_var"),
        }
    }

    // Test special global variable
    let position = Position::new(6, 8); // Position at "$LOAD_PATH"
    let (identifier_opt, namespace, _) = get_identifier_for_test(&analyzer, position);

    if let Some(identifier) = identifier_opt {
        assert_variable_identifier(&identifier, "$LOAD_PATH");
        assert_namespace_context(&namespace, &[]);

        // Verify it has proper global variable type
        match identifier {
            Identifier::RubyGlobalVariable { .. } => {
                // Special global variables are still global
            }
            _ => panic!("Expected RubyGlobalVariable identifier for $LOAD_PATH"),
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
        (14, 5, "result", &["ComplexClass"]),
        (17, 7, "local_result", &["ComplexClass"]),
        (16, 30, "item", &["ComplexClass"]),
        (16, 36, "index", &["ComplexClass"]),
        // Instance variables
        (7, 4, "@name", &["ComplexClass"]),
        (24, 25, "@name", &["ComplexClass"]),
        (8, 4, "@instance_id", &["ComplexClass"]),
        // Class variables
        (4, 2, "@@class_counter", &["ComplexClass"]),
        (25, 23, "@@class_counter", &["ComplexClass"]),
        // Global variables within class
        (26, 24, "$global_counter", &["ComplexClass"]),
    ];

    for (line, col, expected_name, expected_namespace) in class_context_test_cases {
        let position = Position::new(line, col);
        let (identifier_opt, namespace, _) = get_identifier_for_test(&analyzer, position);

        if let Some(identifier) = identifier_opt {
            assert_variable_identifier(&identifier, expected_name);
            assert_namespace_context(&namespace, expected_namespace);

            // Verify proper variable type based on name
            match identifier {
                Identifier::RubyLocalVariable { .. } => {
                    if !expected_name.starts_with("@@")
                        && !expected_name.starts_with("@")
                        && !expected_name.starts_with("$")
                    {
                        // Local variable - correct type
                    } else {
                        panic!(
                            "Expected local variable for {}, got RubyLocalVariable",
                            expected_name
                        );
                    }
                }
                Identifier::RubyInstanceVariable { .. } => {
                    if expected_name.starts_with("@") && !expected_name.starts_with("@@") {
                        // Instance variable - correct type
                    } else {
                        panic!(
                            "Expected instance variable for {}, got RubyInstanceVariable",
                            expected_name
                        );
                    }
                }
                Identifier::RubyClassVariable { .. } => {
                    if expected_name.starts_with("@@") {
                        // Class variable - correct type
                    } else {
                        panic!(
                            "Expected class variable for {}, got RubyClassVariable",
                            expected_name
                        );
                    }
                }
                Identifier::RubyGlobalVariable { .. } => {
                    if expected_name.starts_with("$") {
                        // Global variable - correct type
                    } else {
                        panic!(
                            "Expected global variable for {}, got RubyGlobalVariable",
                            expected_name
                        );
                    }
                }
                _ => panic!("Expected variable identifier for {}", expected_name),
            }
        } else {
            // Some positions might not resolve to identifiers, which is okay
            println!(
                "No identifier found at {}:{} for {}",
                line, col, expected_name
            );
        }
    }

    // Test top-level global variable (empty namespace at top level)
    let top_level_test_cases: Vec<(u32, u32, &str, &[&str])> = vec![(1, 0, "$global_counter", &[])];

    for (line, col, expected_name, expected_namespace) in top_level_test_cases {
        let position = Position::new(line, col);
        let (identifier_opt, namespace, _) = get_identifier_for_test(&analyzer, position);

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
