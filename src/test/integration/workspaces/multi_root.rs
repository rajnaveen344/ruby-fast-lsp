//! Multi-root workspace isolation: two folders, two indices, no bleed.

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

    // Different `Workspace` handles → different underlying `RubyIndex`.
    assert_ne!(
        ws_a.root_uri, ws_b.root_uri,
        "each file should land in its own workspace"
    );

    // Each workspace's index sees only its own User definition.
    let entries_in_a = ws_a.index.lock().entries_len();
    let entries_in_b = ws_b.index.lock().entries_len();
    assert!(entries_in_a > 0, "workspace_a index should be populated");
    assert!(entries_in_b > 0, "workspace_b index should be populated");

    // Sanity: a method named "name_a" exists in workspace_a but not in workspace_b.
    let a_has_name_a = ws_a
        .index
        .lock()
        .methods_by_name()
        .any(|(method, _)| method.get_name() == "name_a");
    let b_has_name_a = ws_b
        .index
        .lock()
        .methods_by_name()
        .any(|(method, _)| method.get_name() == "name_a");
    let b_has_name_b = ws_b
        .index
        .lock()
        .methods_by_name()
        .any(|(method, _)| method.get_name() == "name_b");

    assert!(a_has_name_a, "workspace_a should index its own method");
    assert!(b_has_name_b, "workspace_b should index its own method");
    assert!(
        !b_has_name_a,
        "workspace_b must not see methods from workspace_a"
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

    // The outer `apps` workspace should NOT have indexed the file.
    let outer = editor
        .server()
        .list_workspaces()
        .into_iter()
        .find(|w| w.root_uri.as_str().ends_with("apps/"))
        .expect("outer workspace should still be registered");
    let outer_has_controller = outer
        .index
        .lock()
        .methods_by_name()
        .any(|(m, _)| m.get_name() == "index");
    assert!(
        !outer_has_controller,
        "outer workspace should not see files routed to the nested workspace"
    );
}
