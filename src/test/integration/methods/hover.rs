//! Hover tests for methods.

use crate::test::harness::check;

/// Test hover on method parameter shows YARD type
#[tokio::test]
async fn test_hover_method_parameter_yard_type() {
    check(
        r#"
class Foo
  # @param request_info [Hash] the request data
  def process(request_info<hover label="Hash">)
    request_info
  end
end
"#,
    )
    .await;
}

/// Test hover on method parameter usage inside method body
#[tokio::test]
async fn test_hover_method_parameter_usage_in_body() {
    check(
        r#"
class Foo
  # @param name [String] the user name
  def greet(name)
    puts name<hover label="String">
  end
end
"#,
    )
    .await;
}

/// Test hover on method parameter inside if condition
#[tokio::test]
async fn test_hover_method_parameter_in_if_condition() {
    check(
        r#"
class Foo
  # @param request_info [Hash] the request data
  def process(request_info)
    if reque<hover label="Hash">st_info[:valid]
      puts "valid"
    end
  end
end
"#,
    )
    .await;
}

/// Test hover on method parameter with alternative YARD format @param[Type] name
#[tokio::test]
async fn test_hover_method_parameter_alt_yard_format() {
    check(
        r#"
class Foo
  # @param[Hash] request_info the request data
  def process(request_info)
    reque<hover label="Hash">st_info
  end
end
"#,
    )
    .await;
}

/// Test hover on method parameter without YARD doc shows Unknown (?)
#[tokio::test]
async fn test_hover_method_parameter_no_yard_doc() {
    check(
        r#"
class Foo
  def process(request_info)
    reque<hover label="?">st_info if request_<hover label="?">info[:status]
  end
end
"#,
    )
    .await;
}

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

/// Test hover on method call triggers cross-file type lookup.
/// The helper method is defined in a different file and should be resolved.
#[tokio::test]
async fn test_hover_cross_file_method_inference() {
    use crate::test::harness::check_multi_file;

    // Primary file (main.rb) - where we hover
    // Helper file provides the method definition with YARD return type
    check_multi_file(&[
        (
            "main.rb",
            r#"
class Main
  def greet
    Helper.get_name<hover label="String">
  end

  puts gre<hover label="String">et
end
"#,
        ),
        (
            "helper.rb",
            r#"
class Helper
  # @return [String]
  def self.get_name
    "hello"
  end
end
"#,
        ),
    ])
    .await;
}
