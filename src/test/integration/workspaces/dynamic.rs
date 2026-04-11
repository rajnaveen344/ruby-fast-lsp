//! `workspace/didChangeWorkspaceFolders` add/remove tests.

use crate::handlers::notification::handle_did_change_workspace_folders;
use crate::test::harness::FakeEditor;
use tower_lsp::lsp_types::{
    DidChangeWorkspaceFoldersParams, Url, WorkspaceFolder, WorkspaceFoldersChangeEvent,
};

fn folder(root: &str) -> WorkspaceFolder {
    WorkspaceFolder {
        uri: Url::parse(&format!("file:///{}/", root.trim_end_matches('/'))).unwrap(),
        name: root.to_string(),
    }
}

#[tokio::test]
async fn add_workspace_at_runtime_creates_a_new_index() {
    let editor = FakeEditor::new().await;
    assert_eq!(editor.workspace_count(), 0);

    let params = DidChangeWorkspaceFoldersParams {
        event: WorkspaceFoldersChangeEvent {
            added: vec![folder("late_added")],
            removed: vec![],
        },
    };
    handle_did_change_workspace_folders(editor.server(), params).await;

    assert_eq!(editor.workspace_count(), 1);
    let ws = editor
        .server()
        .list_workspaces()
        .into_iter()
        .next()
        .unwrap();
    assert!(ws.root_uri.as_str().ends_with("late_added/"));
}

#[tokio::test]
async fn remove_workspace_at_runtime_drops_it() {
    let editor = FakeEditor::new().await;
    editor.add_workspace("temporary");
    assert_eq!(editor.workspace_count(), 1);

    let params = DidChangeWorkspaceFoldersParams {
        event: WorkspaceFoldersChangeEvent {
            added: vec![],
            removed: vec![folder("temporary")],
        },
    };
    handle_did_change_workspace_folders(editor.server(), params).await;

    assert_eq!(editor.workspace_count(), 0);
}

#[tokio::test]
async fn add_then_remove_round_trip() {
    let editor = FakeEditor::new().await;
    editor.add_workspace("keep_me");

    // Add `transient`, then remove it.
    handle_did_change_workspace_folders(
        editor.server(),
        DidChangeWorkspaceFoldersParams {
            event: WorkspaceFoldersChangeEvent {
                added: vec![folder("transient")],
                removed: vec![],
            },
        },
    )
    .await;
    assert_eq!(editor.workspace_count(), 2);

    handle_did_change_workspace_folders(
        editor.server(),
        DidChangeWorkspaceFoldersParams {
            event: WorkspaceFoldersChangeEvent {
                added: vec![],
                removed: vec![folder("transient")],
            },
        },
    )
    .await;
    assert_eq!(editor.workspace_count(), 1);

    // `keep_me` should remain.
    let remaining = editor.server().list_workspaces();
    assert_eq!(remaining.len(), 1);
    assert!(remaining[0].root_uri.as_str().ends_with("keep_me/"));
}
