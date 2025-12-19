//! Code lens tests for transitive module usage.

use crate::test::harness::get_code_lenses;

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
