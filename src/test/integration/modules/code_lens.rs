//! Code lens tests for modules.

use crate::test::harness::{check_code_lens, check_no_code_lens};

// ============================================================================
// Mixin Tests
// ============================================================================

/// Basic include shows code lens.
#[tokio::test]
async fn include_shows_code_lens() {
    check_code_lens(
        r#"
module MyModule <lens title="include">
end

class MyClass
  include MyModule
end
"#,
    )
    .await;
}

/// Basic prepend shows code lens.
#[tokio::test]
async fn prepend_shows_code_lens() {
    check_code_lens(
        r#"
module MyModule <lens title="prepend">
end

class MyClass
  prepend MyModule
end
"#,
    )
    .await;
}

/// Basic extend shows code lens.
#[tokio::test]
async fn extend_shows_code_lens() {
    check_code_lens(
        r#"
module MyModule <lens title="extend">
end

class MyClass
  extend MyModule
end
"#,
    )
    .await;
}

/// No usages means no code lens.
#[tokio::test]
async fn no_usage_no_code_lens() {
    check_no_code_lens(
        r#"
module MyModule
end

class MyClass
end
"#,
    )
    .await;
}

/// Nested module with qualified include.
#[tokio::test]
async fn nested_module_include() {
    check_code_lens(
        r#"
module Outer
  module Inner <lens title="include">
  end
end

class MyClass
  include Outer::Inner
end
"#,
    )
    .await;
}

/// Multiple mixin types on same module.
#[tokio::test]
async fn multiple_mixin_types() {
    check_code_lens(
        r#"
module MyModule <lens title="include"> <lens title="extend"> <lens title="prepend"> <lens title="class">
end

class MyClass
  include MyModule
end

class AnotherClass
  extend MyModule
end

module AnotherModule
  prepend MyModule
end
"#,
    )
    .await;
}

// ============================================================================
// Transitive Tests
// ============================================================================

/// Transitive module usage: A -> B -> Class.
#[tokio::test]
async fn transitive_module_usage() {
    check_code_lens(
        r#"
module A <lens title="include"> <lens title="class">
end

module B
  include A
end

class MyClass
  include B
end
"#,
    )
    .await;
}

/// Multiple transitive classes.
#[tokio::test]
async fn multiple_transitive_classes() {
    check_code_lens(
        r#"
module A <lens title="2 include"> <lens title="3 classes">
end

module B
  include A
end

class Class1
  include B
end

class Class2
  include B
end

class Class3
  include A
end
"#,
    )
    .await;
}
