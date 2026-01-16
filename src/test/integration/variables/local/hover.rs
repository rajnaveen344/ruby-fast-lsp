//! Hover tests for variables.

use crate::test::harness::check;

#[tokio::test]
async fn test_hover_local_string() {
    check(r#"x<hover label="String"> = "hello""#).await;
}

#[tokio::test]
async fn test_hover_local_integer() {
    check(r#"x<hover label="Integer"> = 42"#).await;
}

#[tokio::test]
async fn test_hover_local_float() {
    check(r#"x<hover label="Float"> = 3.14"#).await;
}

// TODO: This test requires enhanced type tracking where variables holding class
// references (like `a = Klass`) are tracked as Class<Klass> type
#[ignore]
#[tokio::test]
async fn test_hover_class_in_variable() {
    check(
        r#"
class Klass; end
a<hover label="class Klass"> = Klass
k = a.new
"#,
    )
    .await;
}

/// Test hover on local variable with Unknown type shows "?"
/// This ensures hover is consistent with inlay hints for Unknown types.
#[tokio::test]
async fn test_hover_local_variable_unknown_type() {
    check(
        r#"
class Service
  def fetch_data
    result = some_external_api.get_data
    res<hover label="?">ult
  end
end
"#,
    )
    .await;
}

/// Test hover on local variable accessed inside a block (closure semantics).
/// Variables defined in enclosing scope should be accessible inside blocks.
#[tokio::test]
async fn test_hover_local_variable_in_block() {
    check(
        r#"
class Processor
  def process
    data = "hello"
    items = [1, 2, 3]
    items.each { |item| puts da<hover label="String">ta }
  end
end
"#,
    )
    .await;
}

/// Test hover on local variable inside nested blocks.
#[tokio::test]
async fn test_hover_local_variable_in_nested_block() {
    check(
        r#"
class Calculator
  def calculate
    total = 0
    [1, 2].each do |x|
      [3, 4].each do |y|
        puts to<hover label="Integer">tal
      end
    end
  end
end
"#,
    )
    .await;
}

/// Test that variables don't leak across method boundaries (hard scope).
/// Ruby's scoping rules: variables in one method are NOT accessible in another.
#[tokio::test]
async fn test_hover_variable_does_not_cross_method_boundary() {
    check(
        r#"
class Example
  def first_method
    secret = "password"
  end

  def second_method
    # secret should NOT be accessible here - it's in a different method
    # Hover should show just the variable name (not found), not the type from first_method
    sec<hover label="secret">ret
  end
end
"#,
    )
    .await;
}
