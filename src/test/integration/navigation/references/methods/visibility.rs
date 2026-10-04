//! References filtered by private and protected visibility on explicit receivers.

use crate::test::harness::{check, FakeEditor};

#[tokio::test]
async fn references_private_method_exclude_invalid_explicit_receivers() {
    check(
        r#"
class Vault
  private

  def secret
    "token"
  end

  def call_secret
    <ref>secret$0</ref>
    Vault.new.secret
    Vault.new.public_send(:secret)
    send(:<ref>secret</ref>)
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn references_private_method_exclude_cross_file_invalid_explicit_receivers() {
    let mut editor = FakeEditor::new().await;
    let target = r#"
class Vault
  private

  def secret
    "token"
  end
end
"#;
    let caller = r#"
class Caller
  def run
    Vault.new.secret
    Vault.new.send(:secret)
  end
end
"#;

    editor.open("vault.rb", target).await;
    editor.open("caller.rb", caller).await;

    let refs = editor.references_at("vault.rb", 4, 6).await;
    assert!(
        !refs.iter().any(|location| {
            location.uri.path().ends_with("caller.rb") && location.range.start.line == 3
        }),
        "private method references must exclude invalid explicit receiver calls, got {refs:?}"
    );
    assert!(
        refs.iter().any(|location| {
            location.uri.path().ends_with("caller.rb") && location.range.start.line == 4
        }),
        "private method references should keep send(:secret), got {refs:?}"
    );
}

#[tokio::test]
async fn references_private_method_exclude_invalid_receivers_in_non_ascii_files() {
    let mut editor = FakeEditor::new().await;
    let target = "class Vault\n  private\n\n  def secret\n    \"token\"\n  end\nend\n";
    let caller = "class Caller\n  def run\n    Vault.new.secret\n    \"ü\"; Vault.new.secret\n    \"ü\"; Vault.new.send(:secret)\n    Vault.new.send(:secret)\n  end\nend\n";

    editor.open("vault.rb", target).await;
    editor.open("caller.rb", caller).await;

    let lines = |refs: &[tower_lsp::lsp_types::Location]| {
        let mut lines = refs
            .iter()
            .filter(|location| location.uri.path().ends_with("caller.rb"))
            .map(|location| location.range.start.line)
            .collect::<Vec<_>>();
        lines.sort_unstable();
        lines
    };
    let refs = editor.references_at("vault.rb", 3, 6).await;
    assert_eq!(
        lines(&refs),
        [4, 5],
        "private method references must drop explicit receivers on ASCII and non-ASCII lines \
         of a non-ASCII file, got {refs:?}"
    );
}

#[tokio::test]
async fn references_visibility_argument_form_exclude_invalid_explicit_receivers() {
    check(
        r#"
class Vault
  def secret
    "token"
  end
  private :secret

  def call_secret
    <ref>secret$0</ref>
    Vault.new.secret
    send(:<ref>secret</ref>)
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn references_protected_method_include_same_family_explicit_receiver() {
    check(
        r#"
class Vault
  def semi_secret$0
    "token"
  end
  protected :semi_secret

  def compare
    other = Vault.new
    other.<ref>semi_secret</ref>
  end
end

Vault.new.semi_secret
"#,
    )
    .await;
}

#[tokio::test]
async fn references_private_visibility_override_excludes_included_method_explicit_receiver() {
    check(
        r#"
module SharedSecret
  def hidden$0
    "hidden"
  end
end

class Vault
  include SharedSecret
  private :hidden

  def reveal
    <ref>hidden</ref>
  end
end

Vault.new.hidden
"#,
    )
    .await;
}

#[tokio::test]
async fn references_protected_visibility_override_includes_same_family_included_method_receiver() {
    check(
        r#"
module SharedSecret
  def hidden$0
    "hidden"
  end
end

class Vault
  include SharedSecret
  protected :hidden

  def compare
    other = Vault.new
    other.<ref>hidden</ref>
  end
end

Vault.new.hidden
"#,
    )
    .await;
}

#[tokio::test]
async fn references_public_visibility_override_from_original_mixin_definition_includes_call_site() {
    check(
        r#"
module SharedSecret
  private

  def hidden$0
    "hidden"
  end
end

class Vault
  include SharedSecret
  public :hidden
end

Vault.new.<ref>hidden</ref>
"#,
    )
    .await;
}
