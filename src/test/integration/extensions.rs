//! Integration tests for extension-generated index facts.

use crate::test::harness::{check, FakeEditor};

#[tokio::test]
async fn rspec_let_defines_helper_method() {
    check(
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
async fn rspec_subject_with_name_defines_helper_method() {
    check(
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
async fn rspec_bang_subject_defines_subject_helper_method() {
    check(
        r#"
class User
end

module RSpec
end

RSpec.describe User do
  subject! { User.new }

  it "uses subject helper" do
    sub$0ject
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn rspec_dsl_macros_do_not_report_unresolved_methods() {
    check(
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
async fn rspec_dsl_macros_do_not_report_wrong_arity() {
    check(
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

#[tokio::test]
async fn rspec_inferred_helper_diagnostics_follow_return_type_edits() {
    let mut editor = FakeEditor::new().await;
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
    editor.open("inferred_diagnostic_spec.rb", initial).await;

    let initial_diagnostics = editor.diagnostics("inferred_diagnostic_spec.rb").await;
    let unresolved = initial_diagnostics
        .iter()
        .filter(|diagnostic| {
            matches!(
                &diagnostic.code,
                Some(tower_lsp::lsp_types::NumberOrString::String(code))
                    if code == "unresolved-method"
            ) && diagnostic.message == "Unresolved method `audit` on `User`"
        })
        .collect::<Vec<_>>();
    assert_eq!(
        unresolved.len(),
        1,
        "a missing method on a let-derived receiver must produce one diagnostic: {initial_diagnostics:?}"
    );
    assert_eq!(unresolved[0].range.start.line, 17);

    editor
        .set(
            "inferred_diagnostic_spec.rb",
            &initial.replace("let(:actor) { User.new }", "let(:actor) { Admin.new }"),
        )
        .await;
    let updated_diagnostics = editor.diagnostics("inferred_diagnostic_spec.rb").await;
    assert!(
        updated_diagnostics.iter().all(|diagnostic| {
            !matches!(
                &diagnostic.code,
                Some(tower_lsp::lsp_types::NumberOrString::String(code))
                    if code == "unresolved-method"
            ) || diagnostic.message != "Unresolved method `audit` on `User`"
        }),
        "changing the let block return must remove the stale receiver diagnostic: {updated_diagnostics:?}"
    );
}

#[tokio::test]
async fn rspec_generated_helper_rename_follows_global_method_rename_policy() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "generated_helper_rename_spec.rb",
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
            .rename_at("generated_helper_rename_spec.rb", 7, 6, "principal")
            .await
            .is_none(),
        "generated RSpec methods must not bypass the project-wide method rename policy"
    );
}

#[tokio::test]
async fn rspec_extension_requires_resolved_rspec_constant() {
    check(
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

#[tokio::test]
async fn rspec_include_makes_helper_methods_visible() {
    check(
        r#"
module SpecHelpers
  <def>def reset_db
  end</def>
end

module RSpec
end

module ApiSpec
  RSpec.describe User do
    include SpecHelpers

    before do
      reset_$0db
    end
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn rspec_extend_makes_helper_methods_visible_on_singleton_scope() {
    check(
        r#"
module SpecHelpers
  <def>def reset_db
  end</def>
end

module RSpec
end

module ApiSpec
  RSpec.describe User do
    extend SpecHelpers

    def self.setup
      reset_$0db
    end
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn rspec_extension_does_not_treat_other_describe_as_rspec_scope() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "plain_spec.rb",
            r#"
class User
end

module SpecHelpers
  def describe(*args)
  end
end

class PlainSpec
  include SpecHelpers

  describe User do
    let(:user) { User.new }

    it "does not enter rspec scope" do
      user
    end
  end
end
"#,
        )
        .await;

    let locations = editor.goto_def_at("plain_spec.rb", 16, 8).await;
    assert!(
        locations.is_empty(),
        "INVARIANT VIOLATED: RSpec extension treated non-RSpec describe as RSpec scope. \
         This is a bug because extension hooks must use resolved callees, not call names alone. \
         Fix: require an RSpec resolved callee before entering RSpec scope."
    );
}

#[tokio::test]
async fn rspec_extension_does_not_apply_include_outside_rspec_scope() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "inline_test.rb",
            r#"
module SpecHelpers
  def reset_db
  end
end

module PlainRuby
  include SpecHelpers

  def self.setup
    reset_db
  end
end
"#,
        )
        .await;

    let locations = editor.goto_def_at("inline_test.rb", 10, 8).await;
    assert!(
        locations.is_empty(),
        "INVARIANT VIOLATED: RSpec extension applied include outside confirmed RSpec scope. \
         This is a bug because extension hooks must not mutate singleton lookup for plain Ruby. \
         Fix: gate RSpec mixin patches on resolved RSpec enclosing calls."
    );
}

#[tokio::test]
async fn rspec_example_group_owns_direct_method_definitions() {
    check(
        r#"
module RSpec
end

module Lexical
  RSpec.describe Object do
    <def>def platform
    end</def>

    it "uses the group helper" do
      platf$0orm
    end
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn rspec_example_group_owns_define_method_declarations() {
    check(
        r#"
module RSpec
end

module Lexical
  RSpec.describe Object do
    define_method(:<def>platform</def>) do
      "group helper"
    end

    it "uses the group helper" do
      platf$0orm
    end
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn rspec_nested_group_inherits_outer_hidden_owner() {
    check(
        r#"
module RSpec
end

RSpec.describe Object do
  <def>def outer_helper
  end</def>

  context "nested" do
    it "inherits helpers" do
      outer_$0helper
    end
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn rspec_nested_group_owns_and_isolates_its_methods() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "nested_group_isolation_spec.rb",
            r#"module RSpec
end

RSpec.describe Object do
  context "first nested group" do
    def nested_helper
    end

    it "sees its helper" do
      nested_helper
    end
  end

  context "sibling nested group" do
    it "does not see the helper" do
      nested_helper
    end
  end

  nested_helper
end
"#,
        )
        .await;

    let nested = editor
        .goto_def_at("nested_group_isolation_spec.rb", 9, 10)
        .await;
    let sibling = editor
        .goto_def_at("nested_group_isolation_spec.rb", 15, 10)
        .await;
    let parent = editor
        .goto_def_at("nested_group_isolation_spec.rb", 19, 4)
        .await;

    assert_eq!(nested.len(), 1);
    assert_eq!(nested[0].range.start.line, 5);
    assert!(
        sibling.is_empty(),
        "a nested RSpec group method must not leak into a sibling group"
    );
    assert!(
        parent.is_empty(),
        "a nested RSpec group method must not leak back into its parent group"
    );
}

#[tokio::test]
async fn rspec_sibling_groups_do_not_share_direct_method_definitions() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "sibling_spec.rb",
            r#"module RSpec
end

module Lexical
  RSpec.describe String do
    def platform
      "first"
    end

    it "first" do
      platform
    end
  end

  RSpec.describe Integer do
    def platform
      2
    end

    it "second" do
      platform
    end
  end

  platform
end
"#,
        )
        .await;

    let first = editor.goto_def_at("sibling_spec.rb", 10, 10).await;
    let second = editor.goto_def_at("sibling_spec.rb", 20, 10).await;
    let lexical = editor.goto_def_at("sibling_spec.rb", 24, 4).await;
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].range.start.line, 5);
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].range.start.line, 15);
    assert!(
        lexical.is_empty(),
        "RSpec example-group methods must not leak onto the surrounding lexical module"
    );
}

#[tokio::test]
async fn rspec_sibling_groups_isolate_method_references() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "sibling_references_spec.rb",
            r#"module RSpec
end

RSpec.describe String do
  def platform
    "first"
  end

  it "first" do
    platform
  end
end

RSpec.describe Integer do
  def platform
    2
  end

  it "second" do
    platform
  end
end
"#,
        )
        .await;

    let first = editor
        .references_at("sibling_references_spec.rb", 9, 10)
        .await;
    let second = editor
        .references_at("sibling_references_spec.rb", 19, 10)
        .await;
    let first_lines: Vec<u32> = first
        .iter()
        .map(|location| location.range.start.line)
        .collect();
    let second_lines: Vec<u32> = second
        .iter()
        .map(|location| location.range.start.line)
        .collect();

    assert_eq!(
        first_lines,
        vec![9],
        "the first generated owner must not collect the sibling group's call"
    );
    assert_eq!(
        second_lines,
        vec![19],
        "the second generated owner must not collect the sibling group's call"
    );
}

#[tokio::test]
async fn rspec_execution_context_and_methods_are_replaced_after_edit() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "edit_context_spec.rb",
            r#"module RSpec
end

RSpec.describe Object do
  def old_helper
  end

  it "uses the helper" do
    old_helper
  end
end
"#,
        )
        .await;

    assert_eq!(
        editor.goto_def_at("edit_context_spec.rb", 8, 8).await.len(),
        1
    );

    editor
        .set(
            "edit_context_spec.rb",
            r#"module RSpec
end

RSpec.describe Object do
  def new_helper
  end

  it "no stale helper" do
    old_helper
  end
end
"#,
        )
        .await;

    assert!(
        editor
            .goto_def_at("edit_context_spec.rb", 8, 8)
            .await
            .is_empty(),
        "INVARIANT VIOLATED: an edited RSpec group retained its removed method. This is a bug because extension facts must use per-file replacement. Fix: remove stale generated-owner facts before resolving the replacement."
    );

    editor
        .set(
            "edit_context_spec.rb",
            r#"module RSpec
end

old_helper
"#,
        )
        .await;

    assert!(
        editor
            .goto_def_at("edit_context_spec.rb", 3, 4)
            .await
            .is_empty(),
        "INVARIANT VIOLATED: removing an RSpec group retained its execution context or generated method. This is a bug because contexts and facts must share the file replacement lifecycle. Fix: clear execution-context facts when replacing the file."
    );
}

#[tokio::test]
async fn rspec_before_hook_runtime_methods_flow_to_examples() {
    check(
        r#"
module RSpec
end

RSpec.describe Object do
  before do
    <def>def hook_helper
    end</def>
  end

  it "uses hook state" do
    hook_$0helper
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn rspec_cross_file_shared_context_helpers_flow_to_including_group() {
    let workspace = tempfile::TempDir::new().expect("RSpec shared-context workspace must exist");
    std::fs::write(
        workspace.path().join("Gemfile"),
        "source 'https://rubygems.org'\n",
    )
    .expect("RSpec shared-context Gemfile must be written");
    let root = workspace
        .path()
        .strip_prefix("/")
        .expect("temporary workspace path must be absolute")
        .to_string_lossy()
        .to_string();
    let rspec_file = format!("{root}/lib/rspec.rb");
    let support_file = format!("{root}/spec/support/auth_context.rb");
    let consumer_file = format!("{root}/spec/shared_context_consumer_spec.rb");
    let mut editor = FakeEditor::new().await;
    editor.add_workspace(&root);
    editor.open(&rspec_file, "module RSpec\nend\n").await;
    editor
        .open(
            &support_file,
            r#"RSpec.shared_context "authenticated" do
  def shared_helper
  end

  let(:shared_user) { Object.new }
end
"#,
        )
        .await;
    editor
        .open(
            &consumer_file,
            r#"RSpec.describe Object do
  include_context "authenticated"

  it "uses shared helpers" do
    shared_helper
    shared_user
  end
end
"#,
        )
        .await;

    let direct = editor.goto_def_at(&consumer_file, 4, 8).await;
    let generated = editor.goto_def_at(&consumer_file, 5, 8).await;

    assert_eq!(
        direct.len(),
        1,
        "shared-context direct helper must resolve across files: {direct:?}"
    );
    assert!(direct[0]
        .uri
        .path()
        .ends_with("/spec/support/auth_context.rb"));
    assert_eq!(direct[0].range.start.line, 1);
    assert_eq!(
        generated.len(),
        1,
        "shared-context let helper must resolve across files: {generated:?}"
    );
    assert!(generated[0]
        .uri
        .path()
        .ends_with("/spec/support/auth_context.rb"));
    assert_eq!(generated[0].range.start.line, 4);
}

#[tokio::test]
async fn rspec_shared_context_identity_is_isolated_between_projects() {
    let project_a = tempfile::TempDir::new().expect("RSpec project A must exist");
    let project_b = tempfile::TempDir::new().expect("RSpec project B must exist");
    for project in [&project_a, &project_b] {
        std::fs::write(
            project.path().join("Gemfile"),
            "source 'https://rubygems.org'\n",
        )
        .expect("RSpec isolation Gemfile must be written");
    }
    let root_a = project_a
        .path()
        .strip_prefix("/")
        .expect("project A path must be absolute")
        .to_string_lossy()
        .to_string();
    let root_b = project_b
        .path()
        .strip_prefix("/")
        .expect("project B path must be absolute")
        .to_string_lossy()
        .to_string();
    let mut editor = FakeEditor::new().await;
    editor.add_workspace(&root_a);
    editor.add_workspace(&root_b);
    editor
        .open(&format!("{root_a}/lib/rspec.rb"), "module RSpec\nend\n")
        .await;
    editor
        .open(&format!("{root_b}/lib/rspec.rb"), "module RSpec\nend\n")
        .await;
    editor
        .open(
            &format!("{root_a}/spec/support/shared.rb"),
            r#"RSpec.shared_context "same name" do
  let(:project_a_only) { Object.new }
end
"#,
        )
        .await;
    let project_b_consumer = format!("{root_b}/spec/consumer_spec.rb");
    editor
        .open(
            &project_b_consumer,
            r#"RSpec.describe Object do
  include_context "same name"

  it "cannot see project A" do
    project_a_only
  end
end
"#,
        )
        .await;

    assert!(
        editor
            .goto_def_at(&project_b_consumer, 4, 8)
            .await
            .is_empty(),
        "project B must not resolve a same-named shared context owned by project A"
    );
}

#[tokio::test]
async fn rspec_example_runtime_methods_do_not_leak_to_siblings() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "example_runtime_isolation_spec.rb",
            r#"module RSpec
end

RSpec.describe Object do
  it "defines a singleton helper" do
    def example_helper
    end
    example_helper
  end

  it "does not share the singleton helper" do
    example_helper
  end
end
"#,
        )
        .await;

    let first = editor
        .goto_def_at("example_runtime_isolation_spec.rb", 7, 8)
        .await;
    let second = editor
        .goto_def_at("example_runtime_isolation_spec.rb", 11, 8)
        .await;
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].range.start.line, 5);
    assert!(
        second.is_empty(),
        "an example-local runtime method must not leak to a sibling example instance"
    );
}

#[tokio::test]
async fn rspec_runtime_blocks_preserve_lexical_constant_scope() {
    check(
        r#"
VALUE = "top-level"

module RSpec
end

module LexicalSpec
  <def>VALUE = "lexical"</def>

  RSpec.describe Object do
    it "uses lexical constants" do
      VALUE$0
    end
  end
end
"#,
    )
    .await;
}
