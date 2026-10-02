//! JRuby runtime companion materialization and resource reservations.

use super::*;

#[test]
fn mri_runtime_does_not_materialize_a_jruby_import_provider() {
    let fixture = TempDir::new().unwrap();
    let root = fixture.path().join("tool");
    fs::create_dir_all(&root).unwrap();
    let executable = root.join("ruby");
    fs::write(&executable, b"fixture").unwrap();
    let runtime = SelectedRuntimeDescriptor {
        implementation: RuntimeImplementation::Mri,
        family: "3.3".to_string(),
        engine_version: "3.3.11".to_string(),
        compatibility_version: "3.3".to_string(),
        executable,
        discovery_source: RuntimeDiscoverySource::Path,
        java_home: None,
    };
    let (provider, archive) = build_jruby_import_provider(
        root,
        RubyFastLspConfig::default(),
        Some(runtime),
        None,
        None,
        None,
        None,
    )
    .unwrap();
    assert!(
        provider.is_none(),
        "MRI Gemfile roots must skip JRuby classpath and Java catalog work"
    );
    assert!(archive.is_none());
}

#[tokio::test(flavor = "current_thread")]
async fn jruby_runtime_companion_overlaps_the_active_project_with_exact_resource_claims() {
    let root = PathBuf::from("/workspace/server");
    let mut server = RubyLanguageServer::default();
    server
        .indexing
        .set_resources(admission::IndexingResourceGovernor::new(
            admission::IndexingResourcePolicy::with_limits(6, 2, 512 * MIB, 2),
        ));
    server
        .indexing
        .resources()
        .prioritize_active_project_with_navigation_pending(&root, true);
    let server = Arc::new(server);
    let (started_tx, mut started_rx) = tokio::sync::mpsc::channel(2);
    let (runtime_release_tx, runtime_release_rx) = std::sync::mpsc::channel();
    let (project_release_tx, project_release_rx) = std::sync::mpsc::channel();

    let runtime = {
        let server = server.clone();
        let root = root.clone();
        let started_tx = started_tx.clone();
        tokio::spawn(async move {
            run_cpu_indexing_task(
                server.indexing.resources(),
                Some(root),
                None,
                IndexingWorkClass::RuntimeCompanionParallelIo,
                "fixture JRuby catalog",
                move || {
                    started_tx.blocking_send("runtime").unwrap();
                    runtime_release_rx.recv().unwrap();
                },
            )
            .await
            .unwrap();
        })
    };
    let project = {
        let server = server.clone();
        let root = root.clone();
        tokio::spawn(async move {
            run_cpu_indexing_task(
                server.indexing.resources(),
                Some(root),
                None,
                IndexingWorkClass::ProjectParallelIo,
                "fixture project pass",
                move || {
                    started_tx.blocking_send("project").unwrap();
                    project_release_rx.recv().unwrap();
                },
            )
            .await
            .unwrap();
        })
    };

    let first = tokio::time::timeout(Duration::from_secs(1), started_rx.recv())
        .await
        .expect("one indexing phase must start")
        .expect("start channel must remain open");
    let second = tokio::time::timeout(Duration::from_secs(1), started_rx.recv())
        .await
        .expect("the companion and project pass must overlap")
        .expect("start channel must remain open");
    assert_ne!(first, second);
    let snapshot = server.indexing.resources().snapshot();
    assert_eq!(snapshot.active_tasks, 2);
    assert_eq!(snapshot.active_cpu_lanes, 6);
    assert_eq!(snapshot.active_transient_memory_bytes, 512 * MIB);
    assert_eq!(snapshot.active_io_slots, 2);

    runtime_release_tx.send(()).unwrap();
    project_release_tx.send(()).unwrap();
    runtime.await.unwrap();
    project.await.unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn active_navigation_reservation_blocks_a_sibling_runtime_companion() {
    let active_root = PathBuf::from("/workspace/server");
    let sibling_root = PathBuf::from("/workspace/admin");
    let mut server = RubyLanguageServer::default();
    server
        .indexing
        .set_resources(admission::IndexingResourceGovernor::new(
            admission::IndexingResourcePolicy::with_limits(6, 2, 512 * MIB, 2),
        ));
    server
        .indexing
        .resources()
        .prioritize_active_project_with_navigation_pending(&active_root, true);
    let server = Arc::new(server);
    let (active_started_tx, active_started_rx) = tokio::sync::oneshot::channel();
    let (active_release_tx, active_release_rx) = std::sync::mpsc::channel();
    let active = {
        let server = server.clone();
        let active_root = active_root.clone();
        tokio::spawn(async move {
            run_cpu_indexing_task(
                server.indexing.resources(),
                Some(active_root),
                None,
                IndexingWorkClass::RuntimeCompanionParallelIo,
                "active runtime companion",
                move || {
                    active_started_tx.send(()).unwrap();
                    active_release_rx.recv().unwrap();
                },
            )
            .await
            .unwrap();
        })
    };
    active_started_rx.await.unwrap();

    let (sibling_started_tx, sibling_started_rx) = tokio::sync::oneshot::channel();
    let sibling = {
        let server = server.clone();
        tokio::spawn(async move {
            run_cpu_indexing_task(
                server.indexing.resources(),
                Some(sibling_root),
                None,
                IndexingWorkClass::RuntimeCompanionParallelIo,
                "sibling runtime companion",
                move || {
                    sibling_started_tx.send(()).unwrap();
                },
            )
            .await
            .unwrap();
        })
    };
    let mut sibling_started_rx = sibling_started_rx;
    assert!(
        tokio::time::timeout(Duration::from_millis(50), &mut sibling_started_rx)
            .await
            .is_err(),
        "a sibling runtime companion must stay queued while active-project navigation is pending"
    );

    server
        .indexing
        .resources()
        .prioritize_active_project_with_navigation_pending(&active_root, false);
    tokio::time::timeout(Duration::from_secs(1), sibling_started_rx)
        .await
        .expect("the sibling runtime companion must start after reservation release")
        .expect("the sibling runtime companion must signal");
    active_release_tx.send(()).unwrap();
    active.await.unwrap();
    sibling.await.unwrap();
}
