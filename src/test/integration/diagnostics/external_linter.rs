use crate::config::{LinterKind, RubyFastLspConfig};
use crate::test::harness::FakeEditor;
use std::fs;
use tempfile::TempDir;
use tower_lsp::lsp_types::NumberOrString;

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
