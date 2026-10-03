//! RSpec extension facts: generated helpers, example-group scope, and
//! execution contexts.
//!
//! Each case loads the `extensions/rspec-ruby` package and runs in a project
//! that locks `rspec-core` 3.x.

mod harness;
mod helpers;
mod lifecycle;
mod scopes;

use harness::RspecEditor;

/// The package applies only to projects that lock `rspec-core` 3.x.
#[tokio::test]
async fn package_requires_locked_rspec_core() {
    let source = r#"module RSpec
end

RSpec.describe Object do
  let(:actor) { Object.new }

  it "uses the helper" do
    actor
  end
end
"#;
    let mut editor = RspecEditor::with_projects(&[false]).await;
    let filename = editor.path("spec/unlocked_spec.rb");
    editor.open(&filename, source).await;
    assert!(
        editor.goto_def_at(&filename, 7, 6).await.is_empty(),
        "a project without a locked rspec-core must not get RSpec helpers"
    );
}
