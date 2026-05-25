//! Hover tests for method calls WITHOUT explicit receiver (implicit self).
//!
//! Examples: method_name(), add(1, 2) inside a class

use crate::test::harness::check;

// =============================================================================
// Implicit Self Method Calls
// =============================================================================

/// Hover on implicit self method call shows return type
#[tokio::test]
async fn implicit_self_method_call() {
    check(
        r#"
class Calculator
  # @return [Integer]
  def add(a, b)
    a + b
  end

  def compute
    add<hover label="Integer">(1, 2)
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn super_method_call() {
    check(
        r#"
class ParentProcessor
  def process
    "parent"
  end
end

class ChildProcessor < ParentProcessor
  def process
    super<hover label="String">
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn yielding_block_call_uses_block_return_type() {
    check(
        r#"
class User
  def name
    "Ada"
  end
end

def with_user
  yield User.new
end

def label
  with_user<hover label="String"> do |user|
    user.name
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn included_hook_instance_method_call_uses_target_return_type() {
    check(
        r#"
module DailyTrends
  def self.included(base)
    base.send :include, SharedMethods
  end

  module SharedMethods
    # @return [String]
    def get_html
      "html"
    end
  end
end

class Worker
  include DailyTrends

  def render
    get_html<hover label="String">
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn included_hook_class_eval_instance_method_call_uses_target_return_type() {
    check(
        r#"
module AdminHelper
  def self.included(base)
    base.class_eval do
      include RequestHelpers
    end
  end

  module RequestHelpers
    # @return [String]
    def api_get
      "ok"
    end
  end
end

class SpecContext
  include AdminHelper

  def render
    api_get<hover label="String">
  end
end
"#,
    )
    .await;
}

// =============================================================================
// Method Parameters
// =============================================================================

/// Hover on method parameter shows YARD type
#[tokio::test]
async fn method_parameter_yard_type() {
    check(
        r#"
class Foo
  # @param request_info [Hash] the request data
  def process(request_info<hover label="Hash">)
    request_info
  end
end
"#,
    )
    .await;
}

/// Hover on parameter usage in method body
#[tokio::test]
async fn method_parameter_usage_in_body() {
    check(
        r#"
class Foo
  # @param name [String] the user name
  def greet(name)
    puts name<hover label="String">
  end
end
"#,
    )
    .await;
}

/// Hover on parameter in if condition
#[tokio::test]
async fn method_parameter_in_if_condition() {
    check(
        r#"
class Foo
  # @param request_info [Hash] the request data
  def process(request_info)
    if reque<hover label="Hash">st_info[:valid]
      puts "valid"
    end
  end
end
"#,
    )
    .await;
}

/// Alternative YARD format: @param[Type] name
#[tokio::test]
async fn method_parameter_alt_yard_format() {
    check(
        r#"
class Foo
  # @param[Hash] request_info the request data
  def process(request_info)
    reque<hover label="Hash">st_info
  end
end
"#,
    )
    .await;
}

/// Parameter without YARD doc shows Unknown (?)
#[tokio::test]
async fn method_parameter_no_yard_doc() {
    check(
        r#"
class Foo
  def process(request_info)
    reque<hover label="?">st_info if request_<hover label="?">info[:status]
  end
end
"#,
    )
    .await;
}

/// Hover on unknown method call without receiver shows "?"
#[tokio::test]
async fn unknown_method_call_without_receiver() {
    check(
        r#"
class Foo
  def process
    unknown_method<hover label="?">
  end
end
"#,
    )
    .await;
}
