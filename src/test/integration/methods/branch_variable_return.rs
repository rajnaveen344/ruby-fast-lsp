//! Tests for return type inference when variables are modified in branches.
//!
//! These tests verify that when a variable is assigned different types in different
//! branches (e.g., if/else), the return type correctly reflects the union of all
//! possible types.
//!
//! Also includes tests for goto definition on array methods with inferred receiver types.

use crate::test::harness::*;

/// Test that simple true return works correctly.
#[tokio::test]
async fn test_simple_true_return() {
    let _ = env_logger::builder().is_test(true).try_init();

    check(
        r#"
def always_true<hint label=" -> TrueClass">
  true
end
"#,
    )
    .await;
}

/// Test that simple false return works correctly.
#[tokio::test]
async fn test_simple_false_return() {
    let _ = env_logger::builder().is_test(true).try_init();

    check(
        r#"
def always_false<hint label=" -> FalseClass">
  false
end
"#,
    )
    .await;
}

// ============================================================================
// Goto Definition on Array Methods with Inferred Receiver Type
// ============================================================================

/// Goto definition on << operator should resolve to Array#<< when receiver is an array.
/// This tests that type inference correctly identifies the array type for method lookup.
/// Uses actual Ruby syntax sugar: `items << "hello"` instead of `items.<<("hello")`
#[tokio::test]
async fn test_goto_array_shovel_operator() {
    let _ = env_logger::builder().is_test(true).try_init();

    check(
        r#"
class Array
  <def>def <<(item)
    # stub
  end</def>
end

def collect_items
  items = []
  items <<$0 "hello"
end
"#,
    )
    .await;
}

/// Goto definition on push method for array variable.
#[tokio::test]
async fn test_goto_array_push_method() {
    let _ = env_logger::builder().is_test(true).try_init();

    check(
        r#"
class Array
  <def>def push(item)
    # stub
  end</def>
end

def collect_items
  items = []
  items.push$0("hello")
end
"#,
    )
    .await;
}

// NOTE: Array indexing `items[0]` is parsed as IndexTargetNode, not CallNode,
// so goto definition on `[]` with syntax sugar doesn't currently work.
// This is a known limitation that could be addressed in the future.

/// Goto definition on + operator for Integer.
#[tokio::test]
async fn test_goto_integer_plus_operator() {
    let _ = env_logger::builder().is_test(true).try_init();

    check(
        r#"
class Integer
  <def>def +(other)
    # stub
  end</def>
end

def add_numbers
  x = 1
  x +$0 2
end
"#,
    )
    .await;
}
