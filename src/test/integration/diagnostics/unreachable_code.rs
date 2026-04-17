//! Tests for unreachable-code diagnostic (V1: explicit terminator only).
//!
//! V1 flags statements following an explicit terminator (return/break/next/raise/throw)
//! within the same StatementsNode. Branch-aware (all-branches-terminate) is V2.

use crate::test::harness::check;

#[tokio::test]
async fn test_after_return() {
    check(
        r#"
def foo
  return 1
  <warn code="unreachable-code">puts "dead"</warn>
end
"#,
    )
    .await;
}

#[tokio::test]
async fn test_after_raise() {
    check(
        r#"
def foo
  raise "boom"
  <warn code="unreachable-code">puts "dead"</warn>
end
"#,
    )
    .await;
}

#[tokio::test]
async fn test_after_break_in_loop() {
    check(
        r#"
[1, 2, 3].each do |x|
  break
  <warn code="unreachable-code">puts x</warn>
end
"#,
    )
    .await;
}

#[tokio::test]
async fn test_after_next_in_loop() {
    check(
        r#"
[1, 2, 3].each do |x|
  next
  <warn code="unreachable-code">puts x</warn>
end
"#,
    )
    .await;
}

#[tokio::test]
async fn test_multiple_dead_stmts_each_flagged() {
    check(
        r#"
def foo
  return
  <warn code="unreachable-code">a = 1</warn>
  <warn code="unreachable-code">b = 2</warn>
end
"#,
    )
    .await;
}

#[tokio::test]
async fn test_modifier_if_return_not_terminator() {
    // `return if cond` is conditional → fall-through allowed
    check(
        r#"
<warn none code="unreachable-code">
def foo
  return if true
  puts "live"
end
</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn test_no_diag_when_return_is_last() {
    check(
        r#"
<warn none code="unreachable-code">
def foo
  puts "alive"
  return 1
end
</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn test_no_diag_v1_for_if_with_all_branches_returning() {
    // V2 will flag this. V1 does not — only explicit top-level terminators.
    check(
        r#"
<warn none code="unreachable-code">
def foo
  if x
    return 1
  else
    return 2
  end
  puts "missed by v1"
end
</warn>
"#,
    )
    .await;
}
