//! Tests for `bad-splat` diagnostic.
//!
//! V1: warns when `*expr` target is provably non-Array (Integer/Float/String/
//! Symbol/Boolean/Hash/Range/NilClass) or `**expr` target is provably non-Hash.
//! Conservative — silent on user-defined classes (could define `to_a`/`to_hash`)
//! and Union/Unknown types.

use crate::test::harness::check;

// ---------- positional splat (*expr) ----------

#[tokio::test]
async fn splat_array_literal_no_warn() {
    check(
        r#"
def greet(name)
  name
end

<warn none code="bad-splat">greet(*["a"])</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn splat_nil_no_warn() {
    // Ruby tolerates *nil (treated as []).
    check(
        r#"
def greet(name)
  name
end

<warn none code="bad-splat">greet(*nil)</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn splat_integer_warns() {
    check(
        r#"
def greet(name)
  name
end

greet(<warn code="bad-splat">*42</warn>)
"#,
    )
    .await;
}

#[tokio::test]
async fn splat_string_warns() {
    check(
        r#"
def greet(name)
  name
end

greet(<warn code="bad-splat">*"hi"</warn>)
"#,
    )
    .await;
}

#[tokio::test]
async fn splat_hash_literal_warns() {
    check(
        r#"
def greet(name)
  name
end

greet(<warn code="bad-splat">*{a: 1}</warn>)
"#,
    )
    .await;
}

#[tokio::test]
async fn splat_local_var_integer_warns() {
    check(
        r#"
def greet(name)
  name
end

x = 42
greet(<warn code="bad-splat">*x</warn>)
"#,
    )
    .await;
}

#[tokio::test]
async fn splat_local_var_array_no_warn() {
    check(
        r#"
def greet(name)
  name
end

xs = [1, 2]
<warn none code="bad-splat">greet(*xs)</warn>
"#,
    )
    .await;
}

// ---------- kwarg splat (**expr) ----------

#[tokio::test]
async fn double_splat_hash_literal_no_warn() {
    check(
        r#"
def greet(**opts)
  opts
end

<warn none code="bad-splat">greet(**{a: 1})</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn double_splat_nil_no_warn() {
    check(
        r#"
def greet(**opts)
  opts
end

<warn none code="bad-splat">greet(**nil)</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn double_splat_integer_warns() {
    check(
        r#"
def greet(**opts)
  opts
end

greet(<warn code="bad-splat">**42</warn>)
"#,
    )
    .await;
}

#[tokio::test]
async fn double_splat_array_warns() {
    check(
        r#"
def greet(**opts)
  opts
end

greet(<warn code="bad-splat">**[1, 2]</warn>)
"#,
    )
    .await;
}

#[tokio::test]
async fn double_splat_local_var_hash_no_warn() {
    check(
        r#"
def greet(**opts)
  opts
end

h = {a: 1}
<warn none code="bad-splat">greet(**h)</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn double_splat_local_var_integer_warns() {
    check(
        r#"
def greet(**opts)
  opts
end

x = 42
greet(<warn code="bad-splat">**x</warn>)
"#,
    )
    .await;
}
