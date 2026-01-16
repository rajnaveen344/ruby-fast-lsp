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

/// Test hover on chained method calls shows correct types at each step.
#[tokio::test]
async fn test_hover_chained_method_calls() {
    check(
        r#"
class User
  # @return [Profile]
  def profile
    Profile.new
  end
end

class Profile
  # @return [String]
  def name
    "John"
  end
end

user = User.new
user.profile<hover label="Profile">.name<hover label="String">
"#,
    )
    .await;
}

/// Test hover on array method calls (RBS types).
#[tokio::test]
async fn test_hover_array_methods() {
    check(
        r#"
arr = [1, 2, 3]
arr.length<hover label="Integer">
"#,
    )
    .await;
}

/// Test hover on hash method calls (RBS types).
#[tokio::test]
async fn test_hover_hash_methods() {
    check(
        r#"
hash = { a: 1 }
hash.keys<hover label="Array">
"#,
    )
    .await;
}

/// Test hover on method call with implicit self (inside class).
#[tokio::test]
async fn test_hover_implicit_self_method_call() {
    check(
        r#"
class Calculator
  # @return [Integer]
  def add(a, b)
    a + b
  end

  def compute
    add<hover label="Integer">(1, 2)
  end
end
"#,
    )
    .await;
}

/// Test hover on method call returns Unknown when chain breaks.
#[tokio::test]
async fn test_hover_chain_unknown_propagation() {
    check(
        r#"
class Foo
  def unknown_method
    bar  # bar is undefined, returns unknown
  end
end

x = Foo.new.unknown_method<hover label="def unknown_method">
"#,
    )
    .await;
}
