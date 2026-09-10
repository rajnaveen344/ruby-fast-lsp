//! Tests for `nil-call` diagnostic.
//!
//! The ordinary collector retains a file-owned local-receiver candidate. The
//! engine checks the exact receiver proof after flow resolution and warns only
//! for definite `NilClass`; Unknown and nilable unions remain silent.
//! Diagnostic observations only read the output published by the handlers.

use crate::indexer::file_processor::FileProcessor;
use crate::test::harness::{check, FakeEditor};
use ruby_analysis::{DiagnosticFact, SourceKind, TextRange};
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Position, Range, Url};

fn expected_nil_call() -> Diagnostic {
    Diagnostic {
        range: Range::new(Position::new(1, 2), Position::new(1, 8)),
        severity: Some(DiagnosticSeverity::WARNING),
        code: Some(NumberOrString::String("nil-call".to_string())),
        source: Some("ruby-fast-lsp".to_string()),
        message: "Calling `upcase` on `x` which is `nil` here.".to_string(),
        ..Diagnostic::default()
    }
}

async fn nil_calls(editor: &FakeEditor, filename: &str) -> Vec<Diagnostic> {
    editor
        .diagnostics(filename)
        .await
        .into_iter()
        .filter(|diagnostic| {
            diagnostic.code == Some(NumberOrString::String("nil-call".to_string()))
        })
        .collect()
}

#[tokio::test]
async fn nil_call_publication_survives_save_edit_and_reopen() {
    let mut editor = FakeEditor::new().await;
    let source = "x = nil\nx.upcase\n";
    editor.open("nil_lifecycle.rb", source).await;
    assert_eq!(
        nil_calls(&editor, "nil_lifecycle.rb").await,
        vec![expected_nil_call()],
        "nil-call publication must match the complete expected warning"
    );
    editor.save("nil_lifecycle.rb").await;
    assert_eq!(
        nil_calls(&editor, "nil_lifecycle.rb").await,
        vec![expected_nil_call()]
    );
    editor
        .set("nil_lifecycle.rb", "x = \"ok\"\nx.upcase\n")
        .await;
    assert!(nil_calls(&editor, "nil_lifecycle.rb").await.is_empty());
    editor.set("nil_lifecycle.rb", source).await;
    assert_eq!(
        nil_calls(&editor, "nil_lifecycle.rb").await,
        vec![expected_nil_call()]
    );
    editor.close("nil_lifecycle.rb").await;
    editor.open("nil_lifecycle.rb", source).await;
    assert_eq!(
        nil_calls(&editor, "nil_lifecycle.rb").await,
        vec![expected_nil_call()]
    );
}

#[tokio::test]
async fn nilable_local_union_stays_silent() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "nil_union.rb",
            "x = nil\nx = \"ok\" if condition\nx.upcase\n",
        )
        .await;
    assert!(nil_calls(&editor, "nil_union.rb").await.is_empty());
}

#[tokio::test]
async fn unknown_local_reassignment_stays_silent() {
    let mut editor = FakeEditor::new().await;
    editor
        .open("nil_unknown.rb", "x = nil\nx = unknown_value\nx.upcase\n")
        .await;
    assert!(nil_calls(&editor, "nil_unknown.rb").await.is_empty());
}

#[tokio::test]
async fn cold_nil_call_facts_survive_byte_identical_open_and_save() {
    let directory = tempfile::tempdir().expect("create neutral nil-call project");
    let path = directory.path().join("nil_cold.rb");
    let source = "x = nil\nx.upcase\n";
    std::fs::write(&path, source).expect("write neutral nil-call source");
    let uri = Url::from_file_path(&path).expect("neutral source file URI");
    let filename = path
        .to_str()
        .expect("neutral source path is UTF-8")
        .trim_start_matches('/');
    let mut editor = FakeEditor::new().await;
    editor.add_workspace(
        directory
            .path()
            .to_str()
            .expect("neutral project path is UTF-8")
            .trim_start_matches('/'),
    );
    FileProcessor::new()
        .collect_file_facts_as_deferred_resolution(
            &uri,
            source,
            editor.server(),
            SourceKind::Project,
        )
        .expect("cold nil-call collection succeeds");
    let engine = editor.server().analysis_engine_for_uri(&uri);
    let file_id = {
        let mut engine = engine.write();
        engine.resolve();
        let file_id = engine.file_id(&path).expect("cold source registered");
        let diagnostics = engine
            .diagnostic_facts_in_file(file_id)
            .into_iter()
            .filter(|diagnostic| diagnostic.code == "nil-call")
            .collect::<Vec<_>>();
        assert_eq!(
            diagnostics,
            vec![DiagnosticFact::new(
                TextRange::new(file_id, 10, 16),
                ruby_analysis::DiagnosticSeverity::Warning,
                "nil-call",
                "Calling `upcase` on `x` which is `nil` here.",
            )]
        );
        file_id
    };
    editor.open(filename, source).await;
    assert_eq!(engine.read().file_id(&path), Some(file_id));
    assert_eq!(
        nil_calls(&editor, filename).await,
        vec![expected_nil_call()]
    );
    editor.save(filename).await;
    assert_eq!(
        nil_calls(&editor, filename).await,
        vec![expected_nil_call()]
    );
}

#[tokio::test]
async fn definite_nil_local_warns() {
    check(
        r#"
x = nil
x.<warn code="nil-call">upcase</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn non_nil_local_no_warn() {
    check(
        r#"
<warn none code="nil-call">
x = "hello"
x.upcase
</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn definite_nil_chained_warns_first_link_only() {
    // Once flagged on .upcase, downstream .reverse stays silent — same
    // rationale as expr-receiver chain noise suppression.
    check(
        r#"
x = nil
x.<warn code="nil-call">upcase</warn>.<warn none code="nil-call">reverse</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn nil_reassignment_clears_the_warn() {
    // After reassignment to non-nil, no warning at the later call.
    check(
        r#"
x = nil
x = "hello"
<warn none code="nil-call">x.upcase</warn>
"#,
    )
    .await;
}
