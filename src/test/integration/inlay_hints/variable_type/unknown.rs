//! Inlay hints for variables with Unknown type.
//!
//! Unknown types display as ": ?" to indicate type couldn't be inferred.

use crate::test::harness::check;

/// Variable from unknown method shows ": ?"
#[tokio::test]
async fn unknown_method_result() {
    check(
        r#"
def foo
  x<hint label=": ?"> = some_unknown_method
end
"#,
    )
    .await;
}

/// Variable assigned from another unknown variable
#[tokio::test]
async fn variable_to_variable_unknown() {
    check(
        r#"
def foo
  x = unknown_thing
  y<hint label=": ?"> = x
end
"#,
    )
    .await;
}
