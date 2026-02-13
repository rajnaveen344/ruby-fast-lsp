//! Reference tests for local variables.

use crate::test::harness::check;

/// Find references to a method parameter - should find definition and all usages
#[tokio::test]
async fn references_method_parameter() {
    check(
        r#"
def greet(<ref>name</ref>)
  puts "Hello, #{name}!"
  puts name.upcase
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
x = 1
puts <ref>x</ref>
"#,
    )
    .await;
}

/// Find references to a variable captured in a block
#[tokio::test]
async fn references_captured_variable() {
    check(
        r#"
def example
  x = 1
  [1,2].each do |n|
    puts <ref>x</ref>
  end
end
"#,
    )
    .await;
}
