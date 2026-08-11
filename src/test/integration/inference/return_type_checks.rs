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

#[tokio::test]
async fn unknown_declared_return_type_does_not_prove_a_mismatch() {
    check(
        r#"
class A
  # @return [?]
  def foo
    <warn none code="declared-return-type-mismatch">nil</warn>
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn unknown_nested_return_type_does_not_prove_a_mismatch() {
    check(
        r#"
class A
  # @return [Array<Integer>]
  def foo
    <warn none code="declared-return-type-mismatch">return 1, dynamic_value</warn>
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn inferred_return_type_is_not_treated_as_a_declaration() {
    check(
        r#"
class A
  def foo
    value = "value"
    <warn none code="declared-return-type-mismatch">return value</warn>
  end
end
"#,
    )
    .await;
}
