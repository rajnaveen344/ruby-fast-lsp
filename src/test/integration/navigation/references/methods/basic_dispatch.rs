//! References for plain instance, class, top-level, singleton-class, and constructor calls.

use crate::test::harness::check;

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

#[tokio::test]
async fn references_parent_method_include_super_call() {
    check(
        r#"
class ParentProcessor
  def process$0
    "parent"
  end
end

class ChildProcessor < ParentProcessor
  def process
    <ref>super</ref>
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

Foo.new.<ref>bar</ref>
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

Utils.<ref>process</ref>
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

Foo.<ref>singleton_method</ref>
"#,
    )
    .await;
}

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

/// Find references for constructor (.new calls should reference initialize).
#[tokio::test]
async fn references_constructor_via_new() {
    check(
        r#"
class Foo
  def initialize$0
  end
end

Foo.<ref>new</ref>
"#,
    )
    .await;
}

/// A method declared in a class reopened through a constant alias is found
/// from calls on the aliased class.
#[tokio::test]
async fn references_method_declared_in_class_reopened_through_alias() {
    check(
        r#"
class Engine
end

Motor = Engine

class Motor
  def start$0
  end
end

Engine.new.<ref>start</ref>
"#,
    )
    .await;
}

/// A method declared in a module reopened through a constant alias is found
/// from calls on the aliased module.
#[tokio::test]
async fn references_method_declared_in_module_reopened_through_alias() {
    check(
        r#"
module Toolkit
end

Kit = Toolkit

module Kit
  def self.version$0
  end
end

Toolkit.<ref>version</ref>
"#,
    )
    .await;
}
