use ruby_fast_lsp_test_harness::FakeEditor;

fn rspec_package_dir() -> std::path::PathBuf {
    let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("test harness crate must live under crates/lsp-test-harness");
    workspace_root.join("extensions/rspec-ruby")
}

async fn rspec_editor() -> (tempfile::TempDir, FakeEditor) {
    let workspace = tempfile::TempDir::new().expect("RSpec workspace must exist");
    std::fs::write(
        workspace.path().join("Gemfile"),
        "source 'https://rubygems.org'\ngem 'rspec'\n",
    )
    .expect("RSpec Gemfile must be written");
    std::fs::write(
        workspace.path().join("Gemfile.lock"),
        "GEM\n  remote: https://rubygems.org/\n  specs:\n    rspec-core (3.13.5)\n",
    )
    .expect("RSpec lockfile must be written");
    let editor =
        FakeEditor::with_extension_package_and_workspace(rspec_package_dir(), workspace.path())
            .await;
    (workspace, editor)
}

fn workspace_file(workspace: &tempfile::TempDir, relative: &str) -> String {
    workspace
        .path()
        .join(relative)
        .to_string_lossy()
        .to_string()
}

#[tokio::test]
async fn rspec_extension_symbols_are_available_through_reusable_fake_editor() {
    let (workspace, mut editor) = rspec_editor().await;
    let filename = workspace_file(&workspace, "spec/user_spec.rb");
    let statuses = editor.extension_status().await;
    assert!(
        statuses
            .iter()
            .any(|status| status.id == "rspec-ruby" && status.status == "loaded"),
        "expected rspec-ruby extension loaded, got {statuses:?}"
    );
    editor
        .open(
            &filename,
            r#"
RSpec.describe User do
  context "active" do
    it "returns name" do
    end
  end
end
"#,
        )
        .await;

    let symbols = editor.document_symbols(&filename).await;
    let names: Vec<_> = symbols.iter().map(|symbol| symbol.name.as_str()).collect();

    assert!(names.contains(&"describe User"), "got symbols: {names:?}");
    assert!(names.contains(&"context active"), "got symbols: {names:?}");
    assert!(names.contains(&"it returns name"), "got symbols: {names:?}");
}

#[tokio::test]
async fn rspec_extension_lenses_are_available_through_reusable_fake_editor() {
    let (workspace, mut editor) = rspec_editor().await;
    let filename = workspace_file(&workspace, "spec/user_spec.rb");
    let statuses = editor.extension_status().await;
    assert!(
        statuses
            .iter()
            .any(|status| status.id == "rspec-ruby" && status.status == "loaded"),
        "expected rspec-ruby extension loaded, got {statuses:?}"
    );
    editor
        .open(
            &filename,
            r#"
RSpec.describe User do
  it "returns name" do
  end
end
"#,
        )
        .await;

    let lenses = editor.code_lens(&filename).await;
    let titles: Vec<_> = lenses
        .iter()
        .filter_map(|lens| lens.command.as_ref().map(|command| command.title.as_str()))
        .collect();

    assert!(titles.contains(&"Run RSpec"), "got lenses: {titles:?}");
    assert!(titles.contains(&"Debug RSpec"), "got lenses: {titles:?}");
}

#[tokio::test]
async fn rspec_root_describe_has_a_semantic_definition() {
    let (workspace, mut editor) = rspec_editor().await;
    let rspec_file = workspace_file(&workspace, "lib/rspec.rb");
    let spec_file = workspace_file(&workspace, "spec/user_spec.rb");
    editor.open(&rspec_file, "module RSpec\nend\n").await;
    editor
        .open(&spec_file, "RSpec.describe User do\nend\n")
        .await;

    let definitions = editor.goto_definition(&spec_file, 0, 8).await;

    assert_eq!(
        definitions.len(),
        1,
        "RSpec.describe must resolve only to its canonical semantic target, got {definitions:?}"
    );
    assert_eq!(
        definitions[0].uri.path(),
        "/__ruby_fast_lsp_extension__/semantic_targets.rb"
    );
}

#[tokio::test]
async fn packaged_rspec_wasm_applies_generated_owner_contexts() {
    let (workspace, mut editor) = rspec_editor().await;
    let rspec_file = workspace_file(&workspace, "lib/rspec.rb");
    let spec_file = workspace_file(&workspace, "spec/runtime_spec.rb");
    editor.open(&rspec_file, "module RSpec\nend\n").await;
    editor
        .open(
            &spec_file,
            r#"RSpec.describe Object do
  def group_helper
  end

  before do
    def hook_helper
    end
  end

  it "uses generated owners" do
    group_helper
    hook_helper
  end
end
"#,
        )
        .await;

    let group = editor.goto_definition(&spec_file, 10, 8).await;
    let hook = editor.goto_definition(&spec_file, 11, 8).await;

    assert_eq!(group.len(), 1, "group helper did not resolve: {group:?}");
    assert_eq!(group[0].range.start.line, 1);
    assert_eq!(hook.len(), 1, "hook helper did not resolve: {hook:?}");
    assert_eq!(hook[0].range.start.line, 5);
}

#[tokio::test]
async fn packaged_rspec_wasm_infers_let_and_subject_block_returns() {
    let (workspace, mut editor) = rspec_editor().await;
    let rspec_file = workspace_file(&workspace, "lib/rspec.rb");
    let spec_file = workspace_file(&workspace, "spec/inferred_helpers_spec.rb");
    editor.open(&rspec_file, "module RSpec\nend\n").await;
    let source = r#"class User
  def label
  end
end

class Admin
  def label
  end

  def audit
  end
end

RSpec.describe Object do
  let(:actor) { User.new }
  subject(:record) { Admin.new }

  it "uses inferred helper returns" do
    actor.label
    record.audit
  end
end
"#;
    editor.open(&spec_file, source).await;

    let let_helper = editor.goto_definition(&spec_file, 18, 6).await;
    assert_eq!(
        let_helper.len(),
        1,
        "the generated let helper itself must resolve before its return is used: {let_helper:?}"
    );
    let let_method = editor.goto_definition(&spec_file, 18, 11).await;
    let subject_method = editor.goto_definition(&spec_file, 19, 12).await;
    assert_eq!(
        let_method.len(),
        1,
        "a let helper's block-derived return must drive receiver lookup: {let_method:?}"
    );
    assert_eq!(let_method[0].range.start.line, 1);
    assert_eq!(
        subject_method.len(),
        1,
        "a named subject's block-derived return must drive receiver lookup: {subject_method:?}"
    );
    assert_eq!(subject_method[0].range.start.line, 9);

    editor
        .set(
            &spec_file,
            &source.replace("let(:actor) { User.new }", "let(:actor) { Admin.new }"),
        )
        .await;
    let updated = editor.goto_definition(&spec_file, 18, 11).await;
    assert_eq!(
        updated.len(),
        1,
        "edited let inference must remain resolvable: {updated:?}"
    );
    assert_eq!(
        updated[0].range.start.line, 6,
        "editing the block return must replace the stale inferred receiver type"
    );

    let actor_completion_source = source.replace("actor.label", "actor.");
    editor.set(&spec_file, &actor_completion_source).await;
    let actor_labels = editor
        .completion_after_dot(&spec_file, 18, 10)
        .await
        .into_iter()
        .map(|item| item.label)
        .collect::<Vec<_>>();
    assert!(
        actor_labels.iter().any(|label| label == "label"),
        "let return inference must provide receiver completion: {actor_labels:?}"
    );

    let subject_completion_source = source.replace("record.audit", "record.");
    editor.set(&spec_file, &subject_completion_source).await;
    let subject_labels = editor
        .completion_after_dot(&spec_file, 19, 11)
        .await
        .into_iter()
        .map(|item| item.label)
        .collect::<Vec<_>>();
    assert!(
        subject_labels.iter().any(|label| label == "audit"),
        "subject return inference must provide receiver completion: {subject_labels:?}"
    );
}

#[tokio::test]
async fn packaged_rspec_wasm_connects_cross_file_shared_context_helpers() {
    let (workspace, mut editor) = rspec_editor().await;
    let rspec_file = workspace_file(&workspace, "lib/rspec.rb");
    let support_file = workspace_file(&workspace, "spec/support/auth_context.rb");
    let consumer_file = workspace_file(&workspace, "spec/shared_context_consumer_spec.rb");
    editor.open(&rspec_file, "module RSpec\nend\n").await;
    editor
        .open(
            &support_file,
            r#"RSpec.shared_context "authenticated" do
  def shared_helper
  end

  let(:shared_user) { Object.new }

  before do
    def shared_hook_helper
    end
  end
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
    shared_hook_helper
  end
end

RSpec.describe String do
  it "does not receive un-included helpers" do
    shared_user
  end
end
"#,
        )
        .await;

    let direct = editor.goto_definition(&consumer_file, 4, 8).await;
    let generated = editor.goto_definition(&consumer_file, 5, 8).await;
    let hook = editor.goto_definition(&consumer_file, 6, 8).await;

    assert_eq!(
        direct.len(),
        1,
        "a shared-context direct method must resolve across files: {direct:?}"
    );
    assert!(direct[0]
        .uri
        .path()
        .ends_with("/spec/support/auth_context.rb"));
    assert_eq!(direct[0].range.start.line, 1);
    assert_eq!(
        generated.len(),
        1,
        "a shared-context let helper must resolve across files: {generated:?}"
    );
    assert!(generated[0]
        .uri
        .path()
        .ends_with("/spec/support/auth_context.rb"));
    assert_eq!(generated[0].range.start.line, 4);
    assert_eq!(
        hook.len(),
        1,
        "a method defined by a shared-context hook must flow to consuming examples: {hook:?}"
    );
    assert!(hook[0]
        .uri
        .path()
        .ends_with("/spec/support/auth_context.rb"));
    assert_eq!(hook[0].range.start.line, 7);
    assert!(
        editor
            .goto_definition(&consumer_file, 12, 8)
            .await
            .is_empty(),
        "a sibling group without include_context must not see the project-scoped shared owner"
    );
    let references = editor.references(&consumer_file, 5, 8).await;
    let consumer_reference_lines = references
        .iter()
        .filter(|location| location.uri.path() == consumer_file)
        .map(|location| location.range.start.line)
        .collect::<Vec<_>>();
    assert_eq!(
        consumer_reference_lines,
        vec![5],
        "references must include only the group that applied the shared context: {references:?}"
    );

    editor
        .set(
            &support_file,
            r#"RSpec.shared_context "authenticated" do
end
"#,
        )
        .await;
    assert!(
        editor
            .goto_definition(&consumer_file, 4, 8)
            .await
            .is_empty(),
        "removing a direct shared-context method must remove its cross-file fact"
    );
    assert!(
        editor
            .goto_definition(&consumer_file, 5, 8)
            .await
            .is_empty(),
        "removing a shared-context let must remove its cross-file fact"
    );
    assert!(
        editor
            .goto_definition(&consumer_file, 6, 8)
            .await
            .is_empty(),
        "removing a shared-context hook must remove its generated method fact"
    );

    editor
        .set(
            &support_file,
            r#"RSpec.shared_context "authenticated" do
  def shared_helper
  end

  let(:shared_user) { Object.new }

  before do
    def shared_hook_helper
    end
  end
end
"#,
        )
        .await;
    assert_eq!(
        editor.goto_definition(&consumer_file, 5, 8).await.len(),
        1,
        "restoring a shared-context helper must recreate its project-scoped fact"
    );

    editor
        .set(
            &consumer_file,
            r#"RSpec.describe Object do
  it "does not include shared helpers" do
    shared_helper
    shared_user
    shared_hook_helper
  end
end
"#,
        )
        .await;
    assert!(
        editor
            .goto_definition(&consumer_file, 2, 8)
            .await
            .is_empty(),
        "removing include_context must remove the generated mixin relationship"
    );
    assert!(
        editor
            .goto_definition(&consumer_file, 3, 8)
            .await
            .is_empty(),
        "removed include_context must not retain generated helper visibility"
    );
    assert!(
        editor
            .goto_definition(&consumer_file, 4, 8)
            .await
            .is_empty(),
        "removed include_context must not retain shared hook method visibility"
    );
}

#[tokio::test]
async fn packaged_rspec_wasm_instantiates_cross_file_shared_examples() {
    let (workspace, mut editor) = rspec_editor().await;
    let rspec_file = workspace_file(&workspace, "lib/rspec.rb");
    let support_file = workspace_file(&workspace, "spec/support/auditable_examples.rb");
    let consumer_file = workspace_file(&workspace, "spec/auditable_spec.rb");
    editor.open(&rspec_file, "module RSpec\nend\n").await;
    editor
        .open(
            &support_file,
            r#"RSpec.shared_examples "auditable" do
  def shared_example_helper
  end

  let(:shared_record) { Object.new }

  it "uses definition and application context" do
    shared_example_helper
    shared_record
    consumer_helper
  end
end
"#,
        )
        .await;
    editor
        .open(
            &consumer_file,
            r#"RSpec.describe Object do
  def consumer_helper
  end

  it_behaves_like "auditable"

  it "can use shared helpers" do
    shared_example_helper
    shared_record
  end
end

RSpec.describe String do
  it "does not receive unapplied shared examples" do
    shared_example_helper
  end
end
"#,
        )
        .await;

    let support_direct = editor.goto_definition(&support_file, 7, 8).await;
    let support_generated = editor.goto_definition(&support_file, 8, 8).await;
    let application_method = editor.goto_definition(&support_file, 9, 8).await;
    assert_eq!(
        support_direct.len(),
        1,
        "a shared example body must resolve its directly declared helper: {support_direct:?}"
    );
    assert_eq!(support_direct[0].range.start.line, 1);
    assert_eq!(
        support_generated.len(),
        1,
        "a shared example body must resolve its generated let helper: {support_generated:?}"
    );
    assert_eq!(support_generated[0].range.start.line, 4);
    assert_eq!(
        application_method.len(),
        1,
        "a shared example body with one application must resolve the consuming group's helper: {application_method:?}"
    );
    assert!(application_method[0]
        .uri
        .path()
        .ends_with("/spec/auditable_spec.rb"));
    assert_eq!(application_method[0].range.start.line, 1);

    let consumer_direct = editor.goto_definition(&consumer_file, 7, 8).await;
    let consumer_generated = editor.goto_definition(&consumer_file, 8, 8).await;
    assert_eq!(
        consumer_direct.len(),
        1,
        "an applied shared example helper must resolve in the consuming group: {consumer_direct:?}"
    );
    assert!(consumer_direct[0]
        .uri
        .path()
        .ends_with("/spec/support/auditable_examples.rb"));
    assert_eq!(consumer_direct[0].range.start.line, 1);
    assert_eq!(
        consumer_generated.len(),
        1,
        "an applied shared example let must resolve in the consuming group: {consumer_generated:?}"
    );
    assert_eq!(consumer_generated[0].range.start.line, 4);
    let shared_references = editor.references(&support_file, 7, 8).await;
    let reference_sites = shared_references
        .iter()
        .map(|location| {
            (
                location
                    .uri
                    .path()
                    .rsplit('/')
                    .next()
                    .expect("reference URI must have a filename")
                    .to_string(),
                location.range.start.line,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        reference_sites,
        vec![
            ("auditable_examples.rb".to_string(), 7),
            ("auditable_spec.rb".to_string(), 7),
        ],
        "shared-example references must include the template body and applied group only: {shared_references:?}"
    );
    assert!(
        editor
            .goto_definition(&consumer_file, 14, 8)
            .await
            .is_empty(),
        "a sibling group without shared-example application must not see its helpers"
    );

    editor
        .set(
            &consumer_file,
            r#"RSpec.describe Object do
  def consumer_helper
  end
  it_behaves_like "auditable"
end

RSpec.describe String do
  def consumer_helper
  end
  include_examples "auditable"
end
"#,
        )
        .await;
    let application_definitions = editor.goto_definition(&support_file, 9, 8).await;
    let application_lines = application_definitions
        .iter()
        .map(|location| location.range.start.line)
        .collect::<Vec<_>>();
    assert_eq!(
        application_lines,
        vec![1, 7],
        "a shared example applied by multiple groups must expose every defensible application helper without choosing by indexing order: {application_definitions:?}"
    );

    editor
        .set(
            &consumer_file,
            r#"RSpec.describe Object do
  def consumer_helper
  end

  it "does not apply shared examples" do
    shared_example_helper
    shared_record
  end
end
"#,
        )
        .await;
    assert!(
        editor
            .goto_definition(&consumer_file, 5, 8)
            .await
            .is_empty(),
        "removing it_behaves_like must remove shared helper visibility"
    );
    assert!(
        editor
            .goto_definition(&consumer_file, 6, 8)
            .await
            .is_empty(),
        "removing shared-example application must remove generated let visibility"
    );
    assert!(
        editor.goto_definition(&support_file, 9, 8).await.is_empty(),
        "removing the only application must remove the shared body-to-consumer relationship"
    );
    assert_eq!(
        editor.goto_definition(&support_file, 7, 8).await.len(),
        1,
        "removing an application must retain definition-local shared helper resolution"
    );
}

#[tokio::test]
async fn packaged_rspec_wasm_fails_closed_for_unsupported_locked_version() {
    let workspace = tempfile::TempDir::new().expect("unsupported RSpec workspace must exist");
    std::fs::write(
        workspace.path().join("Gemfile"),
        "source 'https://rubygems.org'\ngem 'rspec'\n",
    )
    .expect("unsupported RSpec Gemfile must be written");
    std::fs::write(
        workspace.path().join("Gemfile.lock"),
        "GEM\n  remote: https://rubygems.org/\n  specs:\n    rspec-core (4.0.0)\n",
    )
    .expect("unsupported RSpec lockfile must be written");
    let mut editor =
        FakeEditor::with_extension_package_and_workspace(rspec_package_dir(), workspace.path())
            .await;
    let rspec_file = workspace_file(&workspace, "lib/rspec.rb");
    let spec_file = workspace_file(&workspace, "spec/unsupported_spec.rb");
    editor.open(&rspec_file, "module RSpec\nend\n").await;
    editor
        .open(&spec_file, "RSpec.describe Object do\nend\n")
        .await;

    assert!(
        editor.goto_definition(&spec_file, 0, 8).await.is_empty(),
        "unsupported RSpec version must not receive the manifest semantic target"
    );
    assert!(
        editor
            .document_symbols(&spec_file)
            .await
            .iter()
            .all(|symbol| symbol.name != "describe Object"),
        "unsupported RSpec version must not receive response patches"
    );
    let statuses = editor.extension_status().await;
    assert!(
        statuses
            .iter()
            .any(|status| status.id == "rspec-ruby" && status.status == "loaded"),
        "inapplicability must skip RSpec without disabling its package: {statuses:?}"
    );
}
