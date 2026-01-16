//! Hover tests for method calls WITH explicit receiver.
//!
//! Examples: obj.method, Class.new, arr.length

use crate::test::harness::check;

// =============================================================================
// Simple Method Calls
// =============================================================================

/// Hover on method call shows return type from YARD
#[tokio::test]
async fn method_call_yard_return_type() {
    check(
        r#"
class Foo
  # @return [Integer]
  def count
    42
  end
end
x = Foo.new.count<hover label="Integer">
"#,
    )
    .await;
}

/// Variable assigned from method call shows return type
#[tokio::test]
async fn variable_from_method_call() {
    check(
        r#"
class Builder
  # @return [Product]
  def build
    Product.new
  end
end

class Product
end

product<hover label="Product"> = Builder.new.build
"#,
    )
    .await;
}

// =============================================================================
// Chained Method Calls
// =============================================================================

/// Hover on chained calls shows type at each step
#[tokio::test]
async fn chained_method_calls() {
    check(
        r#"
class User
  # @return [Profile]
  def profile
    Profile.new
  end
end

class Profile
  # @return [String]
  def name
    "John"
  end
end

user = User.new
user.profile<hover label="Profile">.name<hover label="String">
"#,
    )
    .await;
}

/// Unknown propagates when chain breaks
#[tokio::test]
async fn chain_unknown_propagation() {
    check(
        r#"
class Foo
  def unknown_method
    bar  # bar is undefined, returns unknown
  end
end

x = Foo.new.unknown_method<hover label="def unknown_method">
"#,
    )
    .await;
}

// =============================================================================
// RBS Built-in Types
// =============================================================================

/// Array methods use RBS types
#[tokio::test]
async fn array_methods() {
    check(
        r#"
arr = [1, 2, 3]
arr.length<hover label="Integer">
"#,
    )
    .await;
}

/// Hash methods use RBS types
#[tokio::test]
async fn hash_methods() {
    check(
        r#"
hash = { a: 1 }
hash.keys<hover label="Array">
"#,
    )
    .await;
}
