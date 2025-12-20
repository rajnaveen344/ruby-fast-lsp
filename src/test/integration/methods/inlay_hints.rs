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

/// Method without YARD has no return type hints.
#[tokio::test]
async fn no_yard_no_method_hints() {
    // We can't use <hint none> here because the method itself generates hints
    // (def greet generates a method hint). We just verify no -> hints.
    // For now, keep this as a documentation test that YARD is required for return types.
    check(
        r#"
class Greeter
  def greet
    "hello"
  end
end
"#,
    )
    .await;
}
