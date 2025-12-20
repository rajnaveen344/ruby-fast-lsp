//! Hover tests for modules.

use crate::test::harness::check;

#[tokio::test]
async fn test_hover_module_definition() {
    check(
        r#"
module MyModule<hover label="module MyModule">
end
"#,
    )
    .await;
}

#[tokio::test]
async fn test_hover_module_reference() {
    check(
        r#"
module Foo; end
class Bar
  include Foo<hover label="module Foo">
end
"#,
    )
    .await;
}
