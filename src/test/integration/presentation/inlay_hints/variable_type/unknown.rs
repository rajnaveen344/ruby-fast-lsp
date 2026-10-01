//! Inlay hints for variables with Unknown type.
//!
//! Unknown types display as ": ?" to indicate type couldn't be inferred.

use crate::test::harness::check;

/// Variable from unknown method shows ": ?"
#[tokio::test]
async fn unknown_method_result() {
    check(
        r#"
def foo
  x<hint label=": ?"> = some_unknown_method
end
"#,
    )
    .await;
}

#[tokio::test]
async fn explicit_receiver_private_method_result_is_unknown() {
    check(
        r#"
class Vault
  private

  # @return [String]
  def secret
    "token"
  end
end

result<hint label=": ?"> = Vault.new.secret
"#,
    )
    .await;
}

#[tokio::test]
async fn explicit_receiver_private_argument_form_method_result_is_unknown() {
    check(
        r#"
class Vault
  # @return [String]
  def secret
    "token"
  end
  private :secret
end

result<hint label=": ?"> = Vault.new.secret
"#,
    )
    .await;
}

#[tokio::test]
async fn explicit_receiver_private_mixin_visibility_override_result_is_unknown() {
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

result<hint label=": ?"> = Vault.new.hidden
"#,
    )
    .await;
}

/// Variable assigned from another unknown variable
#[tokio::test]
async fn variable_to_variable_unknown() {
    check(
        r#"
def foo
  x = unknown_thing
  y<hint label=": ?"> = x
end
"#,
    )
    .await;
}
