//! Tests for `raise-non-exception` diagnostic.
//!
//! V1: warns only when the `raise` argument is provably not an Exception
//! subclass. Conservative — silent on uncertain types (variables, method
//! calls, etc.).

use crate::test::harness::check;

#[tokio::test]
async fn raise_string_no_warn() {
    check(r#"
<warn none code="raise-non-exception">raise "boom"</warn>
"#)
    .await;
}

#[tokio::test]
async fn raise_stdlib_exception_no_warn() {
    check(r#"
<warn none code="raise-non-exception">raise StandardError</warn>
<warn none code="raise-non-exception">raise ArgumentError, "msg"</warn>
"#)
    .await;
}

#[tokio::test]
async fn raise_user_exception_subclass_no_warn() {
    check(r#"
class MyError < StandardError
end

<warn none code="raise-non-exception">raise MyError</warn>
"#)
    .await;
}

#[tokio::test]
async fn raise_integer_warns() {
    check(r#"
raise <warn code="raise-non-exception">42</warn>
"#)
    .await;
}

#[tokio::test]
async fn raise_array_warns() {
    check(r#"
raise <warn code="raise-non-exception">[1, 2]</warn>
"#)
    .await;
}

#[tokio::test]
async fn raise_symbol_warns() {
    check(r#"
raise <warn code="raise-non-exception">:oops</warn>
"#)
    .await;
}

#[tokio::test]
async fn raise_nil_warns() {
    check(r#"
raise <warn code="raise-non-exception">nil</warn>
"#)
    .await;
}

#[tokio::test]
async fn raise_non_exception_class_warns() {
    check(r#"
class PlainClass
end

raise <warn code="raise-non-exception">PlainClass</warn>
"#)
    .await;
}

#[tokio::test]
async fn raise_variable_no_warn() {
    // Variable type uncertain — be conservative.
    check(r#"
err = StandardError.new
<warn none code="raise-non-exception">raise err</warn>
"#)
    .await;
}

#[tokio::test]
async fn raise_no_args_no_warn() {
    // Bare `raise` is re-raise of $! — no arg to validate.
    check(r#"
def x
  begin
    1
  rescue
    <warn none code="raise-non-exception">raise</warn>
  end
end
"#)
    .await;
}

#[tokio::test]
async fn raise_unknown_constant_with_error_suffix_no_warn() {
    // Suffix heuristic avoids noise on unindexed user errors.
    check(r#"
<warn none code="raise-non-exception">raise SomeUnindexedError</warn>
"#)
    .await;
}
