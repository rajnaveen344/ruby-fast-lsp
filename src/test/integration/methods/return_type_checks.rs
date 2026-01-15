//! Tests for return type inference and diagnostics.

use crate::test::harness::check;

#[tokio::test]
async fn test_explicit_return_mismatch() {
    check(
        r#"
class A
  # @return [String]
  def foo
    <warn message="Expected return type String, but found Integer">return 1</warn>
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn test_implicit_return_mismatch() {
    check(
        r#"
class A
  # @return [String]
  def foo
    <warn message="Expected return type String, but found Integer">1</warn>
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn test_valid_return() {
    check(
        r#"
<err none>
class A
  # @return [Integer]
  def foo
    1
  end
end
</err>
"#,
    )
    .await;
}
