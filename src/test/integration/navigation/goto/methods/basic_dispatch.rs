//! Goto definition for plain instance, class, top-level, singleton-class, and constructor calls.

use crate::test::harness::check;

/// Goto definition for instance method call.
#[tokio::test]
async fn goto_instance_method() {
    check(
        r#"
class Greeter
  <def>def greet
    puts "Hello"
  end</def>

  def run
    greet$0
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn goto_super_resolves_parent_method() {
    check(
        r#"
class ParentProcessor
  <def>def process
    "parent"
  end</def>
end

class ChildProcessor < ParentProcessor
  def process
    super$0
  end
end
"#,
    )
    .await;
}

/// Goto definition for method call on instance.
#[tokio::test]
async fn goto_method_on_instance() {
    check(
        r#"
class Foo
  <def>def bar
    42
  end</def>
end

Foo.new.bar$0
"#,
    )
    .await;
}

/// Goto definition for class method call.
#[tokio::test]
async fn goto_class_method() {
    check(
        r#"
class Utils
  <def>def self.process
    "processing"
  end</def>
end

Utils.process$0
"#,
    )
    .await;
}

/// Goto definition for top-level method.
#[tokio::test]
async fn goto_top_level_method() {
    check(
        r#"
<def>def helper
  "help"
end</def>

helper$0
"#,
    )
    .await;
}

/// Goto definition for method inside singleton class.
#[tokio::test]
async fn goto_singleton_class_method() {
    check(
        r#"
class Foo
  class << self
    <def>def singleton_method
      "singleton"
    end</def>
  end
end

Foo.singleton_method$0
"#,
    )
    .await;
}

/// Goto definition for .new which maps to initialize.
#[tokio::test]
async fn goto_constructor_via_new() {
    check(
        r#"
class Foo
  <def>def initialize
  end</def>
end

Foo.new$0
"#,
    )
    .await;
}

#[tokio::test]
async fn goto_method_on_instance_variable_receiver() {
    check(
        r#"
class Gateway
  <def>def refund
    "refunded"
  end</def>
end

class Invoice
  def charge
    @gateway = Gateway.new
    @gateway.refund$0
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn goto_method_on_array_element_block_parameter() {
    check(
        r#"
class Gateway
  <def>def capture
    "captured"
  end</def>
end

[Gateway.new].each do |gateway|
  gateway.capture$0
end
"#,
    )
    .await;
}

#[tokio::test]
async fn goto_method_on_yielded_block_parameter() {
    check(
        r#"
class Gateway
  <def>def capture
    "captured"
  end</def>
end

def with_gateway
  yield Gateway.new
end

with_gateway do |gateway|
  gateway.capture$0
end
"#,
    )
    .await;
}

#[tokio::test]
async fn goto_method_through_documented_return_chain() {
    check(
        r#"
class Gateway
  <def>def capture
    "captured"
  end</def>
end

class Account
  # @return [Gateway]
  def gateway
    Gateway.new
  end
end

account = Account.new
account.gateway.capture$0
"#,
    )
    .await;
}

#[tokio::test]
async fn goto_singleton_method_defined_with_own_constant_receiver() {
    check(
        r#"
class Registry
  <def>def Registry.lookup
    "found"
  end</def>
end

Registry.lookup$0
"#,
    )
    .await;
}

#[tokio::test]
async fn goto_singleton_method_defined_with_qualified_constant_receiver() {
    check(
        r#"
module Catalog
  class Registry
  end
end

class Loader
  <def>def (Catalog::Registry).lookup
    "found"
  end</def>
end

Catalog::Registry.lookup$0
"#,
    )
    .await;
}

#[tokio::test]
async fn singleton_method_with_qualified_constant_receiver_does_not_join_enclosing_class() {
    check(
        r#"
module Catalog
  class Registry
  end
end

class Loader
  def (Catalog::Registry).lookup
    "found"
  end
end

Loader.lookup$0<def none>
"#,
    )
    .await;
}
