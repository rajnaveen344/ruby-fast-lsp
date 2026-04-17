//! Goto definition tests for methods.

use crate::test::harness::check;

// ============================================================================
// Instance Methods
// ============================================================================

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

// ============================================================================
// Class Methods
// ============================================================================

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

// ============================================================================
// Mixins
// ============================================================================

/// Goto definition for method from included module.
#[tokio::test]
async fn goto_included_module_method() {
    check(
        r#"
module Loggable
  <def>def log
    puts "logging"
  end</def>
end

class App
  include Loggable

  def run
    log$0
  end
end
"#,
    )
    .await;
}

/// Goto definition for method from module included in another module.
#[tokio::test]
async fn goto_cross_module_method() {
    check(
        r#"
module ModuleA
  <def>def method_a
    "from A"
  end</def>
end

module ModuleB
  include ModuleA
end

class TestClass
  include ModuleB

  def test
    method_a$0
  end
end
"#,
    )
    .await;
}

// ============================================================================
// Inheritance
// ============================================================================

/// Goto definition for method from parent class.
#[tokio::test]
async fn goto_inherited_method() {
    check(
        r#"
class Parent
  <def>def parent_method
    "from parent"
  end</def>
end

class Child < Parent
  def test
    parent_method$0
  end
end
"#,
    )
    .await;
}

/// Goto definition for mixin method through inheritance.
#[tokio::test]
async fn goto_inherited_mixin_method() {
    check(
        r#"
module ApiHelpers
  <def>def api_call
    "api"
  end</def>
end

class BaseController
  include ApiHelpers
end

class AppController < BaseController
  def show
    api_call$0
  end
end
"#,
    )
    .await;
}

// ============================================================================
// Top-level
// ============================================================================

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

// ============================================================================
// Singleton Class (class << self)
// ============================================================================

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

// ============================================================================
// Constructor (initialize -> new)
// ============================================================================

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
