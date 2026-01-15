//! Tests for Unknown type propagation in type inference.
//!
//! These tests verify that:
//! 1. Unknown types propagate deterministically (never guess/predict)
//! 2. Empty methods return NilClass (Ruby semantics)
//! 3. Methods with unknown receiver types return Unknown (no global lookup)
//! 4. Unknown types are explicitly displayed as `?`

use crate::test::harness::check;

/// Known literal return type is inferred correctly.
#[tokio::test]
async fn test_known_literal_returns_correct_type() {
    check(
        r#"
class A
  def get_string<hint label=" -> String">
    "hello"
  end

  def get_integer<hint label=" -> Integer">
    42
  end

  def get_float<hint label=" -> Float">
    3.14
  end

  def get_symbol<hint label=" -> Symbol">
    :foo
  end

  def get_true<hint label=" -> TrueClass">
    true
  end

  def get_false<hint label=" -> FalseClass">
    false
  end

  def get_nil<hint label=" -> NilClass">
    nil
  end
end
"#,
    )
    .await;
}

/// Array literal returns Array type.
#[tokio::test]
async fn test_array_literal_returns_array_type() {
    check(
        r#"
class A
  def get_array<hint label=" -> Array<Integer>">
    [1, 2, 3]
  end
end
"#,
    )
    .await;
}

/// Hash literal returns Hash type.
#[tokio::test]
async fn test_hash_literal_returns_hash_type() {
    check(
        r#"
class A
  def get_hash<hint label=" -> Hash<Symbol, Integer>">
    { a: 1, b: 2 }
  end
end
"#,
    )
    .await;
}
