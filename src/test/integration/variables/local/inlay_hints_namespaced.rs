use crate::test::harness::check;

#[tokio::test]
async fn namespaced_class_new() {
    check(
        r#"
module MyApp
  class User
  end
end

# Should infer MyApp::User
user<hint label="MyApp::User"> = MyApp::User.new
"#,
    )
    .await;
}

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

# Should infer Core::Auth::Identity
id<hint label="Core::Auth::Identity"> = Core::Auth::Identity.new
"#,
    )
    .await;
}
