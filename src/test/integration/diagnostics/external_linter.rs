use crate::config::{LinterKind, RubyFastLspConfig};
#[cfg(unix)]
use crate::test::harness::with_process_clock;
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
printf 'invocation\n' >> "$(dirname "$0")/invocations"
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
    with_process_clock(async {
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
    })
    .await;
}

#[cfg(unix)]
#[tokio::test]
async fn dependency_open_retains_current_linter_output_without_rerunning_it() {
    with_process_clock(async {
        let (temp, command) = fake_linter();
        let mut editor = FakeEditor::new().await;
        *editor.server().config.lock() = RubyFastLspConfig {
            linter: LinterKind::Standard,
            linter_command: vec![command],
            ..RubyFastLspConfig::default()
        };
        editor.open("sample.rb", "name = VALUE\n").await;
        assert!(has_linter_diagnostic(
            &editor.published_diagnostics("sample.rb")
        ));
        editor.open("definition.rb", "VALUE = 1\n").await;
        let refreshed = editor.published_diagnostics("sample.rb");
        assert!(has_linter_diagnostic(&refreshed),
            "refreshing unchanged consumer semantics must retain its current linter output: {refreshed:?}");
        assert!(
            refreshed.iter().all(|diagnostic| diagnostic.code
                != Some(NumberOrString::String("unresolved-constant".into()))),
            "the late dependency must clear the consumer's unresolved constant: {refreshed:?}"
        );
        let invocation_count = || {
            fs::read_to_string(temp.path().join("invocations"))
                .unwrap()
                .lines()
                .count()
        };
        assert_eq!(invocation_count(), 2,
            "only the two opened documents may launch the linter; consumer refresh must reuse its existing output");
        editor.set("sample.rb", "name = VALUE\n\n").await;
        assert!(
            !has_linter_diagnostic(&editor.published_diagnostics("sample.rb")),
            "editing the source must invalidate cached linter output until the next save"
        );
        assert_eq!(invocation_count(), 2);
        editor.save("sample.rb").await;
        assert!(has_linter_diagnostic(
            &editor.published_diagnostics("sample.rb")
        ));
        assert_eq!(invocation_count(), 3);
        editor.close("sample.rb").await;
        let mut retained_after_close = Vec::new();
        editor.server().append_current_external_linter_diagnostics(
            &crate::test::harness::fixture_uri("/sample.rb"),
            &mut retained_after_close,
        );
        assert!(
            retained_after_close.is_empty(),
            "closing a document must release retained linter output"
        );
    })
    .await;
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
async fn cold_coordinator_diagnostics_preserve_current_linter_output() {
    with_process_clock(async {
        let (temp, command) = fake_linter();
        let root = temp.path().join("project");
        fs::create_dir(&root).unwrap();
        let path = root.join("caller.rb");
        let source = "_value = ABSENT\n";
        fs::write(&path, source).unwrap();
        let root_uri = tower_lsp::lsp_types::Url::from_directory_path(&root).unwrap();
        let filename = path.to_str().unwrap().trim_start_matches('/');
        let mut editor = FakeEditor::with_cache_root(temp.path().join("cache")).await;
        let server = editor.server().clone();
        server.set_discovered_runtimes_for_tests(Vec::new());
        *server.config.lock() = RubyFastLspConfig {
            linter: LinterKind::Standard,
            linter_command: vec![command],
            ..RubyFastLspConfig::default()
        };
        let workspace = server.add_workspace(root_uri.clone());
        editor.open(filename, source).await;
        let initial = editor.diagnostics(filename).await;
        assert!(has_linter_diagnostic(&initial));
        crate::capabilities::indexing::init_workspace_for_run(
            &server,
            root_uri,
            workspace.begin_indexing_run(),
        )
        .await
        .expect("cold indexing must succeed");
        assert_eq!(
            editor.diagnostics(filename).await,
            initial,
            "cold publication must retain exact-source linter output and semantic errors"
        );
        assert_eq!(
            fs::read_to_string(temp.path().join("invocations"))
                .unwrap()
                .lines()
                .count(),
            1,
            "only didOpen may launch the linter; cold publication must reuse its output"
        );
    })
    .await;
}

#[cfg(unix)]
#[tokio::test]
async fn dependency_refresh_preserves_current_syntax_and_linter_output() {
    with_process_clock(async {
        let (temp, command) = fake_linter();
        let root = temp.path().join("project");
        let library = temp.path().join("dependency/lib");
        fs::create_dir(&root).unwrap();
        fs::create_dir_all(&library).unwrap();
        fs::write(library.join("feature.rb"), "# dependency feature\n").unwrap();
        let path = root.join("caller.rb");
        let root_uri = tower_lsp::lsp_types::Url::from_directory_path(&root).unwrap();
        let filename = path.to_str().unwrap().trim_start_matches('/');
        let mut editor = FakeEditor::new().await;
        let server = editor.server().clone();
        *server.config.lock() = RubyFastLspConfig {
            linter: LinterKind::Standard,
            linter_command: vec![command],
            ..RubyFastLspConfig::default()
        };
        let workspace = server.add_workspace(root_uri);
        editor
            .open(filename, "require 'feature'\nreturn\n1\n")
            .await;
        let initial = editor.diagnostics(filename).await;
        assert!(has_linter_diagnostic(&initial));
        assert!(initial.iter().any(
            |diagnostic| diagnostic.code == Some(NumberOrString::String("unreachable-code".into()))
        ));
        assert!(initial
            .iter()
            .any(|diagnostic| diagnostic.code
                == Some(NumberOrString::String("unresolved-require".into()))));
        let expected = initial
            .into_iter()
            .filter(|diagnostic| {
                diagnostic.code != Some(NumberOrString::String("unresolved-require".into()))
            })
            .collect::<Vec<_>>();
        workspace.set_dependency_require_paths(vec![library]);
        server
            .refresh_unresolved_require_diagnostics_for_workspace(&workspace)
            .await;
        assert_eq!(editor.diagnostics(filename).await, expected,
            "dependency refresh must clear only the resolved require, preserving syntax and exact-source linter output");
        assert_eq!(
            fs::read_to_string(temp.path().join("invocations"))
                .unwrap()
                .lines()
                .count(),
            1,
            "dependency refresh must never relaunch the linter"
        );
    })
    .await;
}

#[cfg(unix)]
#[tokio::test]
async fn linter_failure_preserves_semantic_diagnostics() {
    with_process_clock(async {
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
    })
    .await;
}

#[cfg(unix)]
#[tokio::test]
async fn correctable_linter_diagnostic_produces_and_applies_a_safe_quick_fix() {
    with_process_clock(async {
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
    })
    .await;
}

#[cfg(unix)]
#[tokio::test]
async fn noncorrectable_linter_diagnostic_does_not_offer_a_quick_fix() {
    with_process_clock(async {
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
    })
    .await;
}

#[cfg(unix)]
#[tokio::test]
async fn failed_safe_fix_returns_no_workspace_edit() {
    with_process_clock(async {
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
    })
    .await;
}
