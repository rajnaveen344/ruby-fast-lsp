//! Hover tests for module constants.

use crate::test::harness::check;

/// Hover on module definition shows "module ModuleName"
#[tokio::test]
async fn module_definition() {
    check(
        r#"
module MyModule<hover label="module MyModule">
end
"#,
    )
    .await;
}

/// Hover on module reference shows "module ModuleName"
#[tokio::test]
async fn module_reference() {
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
