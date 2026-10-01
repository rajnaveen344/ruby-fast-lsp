//! Hover tests for local variables assigned from literals.

use crate::test::harness::check;

#[tokio::test]
async fn string_literal() {
    check(r#"x<hover label="String"> = "hello""#).await;
}

#[tokio::test]
async fn integer_literal() {
    check(r#"x<hover label="Integer"> = 42"#).await;
}

#[tokio::test]
async fn float_literal() {
    check(r#"x<hover label="Float"> = 3.14"#).await;
}
