use ruby_fast_lsp_test_harness::FakeEditor;
use tempfile::TempDir;

fn workspace_root() -> std::path::PathBuf {
    let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("test harness crate must live under crates/lsp-test-harness");
    workspace_root.to_path_buf()
}

fn example_package() -> (TempDir, std::path::PathBuf) {
    let source = workspace_root().join("extensions/example-dsl");
    let temp = TempDir::new().expect("example extension temp package must be created");
    let package = temp.path().join("example-dsl");
    std::fs::create_dir(&package).expect("example extension package directory must be created");
    std::fs::copy(
        source.join("extension.toml"),
        package.join("extension.toml"),
    )
    .expect("example extension manifest must be copied");
    if std::env::var("RUBY_FAST_LSP_TEST_BUILT_EXAMPLE").as_deref() == Ok("1") {
        std::fs::copy(source.join("extension.wasm"), package.join("extension.wasm")).expect(
            "built example Wasm is required; run the mruby SDK builder before setting RUBY_FAST_LSP_TEST_BUILT_EXAMPLE=1",
        );
    } else {
        let mut wat = std::fs::read_to_string(source.join("contract.wat.in"))
            .expect("example extension public-contract fixture must be readable");
        for (name, pointer, file) in [
            ("NAMES", 1024_u64, "indexed_call_names.json"),
            ("INDEX", 2048_u64, "index_output.json"),
            ("RESPONSE", 8192_u64, "response_output.json"),
            ("EMPTY", 16384_u64, "empty_output.json"),
        ] {
            let payload = std::fs::read(source.join(file)).unwrap_or_else(|err| {
                panic!("example extension fixture `{file}` must be readable: {err}")
            });
            serde_json::from_slice::<serde_json::Value>(&payload).unwrap_or_else(|err| {
                panic!("example extension fixture `{file}` must be valid JSON: {err}")
            });
            let escaped = payload
                .iter()
                .map(|byte| format!("\\{byte:02x}"))
                .collect::<String>();
            let packed = (pointer << 32) | payload.len() as u64;
            wat = wat
                .replace(&format!("__{name}_DATA__"), &escaped)
                .replace(&format!("__{name}_PACKED__"), &packed.to_string());
        }
        let wasm =
            wat::parse_str(wat).expect("example extension public-contract fixture must compile");
        std::fs::write(package.join("extension.wasm"), wasm)
            .expect("example extension test wasm must be written");
    }
    (temp, package)
}

#[tokio::test]
async fn independent_extension_package_uses_only_public_contracts() {
    let (_temp, package) = example_package();
    let mut editor = FakeEditor::with_extension_package(package).await;
    let statuses = editor.extension_status().await;
    assert!(
        statuses
            .iter()
            .any(|status| status.id == "example-dsl" && status.status == "loaded"),
        "expected independent example-dsl extension to load, got {statuses:?}"
    );

    editor
        .open(
            "app/example_model.rb",
            "class ExampleModel\n  field :name\n  def display\n    name.upcase\n    self.name\n    GeneratedRecord\n    GeneratedRecord::DEFAULT_NAME\n  end\nend\n",
        )
        .await;

    let symbols = editor.document_symbols("app/example_model.rb").await;
    assert!(
        symbols.iter().any(|symbol| symbol.name == "field name"),
        "expected example extension document symbol, got {symbols:?}"
    );

    let lenses = editor.code_lens("app/example_model.rb").await;
    assert!(
        lenses.iter().any(|lens| {
            lens.command
                .as_ref()
                .is_some_and(|command| command.title == "Inspect field")
        }),
        "expected example extension code lens, got {lenses:?}"
    );

    let definitions = editor.goto_definition("app/example_model.rb", 3, 6).await;
    assert_eq!(definitions.len(), 1, "expected one DSL method definition");
    assert_eq!(definitions[0].range.start.line, 1);
    assert_eq!(definitions[0].range.start.character, 8);

    let return_type_hover = editor.hover("app/example_model.rb", 3, 6).await;
    assert!(
        return_type_hover
            .as_ref()
            .is_some_and(|hover| format!("{:?}", hover.contents).contains("(Array<String> | NilClass)")),
        "extension-declared return type must make the generated method hover as nilable Array<String>, got {return_type_hover:?}"
    );

    let private_explicit_definitions = editor.goto_definition("app/example_model.rb", 4, 10).await;
    assert!(
        private_explicit_definitions.is_empty(),
        "extension-declared private method must reject an explicit receiver, got {private_explicit_definitions:?}"
    );

    let namespace_definitions = editor.goto_definition("app/example_model.rb", 5, 6).await;
    assert_eq!(
        namespace_definitions.len(),
        1,
        "expected one generated namespace definition"
    );
    assert_eq!(namespace_definitions[0].range.start.line, 1);
    assert_eq!(namespace_definitions[0].range.start.character, 8);

    let constant_definitions = editor.goto_definition("app/example_model.rb", 6, 23).await;
    assert_eq!(
        constant_definitions.len(),
        1,
        "expected one generated typed constant definition"
    );
    assert_eq!(constant_definitions[0].range.start.line, 1);
    assert_eq!(constant_definitions[0].range.start.character, 8);
    let constant_hover = editor.hover("app/example_model.rb", 6, 23).await;
    assert!(
        constant_hover
            .as_ref()
            .is_some_and(|hover| format!("{:?}", hover.contents).contains("Hash<Symbol, String>")),
        "extension-declared constant type must hover as Hash<Symbol, String>, got {constant_hover:?}"
    );

    editor
        .set(
            "app/example_model.rb",
            "class ExampleModel\n  def display\n    name\n    GeneratedRecord\n    GeneratedRecord::DEFAULT_NAME\n  end\nend\n",
        )
        .await;
    let stale_definitions = editor.goto_definition("app/example_model.rb", 2, 6).await;
    assert!(
        stale_definitions.is_empty(),
        "removing the DSL declaration must remove its generated method fact, got {stale_definitions:?}"
    );
    let stale_hover = editor.hover("app/example_model.rb", 2, 6).await;
    assert!(
        !stale_hover
            .as_ref()
            .is_some_and(|hover| format!("{:?}", hover.contents).contains("String")),
        "removing the DSL declaration must remove its generated return type, got {stale_hover:?}"
    );
    let stale_namespace = editor.goto_definition("app/example_model.rb", 3, 6).await;
    assert!(
        stale_namespace.is_empty(),
        "removing the DSL declaration must remove its generated namespace, got {stale_namespace:?}"
    );
    let stale_constant = editor.goto_definition("app/example_model.rb", 4, 23).await;
    assert!(
        stale_constant.is_empty(),
        "removing the DSL declaration must remove its generated constant, got {stale_constant:?}"
    );
}
