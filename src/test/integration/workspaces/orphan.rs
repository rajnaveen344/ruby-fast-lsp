//! Orphan-index fallback: files outside any workspace land in `orphan_index`.

use crate::test::harness::FakeEditor;

#[tokio::test]
async fn file_outside_any_workspace_falls_through_to_orphan() {
    let mut editor = FakeEditor::new().await;
    editor.add_workspace("workspace_a");

    // Open a file that does NOT live under workspace_a.
    editor
        .open(
            "stray/loose.rb",
            "class Loose\n  def hello; end\nend\n",
        )
        .await;

    // No workspace should match the orphan path.
    assert!(
        editor.workspace_for("stray/loose.rb").is_none(),
        "stray file must not match any registered workspace"
    );

    // The orphan index should contain the file's definitions.
    let orphan = &editor.server().orphan_index;
    let has_loose_method = orphan
        .lock()
        .methods_by_name()
        .any(|(m, _)| m.get_name() == "hello");
    assert!(
        has_loose_method,
        "orphan index should hold definitions for files outside any workspace"
    );

    // The registered workspace must NOT see the orphan file.
    let ws_a = editor
        .server()
        .list_workspaces()
        .into_iter()
        .next()
        .unwrap();
    let ws_has_loose = ws_a
        .index
        .lock()
        .methods_by_name()
        .any(|(m, _)| m.get_name() == "hello");
    assert!(
        !ws_has_loose,
        "registered workspace must not contain orphan file definitions"
    );
}

#[tokio::test]
async fn no_registered_workspace_means_everything_is_orphan() {
    let mut editor = FakeEditor::new().await;
    assert_eq!(editor.workspace_count(), 0);

    editor
        .open("anywhere.rb", "class Anywhere\n  def ping; end\nend\n")
        .await;

    let orphan_has_ping = editor
        .server()
        .orphan_index
        .lock()
        .methods_by_name()
        .any(|(m, _)| m.get_name() == "ping");
    assert!(
        orphan_has_ping,
        "with no workspaces registered, every file must land in the orphan index"
    );
}
