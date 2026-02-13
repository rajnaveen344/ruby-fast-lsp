//! Rename tests for local variables.
//!
//! Tests for the rename refactoring capability for local variables.
//!
//! Note: Currently only supports local variables within a single scope.
//! Future: ScopeTree for proper capture handling in blocks.

use crate::test::harness::check;

/// Rename a local variable - basic case
#[tokio::test]
async fn rename_basic() {
    check(
        r#"
x = 1
puts x<rename to="counter">
"#,
    )
    .await;
}

/// Rename a local variable with multiple references
#[tokio::test]
async fn rename_multiple_references() {
    check(
        r#"
result = 10
result = result + 5
puts result<rename to="total">
"#,
    )
    .await;
}

/// Rename in method scope
#[tokio::test]
async fn rename_in_method() {
    check(
        r#"
def process
  data = fetch_data
  data.each do |item|
    puts item<rename to="item_data">
  end
end
"#,
    )
    .await;
}

/// Variable capture in block - the variable 'x' is defined in the method
/// and captured in the block. This should rename both the definition and the capture.
#[tokio::test]
async fn rename_captured_variable() {
    check(
        r#"
def example
  x = 1
  [1,2].each do |n|
    puts x<rename to="counter">
  end
end
"#,
    )
    .await;
}

/// Block parameter shadows method variable - should only rename the block param
#[tokio::test]
async fn rename_block_param_shadows() {
    check(
        r#"
def example
  x = 1
  [1,2].each do |x|
    puts x<rename to="item">
  end
end
"#,
    )
    .await;
}
