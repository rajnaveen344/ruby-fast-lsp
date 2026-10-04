use crate::loader::scheduling::status::{
    IndexingPhase, IndexingReuseSnapshot, IndexingStatusParams,
};
use crate::server::Server;
use tower_lsp::lsp_types::Url;

#[test]
fn indexing_snapshot_is_sorted_and_failure_aware() {
    let fixture = tempfile::tempdir().unwrap();
    let admin = fixture.path().join("admin");
    let server_project = fixture.path().join("server");
    std::fs::create_dir_all(&admin).unwrap();
    std::fs::create_dir_all(&server_project).unwrap();
    let language_server = Server::default();
    let server_workspace =
        language_server.add_workspace(Url::from_directory_path(&server_project).unwrap());
    let admin_workspace = language_server.add_workspace(Url::from_directory_path(&admin).unwrap());

    let admin_generation = admin_workspace
        .indexing_status
        .begin_generation()
        .generation;
    admin_workspace
        .indexing_status
        .transition(
            admin_generation,
            IndexingPhase::IndexingProject,
            Some(1),
            Some(2),
        )
        .expect("admin generation must accept project progress");
    let server_generation = server_workspace
        .indexing_status
        .begin_generation()
        .generation;
    server_workspace
        .indexing_status
        .fail(server_generation, "lockfile failed".to_string())
        .expect("server generation must accept failure");

    let snapshot = language_server.indexing_status_snapshot();
    assert_eq!(snapshot.projects[0].root, admin);
    assert_eq!(snapshot.projects[1].root, server_project);
    assert_eq!(snapshot.aggregate.failed, 1);
    assert_eq!(snapshot.aggregate.ready, 0);
    assert_eq!(
        snapshot.reuse,
        IndexingReuseSnapshot::default(),
        "a fresh server must report exact zero process-lifetime reuse counters"
    );
    assert!(!language_server.is_indexing_complete());
}

#[tokio::test]
async fn indexing_status_request_prioritizes_active_document_and_sequences_exact_snapshot() {
    let fixture = tempfile::tempdir().unwrap();
    let admin = fixture.path().join("admin");
    let server_project = fixture.path().join("server");
    std::fs::create_dir_all(&admin).unwrap();
    std::fs::create_dir_all(&server_project).unwrap();
    let language_server = Server::default();
    language_server.add_workspace(Url::from_directory_path(&admin).unwrap());
    let server_workspace =
        language_server.add_workspace(Url::from_directory_path(&server_project).unwrap());
    let active_document_uri = Url::from_file_path(server_project.join("lib/active.rb")).unwrap();

    let first = language_server
        .handle_indexing_status(IndexingStatusParams {
            active_document_uri: Some(active_document_uri),
        })
        .await
        .unwrap();
    let second = language_server
        .handle_indexing_status(IndexingStatusParams::default())
        .await
        .unwrap();

    assert_eq!(first.sequence, 1);
    assert_eq!(second.sequence, 2);
    assert_eq!(
        language_server
            .indexing
            .scheduler()
            .snapshot()
            .active_project,
        Some(server_project.clone())
    );
    assert_eq!(
        language_server
            .indexing
            .resources()
            .snapshot()
            .active_project,
        Some(server_project.clone())
    );
    assert!(
        language_server
            .indexing
            .resources()
            .snapshot()
            .active_project_navigation_pending,
        "a discovered active project must reserve its navigation-critical source pass"
    );

    let generation = server_workspace.indexing_status.begin_run().generation();
    server_workspace
        .indexing_status
        .transition(
            generation,
            IndexingPhase::ProjectNavigationReady,
            None,
            None,
        )
        .unwrap();
    language_server.prioritize_indexing_project(&server_project);
    assert!(
        !language_server
            .indexing
            .resources()
            .snapshot()
            .active_project_navigation_pending,
        "an already navigation-ready active project must not block sibling source passes"
    );
}

#[test]
fn default_project_concurrency_follows_the_top_level_task_budget() {
    let server = Server::default();

    assert_eq!(
        server.indexing_scheduler_snapshot().concurrency_limit,
        server.indexing_resource_policy().top_level_tasks(),
        "admitting more projects than top-level tasks only queues them in the governor"
    );
}
