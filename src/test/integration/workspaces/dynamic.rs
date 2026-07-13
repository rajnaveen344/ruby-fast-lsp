//! `workspace/didChangeWorkspaceFolders` add/remove tests.

use crate::handlers::notification::handle_did_change_workspace_folders;
use crate::test::harness::FakeEditor;
use std::fs;
use tempfile::tempdir;
use tower_lsp::lsp_types::{
    DidChangeWorkspaceFoldersParams, Url, WorkspaceFolder, WorkspaceFoldersChangeEvent,
};

fn folder(root: &str) -> WorkspaceFolder {
    WorkspaceFolder {
        uri: Url::parse(&format!("file:///{}/", root.trim_end_matches('/'))).unwrap(),
        name: root.to_string(),
    }
}

fn folder_path(root: &std::path::Path) -> WorkspaceFolder {
    WorkspaceFolder {
        uri: Url::from_directory_path(root).unwrap(),
        name: root.display().to_string(),
    }
}

#[tokio::test]
async fn add_workspace_at_runtime_creates_a_new_index() {
    let editor = FakeEditor::new().await;
    let workspace = tempdir().unwrap();
    assert_eq!(editor.workspace_count(), 0);

    let params = DidChangeWorkspaceFoldersParams {
        event: WorkspaceFoldersChangeEvent {
            added: vec![folder_path(workspace.path())],
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
    assert_eq!(ws.root_path, workspace.path());
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
async fn removing_workspace_rehomes_open_documents_in_orphan_engine() {
    let mut editor = FakeEditor::new().await;
    editor.add_workspace("temporary");
    editor
        .open(
            "temporary/user.rb",
            "class User\n  def name; 'Ada'; end\nend\n",
        )
        .await;

    handle_did_change_workspace_folders(
        editor.server(),
        DidChangeWorkspaceFoldersParams {
            event: WorkspaceFoldersChangeEvent {
                added: vec![],
                removed: vec![folder("temporary")],
            },
        },
    )
    .await;

    assert!(editor.workspace_for("temporary/user.rb").is_none());
    let path = Url::parse("file:///temporary/user.rb")
        .unwrap()
        .to_file_path()
        .unwrap();
    assert!(editor
        .server()
        .analysis_engine
        .read()
        .file_id(path)
        .is_some());
}

#[tokio::test]
async fn adding_workspace_rehomes_open_orphan_document_in_project_engine() {
    let workspace = tempdir().unwrap();
    let file = workspace.path().join("user.rb");
    let filename = file.to_string_lossy().to_string();
    let mut editor = FakeEditor::new().await;
    editor
        .open(&filename, "class User\n  def name; 'Ada'; end\nend\n")
        .await;
    assert!(editor
        .server()
        .analysis_engine
        .read()
        .file_id(&file)
        .is_some());

    handle_did_change_workspace_folders(
        editor.server(),
        DidChangeWorkspaceFoldersParams {
            event: WorkspaceFoldersChangeEvent {
                added: vec![folder_path(workspace.path())],
                removed: vec![],
            },
        },
    )
    .await;

    let project = editor.workspace_for(&filename).unwrap();
    assert!(project.analysis_engine.read().file_id(&file).is_some());
}

#[tokio::test]
async fn add_then_remove_round_trip() {
    let editor = FakeEditor::new().await;
    editor.add_workspace("keep_me");
    let transient = tempdir().unwrap();
    let transient_folder = folder_path(transient.path());

    // Add `transient`, then remove it.
    handle_did_change_workspace_folders(
        editor.server(),
        DidChangeWorkspaceFoldersParams {
            event: WorkspaceFoldersChangeEvent {
                added: vec![transient_folder.clone()],
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
                removed: vec![transient_folder],
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

#[test]
fn container_workspace_registration_expands_to_isolated_gemfile_projects() {
    let workspace = tempdir().unwrap();
    fs::create_dir_all(workspace.path().join("server")).unwrap();
    fs::create_dir_all(workspace.path().join("admin")).unwrap();
    fs::write(workspace.path().join("server/Gemfile"), "").unwrap();
    fs::write(workspace.path().join("admin/Gemfile"), "").unwrap();
    fs::create_dir_all(workspace.path().join("vendor/cache/dependency")).unwrap();
    fs::write(workspace.path().join("vendor/cache/dependency/Gemfile"), "").unwrap();

    let server = crate::server::RubyLanguageServer::default();
    let uri = Url::from_directory_path(workspace.path()).unwrap();
    let projects = server.add_workspace_folder(uri).unwrap();

    assert_eq!(projects.len(), 2);
    let roots = projects
        .iter()
        .map(|project| project.root_path.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        roots,
        [
            workspace.path().join("admin"),
            workspace.path().join("server")
        ]
    );
}
