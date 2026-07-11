use ruby_fast_lsp_test_harness::FakeEditor;

fn minitest_package_dir() -> std::path::PathBuf {
    let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("test harness crate must live under crates/lsp-test-harness");
    workspace_root.join("extensions/minitest-ruby")
}

#[tokio::test]
async fn minitest_symbols_and_commands_use_the_public_extension_contract() {
    let mut editor = FakeEditor::with_extension_package(minitest_package_dir()).await;
    let statuses = editor.extension_status().await;
    assert!(
        statuses
            .iter()
            .any(|status| status.id == "minitest-ruby" && status.status == "loaded"),
        "expected minitest-ruby extension loaded, got {statuses:?}"
    );
    editor
        .open(
            "test/models/user_test.rb",
            "class UserTest < ActiveSupport::TestCase\n  def test_valid\n  end\n\n  test \"rejects blanks\" do\n  end\nend\n",
        )
        .await;

    let symbols = editor.document_symbols("test/models/user_test.rb").await;
    let names: Vec<_> = symbols.iter().map(|symbol| symbol.name.as_str()).collect();
    assert_eq!(names, ["UserTest", "test_valid", "rejects blanks"]);

    let lenses = editor.code_lens("test/models/user_test.rb").await;
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
        .set(
            "test/models/user_test.rb",
            "class UserTest < ActiveSupport::TestCase\nend\n",
        )
        .await;
    let symbols = editor.document_symbols("test/models/user_test.rb").await;
    assert_eq!(
        symbols
            .iter()
            .map(|symbol| symbol.name.as_str())
            .collect::<Vec<_>>(),
        ["UserTest"],
        "removed test declarations must disappear after didChange"
    );
    assert_eq!(
        editor.code_lens("test/models/user_test.rb").await.len(),
        2,
        "only the class run/debug lenses should remain after didChange"
    );
}
