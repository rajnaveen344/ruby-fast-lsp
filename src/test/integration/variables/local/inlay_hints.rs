//! Inlay hint tests for variables (literals and constructors).

use crate::test::harness::{check_inlay_hints, get_hint_label, get_inlay_hints};

// ============================================================================
// Literal Tests
// ============================================================================

/// String literal gets `: String` type hint.
#[tokio::test]
async fn string_literal() {
    check_inlay_hints(
        r#"
x<hint:String> = "hello"
"#,
    )
    .await;
}

/// Integer literal gets `: Integer` type hint.
#[tokio::test]
async fn integer_literal() {
    check_inlay_hints(
        r#"
x<hint:Integer> = 42
"#,
    )
    .await;
}

/// Float literal gets `: Float` type hint.
#[tokio::test]
async fn float_literal() {
    check_inlay_hints(
        r#"
x<hint:Float> = 3.14
"#,
    )
    .await;
}

/// Symbol literal gets `: Symbol` type hint.
#[tokio::test]
async fn symbol_literal() {
    check_inlay_hints(
        r#"
x<hint:Symbol> = :foo
"#,
    )
    .await;
}

/// Array literal gets `: Array` type hint.
#[tokio::test]
async fn array_literal() {
    check_inlay_hints(
        r#"
x<hint:Array> = [1, 2, 3]
"#,
    )
    .await;
}

/// Hash literal gets `: Hash` type hint.
#[tokio::test]
async fn hash_literal() {
    check_inlay_hints(
        r#"
x<hint:Hash> = { a: 1 }
"#,
    )
    .await;
}

// ============================================================================
// Constructor Tests
// ============================================================================

/// Class.new gets class instance type hint.
#[tokio::test]
async fn class_new() {
    check_inlay_hints(
        r#"
class User
end

user<hint:User> = User.new
"#,
    )
    .await;
}

/// Nested class constructor.
#[tokio::test]
async fn nested_class_new() {
    let hints = get_inlay_hints(
        r#"
module MyApp
  class User
  end
end

user = MyApp::User.new
"#,
    )
    .await;

    // Should have User type hint
    let user_hint = hints.iter().any(|h| {
        let label = get_hint_label(h);
        label.contains("User")
    });

    assert!(
        user_hint,
        "Expected User type hint for MyApp::User.new, got: {:?}",
        hints.iter().map(get_hint_label).collect::<Vec<_>>()
    );
}
