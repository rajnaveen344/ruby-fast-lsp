//! Code lens tests for module mixins (include, extend, prepend).

use crate::test::harness::{check_code_lens, check_no_code_lens, get_code_lenses};

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
