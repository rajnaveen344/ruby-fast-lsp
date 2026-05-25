//! Hover for local variables assigned from begin/rescue expressions.

use crate::test::harness::check;

#[tokio::test]
async fn hover_after_begin_rescue_assignment() {
    check(
        r#"
def m
  result = begin
    1
  rescue StandardError
    "fallback"
  end

  res<hover label="(Integer | String)">ult
end
"#,
    )
    .await;
}

#[tokio::test]
async fn hover_after_rescue_modifier_assignment() {
    check(
        r#"
def m
  result = 1 rescue "fallback"
  res<hover label="(Integer | String)">ult
end
"#,
    )
    .await;
}
