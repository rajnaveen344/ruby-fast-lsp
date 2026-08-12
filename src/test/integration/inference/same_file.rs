//! Same-file method return type inference tests.
//!
//! Tests for inferring method return types within a single file.

use crate::test::harness::{check, FakeEditor};

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
  def con<type label="{ key: String }">fig
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

/// A direct recursive call is a type variable, not missing evidence. The
/// least fixed point of `Integer | count_down(...)` is exactly `Integer`.
#[tokio::test]
async fn direct_recursive_return_reaches_a_proven_fixed_point() {
    check(
        r#"
class Counter
  # @param n [Integer]
  def count_down(n)<hint label=" -> Integer">
    return 0 if n == 0
    count_down(n - 1)
  end
end

Counter.new.count_down<hover label="Integer">(2)
"#,
    )
    .await;
}

/// Return equations must be solved as a call-graph component rather than in
/// source order. Both methods have a concrete terminating base, so the least
/// fixed point of `TrueClass | odd(...)` and `FalseClass | even(...)` is the
/// exhaustive Boolean union for both methods.
#[tokio::test]
async fn mutually_recursive_returns_reach_a_proven_fixed_point() {
    check(
        r#"
class Parity
  # @param n [Integer]
  def even(n)<hint label=" -> (FalseClass | TrueClass)">
    return true if n == 0
    odd(n - 1)
  end

  # @param n [Integer]
  def odd(n)<hint label=" -> (FalseClass | TrueClass)">
    return false if n == 0
    even(n - 1)
  end
end

Parity.new.even<hover label="(FalseClass | TrueClass)">(4)
Parity.new.odd<hover label="(FalseClass | TrueClass)">(3)
"#,
    )
    .await;
}

#[tokio::test]
async fn mutual_recursion_edit_lifecycle_refreshes_chained_resolution() {
    let proven = r#"
class Cycle
  def left(n)
    return "left" if n == 0
    right(n - 1)
  end

  def right(n)
    return "right" if n == 0
    left(n - 1)
  end
end

Cycle.new.left(2).upcase
"#;
    let mut editor = FakeEditor::new().await;
    editor.open("main.rb", proven).await;
    editor
        .check(
            "main.rb",
            r#"
class Cycle
  def left(n)<hint label=" -> String">
    return "left" if n == 0
    right(n - 1)
  end

  def right(n)<hint label=" -> String">
    return "right" if n == 0
    left(n - 1)
  end
end

Cycle.new.left(2).upcase<hover label="String">
"#,
        )
        .await;

    let base_free = r#"
class Cycle
  def left
    right
  end

  def right
    left
  end
end

Cycle.new.left.
"#;
    editor.set("main.rb", base_free).await;
    editor
        .check(
            "main.rb",
            r#"
class Cycle
  def left<hint label=" -> ?">
    right
  end

  def right<hint label=" -> ?">
    left
  end
end

Cycle.new.left.$0
<complete excludes="upcase,abs">
"#,
        )
        .await;

    let reproven = r#"
class Cycle
  def left(n)
    return 1 if n == 0
    right(n - 1)
  end

  def right(n)
    return 2 if n == 0
    left(n - 1)
  end
end

Cycle.new.left(2).abs
"#;
    editor.set("main.rb", reproven).await;
    editor
        .check(
            "main.rb",
            r#"
class Cycle
  def left(n)<hint label=" -> Integer">
    return 1 if n == 0
    right(n - 1)
  end

  def right(n)<hint label=" -> Integer">
    return 2 if n == 0
    left(n - 1)
  end
end

Cycle.new.left(2).abs<hover label="Integer">
"#,
        )
        .await;
}
