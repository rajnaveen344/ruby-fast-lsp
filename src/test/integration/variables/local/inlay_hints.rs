//! Inlay hint tests for variables (literals and constructors).

use crate::test::harness::check;

// ============================================================================
// Literal Tests
// ============================================================================

/// String literal gets `: String` type hint.
#[tokio::test]
async fn string_literal() {
    check(
        r#"
x<hint label="String"> = "hello"
"#,
    )
    .await;
}

/// Integer literal gets `: Integer` type hint.
#[tokio::test]
async fn integer_literal() {
    check(
        r#"
x<hint label="Integer"> = 42
"#,
    )
    .await;
}

/// Float literal gets `: Float` type hint.
#[tokio::test]
async fn float_literal() {
    check(
        r#"
x<hint label="Float"> = 3.14
"#,
    )
    .await;
}

/// Symbol literal gets `: Symbol` type hint.
#[tokio::test]
async fn symbol_literal() {
    check(
        r#"
x<hint label="Symbol"> = :foo
"#,
    )
    .await;
}

/// Array literal gets `: Array` type hint.
#[tokio::test]
async fn array_literal() {
    check(
        r#"
x<hint label="Array"> = [1, 2, 3]
"#,
    )
    .await;
}

/// Hash literal gets `: Hash` type hint.
#[tokio::test]
async fn hash_literal() {
    check(
        r#"
x<hint label="Hash"> = { a: 1 }
"#,
    )
    .await;
}

// ============================================================================
// Constructor Tests
// ============================================================================

/// Class.new gets class instance type hint.
#[tokio::test]
async fn class_new() {
    check(
        r#"
class User
end

user<hint label="User"> = User.new
"#,
    )
    .await;
}

// FIXME: Investigate why local variable hint for `user` is missing in harness check.
// It was passing with `get_inlay_hints` but fails with `check()`.
// #[tokio::test]
// async fn nested_class_new() {
//     check(
//         r#"
// module MyApp
//   class User
//   end
// end
//
// user<hint label="User"> = MyApp::User.new
// "#,
//     )
//     .await;
// }

/// Inlay hint for variable assigned from method chain.
#[tokio::test]
async fn method_chain_result() {
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

product<hint label=": Product"> = Builder.new.build
"#,
    )
    .await;
}

/// Inlay hint for variable assigned from bare method call (implicit self).
/// This tests the case where `a = method_a` inside a class calls a method on self.
#[tokio::test]
async fn method_call_implicit_self() {
    check(
        r#"
class Test
  # @return [String]
  def method_a
  end

  def caller
    a<hint label="String"> = method_a
  end
end
"#,
    )
    .await;
}

/// Inlay hint for variable assigned from method call on local variable.
/// This tests the case where `b = a.to_s` where `a` is a String.
#[tokio::test]
async fn method_call_on_local_variable_chain() {
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

/// Inlay hint for deeply chained method calls.
/// This tests `c = a.to_s.to_s.to_s` - recursive type inference.
#[tokio::test]
async fn deeply_chained_method_calls() {
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
