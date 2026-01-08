use crate::test::harness::check;

#[tokio::test]
async fn unknown_method_assignment() {
    check(
        r#"
def foo
  # We assign an unknown method result to a variable
  # It should show : ? because we don't know the type
  x<hint label=": ?"> = some_unknown_method
end
"#,
    )
    .await;
}

#[tokio::test]
async fn variable_to_variable_unknown() {
    check(
        r#"
def foo
  x = unknown_thing
  # y should inherit unknown from x and also show : ?
  y<hint label=": ?"> = x
end
"#,
    )
    .await;
}
