//! Hover on calls through aliases, `delegate`, and `Forwardable`.

use crate::test::harness::{check, check_multi_file};

#[tokio::test]
async fn alias_method_call_uses_original_return_type() {
    check(
        r#"
class User
  # @return [String]
  def name
    "n"
  end

  alias full_name name
end

User.new.full_name<hover label="String">
"#,
    )
    .await;
}

#[tokio::test]
async fn alias_method_definition_uses_original_return_type() {
    check(
        r#"
class User
  # @return [String]
  def name
    "n"
  end

  alias full_name<hover label="String"> name
end
"#,
    )
    .await;
}

#[tokio::test]
async fn alias_method_call_form_uses_original_return_type() {
    check(
        r#"
class User
  # @return [String]
  def name
    "n"
  end

  alias_method :display_name, :name
end

User.new.display_name<hover label="String">
"#,
    )
    .await;
}

#[tokio::test]
async fn alias_method_call_form_definition_uses_original_return_type() {
    check(
        r#"
class User
  # @return [String]
  def name
    "n"
  end

  alias_method :display_name<hover label="String">, :name
end
"#,
    )
    .await;
}

#[tokio::test]
async fn delegate_method_call_uses_target_return_type() {
    check(
        r#"
class User
  # @return [String]
  def name
    "n"
  end
end

class Order
  delegate :name, to: :user

  # @return [User]
  def user
    User.new
  end
end

Order.new.name<hover label="String">
"#,
    )
    .await;
}

#[tokio::test]
async fn forwardable_def_delegators_call_uses_target_return_type() {
    check(
        r#"
class ServiceFlags
  class << self
    extend Forwardable
    def_delegators :instance, :allow?
  end

  # @return [ServiceFlags]
  def self.instance
    new
  end

  # @return [String]
  def allow?
    "enabled"
  end
end

ServiceFlags.allow?<hover label="String">
"#,
    )
    .await;
}

#[tokio::test]
async fn cross_file_delegate_method_call_uses_target_return_type() {
    check_multi_file(&[
        (
            "user.rb",
            r#"
class User
  # @return [String]
  def name
    "n"
  end
end
"#,
        ),
        (
            "order.rb",
            r#"
class Order
  delegate :name, to: :user

  # @return [User]
  def user
    User.new
  end
end
"#,
        ),
        (
            "report.rb",
            r#"
class Report
  def run
    Order.new.name<hover label="String">
  end
end
"#,
        ),
    ])
    .await;
}

#[tokio::test]
async fn namespaced_delegate_method_call_survives_late_target_file_open() {
    check_multi_file(&[
        (
            "caller.rb",
            r#"
module SimDelegate
  class Report
    def run
      SimDelegate::Order.new.name<hover label="String">
    end
  end
end
"#,
        ),
        (
            "order.rb",
            r#"
module SimDelegate
  class Order
    # @return [SimDelegate::User]
    def user
      SimDelegate::User.new
    end

    delegate :name, to: :user
  end
end
"#,
        ),
        (
            "user.rb",
            r#"
module SimDelegate
  class User
    # @return [String]
    def name
      "n"
    end
  end
end
"#,
        ),
    ])
    .await;
}
