//! Generated helper methods (`let`, `subject`) and DSL macro diagnostics.

use tower_lsp::lsp_types::NumberOrString;

use super::harness::RspecEditor;

#[tokio::test]
async fn let_defines_helper_method() {
    let mut editor = RspecEditor::new().await;
    editor
        .check(
            r#"
class User
end

module RSpec
end

RSpec.describe User do
  let(<def>:user</def>) { User.new }

  it "uses helper" do
    u$0ser
  end
end
"#,
        )
        .await;
}

#[tokio::test]
async fn subject_with_name_defines_helper_method() {
    let mut editor = RspecEditor::new().await;
    editor
        .check(
            r#"
class User
end

module RSpec
end

RSpec.describe User do
  subject(<def>:record</def>) { User.new }

  it "uses subject helper" do
    rec$0ord
  end
end
"#,
        )
        .await;
}

#[tokio::test]
async fn bang_subject_defines_subject_helper_method() {
    let mut editor = RspecEditor::new().await;
    editor
        .check(
            r#"
class User
end

module RSpec
end

RSpec.describe User do
  <def>subject! { User.new }</def>

  it "uses subject helper" do
    sub$0ject
  end
end
"#,
        )
        .await;
}

#[tokio::test]
async fn dsl_macros_do_not_report_unresolved_methods() {
    let mut editor = RspecEditor::new().await;
    editor
        .check(
            r#"
class User
  def name
    "Ada"
  end
end

module RSpec
end

<err none>RSpec.describe User do
  subject(:user) { User.new }

  context "when active" do
    let(:nickname) { "ada" }

    it "returns name" do
      user.name
      nickname
    end
  end
end</err>
"#,
        )
        .await;
}

#[tokio::test]
async fn dsl_macros_do_not_report_wrong_arity() {
    let mut editor = RspecEditor::new().await;
    editor
        .check(
            r#"
class User
end

module RSpec
end

<warn none code="wrong-arity">RSpec.describe User do
  context "when active" do
    let(:nickname) { "ada" }

    it "returns name" do
    end
  end
end</warn>
"#,
        )
        .await;
}

fn is_unresolved_audit_on_user(diagnostic: &tower_lsp::lsp_types::Diagnostic) -> bool {
    matches!(
        &diagnostic.code,
        Some(NumberOrString::String(code)) if code == "unresolved-method"
    ) && diagnostic.message == "Unresolved method `audit` on `User`"
}

#[tokio::test]
async fn inferred_helper_diagnostics_follow_return_type_edits() {
    let mut editor = RspecEditor::new().await;
    let filename = editor.path("spec/inferred_diagnostic_spec.rb");
    let initial = r#"class User
  def name
  end
end

class Admin
  def audit
  end
end

module RSpec
end

RSpec.describe Object do
  let(:actor) { User.new }

  it "checks the inferred receiver" do
    actor.audit
  end
end
"#;
    editor.open(&filename, initial).await;

    let initial_diagnostics = editor.diagnostics(&filename).await;
    let unresolved = initial_diagnostics
        .iter()
        .filter(|diagnostic| is_unresolved_audit_on_user(diagnostic))
        .collect::<Vec<_>>();
    assert_eq!(
        unresolved.len(),
        1,
        "a missing method on a let-derived receiver must produce one diagnostic: {initial_diagnostics:?}"
    );
    assert_eq!(unresolved[0].range.start.line, 17);

    editor
        .set(
            &filename,
            &initial.replace("let(:actor) { User.new }", "let(:actor) { Admin.new }"),
        )
        .await;
    let updated_diagnostics = editor.diagnostics(&filename).await;
    assert!(
        !updated_diagnostics
            .iter()
            .any(is_unresolved_audit_on_user),
        "changing the let block return must remove the stale receiver diagnostic: {updated_diagnostics:?}"
    );
}

#[tokio::test]
async fn generated_helper_rename_follows_global_method_rename_policy() {
    let mut editor = RspecEditor::new().await;
    let filename = editor.path("spec/generated_helper_rename_spec.rb");
    editor
        .open(
            &filename,
            r#"module RSpec
end

RSpec.describe Object do
  let(:actor) { Object.new }

  it "uses the helper" do
    actor
  end
end
"#,
        )
        .await;

    assert!(
        editor
            .rename_at(&filename, 7, 6, "principal")
            .await
            .is_none(),
        "generated RSpec methods must not bypass the project-wide method rename policy"
    );
}

#[tokio::test]
async fn extension_requires_resolved_rspec_constant() {
    let mut editor = RspecEditor::new().await;
    editor
        .check(
            r#"
class User
end

<err>RSpec</err>.describe User do
  let(:user) { User.new }

  it "uses helper" do
    user
  end
end
"#,
        )
        .await;
}
