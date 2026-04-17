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

// V2: type-aware checks for LocalVariableRead and CallNode args.

#[tokio::test]
async fn raise_method_returning_integer_warns() {
    check(r#"
def get_error
  42
end

raise <warn code="raise-non-exception">get_error</warn>
"#)
    .await;
}

#[tokio::test]
async fn raise_method_returning_string_no_warn() {
    // String return → Ruby wraps in RuntimeError.
    check(r#"
def msg
  "boom"
end

<warn none code="raise-non-exception">raise msg</warn>
"#)
    .await;
}

#[tokio::test]
async fn raise_method_returning_array_warns() {
    check(r#"
def stuff
  [1, 2, 3]
end

raise <warn code="raise-non-exception">stuff</warn>
"#)
    .await;
}

#[tokio::test]
async fn raise_method_with_uncertain_return_no_warn() {
    // Method returns Union/Unknown → skip (conservative).
    check(r#"
def maybe(cond)
  if cond
    StandardError
  else
    42
  end
end

<warn none code="raise-non-exception">raise maybe(true)</warn>
"#)
    .await;
}

#[tokio::test]
async fn raise_local_var_assigned_integer_warns() {
    check(r#"
x = 42
raise <warn code="raise-non-exception">x</warn>
"#)
    .await;
}

#[tokio::test]
async fn raise_local_var_assigned_array_warns() {
    check(r#"
errs = [1, 2]
raise <warn code="raise-non-exception">errs</warn>
"#)
    .await;
}

#[tokio::test]
async fn raise_local_var_uncertain_no_warn() {
    // Variable type Unknown after unresolvable method call → skip.
    check(r#"
x = some_unknown_method
<warn none code="raise-non-exception">raise x</warn>
"#)
    .await;
}
