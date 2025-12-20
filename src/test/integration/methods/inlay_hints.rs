//! Inlay hint tests for YARD-documented methods.

use crate::test::harness::{check_inlay_hints, check_no_inlay_hints_containing};

/// YARD @return shows method return type hint.
#[tokio::test]
async fn yard_return_type() {
    // Inlay hint appears at end of method definition
    check_inlay_hints(
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
    check_inlay_hints(
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

/// Method without YARD has no type hints.
#[tokio::test]
async fn no_yard_no_method_hints() {
    check_no_inlay_hints_containing(
        r#"
class Greeter
  def greet
    "hello"
  end
end
"#,
        "->",
    )
    .await;
}
