//! Find references tests for methods.

use crate::test::harness::check;

// ============================================================================
// Instance Methods
// ============================================================================

/// Find references for instance method.
#[tokio::test]
async fn references_instance_method() {
    check(
        r#"
class Greeter
  def greet$0
  end

  def run
    <ref>greet</ref>
  end
end
"#,
    )
    .await;
}

/// Find references for instance method called on an instance via `.new`.
#[tokio::test]
async fn references_instance_method_on_new() {
    check(
        r#"
class Foo
  def bar$0
    42
  end
end

<ref>Foo.new.bar</ref>
"#,
    )
    .await;
}

/// Find references for instance method from multiple call sites.
#[tokio::test]
async fn references_instance_method_multiple_calls() {
    check(
        r#"
class Calculator
  def compute$0
    42
  end

  def run
    <ref>compute</ref>
  end

  def test
    <ref>compute</ref>
  end
end
"#,
    )
    .await;
}

// ============================================================================
// Class Methods
// ============================================================================

/// Find references for class method.
#[tokio::test]
async fn references_class_method() {
    check(
        r#"
class Utils
  def self.process$0
    "processing"
  end
end

<ref>Utils.process</ref>
"#,
    )
    .await;
}

/// Find references for singleton class method.
#[tokio::test]
async fn references_singleton_class_method() {
    check(
        r#"
class Foo
  class << self
    def singleton_method$0
      "singleton"
    end
  end
end

<ref>Foo.singleton_method</ref>
"#,
    )
    .await;
}

// ============================================================================
// Mixins
// ============================================================================

/// Find references for method from included module (called within including class).
#[tokio::test]
async fn references_included_module_method() {
    check(
        r#"
module Loggable
  def log$0
    puts "logging"
  end
end

class App
  include Loggable

  def run
    <ref>log</ref>
  end
end
"#,
    )
    .await;
}

/// Find references for method from module included in another module (transitive).
#[tokio::test]
async fn references_cross_module_method() {
    check(
        r#"
module ModuleA
  def method_a$0
    "from A"
  end
end

module ModuleB
  include ModuleA
end

class TestClass
  include ModuleB

  def test
    <ref>method_a</ref>
  end
end
"#,
    )
    .await;
}

// ============================================================================
// Inheritance
// ============================================================================

/// Find references for method from parent class (called in child).
#[tokio::test]
async fn references_inherited_method() {
    check(
        r#"
class Parent
  def parent_method$0
    "from parent"
  end
end

class Child < Parent
  def test
    <ref>parent_method</ref>
  end
end
"#,
    )
    .await;
}

/// Find references for mixin method through inheritance.
#[tokio::test]
async fn references_inherited_mixin_method() {
    check(
        r#"
module ApiHelpers
  def api_call$0
    "api"
  end
end

class BaseController
  include ApiHelpers
end

class AppController < BaseController
  def show
    <ref>api_call</ref>
  end
end
"#,
    )
    .await;
}

// ============================================================================
// Top-level
// ============================================================================

/// Find references for top-level method.
#[tokio::test]
async fn references_top_level_method() {
    check(
        r#"
def helper$0
end

<ref>helper</ref>
x = <ref>helper</ref>
"#,
    )
    .await;
}

// ============================================================================
// Constructor
// ============================================================================

// ============================================================================
// Chained Methods
// ============================================================================

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
<ref>team.leader.name</ref>
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
<ref>team.leader</ref>.name
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
<ref>user.name</ref>
animal = Animal.new
animal.name
"#,
    )
    .await;
}

// ============================================================================
// Constructor
// ============================================================================

/// Find references for constructor (.new calls should reference initialize).
#[tokio::test]
async fn references_constructor_via_new() {
    check(
        r#"
class Foo
  def initialize$0
  end
end

<ref>Foo.new</ref>
"#,
    )
    .await;
}
