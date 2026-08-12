use ruby_fast_lsp_test_harness::FakeEditor;
use tempfile::TempDir;

fn sinatra_package_dir() -> std::path::PathBuf {
    let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("test harness crate must live under crates/lsp-test-harness");
    workspace_root.join("extensions/sinatra-rust")
}

async fn sinatra_editor(version: &str) -> (TempDir, FakeEditor) {
    let workspace = TempDir::new().expect("Sinatra workspace must be created");
    std::fs::write(
        workspace.path().join("Gemfile"),
        "source 'https://rubygems.org'\ngem 'sinatra'\n",
    )
    .expect("Sinatra Gemfile must be written");
    std::fs::write(
        workspace.path().join("Gemfile.lock"),
        format!("GEM\n  remote: https://rubygems.org/\n  specs:\n    sinatra ({version})\n"),
    )
    .expect("Sinatra lockfile must be written");
    let editor =
        FakeEditor::with_extension_package_and_workspace(sinatra_package_dir(), workspace.path())
            .await;
    (workspace, editor)
}

fn workspace_file(workspace: &TempDir, relative: &str) -> String {
    workspace
        .path()
        .join(relative)
        .to_string_lossy()
        .to_string()
}

async fn open_sinatra_namespaces(editor: &mut FakeEditor, workspace: &TempDir) {
    let stub = workspace_file(workspace, "lib/sinatra.rb");
    editor
        .open(
            &stub,
            r#"module Sinatra
  module Delegator
  end

  class Base
    def params
    end
  end

  class Application < Base
  end
end
"#,
        )
        .await;
}

fn sinatra_artifact_exists() -> bool {
    sinatra_package_dir()
        .join("target/wasm32-wasip1/release/ruby_fast_lsp_sinatra_extension.wasm")
        .exists()
}

#[tokio::test]
async fn packaged_sinatra_rust_wasm_models_modular_request_and_helper_scopes() {
    if !sinatra_artifact_exists() {
        eprintln!(
            "skipping actual Sinatra Rust Wasm test; run extensions/sinatra-rust/build-and-test.sh"
        );
        return;
    }
    let (workspace, mut editor) = sinatra_editor("4.2.1").await;
    open_sinatra_namespaces(&mut editor, &workspace).await;
    let app = workspace_file(&workspace, "app.rb");
    let source = r#"module Admin
  class Marker
  end

  class App < Sinatra::Base
    helpers do
      def greeting
        Marker
      end
    end

    before do
      greeting
    end

    get "/" do
      greeting
      Marker
    end
  end
end

class Other
  def probe
    greeting
  end
end
"#;
    editor.open(&app, source).await;

    let statuses = editor.extension_status().await;
    assert!(
        statuses
            .iter()
            .any(|status| status.id == "sinatra-rust" && status.status == "loaded"),
        "expected Sinatra Rust extension loaded, got {statuses:?}"
    );

    let before_helper = editor.goto_definition(&app, 12, 8).await;
    let route_helper = editor.goto_definition(&app, 16, 8).await;
    assert_eq!(
        before_helper.len(),
        1,
        "a before filter must use the application instance receiver: {before_helper:?}"
    );
    assert_eq!(before_helper[0].range.start.line, 6);
    assert_eq!(
        route_helper.len(),
        1,
        "a route must use the application instance receiver: {route_helper:?}"
    );
    assert_eq!(route_helper[0].range.start.line, 6);
    assert!(
        editor.hover(&app, 16, 8).await.is_some(),
        "hover must resolve through the same Sinatra application receiver as definition"
    );
    assert!(
        editor.goto_definition(&app, 24, 6).await.is_empty(),
        "Sinatra helper methods must not leak into an unrelated lexical class"
    );

    for (line, character) in [(7, 9), (17, 9)] {
        let marker = editor.goto_definition(&app, line, character).await;
        assert_eq!(
            marker.len(),
            1,
            "Sinatra execution contexts must preserve lexical constant lookup at {line}:{character}: {marker:?}"
        );
        assert_eq!(marker[0].range.start.line, 1);
    }

    let references = editor.references(&app, 6, 12).await;
    assert!(
        references
            .iter()
            .any(|location| location.range.start.line == 12)
            && references
                .iter()
                .any(|location| location.range.start.line == 16),
        "helper references must use the same application owner as definition: {references:?}"
    );

    editor
        .set(
            &app,
            &source.replace("      def greeting\n        Marker\n      end\n", ""),
        )
        .await;
    assert!(
        editor.goto_definition(&app, 9, 8).await.is_empty(),
        "removing the helpers declaration must remove the stale filter definition"
    );
    assert!(
        editor.goto_definition(&app, 13, 8).await.is_empty(),
        "removing the helpers declaration must remove the stale route definition"
    );
}

#[tokio::test]
async fn packaged_sinatra_rust_wasm_supports_sinatra_2_cross_file_request_scope() {
    if !sinatra_artifact_exists() {
        eprintln!(
            "skipping actual Sinatra Rust Wasm test; run extensions/sinatra-rust/build-and-test.sh"
        );
        return;
    }
    let workspace = TempDir::new().expect("Sinatra workspace must be created");
    std::fs::create_dir(workspace.path().join("lib"))
        .expect("Sinatra source directory must be created");
    std::fs::write(
        workspace.path().join("Gemfile"),
        "source 'https://rubygems.org'\ngem 'sinatra'\n",
    )
    .expect("Sinatra Gemfile must be written");
    std::fs::write(
        workspace.path().join("Gemfile.lock"),
        "GEM\n  remote: https://rubygems.org/\n  specs:\n    sinatra (2.2.4)\n",
    )
    .expect("Sinatra lockfile must be written");
    std::fs::write(
        workspace.path().join("lib/sinatra.rb"),
        r#"module Sinatra
  module Delegator
  end

  class Base
    def params
    end
  end

  class Application < Base
  end
end
"#,
    )
    .expect("Sinatra stub must be written");
    std::fs::write(
        workspace.path().join("request_support.rb"),
        r#"module RequestSupport
  def fetch_records(context, values)
  end
end
"#,
    )
    .expect("request support source must be written");
    std::fs::write(
        workspace.path().join("request_api.rb"),
        r#"module RequestApi
  include RequestSupport
end
"#,
    )
    .expect("request API source must be written");
    std::fs::write(
        workspace.path().join("base_app.rb"),
        r#"class BaseApp < Sinatra::Base
  helpers do
    include RequestApi
    def access_context
    end
  end
end
"#,
    )
    .expect("base application source must be written");
    let app = workspace_file(&workspace, "app.rb");
    let app_source = r#"class ApiApp < BaseApp
  get "/records" do
    fetch_records(access_context, params)
  end
end
"#;
    std::fs::write(&app, app_source).expect("application source must be written");

    let mut editor =
        FakeEditor::with_extension_package_and_workspace(sinatra_package_dir(), workspace.path())
            .await;
    editor.open(&app, app_source).await;
    editor.wait_for_indexing_complete().await;

    let method = editor.goto_definition(&app, 2, 5).await;
    let statuses = editor.extension_status().await;
    let sinatra = statuses
        .iter()
        .find(|status| status.id == "sinatra-rust")
        .expect("the bundled Sinatra extension must be discoverable");
    assert_eq!(
        method.len(),
        1,
        "Sinatra 2 routes must resolve cross-file application methods: {method:?}; extension: {sinatra:?}"
    );
    assert_eq!(method[0].range.start.line, 1);

    let context = editor.goto_definition(&app, 2, 19).await;
    assert_eq!(
        context.len(),
        1,
        "Sinatra 2 routes must resolve inherited helper methods: {context:?}"
    );
    assert_eq!(context[0].range.start.line, 3);

    let params = editor.goto_definition(&app, 2, 35).await;
    assert_eq!(
        params.len(),
        1,
        "Sinatra 2 routes must resolve framework request methods: {params:?}"
    );
    assert_eq!(params[0].range.start.line, 5);

    assert_eq!(sinatra.status, "loaded");
    assert!(
        sinatra.telemetry.emitted_execution_contexts >= 2,
        "Sinatra 2 helpers and routes must be modeled by the extension, not the generic syntactic fallback: {sinatra:?}"
    );
}

#[tokio::test]
async fn packaged_sinatra_rust_wasm_models_classic_application_scope() {
    if !sinatra_artifact_exists() {
        eprintln!(
            "skipping actual Sinatra Rust Wasm test; run extensions/sinatra-rust/build-and-test.sh"
        );
        return;
    }
    let (workspace, mut editor) = sinatra_editor("4.2.1").await;
    open_sinatra_namespaces(&mut editor, &workspace).await;
    let app = workspace_file(&workspace, "classic.rb");
    editor
        .open(
            &app,
            r#"class ClassicMarker
end

extend Sinatra::Delegator

helpers do
  def classic_helper
    ClassicMarker
  end
end

get "/" do
  classic_helper
  ClassicMarker
end
"#,
        )
        .await;

    let helper = editor.goto_definition(&app, 12, 4).await;
    assert_eq!(
        helper.len(),
        1,
        "classic routes must receive Sinatra::Application helpers: {helper:?}"
    );
    assert_eq!(helper[0].range.start.line, 6);
    let marker = editor.goto_definition(&app, 13, 4).await;
    assert_eq!(
        marker.len(),
        1,
        "classic routes must preserve top-level lexical constants: {marker:?}"
    );
    assert_eq!(marker[0].range.start.line, 0);
}

#[tokio::test]
async fn packaged_sinatra_rust_wasm_includes_helper_modules_in_request_scope() {
    if !sinatra_artifact_exists() {
        eprintln!(
            "skipping actual Sinatra Rust Wasm test; run extensions/sinatra-rust/build-and-test.sh"
        );
        return;
    }
    let (workspace, mut editor) = sinatra_editor("4.2.1").await;
    open_sinatra_namespaces(&mut editor, &workspace).await;
    let app = workspace_file(&workspace, "helper_module.rb");
    editor
        .open(
            &app,
            r#"module SharedHelpers
  def module_helper
  end
end

class App < Sinatra::Base
  helpers SharedHelpers

  get "/" do
    module_helper
  end
end
"#,
        )
        .await;

    let helper = editor.goto_definition(&app, 9, 6).await;
    assert_eq!(
        helper.len(),
        1,
        "Sinatra helper modules must enter the application instance MRO: {helper:?}"
    );
    assert_eq!(helper[0].range.start.line, 1);
}

#[tokio::test]
async fn packaged_sinatra_manifest_fails_closed_for_unsupported_version() {
    if !sinatra_artifact_exists() {
        eprintln!(
            "skipping actual Sinatra Rust Wasm applicability test; run extensions/sinatra-rust/build-and-test.sh"
        );
        return;
    }
    let (workspace, mut editor) = sinatra_editor("5.0.0").await;
    open_sinatra_namespaces(&mut editor, &workspace).await;
    let app = workspace_file(&workspace, "unsupported.rb");
    editor
        .open(
            &app,
            "class App < Sinatra::Base\n  get \"/\" do\n  end\nend\n",
        )
        .await;

    assert!(
        editor.goto_definition(&app, 1, 3).await.is_empty(),
        "unsupported Sinatra versions must not receive semantic targets"
    );
    let statuses = editor.extension_status().await;
    assert!(
        statuses
            .iter()
            .any(|status| status.id == "sinatra-rust" && status.status == "loaded"),
        "inapplicability must skip Sinatra without disabling the package: {statuses:?}"
    );
}
