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
