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
async fn test_if_all_branches_return_flags_after() {
    // V2: branch-aware — `if` whose then+else both return is itself a terminator.
    check(
        r#"
def foo
  if x
    return 1
  else
    return 2
  end
  <warn code="unreachable-code">puts "dead"</warn>
end
"#,
    )
    .await;
}

#[tokio::test]
async fn test_if_no_else_does_not_flag_after() {
    check(
        r#"
<warn none code="unreachable-code">
def foo
  if x
    return 1
  end
  puts "live"
end
</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn test_unless_all_branches_return_flags_after() {
    check(
        r#"
def foo
  unless x
    return 1
  else
    return 2
  end
  <warn code="unreachable-code">puts "dead"</warn>
end
"#,
    )
    .await;
}

#[tokio::test]
async fn test_case_all_branches_return_flags_after() {
    check(
        r#"
def foo
  case x
  when 1 then return 1
  when 2 then raise "boom"
  else return 3
  end
  <warn code="unreachable-code">puts "dead"</warn>
end
"#,
    )
    .await;
}

#[tokio::test]
async fn test_case_missing_else_does_not_flag_after() {
    check(
        r#"
<warn none code="unreachable-code">
def foo
  case x
  when 1 then return 1
  when 2 then return 2
  end
  puts "live"
end
</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn test_begin_body_and_rescue_both_return_flags_after() {
    check(
        r#"
def foo
  begin
    return 1
  rescue => e
    return 2
  end
  <warn code="unreachable-code">puts "dead"</warn>
end
"#,
    )
    .await;
}

#[tokio::test]
async fn test_begin_ensure_diverges_flags_after() {
    check(
        r#"
def foo
  begin
    1
  ensure
    return 99
  end
  <warn code="unreachable-code">puts "dead"</warn>
end
"#,
    )
    .await;
}

#[tokio::test]
async fn test_loop_no_break_flags_after() {
    check(
        r#"
def foo
  loop do
    work
  end
  <warn code="unreachable-code">puts "dead"</warn>
end
"#,
    )
    .await;
}

#[tokio::test]
async fn test_loop_with_break_does_not_flag_after() {
    check(
        r#"
<warn none code="unreachable-code">
def foo
  loop do
    break if cond
  end
  puts "live"
end
</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn test_loop_with_nested_each_break_flags_after() {
    // Inner each's break does not exit the outer loop.
    check(
        r#"
def foo
  loop do
    [1].each { break }
  end
  <warn code="unreachable-code">puts "dead"</warn>
end
"#,
    )
    .await;
}

#[tokio::test]
async fn test_for_loop_does_not_terminate_after() {
    // for may not iterate (empty collection) → trailing stmt is live.
    check(
        r#"
<warn none code="unreachable-code">
def foo
  for x in []
    return
  end
  puts "live"
end
</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn test_nested_if_all_terminating_propagates() {
    check(
        r#"
def foo
  if a
    return 1
  else
    if b
      return 2
    else
      return 3
    end
  end
  <warn code="unreachable-code">puts "dead"</warn>
end
"#,
    )
    .await;
}
