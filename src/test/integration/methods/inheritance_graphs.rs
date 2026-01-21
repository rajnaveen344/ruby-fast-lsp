//! Goto definition tests for various inheritance graph structures.
//!
//! These tests verify that method resolution works correctly across different
//! inheritance patterns including diamond inheritance, prepend vs include priority,
//! and complex mixin hierarchies.
//!
//! Note: Basic graph traversal is unit-tested in `src/indexer/graph.rs`.
//! These integration tests verify the end-to-end behavior through goto definition.

use crate::test::harness::check;

// ============================================================================
// Diamond Inheritance
// ============================================================================

/// Diamond inheritance: A includes B and C, both B and C include D.
/// Method defined in D should be found (no duplicates in chain).
#[tokio::test]
async fn goto_diamond_inheritance_base_method() {
    check(
        r#"
module D
  <def>def shared_method
    "from D"
  end</def>
end

module B
  include D
end

module C
  include D
end

class A
  include B
  include C

  def test
    shared_method$0
  end
end
"#,
    )
    .await;
}

/// Diamond inheritance: method overridden in one branch.
/// Should find the override, not the base.
#[tokio::test]
async fn goto_diamond_inheritance_with_override() {
    check(
        r#"
module Base
  def common
    "base"
  end
end

module Left
  include Base
end

module Right
  include Base

  <def>def common
    "right override"
  end</def>
end

class Consumer
  include Left
  include Right

  def test
    common$0
  end
end
"#,
    )
    .await;
}

// ============================================================================
// Prepend Priority
// ============================================================================

/// Prepend should take priority over the class itself.
#[tokio::test]
async fn goto_prepend_shadows_class_method() {
    check(
        r#"
module Interceptor
  <def>def process
    "intercepted"
  end</def>
end

class Handler
  prepend Interceptor

  def process
    "original"
  end

  def test
    process$0
  end
end
"#,
    )
    .await;
}

/// Prepend should take priority over include.
#[tokio::test]
async fn goto_prepend_shadows_include() {
    check(
        r#"
module Included
  def action
    "included"
  end
end

module Prepended
  <def>def action
    "prepended"
  end</def>
end

class Actor
  include Included
  prepend Prepended

  def test
    action$0
  end
end
"#,
    )
    .await;
}

/// Multiple prepends: last prepend wins (searched first).
#[tokio::test]
async fn goto_multiple_prepends_last_wins() {
    check(
        r#"
module First
  def intercept
    "first"
  end
end

module Second
  <def>def intercept
    "second"
  end</def>
end

class Target
  prepend First
  prepend Second

  def test
    intercept$0
  end
end
"#,
    )
    .await;
}

/// Child's includes are searched before parent's prepends.
/// Ruby MRO: Child → ChildInclude → ParentPrepend → Parent
#[tokio::test]
async fn goto_child_include_before_parent_prepend() {
    check(
        r#"
module ParentPrepend
  def shared
    "parent prepend"
  end
end

module ChildInclude
  <def>def shared
    "child include"
  end</def>
end

class Parent
  prepend ParentPrepend
end

class Child < Parent
  include ChildInclude

  def test
    shared$0
  end
end
"#,
    )
    .await;
}

// ============================================================================
// Include Ordering
// ============================================================================

/// Multiple includes: last include is searched first.
#[tokio::test]
async fn goto_multiple_includes_last_searched_first() {
    check(
        r#"
module First
  def helper
    "first"
  end
end

module Second
  <def>def helper
    "second"
  end</def>
end

class User
  include First
  include Second

  def test
    helper$0
  end
end
"#,
    )
    .await;
}

/// Include ordering with inheritance: child's includes before parent's.
#[tokio::test]
async fn goto_child_include_before_parent_include() {
    check(
        r#"
module ParentMixin
  def feature
    "parent mixin"
  end
end

module ChildMixin
  <def>def feature
    "child mixin"
  end</def>
end

class Parent
  include ParentMixin
end

class Child < Parent
  include ChildMixin

  def test
    feature$0
  end
end
"#,
    )
    .await;
}

// ============================================================================
// Deep Inheritance Chains
// ============================================================================

/// Four levels of inheritance.
#[tokio::test]
async fn goto_four_level_inheritance() {
    check(
        r#"
class Level1
  <def>def base_method
    "level 1"
  end</def>
end

class Level2 < Level1
end

class Level3 < Level2
end

class Level4 < Level3
  def test
    base_method$0
  end
end
"#,
    )
    .await;
}

/// Deep inheritance with method override at intermediate level.
#[tokio::test]
async fn goto_deep_inheritance_intermediate_override() {
    check(
        r#"
class GrandParent
  def legacy
    "grandparent"
  end
end

class Parent < GrandParent
  <def>def legacy
    "parent override"
  end</def>
end

class Child < Parent
end

class GrandChild < Child
  def test
    legacy$0
  end
end
"#,
    )
    .await;
}

// ============================================================================
// Module Includes Module
// ============================================================================

/// Module including another module (transitive includes).
#[tokio::test]
async fn goto_transitive_module_includes() {
    check(
        r#"
module Core
  <def>def core_feature
    "core"
  end</def>
end

module Extended
  include Core

  def extended_feature
    "extended"
  end
end

class Application
  include Extended

  def test
    core_feature$0
  end
end
"#,
    )
    .await;
}

/// Three levels of module includes.
#[tokio::test]
async fn goto_three_level_module_includes() {
    check(
        r#"
module Level1
  <def>def deep_method
    "level 1"
  end</def>
end

module Level2
  include Level1
end

module Level3
  include Level2
end

class Consumer
  include Level3

  def test
    deep_method$0
  end
end
"#,
    )
    .await;
}

// ============================================================================
// Extend (Class Methods via Modules)
// ============================================================================

/// Extend adds module methods as class methods.
#[tokio::test]
async fn goto_extended_module_method() {
    check(
        r#"
module ClassHelpers
  <def>def create_default
    new
  end</def>
end

class Entity
  extend ClassHelpers
end

Entity.create_default$0
"#,
    )
    .await;
}

/// Extend with inheritance: child inherits parent's extended methods.
#[tokio::test]
async fn goto_inherited_extended_method() {
    check(
        r#"
module Findable
  <def>def find_by_id(id)
    "finding"
  end</def>
end

class BaseModel
  extend Findable
end

class User < BaseModel
end

User.find_by_id$0(1)
"#,
    )
    .await;
}

/// Multiple extends: last extend searched first.
#[tokio::test]
async fn goto_multiple_extends_last_wins() {
    check(
        r#"
module FirstClassMethods
  def build
    "first"
  end
end

module SecondClassMethods
  <def>def build
    "second"
  end</def>
end

class Builder
  extend FirstClassMethods
  extend SecondClassMethods
end

Builder.build$0
"#,
    )
    .await;
}

// ============================================================================
// Mixed Prepend, Include, and Extend
// ============================================================================

/// Complex hierarchy with all three mixin types.
#[tokio::test]
async fn goto_complex_mixin_hierarchy() {
    check(
        r#"
module Prependable
  <def>def action
    "prepended"
  end</def>
end

module Includable
  def action
    "included"
  end
end

module Extendable
  def class_action
    "extended"
  end
end

class Complex
  prepend Prependable
  include Includable
  extend Extendable

  def action
    "original"
  end

  def test
    action$0
  end
end
"#,
    )
    .await;
}

// ============================================================================
// Singleton Class Inheritance
// ============================================================================

/// Singleton class inherits from parent's singleton class.
#[tokio::test]
async fn goto_singleton_class_inheritance() {
    check(
        r#"
class Parent
  <def>def self.class_helper
    "parent class method"
  end</def>
end

class Child < Parent
end

Child.class_helper$0
"#,
    )
    .await;
}

/// Singleton class method override.
#[tokio::test]
async fn goto_singleton_class_override() {
    check(
        r#"
class Parent
  def self.factory
    "parent factory"
  end
end

class Child < Parent
  <def>def self.factory
    "child factory"
  end</def>
end

Child.factory$0
"#,
    )
    .await;
}

// ============================================================================
// Edge Cases
// ============================================================================

/// Method defined in class shadows all mixin methods.
#[tokio::test]
async fn goto_class_method_shadows_all_mixins() {
    check(
        r#"
module Mixin
  def shadowed
    "mixin"
  end
end

class WithShadow
  include Mixin

  <def>def shadowed
    "class"
  end</def>

  def test
    shadowed$0
  end
end
"#,
    )
    .await;
}

/// Method in superclass shadows grandparent's mixin.
#[tokio::test]
async fn goto_superclass_shadows_grandparent_mixin() {
    check(
        r#"
module GrandparentMixin
  def inherited_method
    "grandparent mixin"
  end
end

class GrandParent
  include GrandparentMixin
end

class Parent < GrandParent
  <def>def inherited_method
    "parent override"
  end</def>
end

class Child < Parent
  def test
    inherited_method$0
  end
end
"#,
    )
    .await;
}

/// Empty intermediate class doesn't break chain.
#[tokio::test]
async fn goto_through_empty_intermediate_class() {
    check(
        r#"
class Base
  <def>def base_method
    "base"
  end</def>
end

class EmptyMiddle < Base
end

class Leaf < EmptyMiddle
  def test
    base_method$0
  end
end
"#,
    )
    .await;
}
