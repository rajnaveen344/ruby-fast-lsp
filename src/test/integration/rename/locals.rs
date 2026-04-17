//! Rename tests for local variables.
//!
//! Uses AST-based rename with Prism's `depth` field.
//! All `<rename>` tags mark expected rename locations. The tag with `to="new_name"`
//! is the cursor position. Tests verify exact count and exact range matching.

use crate::test::harness::check;

// ── Basic cases ─────────────────────────────────────────────────────────

/// Rename a local variable - basic case
#[tokio::test]
async fn rename_basic() {
    check(
        r#"
<rename to="counter">x</rename> = 1
puts <rename>x</rename>
"#,
    )
    .await;
}

/// Rename a local variable with multiple references
#[tokio::test]
async fn rename_multiple_references() {
    check(
        r#"
<rename>result</rename> = 10
<rename>result</rename> = <rename>result</rename> + 5
puts <rename to="total">result</rename>
"#,
    )
    .await;
}

/// Rename from the definition site (cursor on write)
#[tokio::test]
async fn rename_from_definition() {
    check(
        r#"
<rename to="counter">x</rename> = 1
puts <rename>x</rename>
<rename>x</rename> = 2
"#,
    )
    .await;
}

/// Rename from a read site
#[tokio::test]
async fn rename_from_read() {
    check(
        r#"
<rename>x</rename> = 1
puts <rename to="counter">x</rename>
<rename>x</rename> = 2
"#,
    )
    .await;
}

// ── Method scope ────────────────────────────────────────────────────────

/// Rename in method scope
#[tokio::test]
async fn rename_in_method() {
    check(
        r#"
def process
  <rename to="input">data</rename> = fetch_data
  <rename>data</rename>.each do |item|
    puts item
  end
end
"#,
    )
    .await;
}

/// Sibling methods with same variable name — only rename in the targeted method
#[tokio::test]
async fn rename_sibling_methods_same_var_name() {
    check(
        r#"
def foo
  <rename to="counter">x</rename> = 1
  puts <rename>x</rename>
end

def bar
  x = 2
  puts x
end
"#,
    )
    .await;
}

// ── Block captures and shadowing ────────────────────────────────────────

/// Variable captured in block — rename both definition and captured usage
#[tokio::test]
async fn rename_captured_variable() {
    check(
        r#"
def example
  <rename to="counter">x</rename> = 1
  [1,2].each do |n|
    puts <rename>x</rename>
  end
end
"#,
    )
    .await;
}

/// Block parameter shadows method variable — only rename the block param
#[tokio::test]
async fn rename_block_param_shadows() {
    check(
        r#"
def example
  x = 1
  [1,2].each do |<rename to="item">x</rename>|
    puts <rename>x</rename>
  end
end
"#,
    )
    .await;
}

/// Rename the outer variable when a block param shadows it
#[tokio::test]
async fn rename_outer_when_block_param_shadows() {
    check(
        r#"
def example
  <rename to="counter">x</rename> = 1
  [1,2].each do |x|
    puts x
  end
  puts <rename>x</rename>
end
"#,
    )
    .await;
}

/// Nested blocks with capture (depth=2)
#[tokio::test]
async fn rename_nested_blocks_capture() {
    check(
        r#"
def example
  <rename to="counter">x</rename> = 1
  [1,2].each do |n|
    [3,4].each do |m|
      puts <rename>x</rename>
    end
  end
end
"#,
    )
    .await;
}

/// Deeply nested blocks (depth=3)
#[tokio::test]
async fn rename_deeply_nested_blocks() {
    check(
        r#"
def example
  <rename to="counter">x</rename> = 1
  [1].each do
    [2].each do
      [3].each do
        puts <rename>x</rename>
      end
    end
  end
end
"#,
    )
    .await;
}

// ── Parameters ──────────────────────────────────────────────────────────

/// Rename a method parameter
#[tokio::test]
async fn rename_method_parameter() {
    check(
        r#"
def greet(<rename to="user">name</rename>)
  puts "Hello, #{<rename>name</rename>}!"
  puts <rename>name</rename>.upcase
end
"#,
    )
    .await;
}

/// Method parameter used in nested block
#[tokio::test]
async fn rename_method_parameter_in_block() {
    check(
        r#"
def process(<rename to="data">items</rename>)
  <rename>items</rename>.each do |item|
    puts item
  end
  <rename>items</rename>.size
end
"#,
    )
    .await;
}

/// Block parameter (|item|) rename
#[tokio::test]
async fn rename_block_parameter() {
    check(
        r#"
def process
  data = fetch_data
  data.each do |<rename to="element">item</rename>|
    puts <rename>item</rename>
  end
end
"#,
    )
    .await;
}

// ── Compound assignments ────────────────────────────────────────────────

/// Compound operator assignments (+=)
#[tokio::test]
async fn rename_operator_write() {
    check(
        r#"
def example
  <rename to="counter">x</rename> = 0
  <rename>x</rename> += 1
  puts <rename>x</rename>
end
"#,
    )
    .await;
}

/// Or-assignment (||=)
#[tokio::test]
async fn rename_or_write() {
    check(
        r#"
def example
  <rename to="counter">x</rename> = nil
  <rename>x</rename> ||= 5
  puts <rename>x</rename>
end
"#,
    )
    .await;
}

/// And-assignment (&&=)
#[tokio::test]
async fn rename_and_write() {
    check(
        r#"
def example
  <rename to="counter">x</rename> = true
  <rename>x</rename> &&= false
  puts <rename>x</rename>
end
"#,
    )
    .await;
}

// ── Multi-assignment ────────────────────────────────────────────────────

/// Multi-assignment — only rename the targeted variable
#[tokio::test]
async fn rename_multi_assignment() {
    check(
        r#"
<rename to="first">a</rename>, b = 1, 2
puts <rename>a</rename>
puts b
"#,
    )
    .await;
}

// ── String interpolation ────────────────────────────────────────────────

/// Variable used in string interpolation
#[tokio::test]
async fn rename_in_string_interpolation() {
    check(
        r#"
def greet(<rename to="user">name</rename>)
  puts "Hello, #{<rename>name</rename>}!"
end
"#,
    )
    .await;
}

// ── Rescue variable ────────────────────────────────────────────────────

/// Rescue exception variable
#[tokio::test]
async fn rename_rescue_variable() {
    check(
        r#"
def example
  begin
    risky_operation
  rescue => <rename to="error">e</rename>
    puts <rename>e</rename>.message
  end
end
"#,
    )
    .await;
}

// ── For loop (no new scope) ─────────────────────────────────────────────

/// For loop — variables leak to enclosing scope
#[tokio::test]
async fn rename_for_loop_variable() {
    check(
        r#"
def example
  for <rename to="element">i</rename> in [1, 2, 3]
    puts <rename>i</rename>
  end
  puts <rename>i</rename>
end
"#,
    )
    .await;
}

// ── Class/module scope ──────────────────────────────────────────────────

/// Variable in class body — isolated scope
#[tokio::test]
async fn rename_in_class_body() {
    check(
        r#"
class Foo
  <rename to="counter">x</rename> = 1
  puts <rename>x</rename>
end
"#,
    )
    .await;
}

/// Same variable name in different class bodies — isolated
#[tokio::test]
async fn rename_isolated_class_scopes() {
    check(
        r#"
class Foo
  <rename to="counter">x</rename> = 1
  puts <rename>x</rename>
end

class Bar
  x = 2
  puts x
end
"#,
    )
    .await;
}
