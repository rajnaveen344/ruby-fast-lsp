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
