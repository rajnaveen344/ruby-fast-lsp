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
async fn rspec_describe_has_generated_definition_location() {
    check(
        r#"
class User
end

module RSpec
end

RSpec.<def>desc$0ribe</def> User do
end
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
