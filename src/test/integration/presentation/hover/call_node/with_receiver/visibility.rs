//! Hover on explicit-receiver calls filtered by private and protected visibility.

use crate::test::harness::check;

#[tokio::test]
async fn explicit_receiver_private_method_hover_is_unknown() {
    check(
        r#"
class Vault
  private

  # @return [String]
  def secret
    "token"
  end
end

Vault.new.secret<hover label="?">
"#,
    )
    .await;
}

#[tokio::test]
async fn explicit_receiver_private_argument_form_method_hover_is_unknown() {
    check(
        r#"
class Vault
  # @return [String]
  def secret
    "token"
  end
  private :secret
end

Vault.new.secret<hover label="?">
"#,
    )
    .await;
}

#[tokio::test]
async fn explicit_receiver_private_mixin_visibility_override_hover_is_unknown() {
    check(
        r#"
module SharedSecret
  # @return [String]
  def hidden
    "hidden"
  end
end

class Vault
  include SharedSecret
  private :hidden
end

Vault.new.hidden<hover label="?">
"#,
    )
    .await;
}

#[tokio::test]
async fn explicit_receiver_public_mixin_visibility_override_hover_uses_return_type() {
    check(
        r#"
module SharedSecret
  private

  # @return [String]
  def hidden
    "hidden"
  end
end

class Vault
  include SharedSecret
  public :hidden
end

Vault.new.hidden<hover label="String">
"#,
    )
    .await;
}

#[tokio::test]
async fn same_family_protected_mixin_visibility_override_hover_uses_return_type() {
    check(
        r#"
module SharedSecret
  # @return [String]
  def hidden
    "hidden"
  end
end

class Vault
  include SharedSecret
  protected :hidden

  def compare
    other = Vault.new
    other.hidden<hover label="String">
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn protected_same_family_explicit_receiver_hover_uses_return_type() {
    check(
        r#"
class Vault
  # @return [String]
  def semi_secret
    "token"
  end
  protected :semi_secret

  def compare
    other = Vault.new
    other.semi_secret<hover label="String">
  end
end
"#,
    )
    .await;
}
