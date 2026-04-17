//! Tests for guard narrowing: predicates of diverging guards narrow the post-state env.
//!
//! When one branch of an if/unless diverges (return/raise/...), the surviving
//! branch's predicate truth value is fixed, allowing variables tested in the
//! predicate to be narrowed for code following the conditional.
//!
//! These assertions target the method return-type hint (the consumer that
//! actually flows through TypeTracker's narrowed env).
//!
//! Implemented in `inferrer/type_tracker/narrow.rs`.

use crate::test::harness::check;

#[tokio::test]
async fn return_if_nil_narrows_to_non_nil() {
    // `return if x.nil?` → after the guard, x cannot be nil.
    check(
        r#"
def m<hint label=" -> String">
  x = if cond then "hello" else nil end
  return if x.nil?
  x
end
"#,
    )
    .await;
}

#[tokio::test]
async fn return_unless_nil_narrows_to_nil() {
    // `return unless x.nil?` returns when x is non-nil; after the guard x IS nil.
    check(
        r#"
def m<hint label=" -> NilClass">
  x = if cond then "hello" else nil end
  return unless x.nil?
  x
end
"#,
    )
    .await;
}

#[tokio::test]
async fn raise_if_nil_narrows_to_non_nil() {
    check(
        r#"
def m<hint label=" -> Integer">
  x = if cond then 42 else nil end
  raise "boom" if x.nil?
  x
end
"#,
    )
    .await;
}

#[tokio::test]
async fn full_if_with_diverging_then_narrows_else_path() {
    // `if x.nil? then return end` — after the if, x is non-nil.
    check(
        r#"
def m<hint label=" -> String">
  x = if cond then "hello" else nil end
  if x.nil?
    return
  end
  x
end
"#,
    )
    .await;
}

#[tokio::test]
async fn return_unless_is_a_narrows_to_class() {
    // `return unless x.is_a?(String)` → after the guard, x is String.
    check(
        r#"
def m<hint label=" -> String">
  x = if cond then "hello" else 42 end
  return unless x.is_a?(String)
  x
end
"#,
    )
    .await;
}

#[tokio::test]
async fn return_unless_kind_of_narrows_to_class() {
    check(
        r#"
def m<hint label=" -> Integer">
  x = if cond then "hello" else 42 end
  return unless x.kind_of?(Integer)
  x
end
"#,
    )
    .await;
}

#[tokio::test]
async fn no_narrowing_when_neither_branch_diverges() {
    // Plain if/else without divergence — predicate truth value is not fixed
    // for code after, so no narrowing applies.
    check(
        r#"
def m<hint label=" -> (NilClass | String)">
  x = if cond then "hello" else nil end
  if x.nil?
    a = 1
  else
    a = 2
  end
  x
end
"#,
    )
    .await;
}

#[tokio::test]
async fn negated_nil_predicate_narrows() {
    // `return if !x.nil?` → after, x IS nil.
    check(
        r#"
def m<hint label=" -> NilClass">
  x = if cond then "hello" else nil end
  return if !x.nil?
  x
end
"#,
    )
    .await;
}
