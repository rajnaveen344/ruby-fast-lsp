//! Execution contexts across edits, hooks, examples, shared contexts, and
//! projects.

use super::harness::{Rspec, RspecEditor};

pub(super) async fn execution_context_and_methods_are_replaced_after_edit(
    rspec: Rspec,
) -> Vec<String> {
    let mut editor = RspecEditor::new(rspec).await;
    let filename = editor.path("spec/edit_context_spec.rb");
    editor
        .open(
            &filename,
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

    assert_eq!(editor.goto_def_at(&filename, 8, 8).await.len(), 1);

    editor
        .set(
            &filename,
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

    invariant!(
        editor.goto_def_at(&filename, 8, 8).await.is_empty(),
        what = "an edited RSpec group retained its removed method",
        why = "extension facts must use per-file replacement",
        fix = "remove stale generated-owner facts before resolving the replacement",
    );

    editor
        .set(
            &filename,
            r#"module RSpec
end

old_helper
"#,
        )
        .await;

    invariant!(
        editor.goto_def_at(&filename, 3, 4).await.is_empty(),
        what = "removing an RSpec group retained its execution context or generated method",
        why = "contexts and facts must share the file replacement lifecycle",
        fix = "clear execution-context facts when replacing the file",
    );
    editor.into_transcript()
}

pub(super) async fn before_hook_runtime_methods_flow_to_examples(rspec: Rspec) -> Vec<String> {
    let mut editor = RspecEditor::new(rspec).await;
    editor
        .check(
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
    editor.into_transcript()
}

pub(super) async fn cross_file_shared_context_helpers_flow_to_including_group(
    rspec: Rspec,
) -> Vec<String> {
    let mut editor = RspecEditor::new(rspec).await;
    let rspec_file = editor.path("lib/rspec.rb");
    let support_file = editor.path("spec/support/auth_context.rb");
    let consumer_file = editor.path("spec/shared_context_consumer_spec.rb");
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
    editor.into_transcript()
}

pub(super) async fn shared_context_identity_is_isolated_between_projects(
    rspec: Rspec,
) -> Vec<String> {
    let mut editor = RspecEditor::with_projects(rspec, &[true, true]).await;
    let rspec_a = editor.path_in(0, "lib/rspec.rb");
    let rspec_b = editor.path_in(1, "lib/rspec.rb");
    let shared_a = editor.path_in(0, "spec/support/shared.rb");
    let project_b_consumer = editor.path_in(1, "spec/consumer_spec.rb");
    editor.open(&rspec_a, "module RSpec\nend\n").await;
    editor.open(&rspec_b, "module RSpec\nend\n").await;
    editor
        .open(
            &shared_a,
            r#"RSpec.shared_context "same name" do
  let(:project_a_only) { Object.new }
end
"#,
        )
        .await;
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
    editor.into_transcript()
}

pub(super) async fn example_runtime_methods_do_not_leak_to_siblings(rspec: Rspec) -> Vec<String> {
    let mut editor = RspecEditor::new(rspec).await;
    let filename = editor.path("spec/example_runtime_isolation_spec.rb");
    editor
        .open(
            &filename,
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

    let first = editor.goto_def_at(&filename, 7, 8).await;
    let second = editor.goto_def_at(&filename, 11, 8).await;
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].range.start.line, 5);
    assert!(
        second.is_empty(),
        "an example-local runtime method must not leak to a sibling example instance"
    );
    editor.into_transcript()
}

pub(super) async fn runtime_blocks_preserve_lexical_constant_scope(rspec: Rspec) -> Vec<String> {
    let mut editor = RspecEditor::new(rspec).await;
    editor
        .check(
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
    editor.into_transcript()
}
