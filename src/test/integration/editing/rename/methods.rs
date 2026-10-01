//! Project-wide method rename tests.

use crate::test::harness::FakeEditor;
use tower_lsp::lsp_types::{Position, PrepareRenameResponse, Range};

#[tokio::test]
async fn rename_instance_method_updates_definition_and_resolved_calls() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "user.rb",
            "class User\n  def display_name = 'Naveen'\nend\n",
        )
        .await;
    editor
        .open(
            "service.rb",
            "class Service\n  def call\n    User.new.display_name\n  end\nend\n",
        )
        .await;

    let edit = editor
        .rename_at("user.rb", 1, 8, "label")
        .await
        .expect("an ordinary project method should support cross-file rename");
    editor.apply_edit(&edit).await;

    assert_eq!(
        editor.content("user.rb"),
        "class User\n  def label = 'Naveen'\nend\n"
    );
    assert_eq!(
        editor.content("service.rb"),
        "class Service\n  def call\n    User.new.label\n  end\nend\n"
    );
}

#[tokio::test]
async fn prepare_method_rename_returns_exact_name_and_placeholder() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "user.rb",
            "class User\n  def display_name = 'Naveen'\nend\n",
        )
        .await;

    assert_eq!(
        editor.prepare_rename_at("user.rb", 1, 8).await,
        Some(PrepareRenameResponse::RangeWithPlaceholder {
            range: Range::new(Position::new(1, 6), Position::new(1, 18)),
            placeholder: "display_name".to_string(),
        })
    );
}

#[tokio::test]
async fn rename_keeps_instance_and_singleton_method_identities_separate() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "user.rb",
            "class User\n  def name = 'instance'\n  def self.name = 'class'\nend\nuser = User.new\nuser.name\nUser.name\n",
        )
        .await;

    let edit = editor
        .rename_at("user.rb", 1, 7, "label")
        .await
        .expect("instance method identity should be independently renameable");
    editor.apply_edit(&edit).await;

    assert_eq!(
        editor.content("user.rb"),
        "class User\n  def label = 'instance'\n  def self.name = 'class'\nend\nuser = User.new\nuser.label\nUser.name\n"
    );
}

#[tokio::test]
async fn rename_rejects_invalid_name_owner_collision_and_alias_declaration() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "user.rb",
            "class User\n  def name = 'Naveen'\n  def label = name\n  alias display_name name\nend\n",
        )
        .await;

    assert!(editor
        .rename_at("user.rb", 1, 7, "not valid")
        .await
        .is_none());
    assert!(editor.rename_at("user.rb", 1, 7, "label").await.is_none());
    assert!(
        editor.rename_at("user.rb", 3, 10, "title").await.is_none(),
        "alias-backed declarations must remain fail-closed until alias coupling is modeled"
    );
}

#[tokio::test]
async fn rename_from_inherited_call_updates_reopened_parent_definitions() {
    let mut editor = FakeEditor::new().await;
    editor
        .open("parent_one.rb", "class Parent\n  def name = 'first'\nend\n")
        .await;
    editor
        .open(
            "parent_two.rb",
            "class Parent\n  def name = 'second'\nend\n",
        )
        .await;
    editor
        .open("child.rb", "class Child < Parent\nend\nChild.new.name\n")
        .await;

    let edit = editor
        .rename_at("parent_one.rb", 1, 7, "label")
        .await
        .expect("reopened parent definitions should form one rename identity");
    editor.apply_edit(&edit).await;

    assert_eq!(
        editor.content("parent_one.rb"),
        "class Parent\n  def label = 'first'\nend\n"
    );
    assert_eq!(
        editor.content("parent_two.rb"),
        "class Parent\n  def label = 'second'\nend\n"
    );
    assert_eq!(
        editor.content("child.rb"),
        "class Child < Parent\nend\nChild.new.label\n"
    );
}

#[tokio::test]
async fn rename_rejects_super_coupled_override_family() {
    let mut editor = FakeEditor::new().await;
    editor
        .open("parent.rb", "class Parent\n  def name = 'parent'\nend\n")
        .await;
    editor
        .open(
            "child.rb",
            "class Child < Parent\n  def name\n    super\n  end\nend\nChild.new.name\n",
        )
        .await;

    assert!(
        editor.rename_at("parent.rb", 1, 7, "label").await.is_none(),
        "a parent method targeted by super is coupled to the override family"
    );
    assert!(
        editor.rename_at("child.rb", 1, 7, "label").await.is_none(),
        "renaming an override containing super would change the forwarded method name"
    );
}

#[tokio::test]
async fn rename_private_method_updates_implicit_call_static_send_and_alias_source() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "user.rb",
            "class User\n  def normalize = 'ok'\n  private :normalize\n  alias normalize_account normalize\n  def call\n    normalize\n    send(:normalize)\n  end\nend\n",
        )
        .await;

    let edit = editor
        .rename_at("user.rb", 1, 8, "sanitize")
        .await
        .expect("private methods and static visibility-bypass sends are defensible");
    editor.apply_edit(&edit).await;

    assert_eq!(
        editor.content("user.rb"),
        "class User\n  def sanitize = 'ok'\n  private :sanitize\n  alias normalize_account sanitize\n  def call\n    sanitize\n    send(:sanitize)\n  end\nend\n"
    );
}

#[tokio::test]
async fn rename_rejects_destination_collision_in_inherited_lookup_chain() {
    let mut editor = FakeEditor::new().await;
    editor
        .open("parent.rb", "class Parent\n  def label = 'parent'\nend\n")
        .await;
    editor
        .open(
            "child.rb",
            "class Child < Parent\n  def name = 'child'\nend\nChild.new.name\n",
        )
        .await;

    assert!(
        editor.rename_at("child.rb", 1, 7, "label").await.is_none(),
        "renaming Child#name onto inherited Parent#label would change dispatch"
    );
}

#[tokio::test]
async fn rename_rejects_destination_collision_in_descendant_lookup_chain() {
    let mut editor = FakeEditor::new().await;
    editor
        .open("parent.rb", "class Parent\n  def name = 'parent'\nend\n")
        .await;
    editor
        .open(
            "child.rb",
            "class Child < Parent\n  def label = 'child'\nend\n",
        )
        .await;

    assert!(
        editor
            .rename_at("parent.rb", 1, 7, "label")
            .await
            .is_none(),
        "renaming Parent#name onto Child#label would change descendant dispatch even without a current call site"
    );
}

#[tokio::test]
async fn method_rename_follows_document_replacement_without_stale_identity() {
    let mut editor = FakeEditor::new().await;
    editor
        .open("user.rb", "class User\n  def name = 'Naveen'\nend\n")
        .await;
    editor
        .set("user.rb", "class User\n  def label = 'Naveen'\nend\n")
        .await;

    let edit = editor
        .rename_at("user.rb", 1, 8, "title")
        .await
        .expect("the replacement method identity should be immediately renameable");
    editor.apply_edit(&edit).await;
    assert_eq!(
        editor.content("user.rb"),
        "class User\n  def title = 'Naveen'\nend\n"
    );
}

#[tokio::test]
async fn rename_rejects_macro_generated_and_module_function_coupled_methods() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "macros.rb",
            "class User\n  attr_reader :name\n  define_method(:label) { 'Naveen' }\nend\nmodule Helpers\n  def normalize = 'ok'\n  module_function :normalize\nend\n",
        )
        .await;

    assert!(editor
        .rename_at("macros.rb", 1, 16, "title")
        .await
        .is_none());
    assert!(editor
        .rename_at("macros.rb", 2, 18, "title")
        .await
        .is_none());
    assert!(editor
        .rename_at("macros.rb", 5, 8, "sanitize")
        .await
        .is_none());
}

#[tokio::test]
async fn rename_rejects_method_owner_with_unresolved_lookup_edge() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "user.rb",
            "class User < MissingParent\n  def name = 'Naveen'\nend\nUser.new.name\n",
        )
        .await;

    assert!(
        editor.rename_at("user.rb", 1, 7, "label").await.is_none(),
        "an incomplete MRO cannot prove a collision-free method rename"
    );
}

#[tokio::test]
async fn rename_singleton_method_from_call_does_not_touch_instance_override() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "user.rb",
            "class User\n  def name = 'instance'\n  def self.name = 'class'\nend\nUser.name\nUser.new.name\n",
        )
        .await;

    assert_eq!(
        editor.prepare_rename_at("user.rb", 4, 6).await,
        Some(PrepareRenameResponse::RangeWithPlaceholder {
            range: Range::new(Position::new(4, 5), Position::new(4, 9)),
            placeholder: "name".to_string(),
        })
    );
    let edit = editor
        .rename_at("user.rb", 4, 6, "label")
        .await
        .expect("a resolved singleton call should select only the singleton identity");
    editor.apply_edit(&edit).await;
    assert_eq!(
        editor.content("user.rb"),
        "class User\n  def name = 'instance'\n  def self.label = 'class'\nend\nUser.label\nUser.new.name\n"
    );
}

#[tokio::test]
async fn rename_subclass_override_does_not_touch_parent_identity() {
    let mut editor = FakeEditor::new().await;
    editor
        .open("parent.rb", "class Parent\n  def name = 'parent'\nend\n")
        .await;
    editor
        .open(
            "child.rb",
            "class Child < Parent\n  def name = 'child'\nend\nParent.new.name\nChild.new.name\n",
        )
        .await;

    let edit = editor
        .rename_at("child.rb", 1, 7, "label")
        .await
        .expect("an override without super is an independent method identity");
    editor.apply_edit(&edit).await;
    assert_eq!(
        editor.content("parent.rb"),
        "class Parent\n  def name = 'parent'\nend\n"
    );
    assert_eq!(
        editor.content("child.rb"),
        "class Child < Parent\n  def label = 'child'\nend\nParent.new.name\nChild.new.label\n"
    );
}

#[tokio::test]
async fn method_rename_uses_utf16_cursor_and_edit_positions() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "unicode.rb",
            "class User\n  LABEL = '😀'; def name = 'Naveen'\nend\nUser.new.name\n",
        )
        .await;

    let edit = editor
        .rename_at("unicode.rb", 1, 22, "label")
        .await
        .expect("method rename must convert UTF-16 cursor positions through engine ranges");
    editor.apply_edit(&edit).await;
    assert_eq!(
        editor.content("unicode.rb"),
        "class User\n  LABEL = '😀'; def label = 'Naveen'\nend\nUser.new.label\n"
    );
}

#[tokio::test]
async fn rename_rejects_operator_methods_and_writer_shape_changes() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "syntax.rb",
            "class Value\n  def +(other) = self\n  def name = 'Naveen'\n  def name=(value) = value\nend\n",
        )
        .await;

    assert!(editor.rename_at("syntax.rb", 1, 6, "add").await.is_none());
    assert!(editor
        .rename_at("syntax.rb", 2, 7, "label=")
        .await
        .is_none());
    assert!(editor.rename_at("syntax.rb", 3, 8, "label").await.is_none());
}
