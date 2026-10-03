//! Coordinator lifecycle, resource scheduling, and dependency priority.

use super::*;
use crate::utils::single_flight::SingleFlightStat;

#[test]
fn coordinator_construction_does_not_load_project_extensions() {
    let package = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("extensions/rspec-ruby");
    let config = RubyFastLspConfig {
        extension_packages: vec![package.to_string_lossy().into_owned()],
        ..RubyFastLspConfig::default()
    };

    let coordinator = IndexingCoordinator::new(PathBuf::from("/workspace/server"), config);

    assert!(
        coordinator.extension_registry.is_none(),
        "project coordinators must consume the server-owned extension registry instead of loading every package once per project"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn identical_runtime_stdlib_paths_use_one_server_owned_probe() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = TempDir::new().expect("runtime fixture directory must be created");
    let runtime_root = fixture.path().join("runtime");
    let runtime_bin = runtime_root.join("bin");
    let runtime_stdlib = runtime_root.join("lib/ruby/stdlib");
    fs::create_dir_all(&runtime_bin).expect("runtime bin directory must be created");
    fs::create_dir_all(&runtime_stdlib).expect("runtime stdlib directory must be created");
    let executable = runtime_bin.join("ruby");
    fs::write(
        &executable,
        "#!/bin/sh\nruntime_root=$(CDPATH= cd -- \"$(dirname -- \"$0\")/..\" && pwd)\nprintf 'probe\\n' >> \"$runtime_root/probe-count\"\nsleep 0.2\nprintf '%s\\0' \"$runtime_root/lib/ruby/stdlib\"\n",
    )
    .expect("fake runtime must be written");
    let mut permissions = fs::metadata(&executable)
        .expect("fake runtime metadata must exist")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&executable, permissions).expect("fake runtime must be executable");
    let runtime = SelectedRuntimeDescriptor {
        implementation: RuntimeImplementation::Mri,
        family: "3.3".to_string(),
        engine_version: "3.3.11".to_string(),
        compatibility_version: "3.3".to_string(),
        executable,
        discovery_source: RuntimeDiscoverySource::Path,
        java_home: None,
    };
    let server = Server::default();
    let ctx = server.load_context_for_project(fixture.path());

    let (first, second) = tokio::join!(
        runtime_stdlib_paths_for_project(&ctx, &runtime),
        runtime_stdlib_paths_for_project(&ctx, &runtime)
    );
    let expected = fs::canonicalize(runtime_stdlib).expect("runtime stdlib must canonicalize");
    assert_eq!(first.unwrap().paths(), &[expected.clone()]);
    assert_eq!(second.unwrap().paths(), &[expected]);
    assert_eq!(
        fs::read_to_string(runtime_root.join("probe-count"))
            .expect("probe count must exist")
            .lines()
            .count(),
        1,
        "concurrent projects selecting the same immutable runtime must execute one probe"
    );
    let cache = server.products.stdlib_paths().snapshot();
    assert_eq!(cache.get(SingleFlightStat::Lookups), 2);
    assert_eq!(cache.get(SingleFlightStat::Producers), 1);
    assert_eq!(cache.get(SingleFlightStat::JoinedFlights), 1);
    assert_eq!(cache.get(SingleFlightStat::Failures), 0);
}

#[tokio::test(flavor = "current_thread")]
async fn cpu_indexing_task_does_not_block_the_async_reactor() {
    let resources = crate::utils::admission::IndexingResourceGovernor::new(
        crate::utils::admission::IndexingResourcePolicy::new(2, 2),
    );
    let started = Arc::new(tokio::sync::Notify::new());
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let reactor_thread = std::thread::current().id();
    let started_for_task = started.clone();
    let started_wait = started.notified();
    let task = tokio::spawn(async move {
        resources
            .run_cpu("test project collection", move || {
                started_for_task.notify_one();
                // Fail before waiting if CPU work regresses onto the reactor,
                // so the regression cannot deadlock this handshake.
                assert_ne!(
                    std::thread::current().id(),
                    reactor_thread,
                    "CPU indexing must run off the single-threaded async reactor"
                );
                release_rx.recv().unwrap();
                42
            })
            .await
    });

    started_wait.await;
    // Only the reactor can release the CPU worker: reaching this send proves
    // the reactor made progress while the worker was occupied.
    release_tx.send(()).unwrap();
    assert_eq!(task.await.unwrap().unwrap(), 42);
}

#[test]
fn active_document_constant_roots_stably_prioritize_matching_locked_gems() {
    let constants = active_document_constant_priority_keys(
        "ExampleApp::Platform::Users::AccountRecord.find_by_key(key)\n\
             ExampleApp::Settings.current_api_version\n\
             BSON::ObjectId.new\n",
    );
    assert!(
        constants
            .project_terminals
            .iter()
            .any(|terminal| terminal == "accountrecord"),
        "the terminal constant in a qualified project type must be available for source-file priority"
    );
    assert!(
        constants
            .project_terminals
            .iter()
            .any(|terminal| terminal == "objectid"),
        "terminal dependency constants must remain available alongside their root package"
    );
    assert!(constants.dependency_roots.contains("exampleapp"));
    assert!(constants.dependency_roots.contains("bson"));
    assert!(!constants
        .project_terminals
        .iter()
        .any(|terminal| terminal == "platform"));
    assert!(!constants
        .project_terminals
        .iter()
        .any(|terminal| terminal == "users"));
    assert_eq!(
        prioritize_locked_gem_names(
            vec![
                "actionpack".to_string(),
                "activesupport".to_string(),
                "bson".to_string(),
                "json".to_string(),
            ],
            &constants.dependency_roots,
        ),
        vec![
            "bson".to_string(),
            "actionpack".to_string(),
            "activesupport".to_string(),
            "json".to_string(),
        ],
        "exact active-document constant roots must move matching locked gems ahead while \
            preserving the exhaustive order of every nonmatching dependency"
    );
}

#[test]
fn dynamic_dependency_demand_moves_only_the_exact_remaining_locked_gem() {
    let mut remaining = std::collections::VecDeque::from([
        "actionpack".to_string(),
        "activesupport".to_string(),
        "bson".to_string(),
        "json".to_string(),
    ]);

    let matched = prioritize_demanded_gem_names(
        &mut remaining,
        &["bson".to_string(), "notlocked".to_string()],
    );

    assert_eq!(
        remaining.into_iter().collect::<Vec<_>>(),
        vec![
            "bson".to_string(),
            "actionpack".to_string(),
            "activesupport".to_string(),
            "json".to_string(),
        ]
    );
    assert_eq!(
        matched.get("bson"),
        Some(&vec!["bson".to_string()]),
        "the consumer must know which exact waiter can complete after BSON is bound and \
             resolved"
    );
    assert!(
        !matched.values().flatten().any(|key| key == "notlocked"),
        "an unmatched dependency key must remain pending until another dependency family or \
             the complete stage can answer it"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn scheduler_bounds_parallel_cpu_workers_without_blocking_the_reactor() {
    let scheduler = crate::loader::scheduling::scheduler::IndexingScheduler::new(2);
    let resources = crate::utils::admission::IndexingResourceGovernor::new(
        crate::utils::admission::IndexingResourcePolicy::new(2, 2),
    );
    let active = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let maximum = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut tasks = Vec::new();
    for index in 0..6 {
        let scheduler = scheduler.clone();
        let resources = resources.clone();
        let active = active.clone();
        let maximum = maximum.clone();
        tasks.push(tokio::spawn(async move {
            let _permit = scheduler
                .acquire(
                    PathBuf::from(format!("/workspace/project-{index}")),
                    crate::loader::scheduling::scheduler::IndexingPriority::Background,
                )
                .await;
            let running = active.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            maximum.fetch_max(running, std::sync::atomic::Ordering::SeqCst);
            resources
                .run_cpu("bounded test indexing", move || {
                    std::thread::sleep(Duration::from_millis(50));
                })
                .await
                .unwrap();
            active.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        }));
    }

    tokio::time::timeout(Duration::from_secs(1), async {
        while active.load(std::sync::atomic::Ordering::SeqCst) != 2
            || scheduler.snapshot().queued != 4
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("two scheduler-owned CPU workers must start");
    assert_eq!(scheduler.snapshot().active, 2);
    assert_eq!(scheduler.snapshot().queued, 4);

    let heartbeat = Instant::now();
    tokio::time::sleep(Duration::from_millis(5)).await;
    assert!(
        heartbeat.elapsed() < Duration::from_millis(30),
        "the reactor heartbeat was delayed while bounded CPU workers were saturated"
    );

    for task in tasks {
        task.await.unwrap();
    }
    assert_eq!(
        maximum.load(std::sync::atomic::Ordering::SeqCst),
        2,
        "scheduler admission must bound simultaneous CPU indexing workers"
    );
    assert_eq!(scheduler.snapshot().active, 0);
    assert_eq!(scheduler.snapshot().queued, 0);
}

#[tokio::test]
async fn superseded_coordinator_cannot_advance_replacement_generation() {
    let fixture = TempDir::new().unwrap();
    let project = fixture.path().join("admin");
    fs::create_dir_all(&project).unwrap();
    let server = Server::default();
    let workspace = server.add_workspace(Url::from_directory_path(&project).unwrap());
    let old_run = workspace.indexing_status.begin_run();
    let mut old_coordinator =
        IndexingCoordinator::new(project.clone(), RubyFastLspConfig::default());
    old_coordinator.set_indexing_run(old_run.clone());
    old_coordinator.set_load_target(workspace.handle().load_target());

    let replacement = workspace.indexing_status.begin_run();
    let result = old_coordinator
        .transition_indexing_status(
            &server.load_context_for_project(&project),
            crate::loader::scheduling::status::IndexingPhase::IndexingProject,
        )
        .await;

    assert!(result.is_err());
    assert!(old_run.is_cancelled());
    let snapshot = workspace.indexing_status.snapshot();
    assert_eq!(snapshot.generation, replacement.generation());
    assert_eq!(
        snapshot.phase,
        crate::loader::scheduling::status::IndexingPhase::Queued
    );
}

#[test]
fn removed_coordinator_keeps_detached_engine_instead_of_orphan_project() {
    let fixture = TempDir::new().unwrap();
    let project = fixture.path().join("admin");
    fs::create_dir_all(&project).unwrap();
    let server = Server::default();
    let workspace = server.add_workspace(Url::from_directory_path(&project).unwrap());
    let mut coordinator = IndexingCoordinator::new(project, RubyFastLspConfig::default());
    coordinator.set_load_target(workspace.handle().load_target());

    server.remove_workspace(&workspace.root_uri);

    let selected = coordinator.load_target(&server.load_context_for_project(&workspace.root_path));
    assert!(workspace.handle().is_target(&selected));
    assert!(!server.orphan_project().is_target(&selected));
}
