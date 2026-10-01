use super::*;

#[test]
fn test_get_identifier_simple_constant() {
    let content = "CONST_A";
    let analyzer = create_analyzer(content);

    // Position cursor at "CONST_A"
    let position = Position::new(0, 2);
    let (identifier_opt, ancestors, _scope_stack) = get_identifier_for_test(&analyzer, position);

    // Ensure we found an identifier
    let identifier = identifier_opt.expect("Expected to find an identifier at this position");

    // Use helper function for cleaner assertion
    assert_constant_identifier(&identifier, &["CONST_A"]);
    assert_namespace_context(&ancestors, &[]);
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
    let (identifier_opt, ancestors, _scope_stack) = get_identifier_for_test(&analyzer, position);

    // Ensure we found an identifier
    let identifier = identifier_opt.expect("Expected to find an identifier at this position");

    // Use helper functions for cleaner assertions
    assert_constant_identifier(&identifier, &["CONST_B"]);
    assert_namespace_context(&ancestors, &["Outer"]);
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
    let (identifier_opt, ancestors, _scope_stack) = get_identifier_for_test(&analyzer, position);

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
    let (identifier_opt, ancestors, _scope_stack) = get_identifier_for_test(&analyzer, position);

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
    let (identifier_opt, ancestors, _scope_stack) = get_identifier_for_test(&analyzer, position);

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
        0,
        "Namespace stack should be empty for absolute reference at global scope"
    );

    // Test position at "Inner" in the "::Outer::Inner::CONST_A" reference
    let position = Position::new(7, 18);
    let (identifier_opt, ancestors, _scope_stack) = get_identifier_for_test(&analyzer, position);

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
        0,
        "Namespace stack should be empty for absolute reference at global scope"
    );

    // Test position at "Outer" in the "::Outer::Inner::CONST_A" reference
    let position = Position::new(7, 12);
    let (identifier_opt, ancestors, _scope_stack) = get_identifier_for_test(&analyzer, position);

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
        0,
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
    let (identifier_opt, ancestors, _scope_stack) = get_identifier_for_test(&analyzer, position);

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
    assert_eq!(ancestors.len(), 1);
    assert_eq!(ancestors[0].to_string(), "Outer");
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
    let (identifier_opt, ancestors, _) = get_identifier_for_test(&analyzer, position);

    let identifier = identifier_opt.expect("Expected to find constant identifier");

    assert_constant_identifier(&identifier, &["DEEP_CONST"]);
    assert_namespace_context(&ancestors, &["Level1", "Level2", "Level3"]);

    // Test 2: Level3::DEEP_CONST accessed within Level2
    let position = Position::new(12, 20);
    let (identifier_opt, ancestors, _) = get_identifier_for_test(&analyzer, position);

    let identifier = identifier_opt.expect("Expected to find constant identifier");

    assert_constant_identifier(&identifier, &["Level3", "DEEP_CONST"]);
    assert_namespace_context(&ancestors, &["Level1", "Level2"]);

    // Test 3: Level2::Level3::DEEP_CONST accessed within Level1
    let position = Position::new(17, 25); // Adjusted position for DEEP_CONST
    let (identifier_opt, ancestors, _) = get_identifier_for_test(&analyzer, position);

    let identifier = identifier_opt.expect("Expected to find constant identifier");

    assert_constant_identifier(&identifier, &["Level2", "Level3", "DEEP_CONST"]);
    assert_namespace_context(&ancestors, &["Level1"]);
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
    let (identifier_opt, ancestors, _) = get_identifier_for_test(&analyzer, position);
    let identifier = identifier_opt.expect("Expected to find constant identifier");

    assert_constant_identifier(&identifier, &["Outer", "Inner", "CONST_VALUE"]);
    assert_namespace_context(&ancestors, &["Outer", "AnotherModule"]);

    // Test 2: Absolute reference ::Outer::Inner
    let position = Position::new(7, 20);
    let (identifier_opt, ancestors, _) = get_identifier_for_test(&analyzer, position);
    let identifier = identifier_opt.expect("Expected to find constant identifier");

    assert_constant_identifier(&identifier, &["Outer", "Inner"]);
    assert_namespace_context(&ancestors, &["Outer", "AnotherModule"]);

    // Test 3: Absolute reference ::Outer
    let position = Position::new(7, 14);
    let (identifier_opt, ancestors, _) = get_identifier_for_test(&analyzer, position);
    let identifier = identifier_opt.expect("Expected to find constant identifier");

    assert_constant_identifier(&identifier, &["Outer"]);
    assert_namespace_context(&ancestors, &["Outer", "AnotherModule"]);

    // Test 4: Absolute reference to top-level constant ::TopLevelConst
    let position = Position::new(8, 18);
    let (identifier_opt, ancestors, _) = get_identifier_for_test(&analyzer, position);
    let identifier = identifier_opt.expect("Expected to find constant identifier");

    assert_constant_identifier(&identifier, &["TopLevelConst"]);
    assert_namespace_context(&ancestors, &["Outer", "AnotherModule"]);
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
    let (identifier_opt, ancestors, _) = get_identifier_for_test(&analyzer, position);
    let identifier = identifier_opt.expect("Expected to find constant identifier");

    assert_constant_identifier(&identifier, &["Alpha"]);
    assert_namespace_context(&ancestors, &[]);

    // Test 2: Cursor on Beta
    let position = Position::new(9, 18);
    let (identifier_opt, ancestors, _) = get_identifier_for_test(&analyzer, position);
    let identifier = identifier_opt.expect("Expected to find constant identifier");

    assert_constant_identifier(&identifier, &["Alpha", "Beta"]);
    assert_namespace_context(&ancestors, &[]);

    // Test 3: Cursor on Gamma
    let position = Position::new(9, 25);
    let (identifier_opt, ancestors, _) = get_identifier_for_test(&analyzer, position);
    let identifier = identifier_opt.expect("Expected to find constant identifier");

    assert_constant_identifier(&identifier, &["Alpha", "Beta", "Gamma"]);
    assert_namespace_context(&ancestors, &[]);

    // Test 4: Cursor on DELTA
    let position = Position::new(9, 32);
    let (identifier_opt, ancestors, _) = get_identifier_for_test(&analyzer, position);
    let identifier = identifier_opt.expect("Expected to find constant identifier");

    assert_constant_identifier(&identifier, &["Alpha", "Beta", "Gamma", "DELTA"]);
    assert_namespace_context(&ancestors, &[]);
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
    let (identifier_opt, ancestors, _) = get_identifier_for_test(&analyzer, position);
    let identifier = identifier_opt.expect("Expected to find constant identifier");

    assert_constant_identifier(&identifier, &["GLOBAL_CONST"]);
    assert_namespace_context(&ancestors, &["OuterModule", "OuterClass"]);

    // Test 2: OUTER_CONST accessed from instance method in OuterClass
    let position = Position::new(11, 18);
    let (identifier_opt, ancestors, _) = get_identifier_for_test(&analyzer, position);
    let identifier = identifier_opt.expect("Expected to find constant identifier");

    assert_constant_identifier(&identifier, &["OUTER_CONST"]);
    assert_namespace_context(&ancestors, &["OuterModule", "OuterClass"]);

    // Test 3: CLASS_CONST accessed from instance method in OuterClass
    let position = Position::new(12, 18);
    let (identifier_opt, ancestors, _) = get_identifier_for_test(&analyzer, position);
    let identifier = identifier_opt.expect("Expected to find constant identifier");

    assert_constant_identifier(&identifier, &["CLASS_CONST"]);
    assert_namespace_context(&ancestors, &["OuterModule", "OuterClass"]);

    // Test 4: Qualified access OuterModule::OUTER_CONST
    let position = Position::new(13, 35);
    let (identifier_opt, ancestors, _) = get_identifier_for_test(&analyzer, position);
    let identifier = identifier_opt.expect("Expected to find constant identifier");

    assert_constant_identifier(&identifier, &["OuterModule", "OUTER_CONST"]);
    assert_namespace_context(&ancestors, &["OuterModule", "OuterClass"]);

    // Test 5: GLOBAL_CONST accessed from InnerModule
    let position = Position::new(20, 18);
    let (identifier_opt, ancestors, _) = get_identifier_for_test(&analyzer, position);
    let identifier = identifier_opt.expect("Expected to find constant identifier");

    assert_constant_identifier(&identifier, &["GLOBAL_CONST"]);
    assert_namespace_context(&ancestors, &["OuterModule", "OuterClass", "InnerModule"]);

    // Test 6: INNER_CONST accessed from InnerModule
    let position = Position::new(23, 18);
    let (identifier_opt, ancestors, _) = get_identifier_for_test(&analyzer, position);
    let identifier = identifier_opt.expect("Expected to find constant identifier");

    assert_constant_identifier(&identifier, &["INNER_CONST"]);
    assert_namespace_context(&ancestors, &["OuterModule", "OuterClass", "InnerModule"]);

    // Test 7: Fully qualified access OuterModule::OuterClass::CLASS_CONST
    let position = Position::new(24, 50);
    let (identifier_opt, ancestors, _) = get_identifier_for_test(&analyzer, position);
    let identifier = identifier_opt.expect("Expected to find constant identifier");

    assert_constant_identifier(&identifier, &["OuterModule", "OuterClass", "CLASS_CONST"]);
    assert_namespace_context(&ancestors, &["OuterModule", "OuterClass", "InnerModule"]);
}
