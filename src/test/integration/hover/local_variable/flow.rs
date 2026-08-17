//! Hover tests for flow-sensitive type tracking.
//!
//! When a variable is reassigned, hover should show the type at that
//! specific position in the code flow.

use crate::test::harness::{check, FakeEditor};

/// Reassignment evaluates the RHS against the previous binding.
#[tokio::test]
async fn reassignment_rhs_uses_the_previous_type() {
    check(
        r#"
def tick
  count = 1
  count = count<hover label="Integer"> + 1
  count<hover label="Integer">
end
"#,
    )
    .await;
}

/// Nested call proofs recorded while visiting the RHS must type the assignment.
#[tokio::test]
async fn assigned_call_chain_keeps_the_inner_return_type() {
    check(
        r#"
class User
  # @return [Profile]
  def profile
    Profile.new
  end
end

class Profile
  # @return [String]
  def name
    "Ada"
  end
end

def inspect_user
  user = User.new
  name<hover label="String"> = user.profile.name
end
"#,
    )
    .await;
}

#[tokio::test]
async fn assigned_shape_chain_keeps_the_nested_field_type() {
    check(
        r#"
def inspect_payload
  payload = { user: { name: "Ada" } }
  name<hover label="String"> = payload[:user][:name]
end
"#,
    )
    .await;
}

/// The assigned name is a local while its RHS block runs, not a method call.
#[tokio::test]
async fn assigned_block_treats_the_name_as_a_local() {
    check(
        r#"
def wrap
  values = [1]
  strings = values.map { |value| <err none>strings</err>; value.to_s }
end
"#,
    )
    .await;
}

/// Hover shows different types at different positions after reassignment
#[tokio::test]
async fn reassigned_variable() {
    check(
        r#"
aaa = 1
bbb = 2.1

puts bbb<hover label="Float">

bbb = aaa

puts bbb<hover label="Integer">
"#,
    )
    .await;
}

#[tokio::test]
async fn hash_alias_mutation_updates_every_local_hover() {
    check(
        r#"
def build
  payload = { count: 1, state: :ready }
  copy = payload
  copy[:count] = "many"
  payload<hover label="{ count: String, state: :ready }">
end
"#,
    )
    .await;
}

#[tokio::test]
async fn mutable_hash_escape_and_reproof_replace_exact_flow_evidence() {
    let mut editor = FakeEditor::new().await;
    editor
        .open_and_check_fixture(
            "shape_flow_lifecycle.rb",
            r#"def build
  payload = { count: 1 }
  copy = payload
  copy[:count] = "many"
  payload<hover label="{ count: String }">
end
"#,
        )
        .await;

    editor
        .set_and_check_fixture(
            "shape_flow_lifecycle.rb",
            r#"def build
  payload = { count: 1 }
  copy = payload
  dynamic_sink(copy)
  payload<hover label="Unknown[mutable_shape_invalidated]">
end
"#,
        )
        .await;

    editor
        .set_and_check_fixture(
            "shape_flow_lifecycle.rb",
            r#"def build
  payload = { count: 1 }
  copy = payload
  copy[:count] = true
  payload<hover label="{ count: TrueClass }">
end
"#,
        )
        .await;
}
