//! Goto definition tests for classes and modules.

use crate::test::harness::check;

/// Goto definition for a class reference.
#[tokio::test]
async fn goto_class() {
    check(
        r#"
<def>class Foo
end</def>

Foo$0.new
"#,
    )
    .await;
}

/// Goto definition for a nested class inside a module.
#[tokio::test]
async fn goto_nested_class() {
    check(
        r#"
module MyMod
  <def>class Foo
  end</def>
end

MyMod::Foo$0.new
"#,
    )
    .await;
}

/// Goto definition for a module.
#[tokio::test]
async fn goto_module() {
    check(
        r#"
<def>module MyMod
end</def>

include MyMod$0
"#,
    )
    .await;
}

/// Goto definition for a deep namespaced class (A::B::C).
#[tokio::test]
async fn goto_deep_namespaced_class() {
    check(
        r#"
module A
  module B
    <def>class C
    end</def>
  end
end

A::B::C$0.new
"#,
    )
    .await;
}

/// Goto definition for a deep namespaced module.
#[tokio::test]
async fn goto_deep_namespaced_module() {
    check(
        r#"
module A
  <def>module B
  end</def>
end

include A::B$0
"#,
    )
    .await;
}

/// A method called inside a class reopened through a constant alias resolves
/// on the aliased class.
#[tokio::test]
async fn call_inside_class_reopened_through_alias_resolves_on_target() {
    check(
        r#"
class Engine
  <def>def start
  end</def>
end

Motor = Engine

class Motor
  def run
    start$0
  end
end
"#,
    )
    .await;
}

/// A method defined in a module reopened through an alias belongs to the
/// aliased module.
#[tokio::test]
async fn method_in_module_reopened_through_alias_belongs_to_target() {
    check(
        r#"
module Toolkit
end

Kit = Toolkit

module Kit
  <def>def self.describe
  end</def>
end

Toolkit.describe$0
"#,
    )
    .await;
}

/// A constant declared in the aliased class is found from inside a body that
/// reopens it through the alias.
#[tokio::test]
async fn constant_inside_class_reopened_through_alias_resolves_on_target() {
    check(
        r#"
class Engine
  <def>LIMIT = 3</def>
end

Motor = Engine

class Motor
  def run
    LIMIT$0
  end
end
"#,
    )
    .await;
}

/// A class that names an alias of its own superclass declares a new class
/// rather than reopening the superclass.
#[tokio::test]
async fn subclass_named_by_an_alias_of_its_superclass_is_a_new_class() {
    check(
        r#"
class Scanner
end

module Text
end

Text::Wrapped = Scanner

class Text::Wrapped < Scanner
  def wrapped
  end
end

Scanner.new.wrapped$0<def none>
"#,
    )
    .await;
}
