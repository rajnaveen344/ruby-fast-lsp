use super::*;
use rayon::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};

#[tokio::test]
async fn nested_rayon_work_uses_only_the_process_cpu_lane_budget() {
    let governor = IndexingResourceGovernor::new(IndexingResourcePolicy::new(2, 2));

    let (pool_width, sum) = governor
        .run_cpu("nested Rayon regression", || {
            (
                rayon::current_num_threads(),
                (0usize..64).into_par_iter().sum::<usize>(),
            )
        })
        .await
        .unwrap();

    assert_eq!(
        pool_width, 2,
        "nested Rayon work escaped the server-owned two-lane indexing budget"
    );
    assert_eq!(sum, (0usize..64).sum::<usize>());
    let snapshot = governor.snapshot();
    assert_eq!(snapshot.peak_active_tasks, 1);
    assert_eq!(snapshot.peak_active_cpu_lanes, 2);
    assert_eq!(snapshot.active_tasks, 0);
    assert_eq!(snapshot.completed_tasks, 1);
}

#[tokio::test(flavor = "current_thread")]
async fn cooperative_parallel_work_partitions_lanes_between_projects() {
    let policy = IndexingResourcePolicy::with_limits(6, 2, 512, 2);
    assert_eq!(policy.cooperative_parallel_cpu_lanes(), 3);
    let governor = IndexingResourceGovernor::new(policy);
    let release = Arc::new(std::sync::Barrier::new(3));
    let widths = Arc::new(Mutex::new(Vec::new()));

    let mut tasks = Vec::new();
    for project in ["/workspace/active", "/workspace/background"] {
        let task_governor = governor.clone();
        let task_release = release.clone();
        let task_widths = widths.clone();
        tasks.push(tokio::spawn(async move {
            task_governor
                .run_cooperative_parallel_with_resources(
                    "cooperative project fact pass",
                    IndexingWorkSpec::new(
                        Some(PathBuf::from(project)),
                        IndexingResourcePriority::Background,
                        policy.cooperative_parallel_cpu_lanes(),
                        256,
                        1,
                    ),
                    None,
                    move || {
                        task_widths.lock().push(rayon::current_num_threads());
                        task_release.wait();
                    },
                )
                .await
                .unwrap();
        }));
    }

    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while governor.snapshot().active_tasks != 2 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("both cooperative project passes must run concurrently");
    let saturated = governor.snapshot();
    assert_eq!(saturated.active_cpu_lanes, 6);
    assert_eq!(saturated.active_transient_memory_bytes, 512);
    assert_eq!(saturated.active_io_slots, 2);
    release.wait();

    for task in tasks {
        task.await.unwrap();
    }
    let mut observed_widths = widths.lock().clone();
    observed_widths.sort_unstable();
    assert_eq!(observed_widths, vec![3, 3]);
    let complete = governor.snapshot();
    assert_eq!(complete.active_tasks, 0);
    assert_eq!(complete.completed_tasks, 2);
    assert_eq!(complete.peak_active_cpu_lanes, 6);
}

#[test]
fn active_project_reserves_one_lane_only_while_navigation_is_pending() {
    let policy = IndexingResourcePolicy::with_limits(6, 2, 512, 2);
    let governor = IndexingResourceGovernor::new(policy);
    let active = Path::new("/workspace/active");
    let background = Path::new("/workspace/background");

    assert_eq!(
        governor.project_parallel_cpu_lanes(active),
        policy.cooperative_parallel_cpu_lanes(),
        "without editor ownership every project must retain the cooperative partition"
    );
    governor.prioritize_active_project(active);
    assert_eq!(
        governor.project_parallel_cpu_lanes(active),
        policy.cooperative_parallel_cpu_lanes(),
        "active-project identity alone must not serialize exhaustive sibling work"
    );
    governor.prioritize_active_project_with_navigation_pending(active, true);
    assert_eq!(
        governor.project_parallel_cpu_lanes(active),
        policy.cpu_lanes() - 1,
        "the active project's navigation-critical source pass must leave one bounded lane \
             for exact dependency discovery"
    );
    assert_eq!(
        governor.project_parallel_cpu_lanes(background),
        policy.cooperative_parallel_cpu_lanes(),
        "a sibling project must not inherit the active document's exclusive lane claim"
    );
    governor.mark_project_navigation_complete_if_active(active);
    assert_eq!(
        governor.project_parallel_cpu_lanes(active),
        policy.cooperative_parallel_cpu_lanes(),
        "the active project must return to cooperative lanes after its bounded navigation \
             frontier completes"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn active_partitioned_pass_overlaps_discovery_before_an_older_sibling() {
    let policy = IndexingResourcePolicy::with_limits(6, 2, 512, 2);
    let governor = IndexingResourceGovernor::new(policy);
    let active_root = PathBuf::from("/workspace/active");
    let background_root = PathBuf::from("/workspace/background");
    governor.prioritize_active_project_with_navigation_pending(&active_root, true);
    let active_navigation = governor.project_navigation_reservation(active_root.clone());

    let (background_started_tx, mut background_started_rx) = tokio::sync::oneshot::channel();
    let background_governor = governor.clone();
    let background = tokio::spawn(async move {
        background_governor
            .run_cooperative_parallel_with_resources(
                "older cooperative sibling",
                IndexingWorkSpec::new(
                    Some(background_root),
                    IndexingResourcePriority::Background,
                    policy.cooperative_parallel_cpu_lanes(),
                    256,
                    1,
                )
                .as_project_parallel(),
                None,
                move || {
                    background_started_tx.send(()).unwrap();
                },
            )
            .await
            .unwrap();
    });

    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while governor.snapshot().queued_tasks != 1 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the older sibling must queue behind the active-navigation reservation");
    assert!(
        background_started_rx.try_recv().is_err(),
        "a sibling project pass must not start before the active project requests its pass"
    );

    let (active_started_tx, active_started_rx) = tokio::sync::oneshot::channel();
    let (active_release_tx, active_release_rx) = std::sync::mpsc::channel();
    let active_governor = governor.clone();
    let active_prefetch_root = active_root.clone();
    let active = tokio::spawn(async move {
        active_governor
            .run_partitioned_parallel_with_resources(
                "active partitioned project pass",
                IndexingWorkSpec::new(
                    Some(active_root),
                    IndexingResourcePriority::Background,
                    policy.cpu_lanes() - 1,
                    256,
                    1,
                )
                .as_project_parallel(),
                None,
                move || {
                    active_started_tx.send(()).unwrap();
                    active_release_rx.recv().unwrap();
                },
            )
            .await
            .unwrap();
    });

    active_started_rx.await.unwrap();
    assert_eq!(governor.snapshot().active_cpu_lanes, 5);
    assert!(
        background_started_rx.try_recv().is_err(),
        "the older sibling must remain queued while the active project owns the bounded pool"
    );

    let (prefetch_started_tx, prefetch_started_rx) = tokio::sync::oneshot::channel();
    let (prefetch_release_tx, prefetch_release_rx) = std::sync::mpsc::channel();
    let prefetch_governor = governor.clone();
    let prefetch = tokio::spawn(async move {
        prefetch_governor
            .run_with_resources(
                "active dependency discovery",
                IndexingWorkSpec::new(
                    Some(active_prefetch_root),
                    IndexingResourcePriority::Background,
                    1,
                    256,
                    1,
                ),
                None,
                move || {
                    prefetch_started_tx.send(()).unwrap();
                    prefetch_release_rx.recv().unwrap();
                },
            )
            .await
            .unwrap();
    });
    prefetch_started_rx.await.unwrap();
    let overlapped = governor.snapshot();
    assert_eq!(overlapped.active_tasks, 2);
    assert_eq!(overlapped.active_cpu_lanes, 6);
    assert_eq!(overlapped.active_io_slots, 2);
    assert!(
        background_started_rx.try_recv().is_err(),
        "the sibling project must remain queued while active project navigation is pending"
    );

    prefetch_release_tx.send(()).unwrap();
    prefetch.await.unwrap();
    active_release_tx.send(()).unwrap();
    active.await.unwrap();
    drop(active_navigation);
    background_started_rx.await.unwrap();
    background.await.unwrap();

    let complete = governor.snapshot();
    assert_eq!(complete.active_tasks, 0);
    assert_eq!(complete.queued_tasks, 0);
    assert_eq!(complete.peak_active_cpu_lanes, 6);
}

#[tokio::test(flavor = "current_thread")]
async fn cancellation_removes_a_task_waiting_for_resource_admission() {
    let governor = IndexingResourceGovernor::new(IndexingResourcePolicy::new(1, 1));
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let first_governor = governor.clone();
    let first = tokio::spawn(async move {
        first_governor
            .run_cpu("resource holder", move || {
                started_tx.send(()).unwrap();
                release_rx.recv().unwrap();
            })
            .await
            .unwrap();
    });
    started_rx.await.unwrap();

    let cancellation = CancellationToken::new();
    let waiter_governor = governor.clone();
    let waiter_cancellation = cancellation.clone();
    let waiter = tokio::spawn(async move {
        waiter_governor
            .run_cpu_cancellable(
                "cancelled resource waiter",
                Some(waiter_cancellation),
                || 42,
            )
            .await
    });
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while governor.snapshot().queued_tasks != 1 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("resource waiter must enter the queue");

    cancellation.cancel();
    let error = tokio::time::timeout(std::time::Duration::from_secs(1), waiter)
        .await
        .expect("cancelled resource waiter must wake")
        .unwrap()
        .unwrap_err();
    assert!(error.to_string().contains("cancelled before entering"));
    let cancelled = governor.snapshot();
    assert_eq!(cancelled.queued_tasks, 0);
    assert_eq!(cancelled.active_tasks, 1);
    assert_eq!(cancelled.cancelled_before_start, 1);

    release_tx.send(()).unwrap();
    first.await.unwrap();
    let complete = governor.snapshot();
    assert_eq!(complete.active_tasks, 0);
    assert_eq!(complete.completed_tasks, 1);
}

#[tokio::test(flavor = "current_thread")]
async fn admission_reserves_cpu_memory_and_io_atomically() {
    let governor = IndexingResourceGovernor::new(IndexingResourcePolicy::with_limits(4, 3, 100, 2));
    let (holder_started_tx, holder_started_rx) = tokio::sync::oneshot::channel();
    let (holder_release_tx, holder_release_rx) = std::sync::mpsc::channel();
    let holder_governor = governor.clone();
    let holder = tokio::spawn(async move {
        holder_governor
            .run_with_resources(
                "weighted resource holder",
                IndexingWorkSpec::new(
                    Some(PathBuf::from("/workspace/background-a")),
                    IndexingResourcePriority::Background,
                    3,
                    80,
                    1,
                ),
                None,
                move || {
                    holder_started_tx.send(()).unwrap();
                    holder_release_rx.recv().unwrap();
                },
            )
            .await
            .unwrap();
    });
    holder_started_rx.await.unwrap();

    let (blocked_started_tx, mut blocked_started_rx) = tokio::sync::oneshot::channel();
    let blocked_governor = governor.clone();
    let blocked = tokio::spawn(async move {
        blocked_governor
            .run_with_resources(
                "memory-blocked waiter",
                IndexingWorkSpec::new(
                    Some(PathBuf::from("/workspace/background-b")),
                    IndexingResourcePriority::Background,
                    1,
                    30,
                    1,
                ),
                None,
                move || {
                    blocked_started_tx.send(()).unwrap();
                },
            )
            .await
            .unwrap();
    });

    let (fitting_started_tx, fitting_started_rx) = tokio::sync::oneshot::channel();
    let (fitting_release_tx, fitting_release_rx) = std::sync::mpsc::channel();
    let fitting_governor = governor.clone();
    let fitting = tokio::spawn(async move {
        fitting_governor
            .run_with_resources(
                "exactly fitting waiter",
                IndexingWorkSpec::new(
                    Some(PathBuf::from("/workspace/background-c")),
                    IndexingResourcePriority::Background,
                    1,
                    20,
                    1,
                ),
                None,
                move || {
                    fitting_started_tx.send(()).unwrap();
                    fitting_release_rx.recv().unwrap();
                },
            )
            .await
            .unwrap();
    });
    fitting_started_rx.await.unwrap();
    assert!(
        blocked_started_rx.try_recv().is_err(),
        "the memory-blocked request must not partially reserve CPU or I/O"
    );
    let saturated = governor.snapshot();
    assert_eq!(saturated.active_cpu_lanes, 4);
    assert_eq!(saturated.active_transient_memory_bytes, 100);
    assert_eq!(saturated.active_io_slots, 2);
    assert_eq!(saturated.queued_tasks, 1);

    fitting_release_tx.send(()).unwrap();
    fitting.await.unwrap();
    assert!(
        blocked_started_rx.try_recv().is_err(),
        "free CPU and I/O must not admit work while its memory claim still does not fit"
    );
    holder_release_tx.send(()).unwrap();
    holder.await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(1), &mut blocked_started_rx)
        .await
        .expect("memory-blocked waiter must start when the complete claim fits")
        .unwrap();
    blocked.await.unwrap();

    let complete = governor.snapshot();
    assert_eq!(complete.active_tasks, 0);
    assert_eq!(complete.active_cpu_lanes, 0);
    assert_eq!(complete.active_transient_memory_bytes, 0);
    assert_eq!(complete.active_io_slots, 0);
    assert_eq!(complete.completed_tasks, 3);
}

#[tokio::test(flavor = "current_thread")]
async fn async_external_work_holds_one_exact_resource_lease_until_completion() {
    let governor = IndexingResourceGovernor::new(IndexingResourcePolicy::with_limits(1, 1, 100, 1));
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let release = Arc::new(tokio::sync::Notify::new());
    let holder_governor = governor.clone();
    let holder_release = release.clone();
    let holder = tokio::spawn(async move {
        holder_governor
            .run_async_with_resources(
                "external async holder",
                IndexingWorkSpec::new(
                    Some(PathBuf::from("/workspace/active")),
                    IndexingResourcePriority::OpenDocument,
                    1,
                    100,
                    1,
                ),
                None,
                async move {
                    started_tx.send(()).unwrap();
                    holder_release.notified().await;
                    42
                },
            )
            .await
            .unwrap()
    });
    started_rx.await.unwrap();
    assert_eq!(governor.snapshot().active_tasks, 1);

    let queued_governor = governor.clone();
    let queued = tokio::spawn(async move {
        queued_governor
            .run_with_resources(
                "work behind external async holder",
                IndexingWorkSpec::new(
                    Some(PathBuf::from("/workspace/background")),
                    IndexingResourcePriority::Background,
                    1,
                    1,
                    0,
                ),
                None,
                || (),
            )
            .await
            .unwrap();
    });
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while governor.snapshot().queued_tasks != 1 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("work must queue behind the admitted external async lease");

    release.notify_one();
    assert_eq!(holder.await.unwrap(), 42);
    queued.await.unwrap();
    let complete = governor.snapshot();
    assert_eq!(complete.active_tasks, 0);
    assert_eq!(complete.queued_tasks, 0);
    assert_eq!(complete.completed_tasks, 2);
    assert_eq!(complete.peak_active_cpu_lanes, 1);
    assert_eq!(complete.peak_active_transient_memory_bytes, 100);
    assert_eq!(complete.peak_active_io_slots, 1);
}

#[tokio::test(flavor = "current_thread")]
async fn dropping_admitted_async_external_work_releases_and_records_cancellation() {
    let governor = IndexingResourceGovernor::new(IndexingResourcePolicy::with_limits(1, 1, 100, 1));
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let task_governor = governor.clone();
    let task = tokio::spawn(async move {
        task_governor
            .run_async_with_resources(
                "cancelled external async work",
                IndexingWorkSpec::new(
                    Some(PathBuf::from("/workspace/active")),
                    IndexingResourcePriority::OpenDocument,
                    1,
                    100,
                    1,
                ),
                None,
                async move {
                    started_tx.send(()).unwrap();
                    std::future::pending::<()>().await;
                },
            )
            .await
    });
    started_rx.await.unwrap();
    assert_eq!(governor.snapshot().active_tasks, 1);

    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    let cancelled = governor.snapshot();
    assert_eq!(cancelled.active_tasks, 0);
    assert_eq!(cancelled.queued_tasks, 0);
    assert_eq!(cancelled.completed_tasks, 0);
    assert_eq!(cancelled.cancelled_before_start, 0);
    assert_eq!(cancelled.cancelled_after_start, 1);
}

#[tokio::test(flavor = "current_thread")]
async fn active_project_priority_is_retained_before_weighted_work_is_enqueued() {
    let governor = IndexingResourceGovernor::new(IndexingResourcePolicy::with_limits(1, 1, 100, 1));
    governor.prioritize_active_project(Path::new("/workspace/active"));

    let (holder_started_tx, holder_started_rx) = tokio::sync::oneshot::channel();
    let (holder_release_tx, holder_release_rx) = std::sync::mpsc::channel();
    let holder_governor = governor.clone();
    let holder = tokio::spawn(async move {
        holder_governor
            .run_cpu("weighted priority holder", move || {
                holder_started_tx.send(()).unwrap();
                holder_release_rx.recv().unwrap();
            })
            .await
            .unwrap();
    });
    holder_started_rx.await.unwrap();

    let order = Arc::new(Mutex::new(Vec::new()));
    let background_order = order.clone();
    let background_governor = governor.clone();
    let background = tokio::spawn(async move {
        background_governor
            .run_with_resources(
                "older background work",
                IndexingWorkSpec::new(
                    Some(PathBuf::from("/workspace/background")),
                    IndexingResourcePriority::Background,
                    1,
                    1,
                    0,
                ),
                None,
                move || background_order.lock().push("background"),
            )
            .await
            .unwrap();
    });
    let active_order = order.clone();
    let active_governor = governor.clone();
    let active = tokio::spawn(async move {
        active_governor
            .run_with_resources(
                "active project work",
                IndexingWorkSpec::new(
                    Some(PathBuf::from("/workspace/active")),
                    IndexingResourcePriority::Background,
                    1,
                    1,
                    0,
                ),
                None,
                move || active_order.lock().push("active"),
            )
            .await
            .unwrap();
    });
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while governor.snapshot().queued_tasks != 2 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("both weighted requests must queue behind the holder");

    holder_release_tx.send(()).unwrap();
    holder.await.unwrap();
    background.await.unwrap();
    active.await.unwrap();
    assert_eq!(*order.lock(), vec!["active", "background"]);
    assert_eq!(governor.snapshot().reprioritizations, 1);
}

#[test]
fn oversized_work_is_rejected_instead_of_waiting_forever() {
    let governor = IndexingResourceGovernor::new(IndexingResourcePolicy::with_limits(2, 1, 100, 1));
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        runtime.block_on(governor.run_with_resources(
            "oversized resource work",
            IndexingWorkSpec::new(None, IndexingResourcePriority::Background, 1, 101, 0),
            None,
            || (),
        ))
    }));
    assert!(panic.is_err());
}

#[test]
fn counter_helpers_fail_loudly_on_invalid_accounting() {
    let peak = AtomicUsize::new(0);
    peak.fetch_max(2, Ordering::SeqCst);
    assert_eq!(peak.load(Ordering::SeqCst), 2);
    assert!(std::panic::catch_unwind(|| { checked_sub_usize(0, 1, "test resource") }).is_err());
}
