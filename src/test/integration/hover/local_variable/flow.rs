//! Hover tests for flow-sensitive type tracking.
//!
//! When a variable is reassigned, hover should show the type at that
//! specific position in the code flow.

use crate::test::harness::{check, FakeEditor};

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

#[tokio::test]
async fn hash_alias_mutation_updates_every_local_hover() {
    check(
        r#"
def build
  payload = { count: 1, state: :ready }
  copy = payload
  copy[:count] = "many"
  payload<hover label="{ count: String, state: :ready }">
end
"#,
    )
    .await;
}

#[tokio::test]
async fn mutable_hash_escape_and_reproof_replace_exact_flow_evidence() {
    let mut editor = FakeEditor::new().await;
    editor
        .open_and_check_fixture(
            "shape_flow_lifecycle.rb",
            r#"def build
  payload = { count: 1 }
  copy = payload
  copy[:count] = "many"
  payload<hover label="{ count: String }">
end
"#,
        )
        .await;

    editor
        .set_and_check_fixture(
            "shape_flow_lifecycle.rb",
            r#"def build
  payload = { count: 1 }
  copy = payload
  dynamic_sink(copy)
  payload<hover label="Unknown[mutable_shape_invalidated]">
end
"#,
        )
        .await;

    editor
        .set_and_check_fixture(
            "shape_flow_lifecycle.rb",
            r#"def build
  payload = { count: 1 }
  copy = payload
  copy[:count] = true
  payload<hover label="{ count: TrueClass }">
end
"#,
        )
        .await;
}
