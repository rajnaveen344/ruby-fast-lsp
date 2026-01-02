//! Tests for return type inference when variables are modified in branches.
//!
//! These tests verify that when a variable is assigned different types in different
//! branches (e.g., if/else), the return type correctly reflects the union of all
//! possible types.
//!
//! Also includes tests for goto definition on array methods with inferred receiver types.

use crate::test::harness::*;

/// Test case from first image: variable set to false, then conditionally to true.
/// The return type should be (FalseClass | TrueClass) or equivalently bool.
#[tokio::test]
async fn test_boolean_variable_modified_in_if_branch() {
    let _ = env_logger::builder().is_test(true).try_init();

    check(
        r#"
def is_limit_reached?<hint label=" -> (FalseClass | TrueClass)">
  limit_reached = false
  if some_condition
    limit_reached = true
  end
  limit_reached
end
"#,
    )
    .await;
}

/// Simpler version: explicit return of the modified variable.
#[tokio::test]
async fn test_boolean_variable_explicit_return() {
    let _ = env_logger::builder().is_test(true).try_init();

    check(
        r#"
def check_flag?<hint label=" -> (FalseClass | TrueClass)">
  flag = false
  if true
    flag = true
  end
  return flag
end
"#,
    )
    .await;
}

/// Test with if/else where both branches modify the variable.
#[tokio::test]
async fn test_boolean_variable_modified_in_both_branches() {
    let _ = env_logger::builder().is_test(true).try_init();

    check(
        r#"
def get_status<hint label=" -> (FalseClass | TrueClass)">
  result = false
  if condition
    result = true
  else
    result = false
  end
  result
end
"#,
    )
    .await;
}

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

/// Test array variable return type (from second image).
/// When returning an array that was built up, should show Array<Type>.
#[tokio::test]
async fn test_array_variable_return() {
    let _ = env_logger::builder().is_test(true).try_init();

    check(
        r#"
def get_dates<hint label=" -> Array<?>">
  dates = []
  dates << "2024-01-01"
  dates
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

/// This test simulates the real-world scenario from shipping_label_machine.rb
/// Multiple classes define <<, but only Array#<< should be found because
/// the receiver `available_pickup_dates` is assigned from an array literal.
#[tokio::test]
async fn test_goto_array_shovel_with_multiple_definitions() {
    let _ = env_logger::builder().is_test(true).try_init();

    check(
        r#"
class Array
  <def>def <<(item)
    # Array's shovel operator
  end</def>
end

class CSV
  def <<(row)
    # CSV's shovel operator
  end
end

class IO
  def <<(obj)
    # IO's shovel operator
  end
end

class String
  def <<(other)
    # String's shovel operator
  end
end

def next_available_pickup_dates(country_code)
  available_pickup_dates = []

  current_time = Time.now
  available_pickup_dates <<$0 current_time

  available_pickup_dates
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
