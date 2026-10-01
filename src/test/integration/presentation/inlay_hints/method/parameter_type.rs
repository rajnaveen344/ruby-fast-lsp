//! Inlay hints for method parameter types.
//!
//! Shows type hints for parameters from YARD documentation.

use crate::test::harness::check;

/// YARD @param shows parameter type hint
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
