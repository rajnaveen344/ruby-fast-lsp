//! Inlay hint tests for constructor calls (Class.new).

use crate::test::harness::{check_inlay_hints, get_hint_label, get_inlay_hints};

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
