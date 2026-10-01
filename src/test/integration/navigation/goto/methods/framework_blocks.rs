//! Goto definition for helper calls inside framework route blocks.

use crate::test::harness::FakeEditor;

#[tokio::test]
async fn goto_helper_method_from_framework_route_block() {
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

    let defs = editor.goto_def_at("admin_app.rb", 2, 8).await;
    assert!(
        defs.iter()
            .any(|loc| loc.uri.path().ends_with("/commerce.rb")),
        "route block helper call should resolve to included API method, got {defs:?}"
    );
}

#[tokio::test]
async fn goto_helper_method_with_unresolved_sinatra_superclass_and_nested_api() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "router.rb",
            include_str!("../../../../fixtures/route_helpers/router.rb"),
        )
        .await;
    editor
        .open(
            "support.rb",
            include_str!("../../../../fixtures/route_helpers/support.rb"),
        )
        .await;
    editor
        .open(
            "catalog.rb",
            include_str!("../../../../fixtures/route_helpers/catalog.rb"),
        )
        .await;
    editor
        .open(
            "api.rb",
            include_str!("../../../../fixtures/route_helpers/api.rb"),
        )
        .await;
    editor
        .open(
            "base.rb",
            include_str!("../../../../fixtures/route_helpers/base.rb"),
        )
        .await;
    let api_app = include_str!("../../../../fixtures/route_helpers/web_app.rb");
    editor.open("web_app.rb", api_app).await;
    // Cold project indexing reprocesses already-open buffers. Re-apply the same
    // content so ClassReference-from-prior-declaration cannot drop the class node.
    editor.set("web_app.rb", api_app).await;

    let defs = editor.goto_def_at("web_app.rb", 9, 12).await;
    let diags = editor.diagnostics("web_app.rb").await;
    assert!(
        defs.iter()
            .any(|loc| loc.uri.path().ends_with("/catalog.rb")),
        "expected definition through the synthetic helper include chain after reindex, got {defs:?}; diags={diags:?}"
    );
}
