use crate::config::{FormatterKind, RubyFastLspConfig};
use crate::server::RubyLanguageServer;
use crate::test::harness::FakeEditor;
use std::collections::HashMap;
use std::fs;
use tempfile::TempDir;
use tower_lsp::lsp_types::{InitializeParams, OneOf, Position, Range, Url, WorkspaceEdit};
use tower_lsp::LanguageServer;

#[tokio::test]
async fn initialization_advertises_full_document_formatting() {
    let initialized = RubyLanguageServer::default()
        .initialize(InitializeParams::default())
        .await
        .unwrap();

    assert_eq!(
        initialized.capabilities.document_formatting_provider,
        Some(OneOf::Left(true))
    );
}

#[cfg(unix)]
fn fake_formatter(output: &str, exit_status: i32) -> (TempDir, String, std::path::PathBuf) {
    use std::os::unix::fs::PermissionsExt;

    let temp = TempDir::new().unwrap();
    let executable = temp.path().join("fake-formatter");
    let stdin = temp.path().join("stdin.rb");
    let script = format!(
        "#!/bin/sh\ncat > '{}'\nprintf '%s' '{}'\nexit {}\n",
        stdin.display(),
        output.replace('\'', "'\\''"),
        exit_status
    );
    fs::write(&executable, script).unwrap();
    let mut permissions = fs::metadata(&executable).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&executable, permissions).unwrap();
    (temp, executable.to_string_lossy().to_string(), stdin)
}

#[cfg(unix)]
#[tokio::test]
async fn formats_current_unsaved_buffer_with_utf16_full_document_edit() {
    let (_temp, command, captured_stdin) = fake_formatter("puts 'formatted'\n", 0);
    let mut editor = FakeEditor::new().await;
    *editor.server().config.lock() = RubyFastLspConfig {
        formatter: FormatterKind::Standard,
        formatter_command: vec![command],
        ..RubyFastLspConfig::default()
    };
    editor.open("sample.rb", "puts 'disk'\n").await;
    editor.set("sample.rb", "puts '😀 unsaved'").await;

    let edits = editor.format("sample.rb").await;

    assert_eq!(
        fs::read_to_string(captured_stdin).unwrap(),
        "puts '😀 unsaved'"
    );
    assert_eq!(edits.len(), 1);
    assert_eq!(
        edits[0].range,
        Range::new(Position::new(0, 0), Position::new(0, 17))
    );
    assert_eq!(edits[0].new_text, "puts 'formatted'\n");

    editor
        .apply_edit(&WorkspaceEdit {
            changes: Some(HashMap::from([(
                Url::parse("file:///sample.rb").unwrap(),
                edits,
            )])),
            document_changes: None,
            change_annotations: None,
        })
        .await;
    assert_eq!(editor.content("sample.rb"), "puts 'formatted'\n");
}

#[cfg(unix)]
#[tokio::test]
async fn formatter_failure_and_unchanged_output_return_no_edits() {
    let (_failed_temp, failed_command, _) = fake_formatter("ignored", 2);
    let mut editor = FakeEditor::new().await;
    *editor.server().config.lock() = RubyFastLspConfig {
        formatter: FormatterKind::RuboCop,
        formatter_command: vec![failed_command],
        ..RubyFastLspConfig::default()
    };
    editor.open("sample.rb", "puts 1\n").await;
    assert!(editor.format("sample.rb").await.is_empty());

    let (_same_temp, same_command, _) = fake_formatter("puts 1\n", 0);
    editor.server().config.lock().formatter_command = vec![same_command];
    assert!(editor.format("sample.rb").await.is_empty());
    assert_eq!(editor.content("sample.rb"), "puts 1\n");
}
