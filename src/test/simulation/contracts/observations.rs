//! Observation controls: a test must observe the product's existing state,
//! including faulty state, without silently rebuilding or repairing it.

use crate::test::harness::FakeEditor;
use ruby_analysis::engine::{FileFacts, ResolveMode};
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};

#[tokio::test]
async fn diagnostic_publication_distinguishes_absence_from_an_empty_clear() {
    let editor = FakeEditor::new().await;
    let uri = crate::test::harness::fixture_uri("/not-open.rb");
    assert_eq!(editor.server().last_diagnostic_publication(&uri), None);
    editor
        .server()
        .publish_diagnostics(uri.clone(), Vec::new())
        .await;
    assert_eq!(
        editor.server().last_diagnostic_publication(&uri),
        Some(Vec::new())
    );
}

#[tokio::test]
async fn diagnostic_observation_retains_published_output_and_empty_clears() {
    let mut editor = FakeEditor::new().await;
    editor.open("observation.rb", "value = 1\n").await;
    let uri = crate::test::harness::fixture_uri("/observation.rb");
    let published = vec![Diagnostic {
        range: Range::new(Position::new(0, 0), Position::new(0, 5)),
        severity: Some(DiagnosticSeverity::WARNING),
        source: Some("simulation-observation-control".into()),
        message: "Deliberately retained publication; observing must not replace it".into(),
        ..Diagnostic::default()
    }];
    // Inject at the real publication boundary. Recomputing from the valid Ruby
    // would erase this output and conceal precisely the fault being tested.
    editor
        .server()
        .publish_diagnostics(uri.clone(), published.clone())
        .await;
    assert_eq!(
        editor.diagnostics("observation.rb").await,
        published,
        "diagnostic observation must return the published output without recomputation"
    );
    editor.server().publish_diagnostics(uri, Vec::new()).await;
    assert!(
        editor.diagnostics("observation.rb").await.is_empty(),
        "an explicitly published empty clear must replace the previous diagnostics"
    );
}

#[tokio::test]
async fn diagnostic_observation_cannot_repair_missing_semantic_facts() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "observation.rb",
            "class Service\n  def call; missing; end\nend\n",
        )
        .await;
    let uri = crate::test::harness::fixture_uri("/observation.rb");
    let engine = editor.server().analysis_engine_for_uri(&uri);
    let document = editor
        .server()
        .documents
        .read()
        .get(&uri)
        .unwrap()
        .read()
        .clone();
    engine.write().replace_facts(
        document.analysis_file_id(),
        FileFacts::default(),
        ResolveMode::Immediate,
    );
    let before = engine.read().semantic_result_fingerprint();
    let _observed = editor.diagnostics("observation.rb").await;
    assert_eq!(
        engine.read().semantic_result_fingerprint(),
        before,
        "diagnostic observation must not repair deliberately missing semantic facts"
    );
}

#[tokio::test]
async fn tagged_diagnostic_observation_retains_published_output() {
    let mut editor = FakeEditor::new().await;
    editor.open("observation.rb", "value = 1\n").await;
    let uri = crate::test::harness::fixture_uri("/observation.rb");
    editor
        .server()
        .publish_diagnostics(
            uri,
            vec![Diagnostic {
                range: Range::new(Position::new(0, 0), Position::new(0, 5)),
                severity: Some(DiagnosticSeverity::WARNING),
                message: "published observation control".into(),
                ..Diagnostic::default()
            }],
        )
        .await;
    editor
        .check(
            "observation.rb",
            "<warn message=\"published observation control\">value</warn> = 1\n",
        )
        .await;
}

#[tokio::test]
async fn tagged_diagnostic_observation_cannot_repair_missing_semantic_facts() {
    let mut editor = FakeEditor::new().await;
    let source = "class Service\n  def call; missing; end\nend\n";
    editor.open("observation.rb", source).await;
    let uri = crate::test::harness::fixture_uri("/observation.rb");
    let engine = editor.server().analysis_engine_for_uri(&uri);
    let document = editor
        .server()
        .documents
        .read()
        .get(&uri)
        .unwrap()
        .read()
        .clone();
    engine.write().replace_facts(
        document.analysis_file_id(),
        FileFacts::default(),
        ResolveMode::Immediate,
    );
    let before = engine.read().semantic_result_fingerprint();
    editor
        .check("observation.rb", &format!("<err none>{source}</err>"))
        .await;
    assert_eq!(
        engine.read().semantic_result_fingerprint(),
        before,
        "tagged diagnostic observation must not repair deliberately missing semantic facts"
    );
}

#[tokio::test]
async fn late_definition_open_clears_published_unresolved_constant() {
    for workspace in [None, Some("project")] {
        let mut editor = FakeEditor::new().await;
        if let Some(root) = workspace {
            editor.add_workspace(root);
        }
        let caller = if workspace.is_some() {
            "project/caller.rb"
        } else {
            "caller.rb"
        };
        let definition = if workspace.is_some() {
            "project/definition.rb"
        } else {
            "definition.rb"
        };
        editor.open(caller, "value = VALUE\n").await;
        assert!(
            editor
                .published_diagnostics(caller)
                .iter()
                .any(|diagnostic| diagnostic.code
                    == Some(tower_lsp::lsp_types::NumberOrString::String(
                        "unresolved-constant".into()
                    ))),
            "the initially absent constant must be reported before opening its definition"
        );
        editor.open(definition, "VALUE = 1\n").await;
        let actual = editor.published_diagnostics(caller);
        assert!(
            !actual.iter().any(|diagnostic| diagnostic.code
                == Some(tower_lsp::lsp_types::NumberOrString::String(
                    "unresolved-constant".into()
                ))),
            "opening a late definition must publish the resolved consumer diagnostics: {actual:?}"
        );
        assert!(actual.iter().any(|diagnostic| diagnostic.message.contains("assigned but unused variable")),
            "refreshing semantic diagnostics must retain the current parser diagnostics: {actual:?}");
    }
}

#[tokio::test]
async fn opening_another_project_cannot_rebuild_or_publish_this_projects_state() {
    let mut editor = FakeEditor::new().await;
    editor.add_workspace("alpha");
    editor.add_workspace("beta");
    editor.open("alpha/caller.rb", "value = VALUE\n").await;
    let uri = crate::test::harness::fixture_uri("/alpha/caller.rb");
    let engine = editor.server().analysis_engine_for_uri(&uri);
    let document = editor
        .server()
        .documents
        .read()
        .get(&uri)
        .unwrap()
        .read()
        .clone();
    engine.write().replace_facts(
        document.analysis_file_id(),
        FileFacts::default(),
        ResolveMode::Immediate,
    );
    let facts = engine.read().semantic_result_fingerprint();
    let diagnostics = editor.published_diagnostics("alpha/caller.rb");
    editor.open("beta/definition.rb", "VALUE = 1\n").await;
    assert_eq!(
        engine.read().semantic_result_fingerprint(),
        facts,
        "opening a different project must not rebuild this project's deliberately incomplete facts"
    );
    assert_eq!(
        editor.published_diagnostics("alpha/caller.rb"),
        diagnostics,
        "opening a different project must not replace this project's diagnostic publication"
    );
}

#[tokio::test]
async fn editing_a_definition_cannot_refresh_another_projects_consumer() {
    let mut editor = FakeEditor::new().await;
    editor.add_workspace("alpha");
    editor.add_workspace("beta");
    editor.open("alpha/caller.rb", "_value = VALUE\n").await;
    editor.open("beta/definition.rb", "VALUE = 1\n").await;
    let alpha_uri = crate::test::harness::fixture_uri("/alpha/caller.rb");
    let beta_uri = crate::test::harness::fixture_uri("/beta/definition.rb");
    let alpha = editor.server().analysis_engine_for_uri(&alpha_uri);
    let beta = editor.server().analysis_engine_for_uri(&beta_uri);
    let alpha_facts = alpha.read().semantic_result_fingerprint();
    let alpha_diagnostics = editor.diagnostics("alpha/caller.rb").await;
    assert_eq!(
        alpha_diagnostics.len(),
        1,
        "the unresolved alpha constant must be diagnosed"
    );

    // Changing an exported constant's type requires consumer refresh in beta.
    // Alpha's identically named unresolved constant belongs to another engine.
    editor.set("beta/definition.rb", "VALUE = 'beta'\n").await;
    assert_eq!(
        editor.diagnostics("alpha/caller.rb").await,
        alpha_diagnostics,
        "editing beta must preserve alpha's complete diagnostic publication"
    );
    assert_eq!(
        alpha.read().semantic_result_fingerprint(),
        alpha_facts,
        "editing beta must leave alpha's semantic facts unchanged"
    );
    assert!(
        beta.read()
            .file_id(alpha_uri.to_file_path().unwrap())
            .is_none(),
        "editing beta must not import an open consumer from another project"
    );
    assert!(
        editor
            .references_with_declaration_at("beta/definition.rb", 0, 1, false)
            .await
            .is_empty(),
        "beta's constant must not acquire alpha's references"
    );
}
