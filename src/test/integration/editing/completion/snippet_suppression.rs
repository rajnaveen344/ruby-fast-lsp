//! Tests that snippets are suppressed in value positions.
//!
//! Snippets (if, def, class, etc.) should only appear in statement positions
//! (inside StatementsNode), not in value positions like arguments, arrays,
//! hashes, strings, etc.

use crate::test::harness::check;

// ─── Statement positions: snippets SHOULD appear ───

#[tokio::test]
async fn snippets_in_method_body() {
    check(
        r#"
def foo
  i$0
end
<complete items="if">
"#,
    )
    .await;
}

#[tokio::test]
async fn snippets_at_top_level() {
    check(
        r#"
d$0
<complete items="def">
"#,
    )
    .await;
}

#[tokio::test]
async fn snippets_in_class_body() {
    check(
        r#"
class Foo
  d$0
end
<complete items="def">
"#,
    )
    .await;
}

#[tokio::test]
async fn snippets_in_block_body() {
    check(
        r#"
[1].each do |x|
  i$0
end
<complete items="if">
"#,
    )
    .await;
}

// ─── Value positions: snippets should NOT appear ───

#[tokio::test]
async fn no_snippets_in_method_arguments() {
    check(
        r#"
def foo(x)
end

y = 1
foo(y$0)
<complete items="y" excludes="if,def,class,module,begin rescue">
"#,
    )
    .await;
}

#[tokio::test]
async fn no_snippets_in_array_literal() {
    check(
        r#"
x = 1
a = [x$0]
<complete items="x" excludes="if,def,class,module,begin rescue">
"#,
    )
    .await;
}

#[tokio::test]
async fn no_snippets_in_hash_value() {
    check(
        r#"
x = 1
h = { key: x$0 }
<complete items="x" excludes="if,def,class,module,begin rescue">
"#,
    )
    .await;
}

#[tokio::test]
async fn no_snippets_in_string_interpolation() {
    check(
        r#"
x = 1
s = "hello #{x$0}"
<complete items="x" excludes="if,def,class,module,begin rescue">
"#,
    )
    .await;
}
