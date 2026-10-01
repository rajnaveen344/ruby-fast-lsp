//! References through chained calls and local, instance-variable, and namespaced receivers.

use crate::test::harness::{check, FakeEditor};

/// Find references for method called via chained method (e.g., team.leader.name).
#[tokio::test]
async fn references_chained_method_call() {
    check(
        r#"
class User
  def name$0
    "hello"
  end
end

class Animal
  def name
    "animal"
  end
end

class Team
  def leader
    User.new
  end
end

team = Team.new
team.leader.<ref>name</ref>
"#,
    )
    .await;
}

/// Find references for intermediate method in a chain (e.g., team.leader in team.leader.name).
#[tokio::test]
async fn references_intermediate_chained_method() {
    check(
        r#"
class User
  def name
    "hello"
  end
end

class Team
  def leader$0
    User.new
  end
end

team = Team.new
team.<ref>leader</ref>.name
"#,
    )
    .await;
}

/// Find references for method on variable receiver — must NOT include calls on unrelated types.
#[tokio::test]
async fn references_method_on_variable_receiver() {
    check(
        r#"
class User
  def name$0
    "hello"
  end
end

class Animal
  def name
    "animal"
  end
end

user = User.new
user.<ref>name</ref>
animal = Animal.new
animal.name
"#,
    )
    .await;
}

#[tokio::test]
async fn references_method_on_local_receiver_inside_method() {
    check(
        r#"
class User
  def name$0
    "hello"
  end
end

class Presenter
  def render
    user = User.new
    user.<ref>name</ref>
  end
end
"#,
    )
    .await;
}

/// Local receiver references stay on the receiver type, even when an included module has the same method.
#[tokio::test]
async fn references_local_receiver_prefers_receiver_class_over_included_module_collision() {
    check(
        r#"
module Trackable
  def flow
    "module"
  end
end

class Invoice
  include Trackable

  def flow
    "class"
  end

  def charge
    receiver = Invoice.new
    receiver.<ref>flow$0</ref>
  end
end
"#,
    )
    .await;
}

/// Find references for method on instance-variable receiver.
#[tokio::test]
async fn references_method_on_instance_variable_receiver() {
    check(
        r#"
class User
  def name$0
    "hello"
  end
end

class Animal
  def name
    "animal"
  end
end

class Presenter
  def render
    @user = User.new
    @user.<ref>name</ref>
    @animal = Animal.new
    @animal.name
  end
end
"#,
    )
    .await;
}

/// Cross-file chain references update after the intermediate return method is indexed later.
#[tokio::test]
async fn references_chained_method_after_late_intermediate_file_open() {
    let mut editor = FakeEditor::new().await;
    let target = r#"
class Gateway
  def capture
    "ok"
  end
end
"#;
    let caller = r#"
class Invoice
  def charge
    item = Item.new
    item.gateway.capture
  end
end
"#;
    let intermediate = r#"
class Item
  # @return [Gateway]
  def gateway
    Gateway.new
  end
end
"#;

    editor.open("gateway.rb", target).await;
    editor.open("invoice.rb", caller).await;
    editor.close("invoice.rb").await;
    editor.open("item.rb", intermediate).await;
    editor.open("invoice.rb", caller).await;

    let refs = editor.references_at("gateway.rb", 2, 6).await;
    assert!(
        refs.iter().any(|location| {
            location.uri.path().ends_with("invoice.rb") && location.range.start.line == 4
        }),
        "expected Gateway#capture references to include invoice chain call, got {refs:?}"
    );
}

#[tokio::test]
async fn references_chained_method_after_late_return_owner_file_open() {
    let mut editor = FakeEditor::new().await;
    let target = r#"
module Payments
  class Gateway
    def capture
      "ok"
    end
  end
end
"#;
    let caller = r#"
module Billing
  class Invoice
    def charge
      account = Billing::Account.new
      account.gateway.capture
    end
  end
end
"#;
    let intermediate = r#"
module Billing
  class Account
    # @return [Payments::Gateway]
    def gateway
      Payments::Gateway.new
    end
  end
end
"#;
    let unrelated_late_file = r#"
module Billing
  class Statement
  end
end
"#;

    editor.open("gateway.rb", target).await;
    editor.open("invoice.rb", caller).await;
    editor.open("account.rb", intermediate).await;
    editor.open("statement.rb", unrelated_late_file).await;

    let refs = editor.references_at("gateway.rb", 3, 8).await;
    assert!(
        refs.iter().any(|location| {
            location.uri.path().ends_with("invoice.rb") && location.range.start.line == 5
        }),
        "expected Payments::Gateway#capture references to include invoice chain call after late return-owner open, got {refs:?}"
    );
}

#[tokio::test]
async fn references_namespaced_chained_method_inside_method() {
    check(
        r#"
module Payments
  class Gateway
    def capture$0
      "ok"
    end
  end
end

module Billing
  class Account
    # @return [Payments::Gateway]
    def gateway
      __sim_return_gateway = Payments::Gateway.new
    end
  end

  class Invoice
    def charge
      account = Billing::Account.new
      account.gateway.<ref>capture</ref>
    end
  end
end
"#,
    )
    .await;
}
