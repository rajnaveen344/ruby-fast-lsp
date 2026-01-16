//! Hover tests for flow-sensitive type tracking.
//!
//! When a variable is reassigned, hover should show the type at that
//! specific position in the code flow.

use crate::test::harness::check;

/// Hover shows different types at different positions after reassignment
#[tokio::test]
async fn reassigned_variable() {
    check(
        r#"
aaa = 1
bbb = 2.1

puts bbb<hover label="Float">

bbb = aaa

puts bbb<hover label="Integer">
"#,
    )
    .await;
}
