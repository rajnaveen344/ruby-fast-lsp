//! Inlay hints for variables assigned from Class.new constructors.

use crate::test::harness::check;

/// Simple class constructor
#[tokio::test]
async fn class_new() {
    check(
        r#"
class User
end

user<hint label="User"> = User.new
"#,
    )
    .await;
}

/// Namespaced class constructor
#[tokio::test]
async fn namespaced_class_new() {
    check(
        r#"
module MyApp
  class User
  end
end

user<hint label="MyApp::User"> = MyApp::User.new
"#,
    )
    .await;
}

/// Deeply nested class constructor
#[tokio::test]
async fn deeply_nested_class_new() {
    check(
        r#"
module Core
  module Auth
    class Identity
    end
  end
end

id<hint label="Core::Auth::Identity"> = Core::Auth::Identity.new
"#,
    )
    .await;
}
