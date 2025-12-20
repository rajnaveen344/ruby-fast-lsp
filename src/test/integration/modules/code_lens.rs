//! Code lens tests for modules.

use crate::test::harness::{check_code_lens, check_no_code_lens, get_code_lenses};

// ============================================================================
// Mixin Tests
// ============================================================================

/// Basic include shows code lens.
#[tokio::test]
async fn include_shows_code_lens() {
    check_code_lens(
        r#"
module MyModule <lens:include>
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
module MyModule <lens:prepend>
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
module MyModule <lens:extend>
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
  module Inner <lens:include>
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
    let lenses = get_code_lenses(
        r#"
module MyModule
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

    let titles: Vec<String> = lenses
        .iter()
        .filter_map(|l| l.command.as_ref().map(|c| c.title.clone()))
        .collect();

    assert!(titles.iter().any(|t| t.contains("include")));
    assert!(titles.iter().any(|t| t.contains("extend")));
    assert!(titles.iter().any(|t| t.contains("prepend")));
    assert!(titles.iter().any(|t| t.contains("class")));
}

// ============================================================================
// Transitive Tests
// ============================================================================

/// Transitive module usage: A -> B -> Class.
#[tokio::test]
async fn transitive_module_usage() {
    let lenses = get_code_lenses(
        r#"
module A
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

    // Module A should have code lens (used in B, transitively in MyClass)
    let module_a_lenses: Vec<_> = lenses.iter().filter(|l| l.range.start.line == 1).collect();
    assert!(
        module_a_lenses.len() >= 2,
        "Module A should have include and class code lenses"
    );

    let module_a_titles: Vec<String> = module_a_lenses
        .iter()
        .filter_map(|l| l.command.as_ref().map(|c| c.title.clone()))
        .collect();

    assert!(
        module_a_titles.iter().any(|t| t.contains("include")),
        "Expected include lens for A, got: {:?}",
        module_a_titles
    );
    assert!(
        module_a_titles.iter().any(|t| t.contains("class")),
        "Expected class lens for A, got: {:?}",
        module_a_titles
    );
}

/// Multiple transitive classes.
#[tokio::test]
async fn multiple_transitive_classes() {
    let lenses = get_code_lenses(
        r#"
module A
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

    // Module A: used in B and Class3 directly, transitively in Class1 and Class2
    let module_a_lenses: Vec<_> = lenses.iter().filter(|l| l.range.start.line == 1).collect();

    let module_a_titles: Vec<String> = module_a_lenses
        .iter()
        .filter_map(|l| l.command.as_ref().map(|c| c.title.clone()))
        .collect();

    // Should show 2 includes (B and Class3) and 3 classes
    assert!(
        module_a_titles.iter().any(|t| t == "2 include"),
        "Expected '2 include' for A, got: {:?}",
        module_a_titles
    );
    assert!(
        module_a_titles.iter().any(|t| t == "3 classes"),
        "Expected '3 classes' for A, got: {:?}",
        module_a_titles
    );
}
