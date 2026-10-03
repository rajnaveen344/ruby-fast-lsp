//! Example-group scope: which calls enter RSpec scope, mixins, and the
//! hidden owner of each group.

use super::harness::RspecEditor;

#[tokio::test]
async fn include_makes_helper_methods_visible() {
    let mut editor = RspecEditor::new().await;
    editor
        .check(
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
async fn extend_makes_helper_methods_visible_on_singleton_scope() {
    let mut editor = RspecEditor::new().await;
    editor
        .check(
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
async fn extension_does_not_treat_other_describe_as_rspec_scope() {
    let mut editor = RspecEditor::new().await;
    let filename = editor.path("spec/plain_spec.rb");
    editor
        .open(
            &filename,
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

    let locations = editor.goto_def_at(&filename, 16, 8).await;
    invariant!(
        locations.is_empty(),
        what = "RSpec extension treated non-RSpec describe as RSpec scope",
        why = "extension hooks must use resolved callees, not call names alone",
        fix = "require an RSpec resolved callee before entering RSpec scope",
    );
}

#[tokio::test]
async fn extension_does_not_apply_include_outside_rspec_scope() {
    let mut editor = RspecEditor::new().await;
    let filename = editor.path("spec/inline_test.rb");
    editor
        .open(
            &filename,
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

    let locations = editor.goto_def_at(&filename, 10, 8).await;
    invariant!(
        locations.is_empty(),
        what = "RSpec extension applied include outside confirmed RSpec scope",
        why = "extension hooks must not mutate singleton lookup for plain Ruby",
        fix = "gate RSpec mixin patches on resolved RSpec enclosing calls",
    );
}

#[tokio::test]
async fn example_group_owns_direct_method_definitions() {
    let mut editor = RspecEditor::new().await;
    editor
        .check(
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
async fn example_group_owns_define_method_declarations() {
    let mut editor = RspecEditor::new().await;
    editor
        .check(
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
async fn nested_group_inherits_outer_hidden_owner() {
    let mut editor = RspecEditor::new().await;
    editor
        .check(
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
async fn nested_group_owns_and_isolates_its_methods() {
    let mut editor = RspecEditor::new().await;
    let filename = editor.path("spec/nested_group_isolation_spec.rb");
    editor
        .open(
            &filename,
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

    let nested = editor.goto_def_at(&filename, 9, 10).await;
    let sibling = editor.goto_def_at(&filename, 15, 10).await;
    let parent = editor.goto_def_at(&filename, 19, 4).await;

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
async fn sibling_groups_do_not_share_direct_method_definitions() {
    let mut editor = RspecEditor::new().await;
    let filename = editor.path("spec/sibling_spec.rb");
    editor
        .open(
            &filename,
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

    let first = editor.goto_def_at(&filename, 10, 10).await;
    let second = editor.goto_def_at(&filename, 20, 10).await;
    let lexical = editor.goto_def_at(&filename, 24, 4).await;
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
async fn sibling_groups_isolate_method_references() {
    let mut editor = RspecEditor::new().await;
    let filename = editor.path("spec/sibling_references_spec.rb");
    editor
        .open(
            &filename,
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

    let first = editor.references_at(&filename, 9, 10).await;
    let second = editor.references_at(&filename, 19, 10).await;
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
