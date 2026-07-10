use crate::config::{LinterKind, RubyFastLspConfig};
use crate::test::harness::FakeEditor;
use std::fs;
use tempfile::TempDir;
use tower_lsp::lsp_types::{InitializeParams, NumberOrString};
use tower_lsp::LanguageServer;

#[tokio::test]
async fn advertises_quick_fix_code_actions() {
    let editor = FakeEditor::new().await;
    let initialized = editor
        .server()
        .initialize(InitializeParams::default())
        .await
        .unwrap();
    let provider = initialized
        .capabilities
        .code_action_provider
        .expect("safe linter quick fixes must be advertised");
    assert!(matches!(
        provider,
        tower_lsp::lsp_types::CodeActionProviderCapability::Options(_)
    ));
}

#[cfg(unix)]
fn fake_linter() -> (TempDir, String) {
    use std::os::unix::fs::PermissionsExt;

    let temp = TempDir::new().unwrap();
    let executable = temp.path().join("fake-standardrb");
    fs::write(
        &executable,
        r##"#!/bin/sh
cat >/dev/null
printf '%s' '{"files":[{"path":"sample.rb","offenses":[{"severity":"convention","message":"Style/FrozenStringLiteralComment: Missing frozen string literal comment.","cop_name":"Style/FrozenStringLiteralComment","correctable":true,"location":{"start_line":1,"start_column":1,"last_line":1,"last_column":4}}]}]}'
exit 1
"##,
    )
    .unwrap();
    let mut permissions = fs::metadata(&executable).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&executable, permissions).unwrap();
    (temp, executable.to_string_lossy().to_string())
}

#[cfg(unix)]
#[tokio::test]
async fn external_linter_runs_on_open_and_save_but_not_did_change() {
    let (_temp, command) = fake_linter();
    let mut editor = FakeEditor::new().await;
    *editor.server().config.lock() = RubyFastLspConfig {
        linter: LinterKind::Standard,
        linter_command: vec![command],
        ..RubyFastLspConfig::default()
    };

    editor.open("sample.rb", "puts \"hello\"\n").await;
    assert!(has_linter_diagnostic(
        &editor.published_diagnostics("sample.rb")
    ));

    editor.set("sample.rb", "puts \"changed\"\n").await;
    assert!(
        !has_linter_diagnostic(&editor.published_diagnostics("sample.rb")),
        "didChange must not launch an external process in the typing path"
    );

    editor.save("sample.rb").await;
    assert!(has_linter_diagnostic(
        &editor.published_diagnostics("sample.rb")
    ));
}

fn has_linter_diagnostic(diagnostics: &[tower_lsp::lsp_types::Diagnostic]) -> bool {
    diagnostics.iter().any(|diagnostic| {
        diagnostic.code
            == Some(NumberOrString::String(
                "Style/FrozenStringLiteralComment".to_string(),
            ))
            && diagnostic.source.as_deref() == Some("Standard")
    })
}

#[cfg(unix)]
#[tokio::test]
async fn linter_failure_preserves_semantic_diagnostics() {
    use std::os::unix::fs::PermissionsExt;

    let temp = TempDir::new().unwrap();
    let executable = temp.path().join("broken-rubocop");
    fs::write(
        &executable,
        "#!/bin/sh\ncat >/dev/null\necho 'bundle is broken' >&2\nexit 2\n",
    )
    .unwrap();
    let mut permissions = fs::metadata(&executable).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&executable, permissions).unwrap();

    let mut editor = FakeEditor::new().await;
    *editor.server().config.lock() = RubyFastLspConfig {
        linter: LinterKind::RuboCop,
        linter_command: vec![executable.to_string_lossy().to_string()],
        ..RubyFastLspConfig::default()
    };
    editor.open("broken.rb", "MissingConstant\n").await;

    let diagnostics = editor.published_diagnostics("broken.rb");
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == Some(NumberOrString::String("unresolved-constant".to_string()))
    }));
    assert!(!has_linter_diagnostic(&diagnostics));
}

#[cfg(unix)]
#[tokio::test]
async fn correctable_linter_diagnostic_produces_and_applies_a_safe_quick_fix() {
    use std::os::unix::fs::PermissionsExt;
    use tower_lsp::lsp_types::CodeActionOrCommand;

    let temp = TempDir::new().unwrap();
    let executable = temp.path().join("fixing-rubocop");
    fs::write(
        &executable,
        r##"#!/bin/sh
input=$(cat)
case " $* " in
  *" --autocorrect "*) printf '%s\n' "${input%\"hello\"}'hello'"; exit 0 ;;
esac
printf '%s' '{"files":[{"path":"fix.rb","offenses":[{"severity":"convention","message":"Prefer single quotes.","cop_name":"Style/StringLiterals","correctable":true,"location":{"start_line":1,"start_column":6,"last_line":1,"last_column":12}}]}]}'
exit 1
"##,
    )
    .unwrap();
    let mut permissions = fs::metadata(&executable).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&executable, permissions).unwrap();

    let mut editor = FakeEditor::new().await;
    *editor.server().config.lock() = RubyFastLspConfig {
        linter: LinterKind::RuboCop,
        linter_command: vec![executable.to_string_lossy().to_string()],
        ..RubyFastLspConfig::default()
    };
    editor.open("fix.rb", "puts \"hello\"\n").await;
    let diagnostics = editor.published_diagnostics("fix.rb");
    let actions = editor.code_actions("fix.rb", diagnostics).await;
    assert_eq!(actions.len(), 1, "actual actions: {actions:?}");
    let CodeActionOrCommand::CodeAction(action) = &actions[0] else {
        panic!("quick fix must be a CodeAction with an edit")
    };
    assert_eq!(
        action.kind,
        Some(tower_lsp::lsp_types::CodeActionKind::QUICKFIX)
    );
    editor
        .apply_edit(
            action
                .edit
                .as_ref()
                .expect("quick fix must contain an edit"),
        )
        .await;
    assert_eq!(editor.content("fix.rb"), "puts 'hello'\n");
}

#[cfg(unix)]
#[tokio::test]
async fn noncorrectable_linter_diagnostic_does_not_offer_a_quick_fix() {
    let (_temp, command) = fake_linter();
    let mut editor = FakeEditor::new().await;
    *editor.server().config.lock() = RubyFastLspConfig {
        linter: LinterKind::Standard,
        linter_command: vec![command],
        ..RubyFastLspConfig::default()
    };
    editor.open("sample.rb", "puts \"hello\"\n").await;
    let mut diagnostics = editor.published_diagnostics("sample.rb");
    let linter = diagnostics
        .iter_mut()
        .find(|diagnostic| diagnostic.source.as_deref() == Some("Standard"))
        .expect("fixture must publish a Standard diagnostic");
    linter.data = Some(serde_json::json!({
        "linter": "standard",
        "correctable": false
    }));

    assert!(editor
        .code_actions("sample.rb", diagnostics)
        .await
        .is_empty());
}

#[cfg(unix)]
#[tokio::test]
async fn failed_safe_fix_returns_no_workspace_edit() {
    use std::os::unix::fs::PermissionsExt;

    let temp = TempDir::new().unwrap();
    let executable = temp.path().join("failing-fix-rubocop");
    fs::write(
        &executable,
        r##"#!/bin/sh
cat >/dev/null
case " $* " in
  *" --autocorrect "*) echo 'autocorrect failed' >&2; exit 2 ;;
esac
printf '%s' '{"files":[{"path":"fix.rb","offenses":[{"severity":"convention","message":"Prefer single quotes.","cop_name":"Style/StringLiterals","correctable":true,"location":{"start_line":1,"start_column":6,"last_line":1,"last_column":12}}]}]}'
exit 1
"##,
    )
    .unwrap();
    let mut permissions = fs::metadata(&executable).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&executable, permissions).unwrap();

    let mut editor = FakeEditor::new().await;
    *editor.server().config.lock() = RubyFastLspConfig {
        linter: LinterKind::RuboCop,
        linter_command: vec![executable.to_string_lossy().to_string()],
        ..RubyFastLspConfig::default()
    };
    editor.open("fix.rb", "puts \"hello\"\n").await;
    let diagnostics = editor.published_diagnostics("fix.rb");

    assert!(editor.code_actions("fix.rb", diagnostics).await.is_empty());
    assert_eq!(editor.content("fix.rb"), "puts \"hello\"\n");
}
