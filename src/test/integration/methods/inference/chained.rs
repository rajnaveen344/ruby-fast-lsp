//! Chained cross-file method return type inference tests.
//!
//! Tests for complex inference scenarios with multiple levels of cross-file calls.

use crate::test::harness::check_multi_file;

/// Test: A -> B -> C chain (3 files)
#[tokio::test]
async fn test_three_file_chain() {
    check_multi_file(&[
        (
            "main.rb",
            r#"
class Main
  def res<type label="String">ult
    MiddleService.process
  end
end
"#,
        ),
        (
            "middle_service.rb",
            r#"
class MiddleService
  # @return [String]
  def self.process
    DataService.fetch
  end
end
"#,
        ),
        (
            "data_service.rb",
            r#"
class DataService
  # @return [String]
  def self.fetch
    "raw data"
  end
end
"#,
        ),
    ])
    .await;
}

/// Test: Same file method calls cross-file method
#[tokio::test]
async fn test_same_file_wrapper_calls_cross_file() {
    check_multi_file(&[
        (
            "main.rb",
            r#"
class Main
  def public_a<type label="String">pi
    internal_call
  end

  def internal_call
    ExternalService.get_data
  end
end
"#,
        ),
        (
            "external_service.rb",
            r#"
class ExternalService
  # @return [String]
  def self.get_data
    "external"
  end
end
"#,
        ),
    ])
    .await;
}

/// Test: Cross-file call returns instance, then chain instance methods
#[tokio::test]
async fn test_cross_file_instance_chain() {
    check_multi_file(&[
        (
            "main.rb",
            r#"
class Main
  def get_formatted_na<type label="String">me
    UserRepository.find.full_name
  end
end
"#,
        ),
        (
            "user_repository.rb",
            r#"
class UserRepository
  # @return [User]
  def self.find
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
  def full_name
    "John Doe"
  end
end
"#,
        ),
    ])
    .await;
}

/// Test: Multiple cross-file calls in same method body
#[tokio::test]
async fn test_multiple_cross_file_in_same_method() {
    check_multi_file(&[
        (
            "main.rb",
            r#"
class Main
  def combi<type label="String">ne
    # Last expression determines return type
    ServiceA.get_string
  end
end
"#,
        ),
        (
            "service_a.rb",
            r#"
class ServiceA
  # @return [String]
  def self.get_string
    "from A"
  end
end
"#,
        ),
        (
            "service_b.rb",
            r#"
class ServiceB
  # @return [Integer]
  def self.get_number
    42
  end
end
"#,
        ),
    ])
    .await;
}

/// Test: Deep chain A -> B -> C -> D (4 files)
#[tokio::test]
async fn test_four_file_chain() {
    check_multi_file(&[
        (
            "main.rb",
            r#"
class Main
  def de<type label="String">ep
    Level1.call
  end
end
"#,
        ),
        (
            "level1.rb",
            r#"
class Level1
  # @return [String]
  def self.call
    Level2.call
  end
end
"#,
        ),
        (
            "level2.rb",
            r#"
class Level2
  # @return [String]
  def self.call
    Level3.call
  end
end
"#,
        ),
        (
            "level3.rb",
            r#"
class Level3
  # @return [String]
  def self.call
    "bottom"
  end
end
"#,
        ),
    ])
    .await;
}

/// Test: Cross-file with module namespacing
#[tokio::test]
async fn test_cross_file_with_module() {
    check_multi_file(&[
        (
            "main.rb",
            r#"
class Main
  def get_config_val<type label="String">ue
    Config::Settings.get
  end
end
"#,
        ),
        (
            "config/settings.rb",
            r#"
module Config
  class Settings
    # @return [String]
    def self.get
      "setting_value"
    end
  end
end
"#,
        ),
    ])
    .await;
}

/// Test: Bidirectional - A calls B, B calls A (with YARD to break cycle)
#[tokio::test]
async fn test_bidirectional_with_yard() {
    check_multi_file(&[
        (
            "main.rb",
            r#"
class Main
  def start<type label="String">
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
    ServiceB.transform("input")
  end
end
"#,
        ),
        (
            "service_b.rb",
            r#"
class ServiceB
  # @return [String]
  def self.transform(data)
    data.upcase
  end
end
"#,
        ),
    ])
    .await;
}
