//! Find references tests for constants.

use crate::test::harness::check_references;

/// Find references for a constant.
#[tokio::test]
async fn references_constant() {
    check_references(
        r#"
VALUE = 42

puts <ref>VALUE$0</ref>
x = <ref>VALUE</ref>
"#,
    )
    .await;
}

/// Find references for qualified constant.
#[tokio::test]
async fn references_qualified_constant() {
    check_references(
        r#"
module Alpha
  BETA = 100
end

puts <ref>Alpha::BETA$0</ref>
x = <ref>Alpha::BETA</ref>
"#,
    )
    .await;
}
