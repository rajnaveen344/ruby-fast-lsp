//! Hover tests for variables.

use crate::test::harness::check;

#[tokio::test]
async fn test_hover_local_string() {
    check(r#"x<hover label="String"> = "hello""#).await;
}

#[tokio::test]
async fn test_hover_local_integer() {
    check(r#"x<hover label="Integer"> = 42"#).await;
}

#[tokio::test]
async fn test_hover_local_float() {
    check(r#"x<hover label="Float"> = 3.14"#).await;
}

// TODO: This test requires enhanced type tracking where variables holding class
// references (like `a = Klass`) are tracked as Class<Klass> type
#[ignore]
#[tokio::test]
async fn test_hover_class_in_variable() {
    check(
        r#"
class Klass; end
a<hover label="class Klass"> = Klass
k = a.new
"#,
    )
    .await;
}
