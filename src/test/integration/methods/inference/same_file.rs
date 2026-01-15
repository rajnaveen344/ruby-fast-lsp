//! Same-file method return type inference tests.
//!
//! Tests for inferring method return types within a single file.

use crate::test::harness::check;

/// Test inference from string literal
#[tokio::test]
async fn test_infer_string_literal() {
    check(
        r#"
class Foo
  def gree<type label="String">ting
    "hello"
  end
end
"#,
    )
    .await;
}

/// Test inference from integer literal
#[tokio::test]
async fn test_infer_integer_literal() {
    check(
        r#"
class Foo
  def cou<type label="Integer">nt
    42
  end
end
"#,
    )
    .await;
}

/// Test inference from array literal
#[tokio::test]
async fn test_infer_array_literal() {
    check(
        r#"
class Foo
  def ite<type label="Array">ms
    [1, 2, 3]
  end
end
"#,
    )
    .await;
}

/// Test inference from hash literal
#[tokio::test]
async fn test_infer_hash_literal() {
    check(
        r#"
class Foo
  def con<type label="Hash">fig
    { key: "value" }
  end
end
"#,
    )
    .await;
}

/// Test inference analyzes body, not YARD annotation
#[tokio::test]
async fn test_infer_body_not_yard() {
    check(
        r#"
class Foo
  # @return [CustomType]
  def val<type label="String">ue
    "actually a string"
  end
end
"#,
    )
    .await;
}
