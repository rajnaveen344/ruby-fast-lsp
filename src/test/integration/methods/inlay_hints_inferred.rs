use crate::test::harness::*;

#[tokio::test]
async fn test_inferred_return_type_hint() {
    let _ = env_logger::builder().is_test(true).try_init();

    check(
        r#"
class A
  def foo<hint label=" -> Integer">
    1
  end

  def bar<hint label=" -> String">
    "hello"
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn test_unknown_return_type_hint_without_yard() {
    let _ = env_logger::builder().is_test(true).try_init();

    check(
        r#"
class A
  def process(input)<hint label=" -> ?">
    input.transform
  end

  def handle(obj)<hint label=" -> ?">
    obj.some_unknown_method
  end

  def chain(x, y)<hint label=" -> ?">
    x.foo.bar(y.baz)
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn test_mixed_known_and_unknown_types() {
    let _ = env_logger::builder().is_test(true).try_init();

    check(
        r#"
class Calculator
  # Known types are inferred from literals
  def get_number<hint label=" -> Integer">
    42
  end

  # @return [String]
  def greet<hint label="-> String">
    "hello"
  end

  # Unknown types show "?" as visual cue (method calls without known receiver)
  def compute(x)<hint label=" -> ?">
    x.calculate
  end

  # Expressions with operators are now resolved via RBS method lookups
  def add<hint label=" -> Integer">
    1 + 2
  end
end
"#,
    )
    .await;
}

/// Test that variable assigned from implicit self method call gets correct inlay hint.
/// This tests the fix for the case where hover on method shows correct type but
/// inlay hint for the variable shows "?".
#[tokio::test]
async fn test_variable_inlay_hint_from_implicit_self_method() {
    let _ = env_logger::builder().is_test(true).try_init();

    check(
        r#"
class Inventory
  # @return [Hash]
  def get_details<hint label="-> Hash">
    { status: "active" }
  end

  # process returns Hash because it returns result which is Hash
  def process<hint label=" -> Hash">
    result<hint label=": Hash"> = get_details
    result
  end
end
"#,
    )
    .await;
}

/// Test variable assignment from chained method call.
#[tokio::test]
async fn test_variable_inlay_hint_from_chained_call() {
    let _ = env_logger::builder().is_test(true).try_init();

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

/// Test variable assigned from method defined in included module.
/// Note: The variable hint works correctly (shows Hash), but method return type
/// inference during indexing doesn't yet handle methods from included modules.
#[tokio::test]
async fn test_variable_inlay_hint_from_included_module_method() {
    let _ = env_logger::builder().is_test(true).try_init();

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

  # Method return type is ? because TypeTracker doesn't resolve included module methods
  def process<hint label=" -> ?">
    data<hint label=": Hash"> = fetch_data
    data
  end
end
"#,
    )
    .await;
}

/// Test variable assigned from method inside a module (not class).
#[tokio::test]
async fn test_variable_inlay_hint_in_module_context() {
    let _ = env_logger::builder().is_test(true).try_init();

    check(
        r#"
module Utils
  # @return [String]
  def self.format_name<hint label="-> String">
    "formatted"
  end

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

/// Test variable assigned from parent class method (inheritance).
/// Note: The variable hint works correctly (shows Array), but method return type
/// inference during indexing doesn't yet handle methods from parent classes.
#[tokio::test]
async fn test_variable_inlay_hint_from_parent_class_method() {
    let _ = env_logger::builder().is_test(true).try_init();

    check(
        r#"
class BaseService
  # @return [Array]
  def fetch_all<hint label="-> Array">
    []
  end
end

class UserService < BaseService
  # Method return type is ? because TypeTracker doesn't resolve parent class methods
  def get_users<hint label=" -> ?">
    users<hint label=": Array"> = fetch_all
    users
  end
end
"#,
    )
    .await;
}
