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

/// Test: Two modules included in same class, one calls method from other
#[tokio::test]
async fn test_cross_module_method_call() {
    check_multi_file(&[(
        "main.rb",
        r#"
module Commerce
  def process_or<type label="String" kind="return">der
    get_user_name
  end
end

module Users
  # @return [String]
  def get_user_name
    "John"
  end
end

class Consumer
  include Commerce
  include Users
end
"#,
    )])
    .await;
}

/// Test: Nested modules like GoshPosh::Platform::API::Commerce and Users
#[tokio::test]
async fn test_nested_module_cross_call() {
    check_multi_file(&[
        (
            "commerce.rb",
            r#"
module Platform
  module API
    module Commerce
      def update_redemp<type label="String" kind="return">tion
        get_user_home_domain
      end
    end
  end
end
"#,
        ),
        (
            "users.rb",
            r#"
module Platform
  module API
    module Users
      # @return [String]
      def get_user_home_domain
        "example.com"
      end
    end
  end
end
"#,
        ),
        (
            "consumer.rb",
            r#"
class ActivityConsumer
  include Platform::API::Users
  include Platform::API::Commerce
end
"#,
        ),
    ])
    .await;
}

/// Test: Method in module A calls method in module B, both included in class
#[tokio::test]
async fn test_simple_cross_module_inference() {
    check_multi_file(&[
        (
            "module_a.rb",
            r#"
module ModuleA
  def wrapper_meth<type label="Integer" kind="return">od
    helper_method
  end
end
"#,
        ),
        (
            "module_b.rb",
            r#"
module ModuleB
  # @return [Integer]
  def helper_method
    42
  end
end
"#,
        ),
        (
            "consumer.rb",
            r#"
class MyClass
  include ModuleA
  include ModuleB
end
"#,
        ),
    ])
    .await;
}

/// Test: Chain of cross-module calls (with YARD on intermediate)
#[tokio::test]
async fn test_cross_module_chain() {
    check_multi_file(&[
        (
            "module_a.rb",
            r#"
module ModuleA
  def level<type label="String" kind="return">_1
    level_2
  end
end
"#,
        ),
        (
            "module_b.rb",
            r#"
module ModuleB
  # @return [String]
  def level_2
    level_3
  end
end
"#,
        ),
        (
            "module_c.rb",
            r#"
module ModuleC
  # @return [String]
  def level_3
    "deep"
  end
end
"#,
        ),
        (
            "consumer.rb",
            r#"
class MyClass
  include ModuleA
  include ModuleB
  include ModuleC
end
"#,
        ),
    ])
    .await;
}

/// Test: Cross-module call without YARD should return Unknown, not None
#[tokio::test]
async fn test_cross_module_no_yard_returns_unknown() {
    check_multi_file(&[
        (
            "module_a.rb",
            r#"
module ModuleA
  def wrapper_meth<type label="?" kind="return">od
    helper_no_yard
  end
end
"#,
        ),
        (
            "module_b.rb",
            r#"
module ModuleB
  def helper_no_yard
    some_unknown_call
  end
end
"#,
        ),
        (
            "consumer.rb",
            r#"
class MyClass
  include ModuleA
  include ModuleB
end
"#,
        ),
    ])
    .await;
}

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

/// Test: Method calling another method with multiple definitions (cross-file inference)
#[tokio::test]
async fn test_call_multi_def_method() {
    check_multi_file(&[
        (
            "caller.rb",
            r#"
class Caller
  def process<type label="Integer" kind="return">
    helper
  end

  def helper
    999
  end
end
"#,
        ),
        (
            "helper_override.rb",
            r#"
class Caller
  def helper
    123
  end
end
"#,
        ),
    ])
    .await;
}

/// Test: Chain of calls where intermediate method has multiple definitions
#[tokio::test]
async fn test_chain_with_multi_def_intermediate() {
    check_multi_file(&[
        (
            "entry.rb",
            r#"
class Chain
  def sta<type label="String" kind="return">rt
    middle
  end
end
"#,
        ),
        (
            "middle_a.rb",
            r#"
class Chain
  def middle
    finish
  end
end
"#,
        ),
        (
            "middle_b.rb",
            r#"
class Chain
  def middle
    finish
  end
end
"#,
        ),
        (
            "finish.rb",
            r#"
class Chain
  def finish
    "done"
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

/// Test: Cross-module call to method with multiple definitions (no YARD)
#[tokio::test]
async fn test_cross_module_multi_def_no_yard() {
    check_multi_file(&[
        (
            "module_a.rb",
            r#"
module ModuleA
  def wrap<type label="Integer" kind="return">per
    shared_helper
  end
end
"#,
        ),
        (
            "module_b.rb",
            r#"
module ModuleB
  def shared_helper
    42
  end
end
"#,
        ),
        (
            "module_c.rb",
            r#"
module ModuleC
  def shared_helper
    100
  end
end
"#,
        ),
        (
            "consumer.rb",
            r#"
class MyClass
  include ModuleA
  include ModuleB
  include ModuleC
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

/// Test: Method returning result of call to multi-def method with mixed return types
#[tokio::test]
async fn test_call_multi_def_mixed_returns() {
    check_multi_file(&[
        (
            "caller.rb",
            r#"
class Processor
  def run<type label="Integer | String" kind="return">
    fetch_data
  end
end
"#,
        ),
        (
            "fetch_int.rb",
            r#"
class Processor
  def fetch_data
    42
  end
end
"#,
        ),
        (
            "fetch_str.rb",
            r#"
class Processor
  def fetch_data
    "data"
  end
end
"#,
        ),
    ])
    .await;
}
