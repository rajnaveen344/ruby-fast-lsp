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

/// Variable not in scope is treated as a method call - shows "?"
/// In Ruby, a name without prior assignment in scope is a method call, not a variable read.
#[tokio::test]
async fn undefined_variable() {
    check(
        r#"
class Example
  def first_method
    secret = "password"
  end

  def second_method
    # secret is NOT accessible here - it's in a different method
    # Ruby treats this as a method call, not a variable read
    sec<hover label="?">ret
  end
end
"#,
    )
    .await;
}
