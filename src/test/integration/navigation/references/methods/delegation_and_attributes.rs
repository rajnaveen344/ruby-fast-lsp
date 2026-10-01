//! References through `delegate`, `Forwardable`, and `class_attribute`.

use crate::test::harness::check;

#[tokio::test]
async fn references_delegate_method_call() {
    check(
        r#"
class User
  def name
    "n"
  end
end

class Order
  delegate :name, to: :user

  def user
    User.new
  end
end

class Report
  def run
    Order.new.<ref>name</ref>
  end
end

class Probe
  def run
    Order.new.<ref>name$0</ref>
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn references_delegate_method_definition() {
    check(
        r#"
class User
  def name
    "n"
  end
end

class Order
  delegate :name$0, to: :user

  def user
    User.new
  end
end

class Report
  def run
    Order.new.<ref>name</ref>
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn references_forwardable_def_delegators_definition() {
    check(
        r#"
class ServiceFlags
  class << self
    extend Forwardable
    def_delegators :instance, :allow?$0
  end

  def self.instance
    new
  end

  def allow?
    true
  end
end

ServiceFlags.<ref>allow?</ref>
"#,
    )
    .await;
}

#[tokio::test]
async fn references_class_attribute_definition() {
    check(
        r#"
class Worker
  class_attribute :queue_config

  def self.setup
    <ref>queue_config$0</ref>
    self.queue_config = {}
  end

  def run
    <ref>queue_config</ref>
  end
end

Worker.<ref>queue_config</ref>
Worker.new.<ref>queue_config</ref>
"#,
    )
    .await;
}
