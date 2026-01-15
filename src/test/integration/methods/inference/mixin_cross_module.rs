//! Cross-module mixin type inference tests.
//!
//! Tests for inferring types when a class includes multiple modules,
//! and methods in one module call methods defined in another module.
//!
//! This reproduces the issue where:
//! - Module A has method `foo` that calls `bar`
//! - Module B has method `bar` with a known return type
//! - Class C includes both A and B
//! - When analyzing `foo`, we should be able to infer the return type of `bar`

use crate::test::harness::check_multi_file;

// ============================================================================
// Multi-definition inference tests (no YARD annotations)
// ============================================================================

/// Test: Method with multiple definitions across files, all return same literal type
#[tokio::test]
async fn test_multi_def_same_literal_type() {
    check_multi_file(&[
        (
            "impl_a.rb",
            r#"
class Service
  def get_val<type label="Integer" kind="return">ue
    42
  end
end
"#,
        ),
        (
            "impl_b.rb",
            r#"
class Service
  def get_value
    100
  end
end
"#,
        ),
    ])
    .await;
}

/// Test: Method with multiple definitions returning different literal types (union)
#[tokio::test]
async fn test_multi_def_different_literal_types() {
    check_multi_file(&[
        (
            "impl_a.rb",
            r#"
class Service
  def get_val<type label="Integer | String" kind="return">ue
    42
  end
end
"#,
        ),
        (
            "impl_b.rb",
            r#"
class Service
  def get_value
    "hello"
  end
end
"#,
        ),
    ])
    .await;
}

/// Test: Module method with multiple definitions across files
#[tokio::test]
async fn test_module_multi_def() {
    check_multi_file(&[
        (
            "mod_a.rb",
            r#"
module Utils
  def forma<type label="String" kind="return">t
    "formatted"
  end
end
"#,
        ),
        (
            "mod_b.rb",
            r#"
module Utils
  def format
    "also formatted"
  end
end
"#,
        ),
        (
            "consumer.rb",
            r#"
class Consumer
  include Utils
end
"#,
        ),
    ])
    .await;
}

/// Test: Nested class with multiple definitions
#[tokio::test]
async fn test_nested_class_multi_def() {
    check_multi_file(&[
        (
            "outer_a.rb",
            r#"
class Outer
  class Inner
    def compu<type label="Float" kind="return">te
      3.14
    end
  end
end
"#,
        ),
        (
            "outer_b.rb",
            r#"
class Outer
  class Inner
    def compute
      2.71
    end
  end
end
"#,
        ),
    ])
    .await;
}
