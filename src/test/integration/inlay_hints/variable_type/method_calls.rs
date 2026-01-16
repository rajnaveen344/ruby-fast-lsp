//! Inlay hints for variables assigned from method calls.

use crate::test::harness::check;

// =============================================================================
// Implicit Self (No Receiver)
// =============================================================================

/// Variable from implicit self method call
#[tokio::test]
async fn implicit_self_method() {
    check(
        r#"
class Inventory
  # @return [Hash]
  def get_details<hint label="-> Hash">
    { status: "active" }
  end

  def process<hint label=" -> Hash">
    result<hint label=": Hash"> = get_details
    result
  end
end
"#,
    )
    .await;
}

/// Variable from method in included module
#[tokio::test]
async fn included_module_method() {
    check(
        r#"
module Fetchable
  # @return [Hash]
  def fetch_data<hint label="-> Hash">
    {}
  end
end

class DataService
  include Fetchable

  def process<hint label=" -> ?">
    data<hint label=": Hash"> = fetch_data
    data
  end
end
"#,
    )
    .await;
}

/// Variable from parent class method
#[tokio::test]
async fn parent_class_method() {
    check(
        r#"
class BaseService
  # @return [Array]
  def fetch_all<hint label="-> Array">
    []
  end
end

class UserService < BaseService
  def get_users<hint label=" -> ?">
    users<hint label=": Array"> = fetch_all
    users
  end
end
"#,
    )
    .await;
}

/// Variable in module context
#[tokio::test]
async fn module_context() {
    check(
        r#"
module Utils
  # @return [Integer]
  def helper<hint label="-> Integer">
    42
  end

  def process<hint label=" -> Integer">
    result<hint label=": Integer"> = helper
    result
  end
end
"#,
    )
    .await;
}

// =============================================================================
// With Receiver (Chained Calls)
// =============================================================================

/// Variable from chained method call
#[tokio::test]
async fn chained_method_call() {
    check(
        r#"
class Builder
  # @return [Product]
  def build<hint label="-> Product">
    Product.new
  end
end

class Product
end

result<hint label=": Product"> = Builder.new.build
"#,
    )
    .await;
}

/// Variable from method call on local variable
#[tokio::test]
async fn method_call_on_local() {
    check(
        r#"
class Test
  # @return [String]
  def method_a
  end

  def caller
    a<hint label="String"> = method_a
    b<hint label="String"> = a.to_s
  end
end
"#,
    )
    .await;
}

/// Deeply chained method calls
#[tokio::test]
async fn deeply_chained_calls() {
    check(
        r#"
class Test
  # @return [String]
  def method_a
  end

  def caller
    a<hint label="String"> = method_a
    b<hint label="String"> = a.to_s.to_s
    c<hint label="String"> = a.to_s.to_s.to_s
  end
end
"#,
    )
    .await;
}
