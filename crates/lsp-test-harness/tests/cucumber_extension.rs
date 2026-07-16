use ruby_fast_lsp_test_harness::FakeEditor;
use tempfile::TempDir;

fn cucumber_package_dir() -> std::path::PathBuf {
    let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("test harness crate must live under crates/lsp-test-harness");
    workspace_root.join("extensions/cucumber-rust")
}

fn cucumber_artifact_exists() -> bool {
    cucumber_package_dir()
        .join("target/wasm32-wasip1/release/ruby_fast_lsp_cucumber_extension.wasm")
        .exists()
}

async fn cucumber_editor(version: &str) -> (TempDir, FakeEditor) {
    let workspace = TempDir::new().expect("Cucumber workspace must be created");
    std::fs::write(
        workspace.path().join("Gemfile"),
        "source 'https://rubygems.org'\ngem 'cucumber'\n",
    )
    .expect("Cucumber Gemfile must be written");
    std::fs::write(
        workspace.path().join("Gemfile.lock"),
        format!("GEM\n  remote: https://rubygems.org/\n  specs:\n    cucumber ({version})\n"),
    )
    .expect("Cucumber lockfile must be written");
    let editor =
        FakeEditor::with_extension_package_and_workspace(cucumber_package_dir(), workspace.path())
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

async fn open_cucumber_dsl(editor: &mut FakeEditor, workspace: &TempDir) {
    let stub = workspace_file(workspace, "lib/cucumber.rb");
    editor
        .open(
            &stub,
            r#"module Cucumber
  module Glue
    module Dsl
    end
  end
end

extend Cucumber::Glue::Dsl

module UserSteps
  extend Cucumber::Glue::Dsl
end

class Object
end
"#,
        )
        .await;
}

#[tokio::test]
async fn packaged_cucumber_world_connects_cross_file_steps_hooks_and_modules() {
    if !cucumber_artifact_exists() {
        eprintln!(
            "skipping actual Cucumber Rust Wasm test; run extensions/cucumber-rust/build-and-test.sh"
        );
        return;
    }
    let (workspace, mut editor) = cucumber_editor("11.1.1").await;
    open_cucumber_dsl(&mut editor, &workspace).await;
    let support = workspace_file(&workspace, "features/support/world.rb");
    editor
        .open(
            &support,
            r#"module BrowserHelpers
  def current_user
  end
end

extend Cucumber::Glue::Dsl
World(BrowserHelpers)
"#,
        )
        .await;
    let world_dsl = editor.goto_definition(&support, 6, 2).await;
    assert_eq!(
        world_dsl.len(),
        1,
        "Cucumber World must resolve to its canonical DSL target before applying modules: {world_dsl:?}"
    );
    let steps = workspace_file(&workspace, "features/step_definitions/users_steps.rb");
    let source = r#"class Marker
end

Given "a signed in user" do
  current_user
  Marker
end

Before do
  current_user
end

class Other
  def probe
    current_user
  end
end
"#;
    editor.open(&steps, source).await;
    let given_dsl = editor.goto_definition(&steps, 3, 2).await;
    assert_eq!(
        given_dsl.len(),
        1,
        "Cucumber Given must resolve to its canonical DSL target before entering World scope: {given_dsl:?}"
    );

    let statuses = editor.extension_status().await;
    assert!(
        statuses
            .iter()
            .any(|status| status.id == "cucumber-rust" && status.status == "loaded"),
        "expected Cucumber Rust extension loaded, got {statuses:?}"
    );
    for (line, character) in [(4, 4), (9, 4)] {
        let helper = editor.goto_definition(&steps, line, character).await;
        assert_eq!(
            helper.len(),
            1,
            "Cucumber World helper did not resolve at {line}:{character}: {helper:?}"
        );
        assert_eq!(helper[0].uri.path(), support.as_str());
        assert_eq!(helper[0].range.start.line, 1);
    }
    let marker = editor.goto_definition(&steps, 5, 3).await;
    assert_eq!(
        marker.len(),
        1,
        "step lexical constant did not resolve: {marker:?}"
    );
    assert_eq!(marker[0].range.start.line, 0);
    assert!(
        editor.goto_definition(&steps, 14, 6).await.is_empty(),
        "the generated Cucumber World must not leak into an unrelated class"
    );
    let references = editor.references(&support, 1, 8).await;
    assert!(
        references
            .iter()
            .any(|location| location.range.start.line == 4)
            && references
                .iter()
                .any(|location| location.range.start.line == 9),
        "World helper references must include step and hook bodies: {references:?}"
    );

    editor
        .set(
            &support,
            r#"module BrowserHelpers
  def current_user
  end
end

extend Cucumber::Glue::Dsl
"#,
        )
        .await;
    assert!(
        editor.goto_definition(&steps, 4, 4).await.is_empty(),
        "removing World(module) must remove the stale cross-file mixin edge"
    );
}

#[tokio::test]
async fn packaged_cucumber_world_factory_preserves_ordinary_lexical_scope() {
    if !cucumber_artifact_exists() {
        eprintln!(
            "skipping actual Cucumber Rust Wasm test; run extensions/cucumber-rust/build-and-test.sh"
        );
        return;
    }
    let (workspace, mut editor) = cucumber_editor("11.1.1").await;
    open_cucumber_dsl(&mut editor, &workspace).await;
    let support = workspace_file(&workspace, "features/support/factory.rb");
    editor
        .open(
            &support,
            r#"module FactoryHelpers
  def world_only
  end
end

extend Cucumber::Glue::Dsl
World(FactoryHelpers) do
  world_only
  Object.new
end
"#,
        )
        .await;

    assert!(
        editor.goto_definition(&support, 7, 4).await.is_empty(),
        "World factory blocks must not accidentally receive scenario World mixins"
    );
}

#[tokio::test]
async fn packaged_cucumber_manifest_fails_closed_for_unsupported_version() {
    if !cucumber_artifact_exists() {
        eprintln!(
            "skipping actual Cucumber Rust Wasm test; run extensions/cucumber-rust/build-and-test.sh"
        );
        return;
    }
    let (workspace, mut editor) = cucumber_editor("12.0.0").await;
    open_cucumber_dsl(&mut editor, &workspace).await;
    let steps = workspace_file(&workspace, "features/steps.rb");
    editor
        .open(&steps, "extend Cucumber::Glue::Dsl\nGiven \"x\" do\nend\n")
        .await;

    assert!(
        editor.goto_definition(&steps, 1, 2).await.is_empty(),
        "unsupported Cucumber versions must not receive semantic targets"
    );
    let statuses = editor.extension_status().await;
    assert!(
        statuses
            .iter()
            .any(|status| status.id == "cucumber-rust" && status.status == "loaded"),
        "inapplicability must not disable the Cucumber package: {statuses:?}"
    );
}
