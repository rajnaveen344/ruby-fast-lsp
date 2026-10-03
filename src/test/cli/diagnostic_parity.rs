//! Diagnostic parity between `check` output and LSP published diagnostics.

use super::support::{assert_diagnostic_parity, find_cli_diagnostic, find_lsp_diagnostic};
use crate::loader::file_processor::FileProcessor;
use crate::lsp::check::CheckSession;
use crate::test::harness::FakeEditor;
use tower_lsp::lsp_types::NumberOrString;

#[tokio::test]
async fn local_semantic_diagnostic_matches_lsp_projection() {
    let source = "def greet(name)\n  name\nend\n\ngreet\n";
    let project = tempfile::tempdir().expect("temporary parity project must be created");
    std::fs::write(project.path().join("main.rb"), source).expect("parity fixture must be written");
    let check_report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must analyze the parity fixture");
    let check_diagnostic = check_report
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code.as_deref() == Some("wrong-arity"))
        .expect("check must return the proven wrong-arity diagnostic");

    let mut editor = FakeEditor::new().await;
    editor.open("main.rb", source).await;
    let lsp_diagnostic = editor
        .diagnostics("main.rb")
        .await
        .into_iter()
        .find(|diagnostic| {
            diagnostic.code.as_ref().is_some_and(
                |code| matches!(code, NumberOrString::String(code) if code == "wrong-arity"),
            )
        })
        .expect("LSP must return the proven wrong-arity diagnostic");

    assert_eq!(check_diagnostic.message, lsp_diagnostic.message);
    invariant_eq!(
        (
            check_diagnostic.range.start.line,
            check_diagnostic.range.start.column,
            check_diagnostic.range.end.line,
            check_diagnostic.range.end.column,
        ),
        (
            lsp_diagnostic.range.start.line + 1,
            lsp_diagnostic.range.start.character + 1,
            lsp_diagnostic.range.end.line + 1,
            lsp_diagnostic.range.end.character + 1,
        ),
        what = "CLI and LSP projected different ranges for one engine-owned diagnostic",
        why = "adapters may change indexing conventions but not semantic locations",
        fix = "keep check range conversion aligned with LSP UTF-16 positions",
    );
}

#[tokio::test]
async fn structural_return_diagnostics_match_cli_and_lsp_and_fail_closed() {
    let signature = "class PayloadFactory\n  def build: () -> { id: Integer }\nend\n";
    let mismatched = "class PayloadFactory\n  def build\n    { id: \"wrong\" }\n  end\nend\n";
    let incomplete = "class PayloadFactory\n  def build\n    { id: dynamic_value }\n  end\nend\n";
    let project = tempfile::tempdir().expect("temporary structural-parity project must be created");
    let sig_dir = project.path().join("sig");
    std::fs::create_dir(&sig_dir).expect("the synthetic sig directory must be created");
    std::fs::write(sig_dir.join("payload_factory.rbs"), signature)
        .expect("the synthetic RBS contract must be written");
    let implementation = project.path().join("payload_factory.rb");
    std::fs::write(&implementation, mismatched)
        .expect("the mismatched Ruby implementation must be written");

    let check_report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must evaluate the structural return contract");
    let mut editor = FakeEditor::new().await;
    let signature_uri = crate::test::harness::fixture_uri("/sig/payload_factory.rbs");
    FileProcessor::default()
        .collect_rbs_facts(&signature_uri, signature, editor.server())
        .expect("the RBS contract must enter the LSP engine");
    editor.open("payload_factory.rb", mismatched).await;
    let lsp_diagnostics = editor.diagnostics("payload_factory.rb").await;
    assert_diagnostic_parity(
        find_cli_diagnostic(&check_report, "declared-return-type-mismatch"),
        find_lsp_diagnostic(&lsp_diagnostics, "declared-return-type-mismatch"),
    );

    std::fs::write(&implementation, incomplete)
        .expect("the incomplete Ruby implementation must replace the mismatch");
    let incomplete_check = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must evaluate incomplete structural evidence");
    editor.set("payload_factory.rb", incomplete).await;
    let incomplete_lsp = editor.diagnostics("payload_factory.rb").await;
    assert!(
        incomplete_check.diagnostics.iter().all(|diagnostic| {
            diagnostic.code.as_deref() != Some("declared-return-type-mismatch")
        }),
        "CLI must not diagnose structural incompatibility from incomplete evidence: {:?}",
        incomplete_check.diagnostics
    );
    assert!(
        incomplete_lsp.iter().all(|diagnostic| {
            !matches!(
                &diagnostic.code,
                Some(NumberOrString::String(code)) if code == "declared-return-type-mismatch"
            )
        }),
        "LSP must not diagnose structural incompatibility from incomplete evidence: {incomplete_lsp:?}"
    );
}

#[tokio::test]
async fn unresolved_method_diagnostic_matches_cli_and_lsp() {
    let source = "class User\n  def name\n    \"x\"\n  end\nend\n\nu = User.new\nu.naem\n";
    let project = tempfile::tempdir().expect("temporary parity project must be created");
    std::fs::write(project.path().join("main.rb"), source).expect("parity fixture must be written");
    let check_report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must analyze the unresolved-method fixture");
    let mut editor = FakeEditor::new().await;
    editor.open("main.rb", source).await;
    let published = editor.diagnostics("main.rb").await;
    assert!(
        check_report.diagnostics.iter().all(|diagnostic| {
            diagnostic.code.as_deref() != Some("unresolved-method")
                || !diagnostic.message.contains("`new`")
        }) && published.iter().all(|diagnostic| {
            !matches!(&diagnostic.code, Some(NumberOrString::String(code)) if code == "unresolved-method")
                || !diagnostic.message.contains("`new`")
        }),
        "Class#new must resolve through the shared Class object lookup chain: CLI={:?}, LSP={published:?}",
        check_report.diagnostics
    );

    let check_naem = check_report
        .diagnostics
        .iter()
        .find(|diagnostic| {
            diagnostic.code.as_deref() == Some("unresolved-method")
                && diagnostic.message.contains("Did you mean `name`?")
        })
        .unwrap_or_else(|| {
            panic!(
                "check must suggest `name` for `naem`, got {:?}",
                check_report.diagnostics
            )
        });
    let lsp_naem = published
        .iter()
        .find(|diagnostic| {
            matches!(&diagnostic.code, Some(NumberOrString::String(code)) if code == "unresolved-method")
                && diagnostic.message.contains("Did you mean `name`?")
        })
        .unwrap_or_else(|| panic!("LSP must suggest `name` for `naem`, got {published:?}"));
    assert_diagnostic_parity(check_naem, lsp_naem);
}

#[tokio::test]
async fn unresolved_constant_diagnostic_matches_cli_and_lsp() {
    let source = "UnknownThing.new\n";
    let project = tempfile::tempdir().expect("temporary parity project must be created");
    std::fs::write(project.path().join("main.rb"), source).expect("parity fixture must be written");
    let check_report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must analyze the unresolved-constant fixture");
    let check_diagnostic = find_cli_diagnostic(&check_report, "unresolved-constant");

    let mut editor = FakeEditor::new().await;
    editor.open("main.rb", source).await;
    let published = editor.diagnostics("main.rb").await;
    let lsp_diagnostic = find_lsp_diagnostic(&published, "unresolved-constant");
    assert_diagnostic_parity(check_diagnostic, lsp_diagnostic);
}

#[tokio::test]
async fn missing_kwarg_diagnostic_matches_cli_and_lsp() {
    let source = "def greet(name:, age: 0)\n  name\nend\n\ngreet(age: 30)\n";
    let project = tempfile::tempdir().expect("temporary parity project must be created");
    std::fs::write(project.path().join("main.rb"), source).expect("parity fixture must be written");
    let check_report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must analyze the missing-kwarg fixture");
    let check_diagnostic = find_cli_diagnostic(&check_report, "missing-kwarg");

    let mut editor = FakeEditor::new().await;
    editor.open("main.rb", source).await;
    let published = editor.diagnostics("main.rb").await;
    let lsp_diagnostic = find_lsp_diagnostic(&published, "missing-kwarg");
    assert!(
        check_diagnostic.message.contains("`name:`"),
        "the shared engine must name the missing keyword argument, got `{}`",
        check_diagnostic.message
    );
    assert_diagnostic_parity(check_diagnostic, lsp_diagnostic);
}

#[tokio::test]
async fn yard_unknown_param_diagnostic_matches_cli_and_lsp() {
    let source = "# @param ghost [String]\ndef actual\nend\n";
    let project = tempfile::tempdir().expect("temporary parity project must be created");
    std::fs::write(project.path().join("main.rb"), source).expect("parity fixture must be written");
    let check_report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must analyze the unmatched YARD parameter fixture");
    let check_diagnostic = find_cli_diagnostic(&check_report, "yard-unknown-param");

    let mut editor = FakeEditor::new().await;
    editor.open("main.rb", source).await;
    let published = editor.diagnostics("main.rb").await;
    let lsp_diagnostic = find_lsp_diagnostic(&published, "yard-unknown-param");
    assert_diagnostic_parity(check_diagnostic, lsp_diagnostic);
}

#[tokio::test]
async fn yard_rbs_mismatch_diagnostic_matches_cli_and_lsp() {
    let source = "class String\n  # @return [String]\n  def length\n    1\n  end\nend\n";
    let project = tempfile::tempdir().expect("temporary parity project must be created");
    std::fs::write(project.path().join("main.rb"), source).expect("parity fixture must be written");
    let check_report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must analyze the YARD/RBS mismatch fixture");
    let check_diagnostic = find_cli_diagnostic(&check_report, "yard-rbs-mismatch");
    assert!(
        check_diagnostic.message.contains("conflicts with RBS type"),
        "the shared engine must explain the conflicting RBS type, got `{}`",
        check_diagnostic.message
    );

    let mut editor = FakeEditor::new().await;
    editor.open("main.rb", source).await;
    let published = editor.diagnostics("main.rb").await;
    let lsp_diagnostic = find_lsp_diagnostic(&published, "yard-rbs-mismatch");
    assert_diagnostic_parity(check_diagnostic, lsp_diagnostic);
}

#[tokio::test]
async fn unresolved_require_diagnostic_matches_cli_and_lsp() {
    let project = tempfile::tempdir().expect("temporary parity project must be created");
    std::fs::write(project.path().join("main.rb"), "require \"missing\"\n")
        .expect("parity fixture must be written");
    let check_report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must analyze the unresolved-require fixture");
    let check_diagnostic = find_cli_diagnostic(&check_report, "unresolved-require");

    let mut editor = FakeEditor::new().await;
    editor.add_workspace("project");
    editor
        .open("project/main.rb", "require \"missing\"\n")
        .await;
    let published = editor.diagnostics("project/main.rb").await;
    let lsp_diagnostic = find_lsp_diagnostic(&published, "unresolved-require");
    assert_diagnostic_parity(check_diagnostic, lsp_diagnostic);
}

#[tokio::test]
async fn syntax_diagnostic_matches_cli_and_lsp() {
    let source = "def broken(\n";
    let project = tempfile::tempdir().expect("temporary parity project must be created");
    std::fs::write(project.path().join("main.rb"), source).expect("parity fixture must be written");
    let check_report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must analyze the syntax-error fixture");
    let mut check_syntax = check_report
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code.is_none())
        .map(|diagnostic| (diagnostic.range, diagnostic.message.clone()))
        .collect::<Vec<_>>();
    assert!(
        check_syntax.len() >= 1,
        "check must return the parser syntax diagnostics, got {:?}",
        check_report.diagnostics
    );

    let mut editor = FakeEditor::new().await;
    editor.open("main.rb", source).await;
    let mut lsp_syntax = editor
        .diagnostics("main.rb")
        .await
        .into_iter()
        .filter(|diagnostic| diagnostic.code.is_none())
        .map(|diagnostic| {
            (
                crate::lsp::check::CheckRange {
                    start: crate::lsp::check::CheckPosition {
                        line: diagnostic.range.start.line + 1,
                        column: diagnostic.range.start.character + 1,
                    },
                    end: crate::lsp::check::CheckPosition {
                        line: diagnostic.range.end.line + 1,
                        column: diagnostic.range.end.character + 1,
                    },
                },
                diagnostic.message,
            )
        })
        .collect::<Vec<_>>();
    check_syntax.sort();
    lsp_syntax.sort();
    assert_eq!(
        check_syntax, lsp_syntax,
        "CLI and LSP must project the same parser syntax diagnostics for one byte-identical file"
    );
}

#[tokio::test]
async fn multi_diagnostic_file_keeps_deterministic_cli_lsp_parity() {
    let source = "def greet(name:, age: 0)\n  name\nend\n\nUnknownThing.new\ngreet(age: 30)\n";
    let project = tempfile::tempdir().expect("temporary parity project must be created");
    std::fs::write(project.path().join("main.rb"), source).expect("parity fixture must be written");
    let check_report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must analyze the multi-diagnostic fixture");
    let mut check_codes = check_report
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code.is_some())
        .map(|diagnostic| {
            (
                diagnostic.range,
                diagnostic.code.as_deref().unwrap().to_string(),
                diagnostic.message.clone(),
            )
        })
        .collect::<Vec<_>>();

    let mut editor = FakeEditor::new().await;
    editor.open("main.rb", source).await;
    let mut lsp_codes = editor
        .diagnostics("main.rb")
        .await
        .into_iter()
        .filter_map(|diagnostic| {
            let code = match diagnostic.code {
                Some(NumberOrString::String(code)) => code,
                _ => return None,
            };
            Some((
                crate::lsp::check::CheckRange {
                    start: crate::lsp::check::CheckPosition {
                        line: diagnostic.range.start.line + 1,
                        column: diagnostic.range.start.character + 1,
                    },
                    end: crate::lsp::check::CheckPosition {
                        line: diagnostic.range.end.line + 1,
                        column: diagnostic.range.end.character + 1,
                    },
                },
                code,
                diagnostic.message,
            ))
        })
        .collect::<Vec<_>>();

    check_codes.sort();
    lsp_codes.sort();
    assert!(
        check_codes.len() >= 2,
        "the fixture must produce multiple engine-owned diagnostics, got {check_codes:?}"
    );
    assert_eq!(
        check_codes, lsp_codes,
        "CLI and LSP must project the same deterministic diagnostic set for one file"
    );
}
