use super::diagnostics::DiagnosticPublicationState;
use super::indexing::{
    IndexingStatusPublicationDecision, IndexingStatusPublicationState,
    INDEXING_COUNTER_PUBLICATION_INTERVAL,
};
use super::products::{CORE_ENGINE_CACHE_MAX_ENTRIES, CORE_ENGINE_CACHE_MAX_WEIGHT_BYTES};

use super::RubyLanguageServer;
use crate::config::runtime::{
    ProjectRuntimeSelection, RuntimeMode, RuntimeSelection, RuntimeSelectionConfig,
    SelectedRuntimeDescriptor,
};
use crate::config::RubyFastLspConfig;
use crate::indexing_status::{
    IndexingPhase, IndexingReuseSnapshot, IndexingSingleFlightReuseSnapshot, IndexingStatusParams,
    IndexingStatusSnapshot,
};
use crate::runtime::catalog::{
    DiscoveredRuntime, RuntimeDiscoverySource, RuntimeImplementation, RuntimeStatusParams,
    RuntimeSupportStatus,
};
use parking_lot::{Mutex, RwLock};
use ruby_analysis::engine::AnalysisEngine;
use ruby_analysis::indexer::RubyDocument;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tower_lsp::lsp_types::{Diagnostic, FileChangeType, FileEvent, Url};

#[test]
fn server_ownership_clones_share_document_locks_and_isolate_project_engines() {
    let server = RubyLanguageServer::default();
    let clone = server.clone();
    let uri = Url::parse("file:///ownership/entry.rb").unwrap();
    let other_uri = Url::parse("file:///ownership/other.rb").unwrap();
    let lock = server.document_semantic_lock(&uri);
    assert!(Arc::ptr_eq(&lock, &clone.document_semantic_lock(&uri)));
    assert!(!Arc::ptr_eq(
        &lock,
        &clone.document_semantic_lock(&other_uri)
    ));

    let first = server.add_workspace(Url::parse("file:///ownership/").unwrap());
    let second = clone.add_workspace(Url::parse("file:///neighbor/").unwrap());
    assert_eq!(server.list_workspaces().len(), 2);
    assert!(Arc::ptr_eq(
        &first.analysis_engine,
        &clone.analysis_engine_for_uri(&uri)
    ));
    assert!(!Arc::ptr_eq(
        &first.analysis_engine,
        &second.analysis_engine
    ));
    let orphan = Url::parse("file:///loose.rb").unwrap();
    assert!(Arc::ptr_eq(
        &server.analysis_engine_for_uri(&orphan),
        &clone.analysis_engine_for_uri(&orphan)
    ));
    assert!(!Arc::ptr_eq(
        &first.analysis_engine,
        &clone.analysis_engine_for_uri(&orphan)
    ));
    clone.remove_workspace(&first.root_uri);
    assert!(Arc::ptr_eq(
        &server.analysis_engine_for_uri(&uri),
        &server.analysis_engine_for_uri(&orphan)
    ));
}

#[tokio::test]
async fn server_ownership_client_construction_defers_extension_discovery() {
    let (service, _socket) = tower_lsp::LspService::new(|client| {
        RubyLanguageServer::new(client).expect("construct client server")
    });
    assert!(service.inner().client.is_some());
    assert!(service.inner().extension_status_reports().is_empty());
    assert!(service.inner().list_workspaces().is_empty());
}

#[tokio::test]
async fn core_engine_template_retention_is_bounded_by_entries_and_estimated_heap() {
    let server = RubyLanguageServer::default();
    for index in 0..(CORE_ENGINE_CACHE_MAX_ENTRIES + 2) {
        server
            .products
            .core_templates()
            .get_or_try_init(format!("core-{index}"), || async {
                Ok(ruby_analysis::engine::AnalysisEngine::new())
            })
            .await
            .unwrap();
    }

    let products = server.runtime_product_snapshot();
    assert!(
        products.core_templates.reuse.entries <= CORE_ENGINE_CACHE_MAX_ENTRIES,
        "completed core templates must evict to the server-owned entry bound"
    );
    assert!(
        products.core_templates.retained_weight_bytes <= CORE_ENGINE_CACHE_MAX_WEIGHT_BYTES,
        "completed core templates must remain within the server-owned estimated-heap bound"
    );
}

#[test]
fn indexing_snapshot_is_sorted_and_failure_aware() {
    let fixture = tempfile::tempdir().unwrap();
    let admin = fixture.path().join("admin");
    let server_project = fixture.path().join("server");
    std::fs::create_dir_all(&admin).unwrap();
    std::fs::create_dir_all(&server_project).unwrap();
    let language_server = RubyLanguageServer::default();
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

#[test]
fn indexing_snapshot_reports_process_local_classpath_file_reuse() {
    let fixture = tempfile::tempdir().unwrap();
    let project = fixture.path().join("project");
    let jruby = fixture.path().join("jruby");
    let java_home = fixture.path().join("jdk");
    for (path, bytes) in [
        (jruby.join("bin/jruby"), b"jruby".as_slice()),
        (jruby.join("lib/jruby.jar"), b"runtime".as_slice()),
        (
            java_home.join("jmods/java.base.jmod"),
            b"java base".as_slice(),
        ),
    ] {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
    std::fs::create_dir_all(&project).unwrap();
    let inputs = crate::runtime::jruby::classpath::ClasspathInputs {
        project_root: project,
        jruby_executable: jruby.join("bin/jruby"),
        java_home,
        maven_repository: None,
        java_gem_roots: Vec::new(),
        additional_classpath: Vec::new(),
        additional_sources: Vec::new(),
    };
    let server = RubyLanguageServer::default();
    for _ in 0..2 {
        crate::runtime::jruby::classpath::discover_project_classpath_with_cache(
            &inputs,
            crate::runtime::jruby::classpath::ClasspathLimits::default(),
            &server.products.classpath_files(),
        )
        .unwrap();
    }

    assert_eq!(
        server
            .indexing_status_snapshot()
            .reuse
            .classpath_file_single_flight,
        IndexingSingleFlightReuseSnapshot {
            lookups: 4,
            hits: 2,
            joined_flights: 0,
            producers: 2,
            failures: 0,
        }
    );
}

#[test]
fn diagnostic_publication_queue_keeps_only_the_latest_per_uri() {
    let mut publication = DiagnosticPublicationState::default();
    let first = Url::parse("file:///project/a.rb").unwrap();
    let second = Url::parse("file:///project/b.rb").unwrap();
    assert!(publication.queue(first.clone(), Vec::new()));
    assert!(!publication.queue(first.clone(), vec![Diagnostic::default()]));
    assert!(!publication.queue(second.clone(), Vec::new()));
    let (uri, diagnostics) = publication
        .take_next()
        .expect("first URI must stay pending");
    assert_eq!(uri, first);
    assert_eq!(
        diagnostics.len(),
        1,
        "latest diagnostics for a URI must win"
    );
    let (uri, diagnostics) = publication
        .take_next()
        .expect("second URI must stay pending");
    assert_eq!(uri, second);
    assert!(diagnostics.is_empty());
    assert!(publication.take_next().is_none());
}

#[test]
fn indexing_status_send_queue_keeps_only_the_latest_pending_snapshot() {
    let language_server = RubyLanguageServer::default();
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
async fn semantic_convergence_refreshes_only_projects_with_open_documents() {
    use futures::{SinkExt, StreamExt};
    use serde_json::{json, Value};
    use tower::{Service, ServiceExt};
    use tower_lsp::jsonrpc::{Request, Response};
    use tower_lsp::LspService;

    let (mut service, mut socket) = LspService::new(|client| {
        RubyLanguageServer::new(client).expect("test language server must initialize")
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

    let fixture = tempfile::tempdir().unwrap();
    let open_project = fixture.path().join("open_project");
    let closed_project = fixture.path().join("closed_project");
    std::fs::create_dir_all(&open_project).unwrap();
    std::fs::create_dir_all(&closed_project).unwrap();
    let language_server = service.inner().clone();
    language_server.add_workspace(Url::from_directory_path(&open_project).unwrap());
    language_server.add_workspace(Url::from_directory_path(&closed_project).unwrap());
    let open_uri = Url::from_file_path(open_project.join("consumer.rb")).unwrap();
    language_server.documents.insert(
        open_uri.clone(),
        Arc::new(RwLock::new(RubyDocument::new(
            open_uri,
            "value = Shared::LABEL\n".to_string(),
            1,
        ))),
    );

    language_server
        .refresh_inlay_hints_for_workspace(&closed_project)
        .await;
    assert!(
        tokio::time::timeout(Duration::from_millis(25), socket.next())
            .await
            .is_err(),
        "a project with no open document must not issue a global inlay refresh"
    );

    let refresh_server = language_server.clone();
    let open_project_for_refresh = open_project.clone();
    let refresh = tokio::spawn(async move {
        refresh_server
            .refresh_inlay_hints_for_workspace(&open_project_for_refresh)
            .await;
    });
    let request = tokio::time::timeout(Duration::from_secs(1), socket.next())
        .await
        .expect("an open project's semantic convergence must issue a refresh")
        .expect("client socket must remain open");
    assert_eq!(request.method(), "workspace/inlayHint/refresh");
    let id = request
        .id()
        .cloned()
        .expect("inlay refresh must be a request with an id");
    socket
        .send(Response::from_ok(id, Value::Null))
        .await
        .unwrap();
    refresh.await.unwrap();
}

#[tokio::test]
async fn multi_project_phase_storm_publishes_latest_wins_without_blocking_callers() {
    use futures::StreamExt;
    use serde_json::json;
    use tower::{Service, ServiceExt};
    use tower_lsp::jsonrpc::Request;
    use tower_lsp::LspService;

    let (mut service, mut socket) = LspService::new(|client| {
        RubyLanguageServer::new(client).expect("test language server must initialize")
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
    let language_server = RubyLanguageServer::default();
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

#[test]
fn watched_file_batches_keep_only_the_latest_event_per_uri_and_generation() {
    let fixture = tempfile::tempdir().unwrap();
    let admin = Url::from_file_path(fixture.path().join("admin.rb")).unwrap();
    let server_file = Url::from_file_path(fixture.path().join("server.rb")).unwrap();
    let language_server = RubyLanguageServer::default();

    let first = language_server.queue_watched_file_changes(vec![
        FileEvent {
            uri: server_file.clone(),
            typ: FileChangeType::CREATED,
        },
        FileEvent {
            uri: admin.clone(),
            typ: FileChangeType::CHANGED,
        },
    ]);
    let replacement = language_server.queue_watched_file_changes(vec![
        FileEvent {
            uri: server_file.clone(),
            typ: FileChangeType::DELETED,
        },
        FileEvent {
            uri: admin.clone(),
            typ: FileChangeType::CHANGED,
        },
    ]);

    assert!(
        language_server.take_watched_file_changes(first).is_none(),
        "an older debounce generation must never process a partial filesystem state"
    );
    let changes = language_server
        .take_watched_file_changes(replacement)
        .expect("the newest debounce generation must own the complete normalized batch");
    assert_eq!(
        changes,
        vec![
            FileEvent {
                uri: admin,
                typ: FileChangeType::CHANGED,
            },
            FileEvent {
                uri: server_file,
                typ: FileChangeType::DELETED,
            },
        ]
    );
    assert!(
        language_server
            .take_watched_file_changes(replacement)
            .is_none(),
        "a normalized watcher batch must be consumed exactly once"
    );
}

#[tokio::test]
async fn indexing_status_request_prioritizes_active_document_and_sequences_exact_snapshot() {
    let fixture = tempfile::tempdir().unwrap();
    let admin = fixture.path().join("admin");
    let server_project = fixture.path().join("server");
    std::fs::create_dir_all(&admin).unwrap();
    std::fs::create_dir_all(&server_project).unwrap();
    let language_server = RubyLanguageServer::default();
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

#[tokio::test]
async fn counter_storm_emits_one_bounded_client_flush_and_immediate_phase_transition() {
    use futures::StreamExt;
    use serde_json::json;
    use tower::{Service, ServiceExt};
    use tower_lsp::jsonrpc::Request;
    use tower_lsp::LspService;

    let (mut service, mut socket) = LspService::new(|client| {
        RubyLanguageServer::new(client).expect("test language server must initialize")
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
    let mut language_server = RubyLanguageServer::default();
    language_server.indexing.set_resources(
        crate::indexing_resources::IndexingResourceGovernor::new(
            crate::indexing_resources::IndexingResourcePolicy::with_limits(1, 1, 100, 1),
        ),
    );
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
                crate::indexing_resources::IndexingWorkSpec::new(
                    Some(cancelled_root),
                    crate::indexing_resources::IndexingResourcePriority::Background,
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

#[tokio::test]
async fn runtime_status_reports_server_owned_project_identity_and_classpath() {
    let fixture = tempfile::tempdir().unwrap();
    let admin = fixture.path().join("admin");
    let server_project = fixture.path().join("server");
    std::fs::create_dir_all(&admin).unwrap();
    std::fs::create_dir_all(&server_project).unwrap();
    let language_server = RubyLanguageServer::default();
    let admin_workspace = language_server.add_workspace(Url::from_directory_path(&admin).unwrap());
    language_server.add_workspace(Url::from_directory_path(&server_project).unwrap());
    *language_server.config.lock() = RubyFastLspConfig {
        runtime: RuntimeSelectionConfig {
            mode: RuntimeMode::Auto,
            projects: vec![ProjectRuntimeSelection {
                root: admin.to_string_lossy().to_string(),
                selection: RuntimeSelection::Explicit(SelectedRuntimeDescriptor {
                    implementation: RuntimeImplementation::Jruby,
                    family: "9.2".to_string(),
                    engine_version: "9.2.21.0".to_string(),
                    compatibility_version: "2.5".to_string(),
                    executable: fixture.path().join("jruby/bin/jruby"),
                    discovery_source: RuntimeDiscoverySource::Rvm,
                    java_home: Some(fixture.path().join("jdk")),
                }),
            }],
        },
        ..RubyFastLspConfig::default()
    };
    language_server.set_runtime_classpath_fingerprint(&admin, Some("a".repeat(64)));
    let generation = admin_workspace
        .indexing_status
        .begin_generation()
        .generation;
    admin_workspace
        .indexing_status
        .transition(
            generation,
            crate::indexing_status::IndexingPhase::Ready,
            None,
            None,
        )
        .expect("test workspace must transition to ready");

    let status = language_server
        .handle_runtime_status(RuntimeStatusParams {
            project_root: Some(admin.clone()),
        })
        .await
        .unwrap();

    assert_eq!(status.projects.len(), 1);
    let status = &status.projects[0];
    assert_eq!(status.root, admin);
    assert_eq!(status.mode, "explicit");
    assert_eq!(status.implementation, Some(RuntimeImplementation::Jruby));
    assert_eq!(status.family.as_deref(), Some("9.2"));
    assert_eq!(status.engine_version.as_deref(), Some("9.2.21.0"));
    assert_eq!(status.compatibility_version.as_deref(), Some("2.5"));
    assert_eq!(status.stub_overlay.as_deref(), Some("9.2"));
    assert_eq!(
        status.classpath_fingerprint_sha256.as_deref(),
        Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
    );
    assert!(status.indexing_complete);
    assert_eq!(
        status.indexing.phase,
        crate::indexing_status::IndexingPhase::Ready
    );

    let auto_runtime = SelectedRuntimeDescriptor {
        implementation: RuntimeImplementation::Mri,
        family: "3.3".to_string(),
        engine_version: "3.3.11".to_string(),
        compatibility_version: "3.3".to_string(),
        executable: fixture.path().join("ruby-3.3.11/bin/ruby"),
        discovery_source: RuntimeDiscoverySource::Rbenv,
        java_home: None,
    };
    language_server.set_effective_runtime(&server_project, Some(auto_runtime));
    let auto = language_server
        .handle_runtime_status(RuntimeStatusParams {
            project_root: Some(server_project),
        })
        .await
        .unwrap();
    let auto = &auto.projects[0];
    assert_eq!(auto.mode, "auto");
    assert_eq!(auto.implementation, Some(RuntimeImplementation::Mri));
    assert_eq!(auto.engine_version.as_deref(), Some("3.3.11"));
    assert_eq!(
        auto.executable,
        Some(fixture.path().join("ruby-3.3.11/bin/ruby"))
    );
}

#[tokio::test]
async fn auto_runtime_resolves_exact_project_marker_through_server_catalog() {
    let fixture = tempfile::tempdir().unwrap();
    let admin = fixture.path().join("admin");
    std::fs::create_dir_all(&admin).unwrap();
    std::fs::write(admin.join(".ruby-version"), "jruby-9.2.21.0\n").unwrap();
    let executable = fixture.path().join("jruby-9.2.21.0/bin/jruby");
    let java_home = fixture.path().join("jdk-17");
    let language_server = RubyLanguageServer::default();
    language_server.add_workspace(Url::from_directory_path(&admin).unwrap());
    language_server.set_discovered_runtimes_for_tests(vec![DiscoveredRuntime {
        implementation: RuntimeImplementation::Jruby,
        implementation_label: "JRuby".to_string(),
        family: "9.2".to_string(),
        family_label: "JRuby 9.2 (Ruby 2.5)".to_string(),
        compatibility_version: "2.5".to_string(),
        compatibility_label: "Ruby 2.5".to_string(),
        engine_version: "9.2.21.0".to_string(),
        display_name: "JRuby 9.2.21.0 (Ruby 2.5)".to_string(),
        executable: executable.clone(),
        discovery_source: RuntimeDiscoverySource::Rvm,
        support_status: RuntimeSupportStatus::Supported,
        java_home: Some(java_home.clone()),
    }]);

    let resolved = language_server
        .resolve_auto_runtime(&admin)
        .await
        .unwrap()
        .expect("the exact installed JRuby marker must resolve");
    assert_eq!(resolved.engine_version, "9.2.21.0");
    assert_eq!(resolved.compatibility_version, "2.5");
    assert_eq!(resolved.executable, executable);
    assert_eq!(resolved.java_home, Some(java_home));
}

#[tokio::test]
async fn server_ownership_clones_reuse_products_with_one_ordinary_cache_root() {
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path().join("cache");
    let server = RubyLanguageServer::with_user_cache_root(root.clone()).unwrap();
    let clone = server.clone();
    assert_eq!(clone.products.cache_root(), root);
    let first = server
        .products
        .core_templates()
        .get_or_try_init("shared-core".to_string(), || async {
            Ok(AnalysisEngine::new())
        })
        .await
        .unwrap();
    let second = clone
        .products
        .core_templates()
        .get_or_try_init("shared-core".to_string(), || async {
            Err("a server clone must reuse the existing core product".to_string())
        })
        .await
        .unwrap();
    assert!(Arc::ptr_eq(&first, &second));
    let products = server.runtime_product_snapshot();
    assert_eq!(products.core_templates.reuse.lookups, 2);
    assert_eq!(products.core_templates.reuse.producers, 1);
    assert_eq!(products.core_templates.reuse.hits, 1);
    assert_eq!(clone.runtime_product_snapshot(), products);
}

#[tokio::test]
async fn server_ownership_fake_editor_receives_real_diagnostic_messages_and_clears() {
    let mut editor = crate::test::harness::FakeEditor::new().await;
    assert!(editor.server().client.is_some());
    editor
        .open("delivery.rb", "value = nil\nvalue.upcase\n")
        .await;
    let diagnostics = editor.diagnostics("delivery.rb").await;
    assert!(diagnostics.iter().any(|item| item.code
        == Some(tower_lsp::lsp_types::NumberOrString::String(
            "nil-call".to_string()
        ))));
    let count = editor.delivered_diagnostic_notifications();
    assert!(count > 0);
    editor
        .set("delivery.rb", "value = \"ready\"\nvalue.upcase\n")
        .await;
    assert!(editor.diagnostics("delivery.rb").await.is_empty());
    assert!(editor.delivered_diagnostic_notifications() > count);
}
