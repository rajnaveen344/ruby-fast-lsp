//! Multi-root workspace routing: files map to correct roots while analysis facts
//! live in the shared engine.

use crate::test::harness::FakeEditor;

#[tokio::test]
async fn each_workspace_gets_its_own_index() {
    let mut editor = FakeEditor::new().await;
    editor.add_workspace("workspace_a");
    editor.add_workspace("workspace_b");

    assert_eq!(editor.workspace_count(), 2);

    editor
        .open(
            "workspace_a/user.rb",
            "class User\n  def name_a; end\nend\n",
        )
        .await;
    editor
        .open(
            "workspace_b/user.rb",
            "class User\n  def name_b; end\nend\n",
        )
        .await;

    let ws_a = editor
        .workspace_for("workspace_a/user.rb")
        .expect("workspace_a should match the file URI");
    let ws_b = editor
        .workspace_for("workspace_b/user.rb")
        .expect("workspace_b should match the file URI");

    assert_ne!(
        ws_a.root_uri, ws_b.root_uri,
        "each file should land in its own workspace"
    );

    assert!(
        method_fact_in_path(editor.server(), "name_a", "workspace_a/user.rb"),
        "workspace_a file should produce its method fact"
    );
    assert!(
        method_fact_in_path(editor.server(), "name_b", "workspace_b/user.rb"),
        "workspace_b file should produce its method fact"
    );
    assert!(
        !method_fact_in_path(editor.server(), "name_a", "workspace_b/user.rb"),
        "workspace_b file must not own workspace_a method fact"
    );
}

#[tokio::test]
async fn longest_prefix_wins_for_nested_workspaces() {
    let mut editor = FakeEditor::new().await;
    editor.add_workspace("apps");
    editor.add_workspace("apps/web");

    editor
        .open(
            "apps/web/controller.rb",
            "class Controller\n  def index; end\nend\n",
        )
        .await;

    let ws = editor
        .workspace_for("apps/web/controller.rb")
        .expect("nested workspace should match");
    assert!(
        ws.root_uri.as_str().ends_with("apps/web/"),
        "expected longest-prefix match `apps/web/`, got {}",
        ws.root_uri.as_str()
    );

    let outer = editor
        .server()
        .list_workspaces()
        .into_iter()
        .find(|w| w.root_uri.as_str().ends_with("apps/"))
        .expect("outer workspace should still be registered");
    assert!(
        outer.root_uri.as_str().ends_with("apps/"),
        "outer workspace should remain registered"
    );
}

fn method_fact_in_path(
    server: &crate::server::RubyLanguageServer,
    method_name: &str,
    path_suffix: &str,
) -> bool {
    let engine = server.analysis_engine.lock();
    engine.all_method_facts().into_iter().any(|fact| {
        let ruby_analysis::core::FullyQualifiedName::Method(_, method) = fact.fqn else {
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
