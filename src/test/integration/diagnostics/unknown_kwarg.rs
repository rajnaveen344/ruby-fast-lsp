//! Tests for `unknown-kwarg` diagnostic.
//!
//! V1: warns when a keyword arg at the callsite isn't declared on the method
//! and the method has no `**kwargs` rest. Levenshtein suggestion when close.
//! Skips: callsites with `**opts` splat (unknown keys), receivers we can't
//! statically resolve, defs that accept `**kw` rest.

use crate::test::harness::check;

#[tokio::test]
async fn typo_kwarg_warns_with_suggestion() {
    check(
        r#"
def greet(name:, age: 0)
  name
end

greet(name: "x", <warn code="unknown-kwarg" message="Did you mean `age:`?">agee</warn>: 30)
"#,
    )
    .await;
}

#[tokio::test]
async fn valid_kwarg_no_warn() {
    check(
        r#"
def greet(name:, age: 0)
  name
end

<warn none code="unknown-kwarg">greet(name: "x", age: 30)</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn kwrest_accepts_anything_no_warn() {
    check(
        r#"
def greet(**opts)
  opts
end

<warn none code="unknown-kwarg">greet(anything: 1, here: 2)</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn double_splat_at_callsite_skips_check() {
    check(
        r#"
def greet(name:)
  name
end

opts = { name: "x", agee: 30 }
<warn none code="unknown-kwarg">greet(**opts)</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn unknown_kwarg_constant_receiver_warns() {
    check(
        r#"
class Foo
  def self.bar(name:)
    name
  end
end

Foo.bar(<warn code="unknown-kwarg">nme</warn>: "x")
"#,
    )
    .await;
}

#[tokio::test]
async fn unknown_kwarg_expr_receiver_warns() {
    check(
        r#"
class User
  def update(name:)
    name
  end
end

u = User.new
u.update(<warn code="unknown-kwarg">naem</warn>: "x")
"#,
    )
    .await;
}
