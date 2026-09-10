//! Interleave the real dependency refresh with editor/source and root changes.
//! Observe both engine facts and submitted LSP diagnostics before recovery.

use crate::indexer::test_schedule::Point;
use crate::query::EngineQuery;
use crate::test::harness::FakeEditor;
use std::time::Duration;
use tower_lsp::lsp_types::{Diagnostic, Url};

#[derive(Clone, Copy)]
enum Change {
    CorrectingEdit,
    NewMissingRequire,
    RootsAdded,
    RootsRemoved,
    WorkspaceReplaced,
    Close,
    ProjectFileOpened,
}

async fn delayed_refresh(change: Change) {
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path().join("project");
    let dependency = fixture.path().join("dependency/lib");
    std::fs::create_dir(&root).unwrap();
    std::fs::create_dir_all(&dependency).unwrap();
    std::fs::write(dependency.join("feature.rb"), "# dependency feature\n").unwrap();
    let path = root.join("caller.rb");
    let source = "require 'feature'\n";
    std::fs::write(&path, source).unwrap();
    let uri = Url::from_file_path(&path).unwrap();
    let root_uri = Url::from_directory_path(&root).unwrap();
    let filename = path.to_str().unwrap().trim_start_matches('/');
    let mut editor = FakeEditor::new().await;
    let server = editor.server().clone();
    let observe = || {
        server.last_diagnostic_publication(&uri).expect(
            "the real document handler must submit diagnostics, including explicit empty clears",
        )
    };
    let workspace = server.add_workspace(root_uri.clone());
    if matches!(change, Change::NewMissingRequire | Change::RootsRemoved) {
        workspace.set_dependency_require_paths(vec![dependency.clone()]);
    }
    editor.open(filename, source).await;
    let initial = observe();
    assert_eq!(
        initial.len(),
        usize::from(!matches!(
            change,
            Change::NewMissingRequire | Change::RootsRemoved
        ))
    );
    let source_snapshot = workspace
        .analysis_engine
        .read()
        .source_snapshot_for_path(&path);
    if matches!(change, Change::Close) {
        workspace.set_dependency_require_paths(vec![dependency.clone()]);
    }

    let mut collected = server
        .indexing
        .schedule
        .arm(Point::RequireRefreshCollected, path.clone());
    let refresh_server = server.clone();
    let refresh_workspace = workspace.clone();
    let refreshing = tokio::spawn(async move {
        refresh_server
            .refresh_unresolved_require_diagnostics_for_workspace(&refresh_workspace)
            .await;
    });
    collected.wait().await;
    match change {
        Change::CorrectingEdit => editor.set(filename, "# corrected source\n").await,
        Change::NewMissingRequire => editor.set(filename, "require 'absent'\n").await,
        Change::RootsAdded | Change::RootsRemoved => {
            workspace.set_dependency_require_paths(if matches!(change, Change::RootsAdded) {
                vec![dependency.clone()]
            } else {
                Vec::new()
            });
            server
                .refresh_unresolved_require_diagnostics_for_workspace(&workspace)
                .await;
            assert_eq!(
                workspace
                    .analysis_engine
                    .read()
                    .source_snapshot_for_path(&path),
                source_snapshot,
                "dependency-only changes must exercise unchanged consumer source identity"
            );
        }
        Change::WorkspaceReplaced => {
            server.remove_workspace(&root_uri);
            server.add_workspace(root_uri);
            editor.set(filename, "# corrected source\n").await;
            assert_eq!(
                workspace
                    .analysis_engine
                    .read()
                    .source_snapshot_for_path(&path),
                source_snapshot,
                "the retired engine must retain its old source, distinct from the new owner"
            );
        }
        Change::Close => editor.close(filename).await,
        Change::ProjectFileOpened => {
            let definition = root.join("lib/feature.rb");
            editor
                .open(
                    definition.to_str().unwrap().trim_start_matches('/'),
                    "# dependency feature\n",
                )
                .await;
            assert_eq!(
                workspace
                    .analysis_engine
                    .read()
                    .source_snapshot_for_path(&path),
                source_snapshot,
                "opening a required file must exercise unchanged consumer source identity"
            );
        }
    }
    let expected: Vec<Diagnostic> = observe();
    assert_eq!(
        expected.len(),
        usize::from(matches!(
            change,
            Change::NewMissingRequire | Change::RootsRemoved | Change::Close
        )),
        "the newer operation must establish the independently expected diagnostic count"
    );
    if !expected.is_empty() {
        let argument = if matches!(change, Change::NewMissingRequire) {
            "absent"
        } else {
            "feature"
        };
        assert_eq!(
            expected[0].message,
            format!("Cannot resolve require \"{argument}\"")
        );
    }
    let mut attempted = server
        .indexing
        .schedule
        .arm(Point::RequireRefreshAttempted, path.clone());
    collected.release();
    attempted.wait().await;
    assert_eq!(
        observe(),
        expected,
        "delayed dependency refresh must preserve the exact newer diagnostic publication"
    );
    let expected_facts = if matches!(change, Change::Close) {
        Vec::new()
    } else {
        expected.clone()
    };
    assert_eq!(EngineQuery::with_engine(server.analysis_engine_for_uri(&uri)).get_unresolved_diagnostics(&uri), expected_facts,
        "delayed dependency refresh must preserve current engine diagnostic facts as well as the UI");
    attempted.release();
    tokio::time::timeout(Duration::from_secs(15), refreshing)
        .await
        .expect("released dependency refresh must finish")
        .expect("dependency refresh must not panic");
    assert_eq!(observe(), expected);
    assert_eq!(
        server.indexing.schedule.trace(),
        vec![
            (Point::RequireRefreshCollected, path.clone()),
            (Point::RequireRefreshAttempted, path),
        ]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dependency_refresh_cannot_restore_a_require_removed_by_an_edit() {
    delayed_refresh(Change::CorrectingEdit).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dependency_refresh_cannot_clear_a_new_missing_require() {
    delayed_refresh(Change::NewMissingRequire).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dependency_refresh_rejects_results_from_superseded_roots() {
    delayed_refresh(Change::RootsAdded).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dependency_refresh_cannot_clear_an_error_after_roots_are_removed() {
    delayed_refresh(Change::RootsRemoved).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dependency_refresh_cannot_publish_into_a_replacement_workspace() {
    delayed_refresh(Change::WorkspaceReplaced).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dependency_refresh_updates_facts_without_publishing_after_close() {
    delayed_refresh(Change::Close).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dependency_refresh_uses_project_files_opened_after_collection() {
    delayed_refresh(Change::ProjectFileOpened).await;
}
