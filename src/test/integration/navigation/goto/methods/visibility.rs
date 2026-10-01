//! Goto definition filtered by private and protected visibility on explicit receivers.

use crate::test::harness::FakeEditor;

#[tokio::test]
async fn goto_explicit_receiver_private_method_does_not_resolve() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "visibility_private_receiver.rb",
            r#"class Vault
  private

  def secret
    "token"
  end
end

Vault.new.secret
Vault.new.send(:secret)
"#,
        )
        .await;

    let explicit_receiver_defs = editor
        .goto_def_at("visibility_private_receiver.rb", 8, 11)
        .await;
    assert!(
        explicit_receiver_defs.is_empty(),
        "explicit receiver must not resolve a private method, got {explicit_receiver_defs:?}"
    );

    let send_defs = editor
        .goto_def_at("visibility_private_receiver.rb", 9, 17)
        .await;
    assert_eq!(
        send_defs.len(),
        1,
        "send should resolve a private method because Ruby send bypasses visibility"
    );
}

#[tokio::test]
async fn goto_visibility_argument_form_filters_explicit_receivers() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "visibility_argument_form.rb",
            r#"class Vault
  def secret
    "token"
  end
  private :secret

  def semi_secret
    "token"
  end
  protected :semi_secret
end

Vault.new.secret
Vault.new.send(:secret)
Vault.new.semi_secret
"#,
        )
        .await;

    let private_explicit_defs = editor
        .goto_def_at("visibility_argument_form.rb", 12, 10)
        .await;
    assert!(
        private_explicit_defs.is_empty(),
        "explicit receiver must not resolve private :name methods, got {private_explicit_defs:?}"
    );

    let private_send_defs = editor
        .goto_def_at("visibility_argument_form.rb", 13, 17)
        .await;
    assert_eq!(
        private_send_defs.len(),
        1,
        "send should resolve private :name methods because Ruby send bypasses visibility"
    );

    let protected_explicit_defs = editor
        .goto_def_at("visibility_argument_form.rb", 14, 10)
        .await;
    assert!(
        protected_explicit_defs.is_empty(),
        "external explicit receiver must not resolve protected :name methods, got {protected_explicit_defs:?}"
    );
}

#[tokio::test]
async fn goto_protected_method_allows_same_family_explicit_receiver() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "protected_same_family.rb",
            r#"class Vault
  def semi_secret
    "token"
  end
  protected :semi_secret

  def compare
    other = Vault.new
    other.semi_secret
  end
end

Vault.new.semi_secret
"#,
        )
        .await;

    let same_family_defs = editor.goto_def_at("protected_same_family.rb", 8, 10).await;
    assert_eq!(
        same_family_defs.len(),
        1,
        "same-family explicit receiver should resolve protected method, got {same_family_defs:?}"
    );

    let external_defs = editor.goto_def_at("protected_same_family.rb", 12, 10).await;
    assert!(
        external_defs.is_empty(),
        "external explicit receiver must not resolve protected method, got {external_defs:?}"
    );
}

#[tokio::test]
async fn goto_private_visibility_override_filters_included_method_explicit_receiver() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "mixin_visibility_override.rb",
            r#"module SharedSecret
  def hidden
    "hidden"
  end
end

class Vault
  include SharedSecret
  private :hidden

  def reveal
    hidden
  end
end

Vault.new.hidden
"#,
        )
        .await;

    let bare_defs = editor
        .goto_def_at("mixin_visibility_override.rb", 11, 5)
        .await;
    assert_eq!(
        bare_defs.len(),
        1,
        "bare call should still resolve included private method, got {bare_defs:?}"
    );

    let explicit_defs = editor
        .goto_def_at("mixin_visibility_override.rb", 15, 10)
        .await;
    assert!(
        explicit_defs.is_empty(),
        "private override on included method must block external explicit receiver, got {explicit_defs:?}"
    );
}

#[tokio::test]
async fn goto_public_visibility_override_allows_included_private_method_explicit_receiver() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "mixin_public_visibility_override.rb",
            r#"module SharedSecret
  private

  def hidden
    "hidden"
  end
end

class Vault
  include SharedSecret
  public :hidden
end

Vault.new.hidden
"#,
        )
        .await;

    let explicit_defs = editor
        .goto_def_at("mixin_public_visibility_override.rb", 13, 10)
        .await;
    assert_eq!(
        explicit_defs.len(),
        1,
        "public override on included private method should allow explicit receiver, got {explicit_defs:?}"
    );
}

#[tokio::test]
async fn goto_protected_visibility_override_allows_same_family_included_method_receiver() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "mixin_protected_visibility_override.rb",
            r#"module SharedSecret
  def hidden
    "hidden"
  end
end

class Vault
  include SharedSecret
  protected :hidden

  def compare
    other = Vault.new
    other.hidden
  end
end

Vault.new.hidden
"#,
        )
        .await;

    let same_family_defs = editor
        .goto_def_at("mixin_protected_visibility_override.rb", 12, 10)
        .await;
    assert_eq!(
        same_family_defs.len(),
        1,
        "protected override should allow same-family explicit receiver, got {same_family_defs:?}"
    );

    let external_defs = editor
        .goto_def_at("mixin_protected_visibility_override.rb", 16, 10)
        .await;
    assert!(
        external_defs.is_empty(),
        "protected override should block external explicit receiver, got {external_defs:?}"
    );
}
