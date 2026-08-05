use crate::test::harness::check;

#[tokio::test]
async fn multiline_chain_uses_proven_intermediate_expression_types() {
    check(
        r#"
class Profile
  def name
    "Ada"
  end
end

class User
  def profile
    Profile.new
  end
end

User.new<hint label=": User">
  .profile<hint label=": Profile">
  .name
"#,
    )
    .await;
}

#[tokio::test]
async fn unresolved_multiline_chain_emits_no_type_hint() {
    check(
        r#"
<hint none>unresolved_source
  .profile</hint>
"#,
    )
    .await;
}
