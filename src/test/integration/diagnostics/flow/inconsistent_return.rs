//! Tests for `inconsistent-return` diagnostic.
//!
//! Flags methods where:
//! - Body falls through (`control_flow::analyze` returns Falls), AND
//! - Body contains at least one explicit `return EXPR` (with a value), AND
//! - The body's last statement is a "void call" (e.g., `puts`, `print`, `p`, `pp`, `warn`)
//!   — strongly indicating the method's intent is side-effects, but a path leaks a value.
//!
//! Conservative — won't fire when the last expression has a meaningful value.

use crate::test::harness::check;

#[tokio::test]
async fn flags_explicit_return_then_puts_fallthrough() {
    check(
        r#"
<warn code="inconsistent-return">def find(id)
  return 42 if id > 0
  puts "negative"
end</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn flags_explicit_return_then_print_fallthrough() {
    check(
        r#"
<warn code="inconsistent-return">def m(x)
  return x if x
  print "no value"
end</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn flags_explicit_return_then_warn_fallthrough() {
    check(
        r#"
<warn code="inconsistent-return">def m(x)
  return x.upcase unless x.nil?
  warn "got nil"
end</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn no_flag_when_last_expr_is_value() {
    // Last expression `compute(id)` produces a value — implicit return is meaningful.
    check(
        r#"
<warn none code="inconsistent-return">
def find(id)
  return cached if cache_ok
  compute(id)
end
</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn no_flag_when_all_paths_diverge() {
    check(
        r#"
<warn none code="inconsistent-return">
def m(x)
  if x then return 1 else return 2 end
end
</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn no_flag_when_no_explicit_return() {
    // Method only does side-effects — that's the whole intent. Don't flag.
    check(
        r#"
<warn none code="inconsistent-return">
def log(msg)
  puts msg
end
</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn no_flag_when_explicit_return_is_bare() {
    // `return` (no value) → returns nil; consistent with fallthrough nil. Not a smell.
    check(
        r#"
<warn none code="inconsistent-return">
def m(x)
  return if x
  puts "x"
end
</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn no_flag_when_explicit_return_is_nil_literal() {
    // `return nil` is explicit-but-equivalent-to-fallthrough — not inconsistent.
    check(
        r#"
<warn none code="inconsistent-return">
def m(x)
  return nil if x
  puts "x"
end
</warn>
"#,
    )
    .await;
}
