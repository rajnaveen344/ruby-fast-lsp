//! Orphan fallback: files outside any workspace still enter shared analysis.

use crate::test::harness::FakeEditor;

#[tokio::test]
async fn file_outside_any_workspace_falls_through_to_orphan() {
    let mut editor = FakeEditor::new().await;
    editor.add_workspace("workspace_a");

    // Open a file that does NOT live under workspace_a.
    editor
        .open("stray/loose.rb", "class Loose\n  def hello; end\nend\n")
        .await;

    // No workspace should match the orphan path.
    assert!(
        editor.workspace_for("stray/loose.rb").is_none(),
        "stray file must not match any registered workspace"
    );

    assert!(
        method_fact_in_path(editor.server(), "hello", "stray/loose.rb"),
        "orphan file should produce analysis method facts"
    );

    // The registered workspace must NOT see the orphan file.
    let ws_a = editor
        .server()
        .list_workspaces()
        .into_iter()
        .next()
        .unwrap();
    assert!(
        !ws_a.root_uri.as_str().contains("stray"),
        "registered workspace must not claim orphan path"
    );
}

#[tokio::test]
async fn no_registered_workspace_means_everything_is_orphan() {
    let mut editor = FakeEditor::new().await;
    assert_eq!(editor.workspace_count(), 0);

    editor
        .open("anywhere.rb", "class Anywhere\n  def ping; end\nend\n")
        .await;

    assert!(
        method_fact_in_path(editor.server(), "ping", "anywhere.rb"),
        "with no workspaces registered, file should still produce analysis facts"
    );
}

fn method_fact_in_path(
    server: &crate::server::RubyLanguageServer,
    method_name: &str,
    path_suffix: &str,
) -> bool {
    let engine = server.analysis_engine.lock();
    engine.all_method_facts().into_iter().any(|fact| {
        let ruby_analysis_core::FullyQualifiedName::Method(_, method) = fact.fqn else {
            return false;
        };
        if method.as_str() != method_name {
            return false;
        }
        engine
            .file(fact.range.file_id)
            .map(|file| file.path.to_string_lossy().ends_with(path_suffix))
            .unwrap_or(false)
    })
}
