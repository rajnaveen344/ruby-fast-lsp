//! Tests for type inference precision when one branch of a conditional diverges.
//!
//! When a branch of `if`/`unless`/`case` always exits (return/raise/break/...),
//! its result type is pruned from the union — the join point is never reached
//! via that branch. Implemented in `inferrer/type_tracker` via
//! `ruby_analysis::inference::control_flow`.
//!
//! These assertions target the method return-type hint (the consumer that
//! actually flows through TypeTracker's union). A `raise` path contributes no
//! return value, while an explicit `return` path still contributes its value
//! to the enclosing method even though it is pruned from the local join point.

use crate::test::harness::check;

#[tokio::test]
async fn if_else_raise_prunes_to_then_type() {
    check(
        r#"
def m(cond)<hint label=" -> Integer">
  if cond then 1 else raise "no" end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn if_then_raise_prunes_to_else_type() {
    check(
        r#"
def m(cond)<hint label=" -> String">
  if cond then raise "no" else "ok" end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn unless_else_raise_prunes_to_then_type() {
    check(
        r#"
def m(cond)<hint label=" -> Integer">
  unless cond then 1 else raise "no" end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn case_else_raises_pruned() {
    check(
        r#"
def m(v)<hint label=" -> (Integer | String)">
  case v
  when 1 then 1
  when 2 then "two"
  else raise "no"
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn if_else_return_includes_the_explicit_nil_result() {
    check(
        r#"
def m(cond)<hint label=" -> (Integer | NilClass)">
  if cond then 1 else return end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn nested_if_in_else_with_diverging_propagates() {
    check(
        r#"
def m(a, b)<hint label=" -> Integer">
  if a then 1 else if b then raise "x" else raise "y" end end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn no_pruning_when_no_branch_diverges() {
    check(
        r#"
def m(cond)<hint label=" -> (Integer | String)">
  if cond then 1 else "two" end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn loop_no_break_prunes_branch() {
    // `loop { ... }` with no break diverges → if-else with loop in else
    // prunes to then-branch type.
    check(
        r#"
def m(cond)<hint label=" -> Integer">
  if cond then 1 else loop { do_work } end
end
"#,
    )
    .await;
}
