//! Tests for implicit return inlay hints.

use crate::test::harness::check;

#[tokio::test]
async fn test_implicit_return_hint_placement() {
    // Current behavior: hint is placed before 'if'
    // <hint label="return"> checks for the presence of the hint at that position.
    check(
        r#"
class A
  def foo
    if true
      <hint label="return">1
    else
      <hint label="return">2
    end
  end
end
"#,
    )
    .await;

    check(
        r#"
class B
  def bar
    case 1
    when 1
      <hint label="return">10
    else
      <hint label="return">20
    end
  end
end
"#,
    )
    .await;
}
