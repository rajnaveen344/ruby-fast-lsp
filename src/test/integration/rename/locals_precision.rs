//! Rename precision tests.
//!
//! Tests ensuring rename is precise and doesn't affect unrelated symbols
//! with the same name (e.g. method calls vs local variables).

use crate::test::harness::check;

/// Ensure renaming a local variable doesn't rename a method call with the same name
#[tokio::test]
async fn rename_excludes_method_calls() {
    check(
        r#"
def example
  <rename to="counter">count</rename> = 0
  puts <rename>count</rename>
  obj.count
end
"#,
    )
    .await;
}

/// Mimic the user's screenshot exactly
#[tokio::test]
async fn rename_excludes_receiver_calls_mimic_screenshot() {
    check(
        r#"
def test
  abc = Object.new
  <rename to="counter">aks</rename> = 1
  puts <rename>aks</rename>
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
  <rename to="counter">count</rename> = 0
  puts <rename>count</rename>
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
<rename to="y">x</rename> = 1
puts <rename>x</rename>
puts :x
"#,
    )
    .await;
}
