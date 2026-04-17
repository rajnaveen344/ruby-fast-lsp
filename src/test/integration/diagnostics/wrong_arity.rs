//! Tests for `wrong-arity` diagnostic.
//!
//! V1 scope:
//! - Positional args only — splat (`*args`) at callsite or `*args` in def → skip check.
//! - Kwargs not validated yet (separate `unknown-kwarg` diagnostic later).
//! - Receivers covered: no-receiver (current namespace) and constant receivers (`Foo.bar`).
//! - Expression receivers (`u.foo(...)`) deferred to V2.
//!
//! Skip if method can't be strictly resolved on owner+ancestors (avoid double-warning
//! with `unresolved-method`).

use crate::test::harness::check;

#[tokio::test]
async fn too_few_positional_warns() {
    check(
        r#"
def greet(name)
  name
end

<warn code="wrong-arity">greet</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn too_many_positional_warns() {
    check(
        r#"
def greet(name)
  name
end

<warn code="wrong-arity">greet</warn>("a", "b")
"#,
    )
    .await;
}

#[tokio::test]
async fn exact_match_no_warn() {
    check(
        r#"
def greet(name)
  name
end

<warn none code="wrong-arity">greet("a")</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn optional_param_within_range_no_warn() {
    check(
        r#"
def greet(name, age = 0)
  name
end

<warn none code="wrong-arity">greet("a")</warn>
<warn none code="wrong-arity">greet("a", 1)</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn optional_param_too_many_warns() {
    check(
        r#"
def greet(name, age = 0)
  name
end

<warn code="wrong-arity">greet</warn>("a", 1, 2)
"#,
    )
    .await;
}

#[tokio::test]
async fn rest_param_unbounded_no_warn() {
    check(
        r#"
def greet(*args)
  args
end

<warn none code="wrong-arity">greet</warn>
<warn none code="wrong-arity">greet("a", "b", "c", "d")</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn splat_at_callsite_skips_check() {
    // Splat at callsite means we don't know argument count — be silent.
    check(
        r#"
def greet(name)
  name
end

args = ["a", "b", "c"]
<warn none code="wrong-arity">greet(*args)</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn unresolved_method_no_arity_warn() {
    // Method doesn't exist → unresolved-method handles it; no double-warn.
    check(
        r#"
<warn none code="wrong-arity">does_not_exist("a", "b")</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn constant_receiver_too_few_warns() {
    check(
        r#"
class Foo
  def self.bar(x, y)
    x + y
  end
end

Foo.<warn code="wrong-arity">bar</warn>(1)
"#,
    )
    .await;
}

#[tokio::test]
async fn constant_receiver_exact_no_warn() {
    check(
        r#"
class Foo
  def self.bar(x, y)
    x + y
  end
end

<warn none code="wrong-arity">Foo.bar(1, 2)</warn>
"#,
    )
    .await;
}
