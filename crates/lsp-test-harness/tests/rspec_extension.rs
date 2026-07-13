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

#[tokio::test]
async fn rspec_root_describe_has_a_semantic_definition() {
    let mut editor = FakeEditor::with_extension_package(rspec_package_dir()).await;
    editor.open("lib/rspec.rb", "module RSpec\nend\n").await;
    editor
        .open("spec/user_spec.rb", "RSpec.describe User do\nend\n")
        .await;

    let definitions = editor.goto_definition("spec/user_spec.rb", 0, 8).await;

    assert_eq!(
        definitions.len(),
        1,
        "RSpec.describe must resolve only to its canonical semantic target, got {definitions:?}"
    );
    assert_eq!(
        definitions[0].uri.path(),
        "/__ruby_fast_lsp_extension__/semantic_targets.rb"
    );
}
