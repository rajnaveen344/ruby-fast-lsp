//! Tests for include/extend/prepend mixin functionality
//!
//! Regression tests for panic: "Cannot add includes to non-class/module entry"
//! This can happen when get_last_definition_mut returns a non-class/module entry.

use crate::test::harness::check;

/// Regression test: include on a reopened class should not panic
/// when the last indexed entry for the FQN is not a class/module.
#[tokio::test]
async fn test_include_with_shadowed_constant_does_not_panic() {
    check(
        r#"
module Bar
end

# Define a constant with the same name as a class
Foo = "not a class"

# Then define the actual class with include
class Foo
  include Bar
end
"#,
    )
    .await;
}

/// Test that include inside a method doesn't panic
#[tokio::test]
async fn test_include_inside_method_does_not_panic() {
    check(
        r#"
module Bar
end

class Foo
  def some_method
    include Bar
  end
end
"#,
    )
    .await;
}

/// Test that extend inside a method doesn't panic
#[tokio::test]
async fn test_extend_inside_method_does_not_panic() {
    check(
        r#"
module Bar
end

class Foo
  def some_method
    extend Bar
  end
end
"#,
    )
    .await;
}

/// Test that prepend inside a method doesn't panic
#[tokio::test]
async fn test_prepend_inside_method_does_not_panic() {
    check(
        r#"
module Bar
end

class Foo
  def some_method
    prepend Bar
  end
end
"#,
    )
    .await;
}

/// Test include/extend/prepend work correctly at class level
#[tokio::test]
async fn test_mixins_at_class_level() {
    check(
        r#"
module Mixin
  def helper; end
end

class MyClass
  include Mixin
  extend Mixin
  prepend Mixin
end
"#,
    )
    .await;
}
