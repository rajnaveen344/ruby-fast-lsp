//! Inlay hints for method return types.
//!
//! Shows " -> Type" after method signature.

use crate::test::harness::check;

// =============================================================================
// YARD Documented
// =============================================================================

/// YARD @return shows return type hint
#[tokio::test]
async fn yard_return_type() {
    check(
        r#"
class Greeter
  # @return [String] the greeting
  def greet<hint label="-> String">; "hello"; end
end
"#,
    )
    .await;
}

// =============================================================================
// Inferred from Literals
// =============================================================================

/// Return type inferred from Integer literal
#[tokio::test]
async fn inferred_integer() {
    check(
        r#"
class A
  def foo<hint label=" -> Integer">
    1
  end
end
"#,
    )
    .await;
}

/// Return type inferred from String literal
#[tokio::test]
async fn inferred_string() {
    check(
        r#"
class A
  def bar<hint label=" -> String">
    "hello"
  end
end
"#,
    )
    .await;
}

/// Method without YARD infers from body
#[tokio::test]
async fn no_yard_infers_from_literal() {
    check(
        r#"
class Greeter
  def greet<hint label=" -> String">
    "hello"
  end
end
"#,
    )
    .await;
}

// =============================================================================
// Unknown Return Type
// =============================================================================

/// Unknown return type shows " -> ?"
#[tokio::test]
async fn unknown_return_type() {
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

// =============================================================================
// Mixed Known and Unknown
// =============================================================================

/// Methods with various type inference scenarios
#[tokio::test]
async fn mixed_known_and_unknown() {
    check(
        r#"
class Calculator
  def get_number<hint label=" -> Integer">
    42
  end

  # @return [String]
  def greet<hint label="-> String">
    "hello"
  end

  def compute(x)<hint label=" -> ?">
    x.calculate
  end

  def add<hint label=" -> Integer">
    1 + 2
  end
end
"#,
    )
    .await;
}
