//! Goto definition tests for classes and modules.

use crate::test::harness::check_goto;

/// Goto definition for a class reference.
#[tokio::test]
async fn goto_class() {
    check_goto(
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
    check_goto(
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
    check_goto(
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
    check_goto(
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
    check_goto(
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
