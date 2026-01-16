//! Inlay hints for implicit return statements.
//!
//! Shows "return" before values that are implicitly returned.

use crate::test::harness::check;

/// Implicit return in if/else branches
#[tokio::test]
async fn if_else_branches() {
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
}

/// Implicit return in case/when branches
#[tokio::test]
async fn case_when_branches() {
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
