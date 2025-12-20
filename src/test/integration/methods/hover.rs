//! Hover tests for methods.

use crate::test::harness::check;

#[tokio::test]
async fn test_hover_method_return_type_yard() {
    check(
        r#"
class Foo
  # @return [String]
  def bar<hover label="String">
    "hello"
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn test_hover_method_call() {
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

/// Test hover shows return type for variable assigned from method chain.
#[tokio::test]
async fn test_hover_method_chain_result() {
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
