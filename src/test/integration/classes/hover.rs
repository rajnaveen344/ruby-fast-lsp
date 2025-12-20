//! Hover tests for classes.

use crate::test::harness::check;

#[tokio::test]
async fn test_hover_class_definition() {
    check(
        r#"
class MyClass<hover label="class MyClass">
end
"#,
    )
    .await;
}

#[tokio::test]
async fn test_hover_class_reference() {
    check(
        r#"
class Foo; end
x = Foo<hover label="class Foo">.new
"#,
    )
    .await;
}
