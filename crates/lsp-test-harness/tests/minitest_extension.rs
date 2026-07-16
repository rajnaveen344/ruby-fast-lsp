use ruby_fast_lsp_test_harness::FakeEditor;
use tempfile::TempDir;

fn minitest_package_dir() -> std::path::PathBuf {
    let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("test harness crate must live under crates/lsp-test-harness");
    workspace_root.join("extensions/minitest-ruby")
}

async fn minitest_editor(version: &str) -> (TempDir, FakeEditor) {
    let workspace = TempDir::new().expect("Minitest workspace must be created");
    std::fs::write(
        workspace.path().join("Gemfile"),
        "source 'https://rubygems.org'\ngem 'minitest'\n",
    )
    .expect("Minitest Gemfile must be written");
    std::fs::write(
        workspace.path().join("Gemfile.lock"),
        format!("GEM\n  remote: https://rubygems.org/\n  specs:\n    minitest ({version})\n"),
    )
    .expect("Minitest lockfile must be written");
    let editor =
        FakeEditor::with_extension_package_and_workspace(minitest_package_dir(), workspace.path())
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

async fn open_minitest_spec_stub(editor: &mut FakeEditor, workspace: &TempDir) {
    let stub = workspace_file(workspace, "lib/minitest/spec.rb");
    editor
        .open(
            &stub,
            r#"module Kernel
  def describe(description, &block)
  end
end

class Object
end

module Minitest
  class Test
  end

  class Spec < Test
    module DSL
      def describe(description, &block)
      end

      def it(description, &block)
      end

      alias specify it

      def let(name, &block)
      end

      def subject(&block)
      end

      def before(&block)
      end

      def after(&block)
      end
    end

    extend DSL
  end
end
"#,
        )
        .await;
}

#[tokio::test]
async fn minitest_symbols_and_commands_use_the_public_extension_contract() {
    let (workspace, mut editor) = minitest_editor("6.0.6").await;
    let statuses = editor.extension_status().await;
    assert!(
        statuses
            .iter()
            .any(|status| status.id == "minitest-ruby" && status.status == "loaded"),
        "expected minitest-ruby extension loaded, got {statuses:?}"
    );
    let file = workspace_file(&workspace, "test/models/user_test.rb");
    editor
        .open(
            &file,
            "class UserTest < ActiveSupport::TestCase\n  def test_valid\n  end\n\n  test \"rejects blanks\" do\n  end\nend\n",
        )
        .await;

    let symbols = editor.document_symbols(&file).await;
    let names: Vec<_> = symbols.iter().map(|symbol| symbol.name.as_str()).collect();
    assert_eq!(names, ["UserTest", "test_valid", "rejects blanks"]);

    let lenses = editor.code_lens(&file).await;
    let titles: Vec<_> = lenses
        .iter()
        .filter_map(|lens| lens.command.as_ref().map(|command| command.title.as_str()))
        .collect();
    assert_eq!(
        titles.len(),
        6,
        "each discovered target needs run and debug lenses"
    );
    assert!(titles.contains(&"Run Minitest"));
    assert!(titles.contains(&"Debug Minitest"));
    let method_run = lenses
        .iter()
        .filter_map(|lens| lens.command.as_ref())
        .find(|command| {
            command.title == "Run Minitest"
                && command
                    .arguments
                    .as_ref()
                    .is_some_and(|arguments| arguments.get(1) == Some(&serde_json::json!("2")))
        })
        .expect("test method must have an exact run command");
    assert_eq!(
        method_run
            .arguments
            .as_ref()
            .and_then(|arguments| arguments.get(2)),
        Some(&serde_json::json!("test_valid"))
    );

    editor
        .set(&file, "class UserTest < ActiveSupport::TestCase\nend\n")
        .await;
    let symbols = editor.document_symbols(&file).await;
    assert_eq!(
        symbols
            .iter()
            .map(|symbol| symbol.name.as_str())
            .collect::<Vec<_>>(),
        ["UserTest"],
        "removed test declarations must disappear after didChange"
    );
    assert_eq!(
        editor.code_lens(&file).await.len(),
        2,
        "only the class run/debug lenses should remain after didChange"
    );
}

#[tokio::test]
async fn minitest_fails_closed_for_unsupported_locked_versions() {
    let (workspace, mut editor) = minitest_editor("7.0.0").await;
    let file = workspace_file(&workspace, "test/models/user_test.rb");
    editor
        .open(&file, "test \"unsupported version\" do\nend\n")
        .await;

    assert!(
        editor.document_symbols(&file).await.is_empty(),
        "unsupported Minitest versions must not receive extension symbols"
    );
    assert!(
        editor.code_lens(&file).await.is_empty(),
        "unsupported Minitest versions must not receive extension lenses"
    );
    let status = editor
        .extension_status()
        .await
        .into_iter()
        .find(|status| status.id == "minitest-ruby")
        .expect("Minitest package must remain discoverable");
    assert_eq!(
        status.status, "loaded",
        "inapplicability must not disable the healthy package"
    );
}

#[tokio::test]
async fn minitest_ignores_test_shaped_code_outside_test_files() {
    let (workspace, mut editor) = minitest_editor("5.27.0").await;
    let file = workspace_file(&workspace, "lib/user.rb");
    editor
        .open(&file, "class UserTest\n  def test_valid\n  end\nend\n")
        .await;

    let symbols = editor.document_symbols(&file).await;
    assert_eq!(
        symbols
            .iter()
            .map(|symbol| symbol.name.as_str())
            .collect::<Vec<_>>(),
        ["UserTest"],
        "core symbols remain, but the Minitest guest must add none"
    );
    assert!(
        editor.code_lens(&file).await.is_empty(),
        "non-test files must not receive Minitest lenses"
    );
}

#[tokio::test]
async fn minitest_spec_contexts_isolate_groups_and_preserve_lexical_lookup() {
    let (workspace, mut editor) = minitest_editor("6.0.6").await;
    open_minitest_spec_stub(&mut editor, &workspace).await;
    let file = workspace_file(&workspace, "test/service_test.rb");
    let source = r#"class Marker
end

describe "outer" do
  def helper
    Marker
  end

  let(:service) { Marker.new }

  it "uses helper" do
    helper
    service
  end

  describe "inner" do
    it "inherits helper" do
      helper
    end
  end
end

describe "sibling" do
  it "does not inherit" do
    helper
  end
end
"#;
    editor.open(&file, source).await;

    let symbol_names = editor
        .document_symbols(&file)
        .await
        .into_iter()
        .map(|symbol| symbol.name)
        .collect::<Vec<_>>();
    assert_eq!(
        symbol_names,
        [
            "Marker",
            "helper",
            "outer",
            "uses helper",
            "inner",
            "inherits helper",
            "sibling",
            "does not inherit",
        ],
        "Minitest::Spec groups and examples must be discoverable"
    );
    assert_eq!(
        editor.code_lens(&file).await.len(),
        12,
        "each Minitest::Spec group and example needs run and debug lenses"
    );

    let helper = editor.goto_definition(&file, 11, 6).await;
    assert_eq!(
        helper.len(),
        1,
        "example body must resolve its group helper"
    );
    assert_eq!(helper[0].range.start.line, 4);

    let nested_helper = editor.goto_definition(&file, 17, 8).await;
    assert_eq!(
        nested_helper.len(),
        1,
        "nested generated groups must inherit parent group helpers"
    );
    assert_eq!(nested_helper[0].range.start.line, 4);

    assert!(
        editor.goto_definition(&file, 24, 6).await.is_empty(),
        "sibling generated groups must not leak helper methods"
    );

    let service = editor.goto_definition(&file, 12, 6).await;
    assert_eq!(service.len(), 1, "let must define an instance helper");
    assert_eq!(service[0].range.start.line, 8);

    let marker = editor.goto_definition(&file, 5, 6).await;
    assert_eq!(
        marker.len(),
        1,
        "group execution must preserve lexical constants"
    );
    assert_eq!(marker[0].range.start.line, 0);

    editor
        .set(
            &file,
            "class Marker\nend\n\ndescribe \"outer\" do\n  it \"removed helpers\" do\n    helper\n    service\n  end\nend\n",
        )
        .await;
    assert!(
        editor.goto_definition(&file, 5, 6).await.is_empty(),
        "editing out a generated group method must remove its facts"
    );
    assert!(
        editor.goto_definition(&file, 6, 6).await.is_empty(),
        "editing out a let declaration must remove its facts"
    );
}
