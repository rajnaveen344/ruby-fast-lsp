//! Rename precision tests.
//!
//! Tests ensuring rename is precise and doesn't affect unrelated symbols
//! with the same name (e.g. method calls vs local variables).

use crate::test::harness::check;

/// Ensure renaming a local variable doesn't rename a method call with the same name
#[tokio::test]
async fn rename_excludes_method_calls() {
    // Should rename 'count = 0' and 'puts count' -> 2 changes
    // 'obj.count' should remain untouched
    check(
        r#"
def example
  count = 0
  puts count<rename to="counter">
  obj.count
end
"#,
    )
    .await;
}

/// Mimic the user's screenshot exactly
#[tokio::test]
async fn rename_excludes_receiver_calls_mimic_screenshot() {
    // Should rename 'aks = 1' and 'puts aks' -> 2 changes
    // 'abc.aks' should remain untouched
    check(
        r#"
def test
  abc = Object.new
  aks = 1
  puts aks<rename to="counter">
  puts abc.aks
end
"#,
    )
    .await;
}

/// Ensure renaming a local variable doesn't rename a method definition with the same name
#[tokio::test]
async fn rename_excludes_method_definitions() {
    check(
        r#"
def count
  "method"
end

def example
  count = 0
  puts count<rename to="counter">
end
"#,
    )
    .await;
}

/// Ensure renaming a local variable doesn't rename a symbol literal
#[tokio::test]
async fn rename_excludes_symbols() {
    check(
        r#"
x = 1
puts x<rename to="y">
puts :x
"#,
    )
    .await;
}
