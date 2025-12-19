//! Goto definition tests for methods.

use crate::test::harness::check_goto;

// ============================================================================
// Instance Methods
// ============================================================================

/// Goto definition for instance method call.
#[tokio::test]
async fn goto_instance_method() {
    check_goto(
        r#"
class Greeter
  def <def>greet</def>
    puts "Hello"
  end

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
    check_goto(
        r#"
class Foo
  def <def>bar</def>
    42
  end
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
    check_goto(
        r#"
class Utils
  def self.<def>process</def>
    "processing"
  end
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
    check_goto(
        r#"
module Loggable
  def <def>log</def>
    puts "logging"
  end
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
    check_goto(
        r#"
module ModuleA
  def <def>method_a</def>
    "from A"
  end
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
    check_goto(
        r#"
class Parent
  def <def>parent_method</def>
    "from parent"
  end
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
    check_goto(
        r#"
module ApiHelpers
  def <def>api_call</def>
    "api"
  end
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
    check_goto(
        r#"
def <def>helper</def>
  "help"
end

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
    check_goto(
        r#"
class Foo
  class << self
    def <def>singleton_method</def>
      "singleton"
    end
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
    check_goto(
        r#"
class Foo
  def <def>initialize</def>
  end
end

Foo.new$0
"#,
    )
    .await;
}
