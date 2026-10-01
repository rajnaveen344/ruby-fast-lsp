//! Inlay hints for value-constant assignments.

use crate::test::harness::check;

#[tokio::test]
async fn integer_literal_constant() {
    check(r#"A<hint label="Integer"> = 1"#).await;
}

#[tokio::test]
async fn string_literal_constant() {
    check(r#"B<hint label="String"> = "a""#).await;
}

#[tokio::test]
async fn nested_module_constant() {
    check(
        r#"
module M
  C<hint label="Integer"> = 1
end
"#,
    )
    .await;
}

#[tokio::test]
async fn constant_path_write() {
    check(
        r#"
module Foo
end

Foo::A<hint label="Integer"> = 1
"#,
    )
    .await;
}

#[tokio::test]
async fn class_reference_constant() {
    check(
        r#"
class User
end

MODEL<hint label="Class<User>"> = User
"#,
    )
    .await;
}

#[tokio::test]
async fn class_declaration_has_no_type_inlay() {
    // End-label hints on `end` are fine; class names must not get `: Type` inlays.
    check(
        r#"
class <hint none>Foo</hint>
end
"#,
    )
    .await;
}

#[tokio::test]
async fn unknown_rhs_has_no_constant_hint() {
    check(
        r#"
<hint none>
DYN = some_unknown_method
</hint>
"#,
    )
    .await;
}

#[tokio::test]
async fn constant_or_write_literal() {
    check(r#"A<hint label="Integer"> ||= 1"#).await;
}

#[tokio::test]
async fn constant_and_write_literal() {
    check(r#"A<hint label="String"> &&= "x""#).await;
}

#[tokio::test]
async fn constant_operator_write_literal() {
    check(r#"A<hint label="Integer"> += 1"#).await;
}

#[tokio::test]
async fn constant_path_or_write_literal() {
    check(
        r#"
module Foo
end

Foo::A<hint label="Integer"> ||= 1
"#,
    )
    .await;
}

#[tokio::test]
async fn multi_assign_constants() {
    check(r#"A<hint label="Integer">, B<hint label="String"> = 1, "x""#).await;
}

#[tokio::test]
async fn multi_assign_constants_from_array() {
    check(r#"A<hint label="Integer">, B<hint label="String"> = [1, "x"]"#).await;
}
