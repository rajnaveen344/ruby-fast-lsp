//! Tests for Unknown type propagation in type inference.
//!
//! These tests verify that:
//! 1. Unknown types propagate deterministically (never guess/predict)
//! 2. Empty methods return NilClass (Ruby semantics)
//! 3. Methods with unknown receiver types return Unknown (no global lookup)
//! 4. Unknown types are explicitly displayed as `?`

use crate::test::harness::check;

/// Empty method body deterministically returns nil in Ruby.
#[tokio::test]
async fn test_empty_method_returns_nil_class() {
    check(
        r#"
class A
  def empty_method<hint label=" -> NilClass">
  end
end
"#,
    )
    .await;
}

/// Method with only comments (effectively empty) returns NilClass.
#[tokio::test]
async fn test_method_with_only_comments_returns_nil_class() {
    check(
        r#"
class A
  def commented_method<hint label=" -> NilClass">
    # This is just a comment
    # No actual code
  end
end
"#,
    )
    .await;
}

/// When a parameter's type is unknown and returned, the return type is Unknown.
#[tokio::test]
async fn test_unknown_param_propagates_as_unknown() {
    check(
        r#"
class A
  def identity(x)<hint label=" -> ?">
    x
  end
end
"#,
    )
    .await;
}

/// When calling a method on a parameter with unknown type, result is Unknown.
#[tokio::test]
async fn test_method_call_on_unknown_param_returns_unknown() {
    check(
        r#"
class A
  def process(obj)<hint label=" -> ?">
    obj.some_method
  end
end
"#,
    )
    .await;
}

/// Chained method calls on unknown types propagate Unknown.
#[tokio::test]
async fn test_chained_calls_on_unknown_propagate_unknown() {
    check(
        r#"
class A
  def chain(obj)<hint label=" -> ?">
    obj.foo.bar.baz
  end
end
"#,
    )
    .await;
}

/// Indexing an unknown type returns Unknown (no global [] lookup).
#[tokio::test]
async fn test_indexing_unknown_type_returns_unknown() {
    check(
        r#"
class A
  def get_item(container)<hint label=" -> ?">
    container[:key]
  end
end
"#,
    )
    .await;
}

/// Local variable with unknown type (from method call on unknown) propagates Unknown.
#[tokio::test]
async fn test_local_var_from_unknown_propagates_unknown() {
    check(
        r#"
class A
  def process(input)<hint label=" -> ?">
    result = input.transform
    result
  end
end
"#,
    )
    .await;
}

/// Union with Unknown absorbs all other types (strict propagation).
#[tokio::test]
async fn test_union_with_unknown_becomes_unknown() {
    check(
        r#"
class A
  def maybe_string(flag, obj)<hint label=" -> ?">
    if flag
      "hello"
    else
      obj.something
    end
  end
end
"#,
    )
    .await;
}

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

/// Method calling known methods gets correct return type.
#[tokio::test]
async fn test_known_method_call_returns_correct_type() {
    check(
        r#"
class A
  def helper
    100
  end

  def caller<hint label=" -> Integer">
    helper
  end
end
"#,
    )
    .await;
}

/// Mixed known and unknown in union - Unknown absorbs all.
#[tokio::test]
async fn test_mixed_known_unknown_union_becomes_unknown() {
    check(
        r#"
class A
  def helper
    100
  end

  def mixed(obj, flag)<hint label=" -> ?">
    if flag
      helper
    else
      obj.unknown_method
    end
  end
end
"#,
    )
    .await;
}

/// Explicit return with unknown expression returns Unknown.
#[tokio::test]
async fn test_explicit_return_unknown_expression() {
    check(
        r#"
class A
  def explicit_unknown(obj)<hint label=" -> ?">
    return obj.something
  end
end
"#,
    )
    .await;
}

/// Multiple return paths - all unknown results in Unknown.
#[tokio::test]
async fn test_multiple_unknown_returns() {
    check(
        r#"
class A
  def multi_unknown(a, b)<hint label=" -> ?">
    if a.check
      return a.value
    else
      return b.value
    end
  end
end
"#,
    )
    .await;
}

/// String method returns known type from RBS.
#[tokio::test]
async fn test_string_method_returns_rbs_type() {
    check(
        r#"
class A
  def string_upcase<hint label=" -> String">
    "hello".upcase
  end

  def string_length<hint label=" -> Integer">
    "hello".length
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
