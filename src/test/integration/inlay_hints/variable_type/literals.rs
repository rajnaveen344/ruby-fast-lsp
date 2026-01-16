//! Inlay hints for variables assigned from literals.

use crate::test::harness::check;

#[tokio::test]
async fn string_literal() {
    check(r#"x<hint label="String"> = "hello""#).await;
}

#[tokio::test]
async fn integer_literal() {
    check(r#"x<hint label="Integer"> = 42"#).await;
}

#[tokio::test]
async fn float_literal() {
    check(r#"x<hint label="Float"> = 3.14"#).await;
}

#[tokio::test]
async fn symbol_literal() {
    check(r#"x<hint label="Symbol"> = :foo"#).await;
}

#[tokio::test]
async fn array_literal() {
    check(r#"x<hint label="Array"> = [1, 2, 3]"#).await;
}

#[tokio::test]
async fn hash_literal() {
    check(r#"x<hint label="Hash"> = { a: 1 }"#).await;
}
