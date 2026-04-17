//! Tests for `nil-call` diagnostic.
//!
//! V1: flag method calls where the receiver is a local variable whose
//! tracked type at the call position contains `NilClass`.
//!
//! Conservative trade-off: we currently rely on `VariableScopes` (assignment-
//! time types only — guard narrowing isn't reflected). To avoid false positives
//! on guard-narrowed code, V1 flags only when the variable type is *exactly*
//! `NilClass` (definitely nil, not just possibly nil). Nilable unions stay
//! silent until narrowing flows through `VariableScopes`.

use crate::test::harness::check;

#[tokio::test]
async fn definite_nil_local_warns() {
    check(
        r#"
x = nil
x.<warn code="nil-call">upcase</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn non_nil_local_no_warn() {
    check(
        r#"
<warn none code="nil-call">
x = "hello"
x.upcase
</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn definite_nil_chained_warns_first_link_only() {
    // Once flagged on .upcase, downstream .reverse stays silent — same
    // rationale as expr-receiver chain noise suppression.
    check(
        r#"
x = nil
x.<warn code="nil-call">upcase</warn>.<warn none code="nil-call">reverse</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn nil_reassignment_clears_the_warn() {
    // After reassignment to non-nil, no warning at the later call.
    check(
        r#"
x = nil
x = "hello"
<warn none code="nil-call">x.upcase</warn>
"#,
    )
    .await;
}
