//! Flow-sensitive hover tests - verifies hover shows correct type at each position.

use crate::test::harness::check;

/// Test that hover shows different types at different positions for reassigned variable.
/// bbb is Float at line 6, then Integer at line 12.
#[tokio::test]
async fn test_hover_flow_sensitive_variable() {
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
