//! Hover tests for local variable scope handling.
//!
//! Tests Ruby's scoping rules:
//! - Blocks CAN access variables from enclosing scope (soft boundary)
//! - Methods/Classes CANNOT access outer local vars (hard boundary)

use crate::test::harness::check;

/// Variable accessed inside a block (closure semantics)
#[tokio::test]
async fn variable_in_block() {
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

/// Variable accessed inside nested blocks
#[tokio::test]
async fn variable_in_nested_block() {
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

/// Variables don't cross method boundaries (hard scope)
#[tokio::test]
async fn variable_does_not_cross_method_boundary() {
    check(
        r#"
class Example
  def first_method
    secret = "password"
  end

  def second_method
    # secret is NOT accessible here - different method scope
    sec<hover label="secret">ret
  end
end
"#,
    )
    .await;
}
