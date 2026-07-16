use ruby_fast_lsp_test_harness::FakeEditor;
use tempfile::TempDir;

fn rust_example_package_dir() -> std::path::PathBuf {
    let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("test harness crate must live under crates/lsp-test-harness");
    workspace_root.join("extensions/example-rust")
}

#[tokio::test]
async fn typed_rust_wasm_guest_uses_public_execution_context_contract() {
    let package = rust_example_package_dir();
    let artifact =
        package.join("target/wasm32-wasip1/release/ruby_fast_lsp_example_rust_extension.wasm");
    if !artifact.exists() {
        eprintln!(
            "skipping actual Rust Wasm black-box test; run extensions/example-rust/build-and-test.sh"
        );
        return;
    }
    let workspace = TempDir::new().expect("Rust guest workspace must be created");
    std::fs::write(
        workspace.path().join("Gemfile"),
        "source 'https://rubygems.org'\n",
    )
    .expect("Rust guest Gemfile must be written");
    std::fs::write(
        workspace.path().join("Gemfile.lock"),
        "GEM\n  remote: https://rubygems.org/\n  specs:\n    example-framework (1.0.0)\n",
    )
    .expect("Rust guest lockfile must be written");
    let mut editor =
        FakeEditor::with_extension_package_and_workspace(package, workspace.path()).await;
    let filename = workspace
        .path()
        .join("spec/example_rust.rb")
        .to_string_lossy()
        .to_string();
    let statuses = editor.extension_status().await;
    assert!(
        statuses
            .iter()
            .any(|status| status.id == "example-rust" && status.status == "loaded"),
        "expected typed Rust Wasm extension loaded, got {statuses:?}. Build it with the command in extensions/example-rust/README.md"
    );

    editor
        .open(
            &filename,
            r#"module ExampleDsl
end

ExampleDsl.scope do
  property :generated_name
  isolation_probe

  def direct_helper
  end

  generated_name
  direct_helper
end
"#,
        )
        .await;
    let statuses = editor.extension_status().await;
    assert!(
        statuses
            .iter()
            .any(|status| status.id == "example-rust" && status.status == "loaded"),
        "Rust guest became unhealthy while applying execution contexts: {statuses:?}"
    );

    let generated = editor.goto_definition(&filename, 10, 8).await;
    let direct = editor.goto_definition(&filename, 11, 8).await;
    let root_dsl = editor.goto_definition(&filename, 3, 12).await;
    assert_eq!(
        root_dsl.len(),
        1,
        "applicable project must receive its manifest semantic target: {root_dsl:?}"
    );
    assert_eq!(
        direct.len(),
        1,
        "Rust guest execution owner did not own direct def: {direct:?}"
    );
    assert_eq!(direct[0].range.start.line, 7);
    assert_eq!(
        generated.len(),
        1,
        "Rust guest generated method did not resolve: {generated:?}"
    );
    assert_eq!(generated[0].range.start.line, 4);
    let symbols = editor.document_symbols(&filename).await;
    assert!(
        symbols
            .iter()
            .any(|symbol| symbol.name == "project-isolated-symbol"),
        "document symbols must route through the same project-private Wasm instance as index.call: {symbols:?}"
    );
    let lenses = editor.code_lens(&filename).await;
    assert!(
        lenses.iter().any(|lens| lens
            .command
            .as_ref()
            .is_some_and(|command| command.title == "Project-isolated lens")),
        "code lenses must route through the same project-private Wasm instance as index.call: {lenses:?}"
    );

    editor
        .set(
            &filename,
            r#"module ExampleDsl
end

ExampleDsl.scope do
  generated_name
  direct_helper
end
"#,
        )
        .await;

    assert!(
        editor.goto_definition(&filename, 4, 8).await.is_empty(),
        "editing out a Rust guest patch must remove the generated method"
    );
    assert!(
        editor.goto_definition(&filename, 5, 8).await.is_empty(),
        "editing out a direct definition must remove the generated-owner method"
    );
}

#[tokio::test]
async fn typed_rust_wasm_manifest_fails_closed_for_unsupported_locked_version() {
    let package = rust_example_package_dir();
    let artifact =
        package.join("target/wasm32-wasip1/release/ruby_fast_lsp_example_rust_extension.wasm");
    if !artifact.exists() {
        eprintln!(
            "skipping actual Rust Wasm applicability test; run extensions/example-rust/build-and-test.sh"
        );
        return;
    }
    let workspace = TempDir::new().expect("unsupported Rust guest workspace must be created");
    std::fs::write(
        workspace.path().join("Gemfile"),
        "source 'https://rubygems.org'\n",
    )
    .expect("unsupported Rust guest Gemfile must be written");
    std::fs::write(
        workspace.path().join("Gemfile.lock"),
        "GEM\n  remote: https://rubygems.org/\n  specs:\n    example-framework (2.0.0)\n",
    )
    .expect("unsupported Rust guest lockfile must be written");
    let mut editor =
        FakeEditor::with_extension_package_and_workspace(package, workspace.path()).await;
    let filename = workspace
        .path()
        .join("spec/unsupported.rb")
        .to_string_lossy()
        .to_string();
    editor
        .open(
            &filename,
            r#"module ExampleDsl
end

ExampleDsl.scope do
  property :generated_name
  isolation_probe
  generated_name
end
"#,
        )
        .await;

    assert!(
        editor.goto_definition(&filename, 3, 12).await.is_empty(),
        "unsupported locked version must not receive the extension semantic target"
    );
    assert!(
        editor.goto_definition(&filename, 5, 4).await.is_empty(),
        "unsupported locked version must not receive guest-generated facts"
    );
    assert!(
        editor
            .document_symbols(&filename)
            .await
            .iter()
            .all(|symbol| symbol.name != "project-isolated-symbol"),
        "unsupported locked version must not receive guest response patches"
    );
    assert!(
        editor.code_lens(&filename).await.iter().all(|lens| lens
            .command
            .as_ref()
            .is_none_or(|command| command.title != "Project-isolated lens")),
        "unsupported locked version must not receive guest code lenses"
    );
    let statuses = editor.extension_status().await;
    assert!(
        statuses
            .iter()
            .any(|status| status.id == "example-rust" && status.status == "loaded"),
        "inapplicability must skip a guest without disabling it: {statuses:?}"
    );
}
