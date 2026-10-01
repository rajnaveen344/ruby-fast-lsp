//! References for helper calls inside framework and top-level DSL blocks.

use crate::test::harness::{check, FakeEditor};

#[tokio::test]
async fn references_instance_method_ignores_same_named_top_level_dsl_call() {
    check(
        r#"
class AdminAPIClient
  def post$0
    "ok"
  end
end

post "/admin/widgets", to: "widgets#create"

AdminAPIClient.new.<ref>post</ref>
"#,
    )
    .await;
}

#[tokio::test]
async fn references_helper_method_from_framework_route_block() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "commerce.rb",
            r#"module App
  module API
    module Commerce
      def fetch_credits
        []
      end
    end
  end
end
"#,
        )
        .await;
    editor
        .open(
            "api.rb",
            r#"module App
  module API
    include App::API::Commerce
  end
end
"#,
        )
        .await;
    editor
        .open(
            "base_app.rb",
            r#"class BaseApp
  helpers do
    include App::API
  end
end
"#,
        )
        .await;
    editor
        .open(
            "admin_app.rb",
            r#"class AdminApp < BaseApp
  get "/credits" do
    fetch_credits
  end
end
"#,
        )
        .await;

    let refs = editor.references_at("commerce.rb", 3, 10).await;
    assert!(
        refs.iter()
            .any(|loc| loc.uri.path().ends_with("/admin_app.rb")),
        "included API method references should include framework route block call, got {refs:?}"
    );
}
