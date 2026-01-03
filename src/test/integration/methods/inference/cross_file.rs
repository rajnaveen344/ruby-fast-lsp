//! Cross-file method return type inference tests.
//!
//! Tests for inferring method return types when methods call methods in other files.

use crate::test::harness::check_multi_file;

/// Test: method_a.rb defines method, main.rb calls it
#[tokio::test]
async fn test_main_calls_helper() {
    check_multi_file(&[
        (
            "main.rb",
            r#"
class Main
  def gre<type label="String">et
    Helper.get_name
  end
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

/// Test: method_a.rb calls method_b.rb, main.rb calls method_a
#[tokio::test]
async fn test_main_calls_a_calls_b() {
    check_multi_file(&[
        (
            "main.rb",
            r#"
class Main
  def run<type label="String">
    ServiceA.process
  end
end
"#,
        ),
        (
            "service_a.rb",
            r#"
class ServiceA
  # @return [String]
  def self.process
    ServiceB.fetch_data
  end
end
"#,
        ),
        (
            "service_b.rb",
            r#"
class ServiceB
  # @return [String]
  def self.fetch_data
    "data"
  end
end
"#,
        ),
    ])
    .await;
}

/// Test: class method returning value from another file
#[tokio::test]
async fn test_class_method_cross_file() {
    check_multi_file(&[
        (
            "main.rb",
            r#"
class Main
  def get_user_na<type label="String">me
    User.default_name
  end
end
"#,
        ),
        (
            "user.rb",
            r#"
class User
  # @return [String]
  def self.default_name
    "John"
  end
end
"#,
        ),
    ])
    .await;
}

/// Test: class method returning instance, then calling instance method
#[tokio::test]
async fn test_factory_pattern_cross_file() {
    check_multi_file(&[
        (
            "main.rb",
            r#"
class Main
  def build_and_na<type label="String">me
    UserFactory.create.name
  end
end
"#,
        ),
        (
            "user_factory.rb",
            r#"
class UserFactory
  # @return [User]
  def self.create
    User.new
  end
end
"#,
        ),
        (
            "user.rb",
            r#"
class User
  # @return [String]
  def name
    "Created User"
  end
end
"#,
        ),
    ])
    .await;
}

/// Test: method without YARD inferred from cross-file call
#[tokio::test]
async fn test_infer_without_yard_cross_file() {
    check_multi_file(&[
        (
            "main.rb",
            r#"
class Main
  def wrap<type label="String">per
    Helper.get_value
  end
end
"#,
        ),
        (
            "helper.rb",
            r#"
class Helper
  # @return [String]
  def self.get_value
    "value"
  end
end
"#,
        ),
    ])
    .await;
}

/// Test: multiple methods in same class calling different cross-file methods
#[tokio::test]
async fn test_multiple_methods_different_cross_file_calls() {
    check_multi_file(&[
        (
            "main.rb",
            r#"
class Main
  def get_us<type label="String">er
    UserService.fetch
  end

  def get_pro<type label="Integer">duct
    ProductService.count
  end
end
"#,
        ),
        (
            "user_service.rb",
            r#"
class UserService
  # @return [String]
  def self.fetch
    "user"
  end
end
"#,
        ),
        (
            "product_service.rb",
            r#"
class ProductService
  # @return [Integer]
  def self.count
    42
  end
end
"#,
        ),
    ])
    .await;
}
