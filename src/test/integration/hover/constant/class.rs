//! Hover tests for class constants.

use crate::test::harness::check;

/// Hover on class definition shows "class ClassName"
#[tokio::test]
async fn class_definition() {
    check(
        r#"
class MyClass<hover label="class MyClass">
end
"#,
    )
    .await;
}

/// Hover on class reference shows "class ClassName"
#[tokio::test]
async fn class_reference() {
    check(
        r#"
class Foo; end
x = Foo<hover label="class Foo">.new
"#,
    )
    .await;
}

/// Hover on method definition shows return type (from YARD)
#[tokio::test]
async fn method_definition_return_type() {
    check(
        r#"
class Foo
  # @return [String]
  def bar<hover label="String">
    "hello"
  end
end
"#,
    )
    .await;
}
