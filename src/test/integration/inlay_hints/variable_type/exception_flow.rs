//! Inlay hints for local variables assigned from begin/rescue/ensure expressions.

use crate::test::harness::check;

#[tokio::test]
async fn assignment_from_begin_rescue_expression() {
    check(
        r#"
def m
  result<hint label="(Integer | String)"> = begin
    1
  rescue StandardError
    "fallback"
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn assignment_from_begin_rescue_else_expression() {
    check(
        r#"
def m
  result<hint label="(Float | String)"> = begin
    1
  rescue StandardError
    "fallback"
  else
    1.0
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn assignment_from_begin_rescue_ensure_expression() {
    check(
        r#"
def m
  result<hint label="(Integer | String)"> = begin
    1
  rescue StandardError
    "fallback"
  ensure
    cleanup
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn assignment_from_rescue_modifier_expression() {
    check(
        r#"
def m
  result<hint label="(Integer | String)"> = 1 rescue "fallback"
end
"#,
    )
    .await;
}
