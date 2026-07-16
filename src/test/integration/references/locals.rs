//! Reference tests for local variables.

use crate::test::harness::{check, FakeEditor};

/// Find references to a method parameter - should find definition and all usages
#[tokio::test]
async fn references_method_parameter() {
    check(
        r#"
def greet(<ref>name$0</ref>)
  puts "Hello, #{<ref>name</ref>}!"
  puts <ref>name</ref>.upcase
end
"#,
    )
    .await;
}

/// Multiple method parameters - first param
#[tokio::test]
async fn references_multiple_params_first() {
    check(
        r#"
def test(<ref>abc$0</ref>, defg)
  puts <ref>abc</ref>
  puts defg
end
"#,
    )
    .await;
}

/// Multiple method parameters - second param
#[tokio::test]
async fn references_multiple_params_second() {
    check(
        r#"
def test(abc, <ref>defg$0</ref>)
  puts abc
  puts <ref>defg</ref>
end
"#,
    )
    .await;
}

/// Multiple params with code in between
#[tokio::test]
async fn references_multiple_params_with_code() {
    check(
        r#"
def test(<ref>a$0</ref>, b, c)
  puts <ref>a</ref>
  # code
  puts b
  puts c
end
"#,
    )
    .await;
}

/// Find references to a local variable
#[tokio::test]
async fn references_local_variable() {
    check(
        r#"
<ref>x$0</ref> = 1
puts <ref>x</ref>
"#,
    )
    .await;
}

#[tokio::test]
async fn references_pattern_capture() {
    check(
        r#"
case {user: "Ada"}
in {user: <ref>user$0</ref>}
  puts <ref>user</ref>
end
"#,
    )
    .await;
}

#[tokio::test]
async fn references_local_variable_survives_reopen_without_reindex() {
    let mut editor = FakeEditor::new().await;
    let content = "def work\n  user = 1\n  puts user\nend\n";

    editor.open("local_refs_reopen.rb", content).await;
    editor.close("local_refs_reopen.rb").await;
    editor.open("local_refs_reopen.rb", content).await;

    let locations = editor.references_at("local_refs_reopen.rb", 2, 7).await;
    assert_eq!(locations.len(), 2);
    assert!(locations
        .iter()
        .any(|location| location.range.start.line == 1 && location.range.start.character == 2));
    assert!(locations
        .iter()
        .any(|location| location.range.start.line == 2 && location.range.start.character == 7));
}

/// Find references to a variable captured in a block
#[tokio::test]
async fn references_captured_variable() {
    check(
        r#"
def example
  <ref>x$0</ref> = 1
  [1,2].each do |n|
    puts <ref>x</ref>
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn references_local_captured_by_eval_and_exec_blocks() {
    check(
        r#"
class MetaTarget
end

<ref>captured$0</ref> = "outside"
MetaTarget.class_eval do
  puts <ref>captured</ref>
end
MetaTarget.instance_exec do
  puts <ref>captured</ref>
end
"#,
    )
    .await;
}

#[tokio::test]
async fn all_static_execution_and_dynamic_definition_blocks_capture_outer_locals() {
    check(
        r#"
class MetaTarget
end

<ref>captured$0</ref> = "outside"
MetaTarget.module_eval do
  puts <ref>captured</ref>
end
MetaTarget.class_exec do
  puts <ref>captured</ref>
end
MetaTarget.module_exec do
  puts <ref>captured</ref>
end
MetaTarget.instance_eval do
  puts <ref>captured</ref>
end
MetaTarget.instance_exec do
  puts <ref>captured</ref>
end
MetaTarget.send(:define_method, :generated) do
  puts <ref>captured</ref>
end
MetaTarget.define_singleton_method(:generated_singleton) do
  puts <ref>captured</ref>
end
"#,
    )
    .await;
}
