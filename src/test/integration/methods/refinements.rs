use crate::test::harness::check;

#[tokio::test]
async fn test_hover_array_mixed() {
    // Should show Array<Integer | String>
    check(r#"x<hover label="Array<Integer | String>"> = [1, "s"]"#).await;
}

#[tokio::test]
async fn test_hover_new_method() {
    // Hovering over .new should show the class instance type "Foo"
    check(
        r#"
class Foo; end
Foo.new<hover label="Foo">
"#,
    )
    .await;
}

#[tokio::test]
async fn test_hover_method_return_type_only() {
    // User requested "return the type only"
    // Assuming this means just "String" instead of "def foo -> String"
    check(
        r#"
class Foo
  # @return [String]
  def bar
    "s"
  end
end
Foo.new.bar<hover label="String">
"#,
    )
    .await;
}
