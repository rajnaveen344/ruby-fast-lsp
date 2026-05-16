use ruby_fast_lsp_test_harness::FakeEditor;

fn rspec_package_dir() -> std::path::PathBuf {
    let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("test harness crate must live under crates/lsp-test-harness");
    workspace_root.join("extensions/rspec-ruby")
}

#[tokio::test]
async fn rspec_extension_symbols_are_available_through_reusable_fake_editor() {
    let mut editor = FakeEditor::with_extension_package(rspec_package_dir()).await;
    let statuses = editor.extension_status().await;
    assert!(
        statuses
            .iter()
            .any(|status| status.id == "rspec-ruby" && status.status == "loaded"),
        "expected rspec-ruby extension loaded, got {statuses:?}"
    );
    editor
        .open(
            "spec/user_spec.rb",
            r#"
RSpec.describe User do
  context "active" do
    it "returns name" do
    end
  end
end
"#,
        )
        .await;

    let direct_symbols = ruby_fast_lsp::extensions::document_symbols(
        "file:///spec/user_spec.rb",
        editor.content("spec/user_spec.rb"),
    );
    assert!(
        !direct_symbols.is_empty(),
        "direct extension symbols empty after loaded status; status now {:?}",
        editor.extension_status().await
    );
    let symbols = editor.document_symbols("spec/user_spec.rb").await;
    let names: Vec<_> = symbols.iter().map(|symbol| symbol.name.as_str()).collect();

    assert!(names.contains(&"describe User"), "got symbols: {names:?}");
    assert!(names.contains(&"context active"), "got symbols: {names:?}");
    assert!(names.contains(&"it returns name"), "got symbols: {names:?}");
}

#[tokio::test]
async fn rspec_extension_lenses_are_available_through_reusable_fake_editor() {
    let mut editor = FakeEditor::with_extension_package(rspec_package_dir()).await;
    let statuses = editor.extension_status().await;
    assert!(
        statuses
            .iter()
            .any(|status| status.id == "rspec-ruby" && status.status == "loaded"),
        "expected rspec-ruby extension loaded, got {statuses:?}"
    );
    editor
        .open(
            "spec/user_spec.rb",
            r#"
RSpec.describe User do
  it "returns name" do
  end
end
"#,
        )
        .await;

    let lenses = editor.code_lens("spec/user_spec.rb").await;
    let titles: Vec<_> = lenses
        .iter()
        .filter_map(|lens| lens.command.as_ref().map(|command| command.title.as_str()))
        .collect();

    assert!(titles.contains(&"Run RSpec"), "got lenses: {titles:?}");
    assert!(titles.contains(&"Debug RSpec"), "got lenses: {titles:?}");
}
