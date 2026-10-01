//! Cross-file rename tests for classes, modules, and value constants.

use crate::server::RubyLanguageServer;
use crate::test::harness::FakeEditor;
use tower_lsp::lsp_types::{InitializeParams, OneOf, Position, PrepareRenameResponse, Range};
use tower_lsp::LanguageServer;

#[tokio::test]
async fn initialization_advertises_prepare_rename() {
    let server = RubyLanguageServer::default();
    let initialized = server
        .initialize(InitializeParams::default())
        .await
        .expect("server initialization should succeed");

    let provider = initialized
        .capabilities
        .rename_provider
        .expect("rename capability should be advertised");
    let OneOf::Right(options) = provider else {
        panic!("rename capability should advertise RenameOptions");
    };
    assert_eq!(options.prepare_provider, Some(true));
}

#[tokio::test]
async fn rename_class_updates_declaration_and_cross_file_reference() {
    let mut editor = FakeEditor::new().await;
    editor.open("user.rb", "class User\nend\n").await;
    editor
        .open("service.rb", "class Service\n  MODEL = User\nend\n")
        .await;

    let edit = editor
        .rename_at("user.rb", 0, 7, "Account")
        .await
        .expect("class rename should return a cross-file workspace edit");
    editor.apply_edit(&edit).await;

    assert_eq!(editor.content("user.rb"), "class Account\nend\n");
    assert_eq!(
        editor.content("service.rb"),
        "class Service\n  MODEL = Account\nend\n"
    );
}

#[tokio::test]
async fn prepare_class_rename_returns_exact_name_and_placeholder() {
    let mut editor = FakeEditor::new().await;
    editor.open("user.rb", "class User\nend\n").await;

    assert_eq!(
        editor.prepare_rename_at("user.rb", 0, 7).await,
        Some(PrepareRenameResponse::RangeWithPlaceholder {
            range: Range::new(Position::new(0, 6), Position::new(0, 10)),
            placeholder: "User".to_string(),
        })
    );
}

#[tokio::test]
async fn rename_module_and_value_constant_across_files() {
    let mut editor = FakeEditor::new().await;
    editor
        .open("feature.rb", "module Feature\n  LIMIT = 3\nend\n")
        .await;
    editor
        .open("consumer.rb", "include Feature\nputs Feature::LIMIT\n")
        .await;

    let module_edit = editor
        .rename_at("feature.rb", 0, 8, "Capability")
        .await
        .expect("module rename should succeed");
    editor.apply_edit(&module_edit).await;
    let constant_edit = editor
        .rename_at("feature.rb", 1, 3, "MAXIMUM")
        .await
        .expect("value constant rename should succeed after reindex");
    editor.apply_edit(&constant_edit).await;

    assert_eq!(
        editor.content("feature.rb"),
        "module Capability\n  MAXIMUM = 3\nend\n"
    );
    assert_eq!(
        editor.content("consumer.rb"),
        "include Capability\nputs Capability::MAXIMUM\n"
    );
}

#[tokio::test]
async fn rename_respects_namespace_identity() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "models.rb",
            "module A\n  class User\n  end\nend\nmodule B\n  class User\n  end\nend\n",
        )
        .await;
    editor.open("use.rb", "A::User.new\nB::User.new\n").await;

    let edit = editor
        .rename_at("models.rb", 1, 9, "Account")
        .await
        .expect("nested class rename should succeed");
    editor.apply_edit(&edit).await;

    assert_eq!(
        editor.content("models.rb"),
        "module A\n  class Account\n  end\nend\nmodule B\n  class User\n  end\nend\n"
    );
    assert_eq!(editor.content("use.rb"), "A::Account.new\nB::User.new\n");
}

#[tokio::test]
async fn rename_rejects_invalid_or_colliding_constant_name() {
    let mut editor = FakeEditor::new().await;
    editor
        .open("models.rb", "class User\nend\nclass Account\nend\n")
        .await;

    assert!(editor.rename_at("models.rb", 0, 7, "user").await.is_none());
    assert!(editor
        .rename_at("models.rb", 0, 7, "Account")
        .await
        .is_none());
}

#[tokio::test]
async fn rename_uses_utf16_positions_after_multibyte_text() {
    let mut editor = FakeEditor::new().await;
    editor
        .open("unicode.rb", "LABEL = \"😀\"; class User\nend\nUser.new\n")
        .await;

    let edit = editor
        .rename_at("unicode.rb", 0, 21, "Account")
        .await
        .expect("rename should resolve UTF-16 cursor positions");
    editor.apply_edit(&edit).await;

    assert_eq!(
        editor.content("unicode.rb"),
        "LABEL = \"😀\"; class Account\nend\nAccount.new\n"
    );
}

#[tokio::test]
async fn rename_from_reference_updates_reopened_definitions() {
    let mut editor = FakeEditor::new().await;
    editor.open("first.rb", "class User\nend\n").await;
    editor
        .open("second.rb", "class User\n  def active? = true\nend\n")
        .await;
    editor.open("use.rb", "User.new\n").await;

    let edit = editor
        .rename_at("use.rb", 0, 1, "Account")
        .await
        .expect("rename from a reference should update every reopened definition");
    editor.apply_edit(&edit).await;

    assert_eq!(editor.content("first.rb"), "class Account\nend\n");
    assert_eq!(
        editor.content("second.rb"),
        "class Account\n  def active? = true\nend\n"
    );
    assert_eq!(editor.content("use.rb"), "Account.new\n");
}
