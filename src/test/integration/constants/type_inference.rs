//! Constant value type inference tests.

use crate::test::harness::check;

#[tokio::test]
async fn constant_literal_type_is_value_type() {
    check(
        r#"
A<type label="Integer" kind="const"> = 1
"#,
    )
    .await;
}

#[tokio::test]
async fn constant_string_type_is_value_type() {
    check(
        r#"
NAME<type label="String" kind="const"> = "Ada"
"#,
    )
    .await;
}

#[tokio::test]
async fn constant_class_object_type_is_class_reference() {
    check(
        r#"
class User
end

MODEL<type label="Class<User>" kind="const"> = User
"#,
    )
    .await;
}

#[tokio::test]
async fn constant_path_literal_type_is_value_type() {
    check(
        r#"
module Foo
end

Foo::A<type label="Integer" kind="const"> = 1
"#,
    )
    .await;
}
