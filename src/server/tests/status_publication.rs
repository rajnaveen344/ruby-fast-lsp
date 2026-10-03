use crate::server::indexing::{
    IndexingStatusPublicationDecision, IndexingStatusPublicationState,
    INDEXING_COUNTER_PUBLICATION_INTERVAL,
};
use crate::utils::admission;

use crate::loader::scheduling::status::{
    IndexingPhase, IndexingStatusParams, IndexingStatusSnapshot,
};
use crate::server::Server;
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tower_lsp::lsp_types::Url;

#[test]
fn indexing_status_send_queue_keeps_only_the_latest_pending_snapshot() {
    let language_server = Server::default();
    let mut publication = IndexingStatusPublicationState::default();
    let first = language_server
        .sequence_indexing_status_snapshot(language_server.indexing_status_snapshot());
    assert!(
        publication.queue_send(first.clone()),
        "the first queued snapshot must schedule the sender"
    );
    let second = language_server
        .sequence_indexing_status_snapshot(language_server.indexing_status_snapshot());
    assert!(
        !publication.queue_send(second.clone()),
        "a later snapshot must replace pending state without scheduling a second sender"
    );
    let pending = publication
        .take_pending_send()
        .expect("latest snapshot must remain pending");
    assert_eq!(pending.sequence, second.sequence);
    assert!(pending.sequence > first.sequence);
    assert!(
        publication.take_pending_send().is_none(),
        "taking the pending snapshot must clear the sender schedule"
    );
}

#[tokio::test]
async fn multi_project_phase_storm_publishes_latest_wins_without_blocking_callers() {
    use futures::StreamExt;
    use serde_json::json;
    use tower::{Service, ServiceExt};
    use tower_lsp::jsonrpc::Request;
    use tower_lsp::LspService;

    let (mut service, mut socket) = LspService::new(|client| {
        Server::new(client).expect("test language server must initialize")
    });
    let initialize = Request::build("initialize")
        .params(json!({"capabilities": {}}))
        .id(1)
        .finish();
    service
        .ready()
        .await
        .unwrap()
        .call(initialize)
        .await
        .unwrap()
        .expect("initialize must return a response");

    let received = Arc::new(Mutex::new(Vec::<IndexingStatusSnapshot>::new()));
    let received_by_reader = received.clone();
    let socket_reader = tokio::spawn(async move {
        while let Some(request) = socket.next().await {
            if request.method() == "ruby-fast-lsp/indexing/statusChanged" {
                received_by_reader.lock().push(
                    serde_json::from_value::<IndexingStatusSnapshot>(
                        request
                            .params()
                            .cloned()
                            .expect("status notification must carry parameters"),
                    )
                    .expect("status notification must carry a valid complete snapshot"),
                );
            }
        }
    });

    let fixture = tempfile::tempdir().unwrap();
    let language_server = service.inner().clone();
    let mut workspaces = Vec::new();
    for index in 0..8 {
        let project = fixture.path().join(format!("project-{index}"));
        std::fs::create_dir_all(&project).unwrap();
        workspaces.push(language_server.add_workspace(Url::from_directory_path(&project).unwrap()));
    }

    let publish_started = Instant::now();
    for workspace in &workspaces {
        let run = workspace.indexing_status.begin_run();
        for phase in [
            IndexingPhase::IndexingProject,
            IndexingPhase::ProjectNavigationReady,
            IndexingPhase::IndexingDependencies,
            IndexingPhase::DependencyNavigationReady,
            IndexingPhase::Ready,
        ] {
            workspace
                .indexing_status
                .transition(run.generation(), phase, None, None)
                .unwrap();
            language_server.publish_indexing_status().await;
        }
    }
    assert!(
        publish_started.elapsed() < Duration::from_millis(200),
        "status publication must not await client IO on the caller; elapsed={:?}",
        publish_started.elapsed()
    );

    tokio::time::sleep(Duration::from_millis(150)).await;
    socket_reader.abort();
    let _ = socket_reader.await;
    let snapshots = received.lock().clone();
    assert!(
        !snapshots.is_empty(),
        "at least the latest indexing snapshot must reach the client"
    );
    assert!(
        snapshots.len() < 40,
        "latest-wins publication must drop intermediate multi-project phase storms, got {}",
        snapshots.len()
    );
    assert!(
        snapshots
            .last()
            .unwrap()
            .projects
            .iter()
            .all(|project| project.phase == IndexingPhase::Ready),
        "the final delivered snapshot must reflect the latest ready state"
    );
    assert!(
        snapshots
            .windows(2)
            .all(|pair| pair[0].sequence < pair[1].sequence),
        "client-visible status sequence must remain strictly monotonic"
    );
}

#[test]
fn counter_only_status_publication_is_bounded_while_phase_changes_are_immediate() {
    let fixture = tempfile::tempdir().unwrap();
    let project = fixture.path().join("server");
    std::fs::create_dir_all(&project).unwrap();
    let language_server = Server::default();
    let workspace = language_server.add_workspace(Url::from_directory_path(&project).unwrap());
    let run = workspace.indexing_status.begin_run();
    workspace
        .indexing_status
        .transition(
            run.generation(),
            IndexingPhase::IndexingProject,
            Some(0),
            Some(100),
        )
        .unwrap();

    let mut publication = IndexingStatusPublicationState::default();
    assert_eq!(
        publication.observe(&language_server.indexing_status_snapshot()),
        IndexingStatusPublicationDecision::Immediate
    );

    workspace
        .indexing_status
        .transition(
            run.generation(),
            IndexingPhase::IndexingProject,
            Some(1),
            Some(100),
        )
        .unwrap();
    assert_eq!(
        publication.observe(&language_server.indexing_status_snapshot()),
        IndexingStatusPublicationDecision::ScheduleCounterFlush
    );
    for completed in 2..=50 {
        workspace
            .indexing_status
            .transition(
                run.generation(),
                IndexingPhase::IndexingProject,
                Some(completed),
                Some(100),
            )
            .unwrap();
        assert_eq!(
            publication.observe(&language_server.indexing_status_snapshot()),
            IndexingStatusPublicationDecision::Coalesced
        );
    }
    assert!(publication.flush_counter(&language_server.indexing_status_snapshot()));
    assert!(!publication.flush_counter(&language_server.indexing_status_snapshot()));

    workspace
        .indexing_status
        .transition(
            run.generation(),
            IndexingPhase::ProjectNavigationReady,
            None,
            None,
        )
        .unwrap();
    assert_eq!(
        publication.observe(&language_server.indexing_status_snapshot()),
        IndexingStatusPublicationDecision::Immediate
    );
    assert!(
        !publication.flush_counter(&language_server.indexing_status_snapshot()),
        "an immediate phase publication must cancel the stale counter flush"
    );

    let replacement = workspace.indexing_status.begin_run();
    assert_eq!(
        publication.observe(&language_server.indexing_status_snapshot()),
        IndexingStatusPublicationDecision::Immediate,
        "a replacement generation must publish immediately even when its phase repeats"
    );
    workspace
        .indexing_status
        .fail(
            replacement.generation(),
            "runtime replacement failed".to_string(),
        )
        .unwrap();
    assert_eq!(
        publication.observe(&language_server.indexing_status_snapshot()),
        IndexingStatusPublicationDecision::Immediate,
        "terminal failures must bypass counter throttling"
    );
}

#[tokio::test]
async fn counter_storm_emits_one_bounded_client_flush_and_immediate_phase_transition() {
    use futures::StreamExt;
    use serde_json::json;
    use tower::{Service, ServiceExt};
    use tower_lsp::jsonrpc::Request;
    use tower_lsp::LspService;

    let (mut service, mut socket) = LspService::new(|client| {
        Server::new(client).expect("test language server must initialize")
    });
    let initialize = Request::build("initialize")
        .params(json!({"capabilities": {}}))
        .id(1)
        .finish();
    service
        .ready()
        .await
        .unwrap()
        .call(initialize)
        .await
        .unwrap()
        .expect("initialize must return a response");

    let received = Arc::new(Mutex::new(Vec::<IndexingStatusSnapshot>::new()));
    let received_by_reader = received.clone();
    let socket_reader = tokio::spawn(async move {
        while let Some(request) = socket.next().await {
            if request.method() == "ruby-fast-lsp/indexing/statusChanged" {
                received_by_reader.lock().push(
                    serde_json::from_value::<IndexingStatusSnapshot>(
                        request
                            .params()
                            .cloned()
                            .expect("status notification must carry parameters"),
                    )
                    .expect("status notification must carry a valid complete snapshot"),
                );
            }
        }
    });

    let fixture = tempfile::tempdir().unwrap();
    let project = fixture.path().join("server");
    std::fs::create_dir_all(&project).unwrap();
    let language_server = service.inner().clone();
    let workspace = language_server.add_workspace(Url::from_directory_path(&project).unwrap());
    let run = workspace.indexing_status.begin_run();
    workspace
        .indexing_status
        .transition(
            run.generation(),
            IndexingPhase::IndexingProject,
            Some(0),
            Some(50),
        )
        .unwrap();
    language_server.publish_indexing_status().await;

    for completed in 1..=50 {
        workspace
            .indexing_status
            .transition(
                run.generation(),
                IndexingPhase::IndexingProject,
                Some(completed),
                Some(50),
            )
            .unwrap();
        language_server.publish_indexing_status().await;
    }
    tokio::time::sleep(INDEXING_COUNTER_PUBLICATION_INTERVAL + Duration::from_millis(50)).await;

    workspace
        .indexing_status
        .transition(
            run.generation(),
            IndexingPhase::ProjectNavigationReady,
            None,
            None,
        )
        .unwrap();
    language_server.publish_indexing_status().await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    socket_reader.abort();
    let _ = socket_reader.await;
    let snapshots = received.lock().clone();

    assert_eq!(
            snapshots.len(),
            3,
            "fifty counter changes must emit the initial state, one bounded counter flush, and one immediate phase transition"
        );
    assert!(
        snapshots
            .windows(2)
            .all(|pair| pair[0].sequence < pair[1].sequence),
        "client-visible status sequence must remain strictly monotonic"
    );
    assert_eq!(
        snapshots.last().unwrap().projects[0].phase,
        IndexingPhase::ProjectNavigationReady,
        "the phase transition must bypass counter throttling"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn saturated_indexing_keeps_status_switch_and_queued_cancellation_responsive() {
    let fixture = tempfile::tempdir().unwrap();
    let admin = fixture.path().join("admin");
    let server_project = fixture.path().join("server");
    std::fs::create_dir_all(&admin).unwrap();
    std::fs::create_dir_all(&server_project).unwrap();
    let mut language_server = Server::default();
    language_server
        .indexing
        .set_resources(admission::IndexingResourceGovernor::new(
            admission::IndexingResourcePolicy::with_limits(1, 1, 100, 1),
        ));
    language_server.add_workspace(Url::from_directory_path(&admin).unwrap());
    language_server.add_workspace(Url::from_directory_path(&server_project).unwrap());

    let (holder_started_tx, holder_started_rx) = tokio::sync::oneshot::channel();
    let (holder_release_tx, holder_release_rx) = std::sync::mpsc::channel();
    let holder_resources = language_server.indexing.resources().clone();
    let holder = tokio::spawn(async move {
        holder_resources
            .run_cpu("saturated status holder", move || {
                holder_started_tx.send(()).unwrap();
                holder_release_rx.recv().unwrap();
            })
            .await
            .unwrap();
    });
    holder_started_rx.await.unwrap();

    let cancellation = tokio_util::sync::CancellationToken::new();
    let cancelled_resources = language_server.indexing.resources().clone();
    let cancelled_token = cancellation.clone();
    let cancelled_root = admin.clone();
    let cancelled = tokio::spawn(async move {
        cancelled_resources
            .run_with_resources(
                "cancelled saturated waiter",
                admission::IndexingWorkSpec::new(
                    Some(cancelled_root),
                    admission::IndexingResourcePriority::Background,
                    1,
                    1,
                    0,
                ),
                Some(cancelled_token),
                || (),
            )
            .await
    });
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while language_server.indexing.resources().snapshot().queued_tasks != 1 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the cancellable waiter must queue behind saturated indexing");

    let active_document_uri = Url::from_file_path(server_project.join("lib/active.rb")).unwrap();
    let status = tokio::time::timeout(
        std::time::Duration::from_millis(100),
        language_server.handle_indexing_status(IndexingStatusParams {
            active_document_uri: Some(active_document_uri),
        }),
    )
    .await
    .expect("status and active-editor routing must remain available within 100 ms")
    .unwrap();
    assert_eq!(status.sequence, 1);
    assert_eq!(
        language_server
            .indexing
            .resources()
            .snapshot()
            .active_project,
        Some(server_project)
    );

    cancellation.cancel();
    let cancellation_error = tokio::time::timeout(std::time::Duration::from_millis(100), cancelled)
        .await
        .expect("queued cancellation must remain available within 100 ms")
        .unwrap()
        .unwrap_err();
    assert!(cancellation_error
        .to_string()
        .contains("cancelled before entering"));
    assert_eq!(
        language_server.indexing.resources().snapshot().queued_tasks,
        0
    );

    holder_release_tx.send(()).unwrap();
    holder.await.unwrap();
}
