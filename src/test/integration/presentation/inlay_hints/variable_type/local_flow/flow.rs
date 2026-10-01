//! Inlay hints for flow-sensitive variable type tracking.
//!
//! When a variable is reassigned, each assignment shows its type.

use crate::test::harness::check;

/// Multiple assignments show type at each position
#[tokio::test]
async fn flow_sensitive_reassignment() {
    check(
        r#"
a<hint label=": Integer"> = 1
b<hint label=": Float"> = 2.1
b<hint label=": Integer"> = a
c<hint label=": Integer"> = b
"#,
    )
    .await;
}
