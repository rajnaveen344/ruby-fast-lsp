//! Tests for textDocument/implementation
//!
//! Tests the "find implementations" feature which answers:
//! - For a module: "which classes include/prepend/extend this module?"
//! - For a method: "which classes override this method?"

use crate::test::harness::{check, check_multi_file, FakeEditor};

// ============================================================================
// Module implementations — cursor on module name
// ============================================================================

/// Cursor on a module name should find all classes that include it.
#[tokio::test]
async fn module_included_by_classes() {
    check_multi_file(&[
        (
            "main.rb",
            r#"
module Serializable$0
  def to_json; end
end

<impl>class User
  include Serializable
end</impl>

<impl>class Post
  include Serializable
end</impl>
"#,
        ),
    ])
    .await;
}

/// Cursor on a module name should find classes that prepend it.
#[tokio::test]
async fn module_prepended_by_class() {
    check_multi_file(&[
        (
            "main.rb",
            r#"
module Logging$0
  def log; end
end

<impl>class Server
  prepend Logging
end</impl>
"#,
        ),
    ])
    .await;
}

/// Cursor on a module name should find classes that extend it.
#[tokio::test]
async fn module_extended_by_class() {
    check_multi_file(&[
        (
            "main.rb",
            r#"
module ClassMethods$0
  def find; end
end

<impl>class User
  extend ClassMethods
end</impl>
"#,
        ),
    ])
    .await;
}

// ============================================================================
// Method implementations — cursor on method definition
// ============================================================================

/// Cursor on a module method should find overrides in including classes.
#[tokio::test]
async fn method_overrides_in_includers() {
    check_multi_file(&[
        (
            "main.rb",
            r#"
module Serializable
  def to_json$0; end
end

class User
  include Serializable
  <impl>def to_json
    { name: @name }
  end</impl>
end

class Post
  include Serializable
  <impl>def to_json
    { title: @title }
  end</impl>
end
"#,
        ),
    ])
    .await;
}

/// Cursor on a superclass method should find overrides in subclasses.
#[tokio::test]
async fn method_overrides_in_subclasses() {
    check_multi_file(&[
        (
            "main.rb",
            r#"
class Animal
  def speak$0; end
end

class Dog < Animal
  <impl>def speak
    "woof"
  end</impl>
end

class Cat < Animal
  <impl>def speak
    "meow"
  end</impl>
end
"#,
        ),
    ])
    .await;
}

/// Subclass without override should NOT appear in results.
#[tokio::test]
async fn subclass_without_override_excluded() {
    check_multi_file(&[
        (
            "main.rb",
            r#"
class Animal
  def speak$0; end
end

class Dog < Animal
  <impl>def speak
    "woof"
  end</impl>
end

class Fish < Animal
end
"#,
        ),
    ])
    .await;
}

/// Deep inheritance — override in grandchild.
#[tokio::test]
async fn method_override_in_grandchild() {
    check_multi_file(&[
        (
            "main.rb",
            r#"
class Base
  def render$0; end
end

class Middle < Base
end

class Leaf < Middle
  <impl>def render
    "leaf"
  end</impl>
end
"#,
        ),
    ])
    .await;
}

// ============================================================================
// Cross-file implementations
// ============================================================================

/// Implementation found in a different file (programmatic — cross-file tags
/// don't work with check_multi_file since it only extracts tags from the primary file).
#[tokio::test]
async fn cross_file_method_override() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "base.rb",
            r#"
class Animal
  def speak; end
end
"#,
        )
        .await;
    editor
        .open(
            "dog.rb",
            r#"
class Dog < Animal
  def speak
    "woof"
  end
end
"#,
        )
        .await;

    // Cursor on `speak` in Animal (line 2, char 6)
    let impls = editor.goto_impl_at("base.rb", 2, 6).await;
    assert_eq!(impls.len(), 1, "Expected 1 implementation, got {:?}", impls);
    assert!(
        impls[0].uri.path().ends_with("dog.rb"),
        "Expected implementation in dog.rb, got {:?}",
        impls[0].uri
    );
}

/// Module included cross-file.
#[tokio::test]
async fn cross_file_module_inclusion() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "serializable.rb",
            r#"
module Serializable
  def to_json; end
end
"#,
        )
        .await;
    editor
        .open(
            "user.rb",
            r#"
class User
  include Serializable
end
"#,
        )
        .await;

    // Cursor on `Serializable` (line 1, char 7)
    let impls = editor.goto_impl_at("serializable.rb", 1, 7).await;
    assert_eq!(impls.len(), 1, "Expected 1 implementation, got {:?}", impls);
    assert!(
        impls[0].uri.path().ends_with("user.rb"),
        "Expected implementation in user.rb, got {:?}",
        impls[0].uri
    );
}

// ============================================================================
// Class implementations — cursor on class name
// ============================================================================

/// Cursor on a class name should find subclasses.
#[tokio::test]
async fn class_subclasses() {
    check(
        r#"
class Animal$0
end

<impl>class Dog < Animal
end</impl>

<impl>class Cat < Animal
end</impl>
"#,
    )
    .await;
}

/// Cursor on a class with no subclasses — returns nothing.
#[tokio::test]
async fn class_with_no_subclasses() {
    check(
        r#"
class Leaf$0
end
"#,
    )
    .await;
}

// ============================================================================
// Method call — cursor on a method call site
// ============================================================================

/// Cursor on a method call should find implementations via receiver type.
#[tokio::test]
async fn method_call_finds_overrides() {
    check(
        r#"
class Animal
  def speak; end
end

class Dog < Animal
  <impl>def speak
    "woof"
  end</impl>
end

class Cat < Animal
  <impl>def speak
    "meow"
  end</impl>
end

animal = Animal.new
animal.speak$0
"#,
    )
    .await;
}

// ============================================================================
// Module included by another module (not a class)
// ============================================================================

/// Module included by another module.
#[tokio::test]
async fn module_included_by_module() {
    check(
        r#"
module Base$0
end

<impl>module Extended
  include Base
end</impl>
"#,
    )
    .await;
}

// ============================================================================
// Mixed: both includers and subclasses
// ============================================================================

/// Method has overrides in both includers and subclasses of includers.
#[tokio::test]
async fn method_overrides_in_includers_and_their_subclasses() {
    check(
        r#"
module Renderable
  def render$0; end
end

class Page
  include Renderable
  <impl>def render
    "page"
  end</impl>
end

class SpecialPage < Page
  <impl>def render
    "special"
  end</impl>
end
"#,
    )
    .await;
}

// ============================================================================
// Edge cases
// ============================================================================

/// Method with no overrides anywhere — returns nothing.
#[tokio::test]
async fn method_with_no_overrides() {
    check(
        r#"
class Base
  def unique_method$0; end
end

class Child < Base
end
"#,
    )
    .await;
}

/// Transitive module chain: Module A included by Module B included by Class C.
/// Cursor on A's method should find overrides in both B and C.
#[tokio::test]
async fn transitive_module_chain_method_override() {
    check(
        r#"
module Base
  def work$0; end
end

module Middle
  include Base
  <impl>def work
    "middle"
  end</impl>
end

class Final
  include Middle
  <impl>def work
    "final"
  end</impl>
end
"#,
    )
    .await;
}

/// Transitive module chain for namespace: cursor on module name finds
/// both the intermediate module and the final class.
#[tokio::test]
async fn transitive_module_chain_namespace() {
    check(
        r#"
module Base$0
end

<impl>module Middle
  include Base
end</impl>

<impl>class Final
  include Middle
end</impl>
"#,
    )
    .await;
}

/// Multiple inheritance levels with selective overrides.
#[tokio::test]
async fn selective_overrides_in_deep_chain() {
    check(
        r#"
class A
  def work$0; end
end

class B < A
end

class C < B
  <impl>def work
    "c"
  end</impl>
end

class D < C
end

class E < D
  <impl>def work
    "e"
  end</impl>
end
"#,
    )
    .await;
}

// ============================================================================
// Singleton methods (def self.method)
// ============================================================================

/// Singleton method override in subclass.
#[tokio::test]
async fn singleton_method_override() {
    check(
        r#"
class Animal
  def self.create$0; end
end

class Dog < Animal
  <impl>def self.create
    "dog"
  end</impl>
end
"#,
    )
    .await;
}
