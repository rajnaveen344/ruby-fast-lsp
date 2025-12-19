//! Find references tests for classes and modules.

use crate::test::harness::check_references;

/// Find references for a class.
#[tokio::test]
async fn references_class() {
    check_references(
        r#"
class <ref>Foo$0</ref>
end

<ref>Foo</ref>.new
x = <ref>Foo</ref>.new
"#,
    )
    .await;
}

/// Find references for a module.
#[tokio::test]
async fn references_module() {
    check_references(
        r#"
module <ref>MyMod$0</ref>
end

include <ref>MyMod</ref>
extend <ref>MyMod</ref>
"#,
    )
    .await;
}
