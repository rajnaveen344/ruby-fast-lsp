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

/// Test inference from same-file method call with YARD
#[tokio::test]
#[ignore = "Requires CFG-based return type inference"]
async fn test_infer_from_method_call_with_yard() {
    check(
        r#"
class Foo
  # @return [String]
  def name
    "foo"
  end

  def get_na<type label="String">me
    name
  end
end
"#,
    )
    .await;
}

/// Test inference from same-file method call without YARD (recursive inference)
#[tokio::test]
#[ignore = "Requires CFG-based return type inference"]
async fn test_infer_from_method_call_no_yard() {
    check(
        r#"
class Foo
  def inner
    "hello"
  end

  def out<type label="String">er
    inner
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

/// Test inference through multiple same-file method calls
#[tokio::test]
#[ignore = "Requires CFG-based return type inference"]
async fn test_infer_chained_same_file() {
    check(
        r#"
class Foo
  def level_1
    "deep"
  end

  def level_2
    level_1
  end

  def level<type label="String">_3
    level_2
  end
end
"#,
    )
    .await;
}
