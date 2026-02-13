//! Reference tests for local variables.

use crate::test::harness::check;

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
