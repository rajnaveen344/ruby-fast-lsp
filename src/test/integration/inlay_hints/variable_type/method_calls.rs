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

  def process<hint label=" -> Hash">
    data<hint label=": Hash"> = fetch_data
    data
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn concern_class_method_result_type() {
    check(
        r#"
module Searchable
  extend ActiveSupport::Concern

  class_methods do
    # @return [String]
    def find_by_term
      "ok"
    end
  end
end

class Product
  include Searchable
end

result<hint label=": String"> = Product.find_by_term
"#,
    )
    .await;
}

#[tokio::test]
async fn protected_same_family_explicit_receiver_result_type() {
    check(
        r#"
class Vault
  # @return [String]
  def semi_secret<hint label="-> String">
    "token"
  end
  protected :semi_secret

  def compare
    other<hint label=": Vault"> = Vault.new
    result<hint label=": String"> = other.semi_secret
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn public_mixin_visibility_override_result_type() {
    check(
        r#"
module SharedSecret
  private

  # @return [String]
  def hidden<hint label="-> String">
    "hidden"
  end
end

class Vault
  include SharedSecret
  public :hidden
end

result<hint label=": String"> = Vault.new.hidden
"#,
    )
    .await;
}

#[tokio::test]
async fn protected_mixin_visibility_override_same_family_result_type() {
    check(
        r#"
module SharedSecret
  # @return [String]
  def hidden<hint label="-> String">
    "hidden"
  end
end

class Vault
  include SharedSecret
  protected :hidden

  def compare
    other<hint label=": Vault"> = Vault.new
    result<hint label=": String"> = other.hidden
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
  def get_users<hint label=" -> Array">
    users<hint label=": Array"> = fetch_all
    users
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn method_missing_call_uses_fallback_return_type() {
    check(
        r#"
class DynamicRecord
  # @return [String]
  def method_missing(name, *args)<hint label=" -> String">
    "dynamic"
  end
end

value<hint label=": String"> = DynamicRecord.new.virtual_total
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

#[tokio::test]
async fn extend_self_module_method_call() {
    check(
        r#"
module Utils
  extend self

  # @return [String]
  def helper<hint label=" -> String">
    "helping"
  end
end

value<hint label=": String"> = Utils.helper
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

#[tokio::test]
async fn forwarded_args_call_uses_target_return_type() {
    check(
        r#"
class Forwarder
  # @return [String]
  def target(value)
    "ok"
  end

  def wrapper(...)
    target(...)
  end
end

result<hint label="String"> = Forwarder.new.wrapper(1)
"#,
    )
    .await;
}

#[tokio::test]
async fn forwarding_super_uses_parent_return_type() {
    check(
        r#"
class ParentForwarder
  # @return [String]
  def target(value)
    "ok"
  end
end

class ChildForwarder < ParentForwarder
  def target(...)
    super
  end
end

result<hint label="String"> = ChildForwarder.new.target(1)
"#,
    )
    .await;
}

#[tokio::test]
async fn anonymous_block_forwarding_feeds_yielding_method_return_type() {
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

def each_user(&)
  with_user(&)
end

result<hint label="String"> = each_user do |user|
  user.name
end
"#,
    )
    .await;
}

#[tokio::test]
async fn dot_forwarding_feeds_yielding_method_return_type() {
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

def each_user(...)
  with_user(...)
end

result<hint label="String"> = each_user do |user|
  user.name
end
"#,
    )
    .await;
}

#[tokio::test]
async fn block_param_assignment_from_array_each() {
    check(
        r#"
class Test
  def caller
    [1, 2, 3].each do |item|
      copy<hint label="Integer"> = item
    end
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn numbered_block_param_assignment_from_array_each() {
    check(
        r#"
class Test
  def caller
    [1, 2, 3].each { copy<hint label="Integer"> = _1 }
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn block_param_assignment_from_yield_argument() {
    check(
        r#"
class User
end

def with_user
  yield User.new
end

with_user do |user|
  copy<hint label="User"> = user
end
"#,
    )
    .await;
}

#[tokio::test]
async fn numbered_block_param_assignment_from_yield_argument() {
    check(
        r#"
class User
end

def with_user
  yield User.new
end

with_user do
  copy<hint label="User"> = _1
end
"#,
    )
    .await;
}

#[tokio::test]
async fn class_block_param_assignment_from_yield_argument() {
    check(
        r#"
class User
end

class Builder
  def with_user
    yield User.new
  end

  def caller
    result = with_user do |user|
      copy<hint label="User"> = user
      user
    end
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn assignment_from_yielding_block_return() {
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

result<hint label="String"> = with_user do |user|
  user.name
end
"#,
    )
    .await;
}

#[tokio::test]
async fn assignment_from_lambda_call_return() {
    check(
        r#"
builder = -> { "ready" }
result<hint label="String"> = builder.call
"#,
    )
    .await;
}

#[tokio::test]
async fn assignment_from_proc_call_return() {
    check(
        r#"
builder = Proc.new { 1 }
result<hint label="Integer"> = builder.call
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

// =============================================================================
// Generic Type Resolution
// =============================================================================

/// Array#first should resolve generic Elem to Integer
#[tokio::test]
async fn array_first_resolves_generic() {
    check(
        r#"
a<hint label="Integer"> = [1, 2, 3].first
"#,
    )
    .await;
}

/// Integer#abs should return Integer, not (Integer | Numeric)
#[tokio::test]
async fn integer_abs_returns_integer() {
    check(
        r#"
b<hint label="Integer"> = 2.abs
"#,
    )
    .await;
}

/// After re-indexing (simulating edit), types should still be correct
#[tokio::test]
async fn generic_types_survive_reindex() {
    use crate::test::harness::FakeEditor;

    let mut editor = FakeEditor::new().await;
    let code = "a = [1, 2, 3].first\nb = 2.abs";

    // First indexing
    editor.open("test.rb", code).await;
    editor
        .check(
            "test.rb",
            r#"a<hint label="Integer"> = [1, 2, 3].first
b<hint label="Integer"> = 2.abs"#,
        )
        .await;

    // Simulate edit (same content, triggers re-indexing)
    editor.set("test.rb", code).await;
    editor
        .check(
            "test.rb",
            r#"a<hint label="Integer"> = [1, 2, 3].first
b<hint label="Integer"> = 2.abs"#,
        )
        .await;
}

/// Repeated indexing preserves a user-defined method's inferred nil return.
#[tokio::test]
async fn generic_types_survive_reindex_with_class() {
    use crate::test::harness::FakeEditor;

    let mut editor = FakeEditor::new().await;
    let code = r#"class UserA
  def namea
  end
end

a = UserA.new.namea

a = [1,2,3].first

b = 2.abs"#;

    // First indexing
    editor.open("test.rb", code).await;
    editor
        .check(
            "test.rb",
            r#"class UserA
  def namea<hint label="NilClass">
  end
end

a<hint label="NilClass"> = UserA.new.namea

a<hint label="Integer"> = [1,2,3].first

b<hint label="Integer"> = 2.abs"#,
        )
        .await;

    // Simulate edit (triggers re-indexing)
    editor.set("test.rb", code).await;
    editor
        .check(
            "test.rb",
            r#"class UserA
  def namea<hint label="NilClass">
  end
end

a<hint label="NilClass"> = UserA.new.namea

a<hint label="Integer"> = [1,2,3].first

b<hint label="Integer"> = 2.abs"#,
        )
        .await;

    // Third edit — still correct
    editor.set("test.rb", code).await;
    editor
        .check(
            "test.rb",
            r#"class UserA
  def namea<hint label="NilClass">
  end
end

a<hint label="NilClass"> = UserA.new.namea

a<hint label="Integer"> = [1,2,3].first

b<hint label="Integer"> = 2.abs"#,
        )
        .await;
}

/// Chained method call: [1,2,3].first.abs should return Integer, not (Integer | Numeric)
#[tokio::test]
async fn chained_method_stops_at_first_ancestor_match() {
    check(
        r#"
a<hint label="Integer"> = [1, 2, 3].first.abs
"#,
    )
    .await;
}

/// Method resolver should stop at the first (most specific) ancestor that defines the method.
/// When both a child class and parent class define the same method, only the child's return type
/// should be used — not a union of both.
#[tokio::test]
async fn method_resolver_stops_at_most_specific_ancestor() {
    check(
        r#"
class Animal
  # @return [String]
  def sound
    "..."
  end
end

class Dog < Animal
  # @return [Symbol]
  def sound
    :bark
  end
end

dog<hint label="Dog"> = Dog.new
result<hint label="Symbol"> = dog.sound
"#,
    )
    .await;
}
