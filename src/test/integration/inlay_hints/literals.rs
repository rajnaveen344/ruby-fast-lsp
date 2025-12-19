//! Inlay hint tests for literal types.

use crate::test::harness::check_inlay_hints;

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
