//! Hover on receiver calls whose type comes from YARD, block bodies, `method_missing`, or edited bodies.

use crate::test::harness::{check, FakeEditor};

/// Hover on method call shows return type from YARD
#[tokio::test]
async fn method_call_yard_return_type() {
    check(
        r#"
class Foo
  # @return [Integer]
  def count
    42
  end
end
x = Foo.new.count<hover label="Integer">
"#,
    )
    .await;
}

/// Variable assigned from method call shows return type
#[tokio::test]
async fn variable_from_method_call() {
    check(
        r#"
class Builder
  # @return [Product]
  def build
    Product.new
  end
end

class Product
end

product<hover label="Product"> = Builder.new.build
"#,
    )
    .await;
}

#[tokio::test]
async fn lambda_call_uses_block_return_type() {
    check(
        r#"
builder = -> { "ready" }
builder.call<hover label="String">
"#,
    )
    .await;
}

#[tokio::test]
async fn proc_call_uses_block_return_type() {
    check(
        r#"
builder = Proc.new { 1 }
builder.call<hover label="Integer">
"#,
    )
    .await;
}

#[tokio::test]
async fn method_missing_call_uses_fallback_return_type() {
    check(
        r#"
class DynamicRecord
  # @return [String]
  def method_missing(name, *args)
    "dynamic"
  end
end

DynamicRecord.new.virtual_total<hover label="String">
"#,
    )
    .await;
}

#[tokio::test]
async fn call_expression_proof_outcome_is_replaced_after_edit() {
    let mut editor = FakeEditor::new().await;
    editor
        .open_and_check_fixture(
            "call_outcome_lifecycle.rb",
            r#"class User
  def profile
    dynamic_profile
  end
end

User.new.profile<hover label="Unknown[unresolved_method_return]">
"#,
        )
        .await;

    editor
        .set_and_check_fixture(
            "call_outcome_lifecycle.rb",
            r#"class User
  def profile
    "ready"
  end
end

User.new.profile<hover label="String">
"#,
        )
        .await;

    editor
        .set_and_check_fixture(
            "call_outcome_lifecycle.rb",
            r#"class User
  def profile
    dynamic_profile
  end
end

User.new.profile<hover label="Unknown[unresolved_method_return]">
"#,
        )
        .await;
}
