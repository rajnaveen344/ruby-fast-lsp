use crate::test::harness::check;

#[tokio::test]
async fn test_multiple_definitions_in_same_module() {
    // Current behavior: Returns first found type (String or Integer)
    // Desired behavior: Returns (Integer | String)
    check(
        r#"
module M
  # @return [String]
  def foo; end

  # @return [Integer]
  def foo; end
end

include M
foo<hover label="Integer | String">
"#,
    )
    .await;
}

#[tokio::test]
async fn test_multiple_modules_union() {
    // Current behavior: Returns one of them.
    // Desired behavior: Returns (Integer | String)
    check(
        r#"
module A
  # @return [String]
  def foo; end
end

module B
  # @return [Integer]
  def foo; end
end

class C
  include A
  include B
end

c = C.new
c.foo<hover label="Integer | String">
"#,
    )
    .await;
}
