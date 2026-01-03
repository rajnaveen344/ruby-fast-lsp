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
