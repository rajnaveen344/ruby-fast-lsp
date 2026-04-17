//! Tests for `missing-kwarg` diagnostic.
//!
//! Flags callsites that omit required keyword args declared on the resolved
//! method. Skips when the callsite passes `**opts` splat (could provide any
//! key) or when the method can't be strictly resolved.

use crate::test::harness::check;

#[tokio::test]
async fn missing_required_kwarg_warns() {
    check(r#"
def greet(name:, age: 0)
  name
end

<warn code="missing-kwarg">greet</warn>(age: 30)
"#)
    .await;
}

#[tokio::test]
async fn all_required_provided_no_warn() {
    check(r#"
def greet(name:, age: 0)
  name
end

<warn none code="missing-kwarg">greet(name: "x", age: 30)</warn>
"#)
    .await;
}

#[tokio::test]
async fn optional_kwargs_omittable() {
    check(r#"
def greet(name:, age: 0)
  name
end

<warn none code="missing-kwarg">greet(name: "x")</warn>
"#)
    .await;
}

#[tokio::test]
async fn double_splat_at_callsite_skips_check() {
    check(r#"
def greet(name:, role:)
  name
end

opts = { name: "x" }
<warn none code="missing-kwarg">greet(**opts)</warn>
"#)
    .await;
}

#[tokio::test]
async fn multiple_missing_grouped() {
    // All missing required kwargs reported in one warning.
    check(r#"
def greet(name:, role:, age: 0)
  name
end

<warn code="missing-kwarg" message="`name:`, `role:`">greet</warn>(age: 30)
"#)
    .await;
}

#[tokio::test]
async fn expr_receiver_missing_warns() {
    check(r#"
class User
  def update(name:, role:)
    name
  end
end

u = User.new
u.<warn code="missing-kwarg">update</warn>(name: "x")
"#)
    .await;
}

#[tokio::test]
async fn constant_receiver_missing_warns() {
    check(r#"
class Foo
  def self.bar(name:, age:)
    name
  end
end

Foo.<warn code="missing-kwarg">bar</warn>(name: "x")
"#)
    .await;
}
