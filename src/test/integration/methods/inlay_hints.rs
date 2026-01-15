//! Inlay hint tests for YARD-documented methods.

use crate::test::harness::check;

/// YARD @return shows method return type hint.
#[tokio::test]
async fn yard_return_type() {
    // Inlay hint appears at end of method definition
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

/// YARD @param shows parameter type hint.
#[tokio::test]
async fn yard_param_type() {
    check(
        r#"
class Greeter
  # @param name [String] the name
  # @return [String]
  def greet(name<hint label="String">)
    "Hello, #{name}"
  end
end
"#,
    )
    .await;
}

/// Method without YARD/RBS infers return type from body when possible.
#[tokio::test]
async fn no_yard_infers_from_literal() {
    // Even without YARD, TypeTracker infers String from the literal.
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
