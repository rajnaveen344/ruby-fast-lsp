use ruby_fast_lsp_test_harness::FakeEditor;
use tempfile::TempDir;

fn workspace_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("test harness crate must live under crates/lsp-test-harness")
        .to_path_buf()
}

fn rails_package(index_output: &str) -> (TempDir, std::path::PathBuf) {
    let source = workspace_root().join("extensions/rails-ruby");
    let temp = TempDir::new().expect("rails extension temp package must be created");
    let package = temp.path().join("rails-ruby");
    std::fs::create_dir(&package).expect("rails extension package directory must be created");
    if std::env::var("RUBY_FAST_LSP_TEST_BUILT_RAILS").as_deref() == Ok("1") {
        std::fs::copy(
            source.join("extension.toml"),
            package.join("extension.toml"),
        )
        .expect("rails extension manifest must be copied");
        std::fs::copy(source.join("extension.wasm"), package.join("extension.wasm")).expect(
            "built rails Wasm is required; run the mruby SDK builder before setting RUBY_FAST_LSP_TEST_BUILT_RAILS=1",
        );
        return (temp, package);
    }
    let manifest = std::fs::read_to_string(source.join("extension.toml"))
        .expect("rails extension manifest must be readable")
        .lines()
        .filter(|line| !line.starts_with("checksum_sha256"))
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(package.join("extension.toml"), format!("{manifest}\n"))
        .expect("deterministic fixture manifest must be written without the production checksum");
    let mut wat = std::fs::read_to_string(source.join("contract.wat.in"))
        .expect("rails extension contract fixture must be readable");
    for (name, pointer, file) in [
        ("NAMES", 1024_u64, "indexed_call_names.json"),
        ("INDEX", 2048_u64, index_output),
        ("EMPTY", 8192_u64, "empty_output.json"),
    ] {
        let payload = std::fs::read(source.join(file))
            .unwrap_or_else(|err| panic!("rails fixture `{file}` must be readable: {err}"));
        serde_json::from_slice::<serde_json::Value>(&payload)
            .unwrap_or_else(|err| panic!("rails fixture `{file}` must be valid JSON: {err}"));
        let escaped = payload
            .iter()
            .map(|byte| format!("\\{byte:02x}"))
            .collect::<String>();
        let packed = (pointer << 32) | payload.len() as u64;
        wat = wat
            .replace(&format!("__{name}_DATA__"), &escaped)
            .replace(&format!("__{name}_PACKED__"), &packed.to_string());
    }
    let wasm = wat::parse_str(wat).expect("rails extension contract fixture must compile");
    std::fs::write(package.join("extension.wasm"), wasm)
        .expect("rails extension fixture Wasm must be written");
    (temp, package)
}

#[tokio::test]
async fn active_record_association_uses_public_semantic_contracts() {
    let (_temp, package) = rails_package("index_output.json");
    let mut editor = FakeEditor::with_extension_package(package).await;
    editor
        .open(
            "app/models/user.rb",
            "module Billing\n  class Account\n    def label\n      \"account\"\n    end\n  end\nend\n\nclass User\n  belongs_to :account, class_name: \"Billing::Account\"\n  def display\n    account.label\n  end\nend\n",
        )
        .await;
    let statuses = editor.extension_status().await;
    assert!(
        statuses
            .iter()
            .any(|status| status.id == "rails-ruby" && status.status == "loaded"),
        "rails extension must remain loaded after association indexing, got {statuses:?}"
    );

    let target = editor.goto_definition("app/models/user.rb", 9, 40).await;
    assert_eq!(
        target.len(),
        1,
        "class_name must reference Billing::Account"
    );
    assert_eq!(target[0].range.start.line, 1);

    let definition = editor.goto_definition("app/models/user.rb", 11, 6).await;
    assert_eq!(definition.len(), 1, "association reader must resolve");
    assert_eq!(definition[0].range.start.line, 9);
    assert_eq!(definition[0].range.start.character, 13);

    let hover = editor.hover("app/models/user.rb", 11, 6).await;
    assert!(
        hover.as_ref().is_some_and(|hover| {
            format!("{:?}", hover.contents).contains("(Billing::Account | NilClass)")
        }),
        "association reader must carry its structured target type, got {hover:?}"
    );

    editor
        .set(
            "app/models/user.rb",
            "module Billing\n  class Account\n    def label\n      \"account\"\n    end\n  end\nend\n\nclass User\n  def display\n    account\n  end\nend\n",
        )
        .await;
    assert!(
        editor
            .goto_definition("app/models/user.rb", 10, 6)
            .await
            .is_empty(),
        "removing the association must remove its generated reader"
    );
}

#[tokio::test]
async fn polymorphic_association_does_not_invent_a_constant_target() {
    let (_temp, package) = rails_package("polymorphic_index_output.json");
    let mut editor = FakeEditor::with_extension_package(package).await;
    editor
        .open(
            "app/models/attachment.rb",
            "class Attachment\n  belongs_to :subject, polymorphic: true\n  def attached\n    subject\n  end\nend\n",
        )
        .await;

    let statuses = editor.extension_status().await;
    assert!(
        statuses
            .iter()
            .any(|status| status.id == "rails-ruby" && status.status == "loaded"),
        "polymorphic indexing must not disable the Rails extension, got {statuses:?}"
    );
    assert!(
        editor
            .goto_definition("app/models/attachment.rb", 1, 16)
            .await
            .is_empty(),
        "polymorphic DSL argument must not guess a Subject constant"
    );
    let definition = editor
        .goto_definition("app/models/attachment.rb", 3, 6)
        .await;
    assert_eq!(definition.len(), 1, "polymorphic reader must still exist");
    assert_eq!(definition[0].range.start.line, 1);
}

#[tokio::test]
async fn callbacks_and_custom_validations_reference_private_methods() {
    let (_temp, package) = rails_package("callback_index_output.json");
    let mut editor = FakeEditor::with_extension_package(package).await;
    editor
        .open(
            "app/models/user.rb",
            "class User\n  before_save :normalize_account\n  validate :account_is_active\n  private\n  def normalize_account\n  end\n  def account_is_active\n  end\nend\n",
        )
        .await;

    let callback = editor.goto_definition("app/models/user.rb", 1, 18).await;
    assert_eq!(callback.len(), 1, "callback symbol must resolve");
    assert_eq!(callback[0].range.start.line, 4);

    let validation = editor.goto_definition("app/models/user.rb", 2, 15).await;
    assert_eq!(validation.len(), 1, "custom validation symbol must resolve");
    assert_eq!(validation[0].range.start.line, 6);

    let references = editor.references("app/models/user.rb", 4, 8).await;
    assert!(
        references.iter().any(|location| {
            location.range.start.line == 1 && location.range.start.character == 14
        }),
        "callback symbol must enter ordinary engine method references, got {references:?}"
    );

    editor
        .set(
            "app/models/user.rb",
            "class User\n  private\n  def normalize_account\n  end\n  def account_is_active\n  end\nend\n",
        )
        .await;
    let references = editor.references("app/models/user.rb", 2, 8).await;
    assert!(
        references.iter().all(|location| {
            !(location.range.start.line == 1 && location.range.start.character == 14)
        }),
        "removing callback declarations must remove stale method references, got {references:?}"
    );
}
