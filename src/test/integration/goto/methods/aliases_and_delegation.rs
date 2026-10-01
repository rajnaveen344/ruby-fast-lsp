//! Goto definition through aliases, `delegate`, and `Forwardable`.

use crate::test::harness::check;

#[tokio::test]
async fn goto_alias_method_call() {
    check(
        r#"
class User
  def name
    "n"
  end

  <def>alias full_name name</def>
end

User.new.full_name$0
"#,
    )
    .await;
}

#[tokio::test]
async fn goto_alias_method_call_form() {
    check(
        r#"
class User
  def name
    "n"
  end

  <def>alias_method :display_name, :name</def>
end

User.new.display_name$0
"#,
    )
    .await;
}

#[tokio::test]
async fn goto_delegate_method_call() {
    check(
        r#"
class User
  def name
    "n"
  end
end

class Order
  <def>delegate :name, to: :user</def>

  def user
    User.new
  end
end

Order.new.name$0
"#,
    )
    .await;
}

#[tokio::test]
async fn goto_forwardable_def_delegators_class_method_call() {
    check(
        r#"
class ServiceFlags
  class << self
    extend Forwardable
    <def>def_delegators :instance, :allow?</def>
  end

  def self.instance
    new
  end

  def allow?
    true
  end
end

ServiceFlags.allow?$0
"#,
    )
    .await;
}

#[tokio::test]
async fn goto_forwardable_def_delegator_class_method_call() {
    check(
        r#"
class S3Storage
  class << self
    extend Forwardable
    <def>def_delegator :instance, :get_storage</def>
  end

  def self.instance
    new
  end

  def get_storage
    "storage"
  end
end

S3Storage.get_storage$0
"#,
    )
    .await;
}
