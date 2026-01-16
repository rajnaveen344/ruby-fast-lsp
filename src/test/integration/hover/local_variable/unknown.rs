//! Hover tests for local variables with Unknown type.
//!
//! When type cannot be inferred, hover should show "?" for consistency
//! with inlay hints.

use crate::test::harness::check;

/// Variable assigned from unknown method shows "?"
#[tokio::test]
async fn unknown_method_result() {
    check(
        r#"
class Service
  def fetch_data
    result = some_external_api.get_data
    res<hover label="?">ult
  end
end
"#,
    )
    .await;
}

/// Variable not in scope shows just the variable name
#[tokio::test]
async fn undefined_variable() {
    check(
        r#"
class Example
  def first_method
    secret = "password"
  end

  def second_method
    # secret should NOT be accessible here - it's in a different method
    sec<hover label="secret">ret
  end
end
"#,
    )
    .await;
}
