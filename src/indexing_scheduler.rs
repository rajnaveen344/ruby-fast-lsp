use parking_lot::Mutex;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

const MAX_PRIORITY_ADMISSIONS_WHILE_BACKGROUND_WAITS: usize = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum IndexingPriority {
    ActiveDocument,
    OpenDocument,
    Background,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct QueueEntry {
    id: u64,
    project_root: PathBuf,
    requested_priority: IndexingPriority,
    priority: IndexingPriority,
    insertion_order: u64,
}

#[derive(Debug)]
struct SchedulerState {
    active: usize,
    active_projects: BTreeSet<PathBuf>,
    queued: Vec<QueueEntry>,
    active_project: Option<PathBuf>,
    reprioritizations: u64,
    priority_admissions_while_background_waits: usize,
}

#[derive(Debug)]
struct SchedulerInner {
    concurrency_limit: usize,
    next_id: AtomicU64,
    state: Mutex<SchedulerState>,
    changed: Notify,
}

#[derive(Clone, Debug)]
pub struct IndexingScheduler {
    inner: Arc<SchedulerInner>,
}

impl IndexingScheduler {
    pub fn new(concurrency_limit: usize) -> Self {
        assert!(
            concurrency_limit > 0,
            "INVARIANT VIOLATED: indexing scheduler concurrency is zero. This is a bug because queued projects could never make progress. Fix: configure at least one indexing worker."
        );
        Self {
            inner: Arc::new(SchedulerInner {
                concurrency_limit,
                next_id: AtomicU64::new(0),
                state: Mutex::new(SchedulerState {
                    active: 0,
                    active_projects: BTreeSet::new(),
                    queued: Vec::new(),
                    active_project: None,
                    reprioritizations: 0,
                    priority_admissions_while_background_waits: 0,
                }),
                changed: Notify::new(),
            }),
        }
    }

    pub async fn acquire(
        &self,
        project_root: PathBuf,
        priority: IndexingPriority,
    ) -> IndexingPermit {
        self.acquire_cancellable(project_root, priority, CancellationToken::new())
            .await
            .expect(
                "INVARIANT VIOLATED: a non-cancellable indexing admission was cancelled. This is a bug because its private cancellation token is never exposed. Fix: inspect scheduler cancellation ownership.",
            )
    }

    pub async fn acquire_cancellable(
        &self,
        project_root: PathBuf,
        priority: IndexingPriority,
        cancellation: CancellationToken,
    ) -> Option<IndexingPermit> {
        self.register_cancellable(project_root, priority, cancellation)
            .wait()
            .await
    }

    /// Register work synchronously so a caller can publish a complete batch
    /// before any asynchronous waiter is eligible for admission.
    ///
    /// Initial multi-project startup must use this boundary. Otherwise Tokio
    /// poll order can admit a background sibling before an already-known
    /// active project has entered the queue.
    pub fn register_cancellable(
        &self,
        project_root: PathBuf,
        priority: IndexingPriority,
        cancellation: CancellationToken,
    ) -> IndexingAdmission {
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        assert!(
            id != u64::MAX,
            "INVARIANT VIOLATED: indexing scheduler ticket overflowed. This is a bug because one server cannot enqueue 2^64 indexing jobs. Fix: inspect the loop continuously rebuilding projects."
        );
        {
            let mut state = self.inner.state.lock();
            let effective_priority =
                effective_priority(&project_root, priority, state.active_project.as_deref());
            state.queued.push(QueueEntry {
                id,
                project_root,
                requested_priority: priority,
                priority: effective_priority,
                insertion_order: id,
            });
        }
        self.inner.changed.notify_waiters();
        IndexingAdmission {
            registration: QueueRegistration {
                inner: self.inner.clone(),
                id,
                admitted: false,
            },
            cancellation,
        }
    }
}

pub struct IndexingAdmission {
    registration: QueueRegistration,
    cancellation: CancellationToken,
}

impl IndexingAdmission {
    pub async fn wait(mut self) -> Option<IndexingPermit> {
        loop {
            if self.cancellation.is_cancelled() {
                return None;
            }
            let changed = self.registration.inner.changed.notified();
            {
                let mut state = self.registration.inner.state.lock();
                if self.cancellation.is_cancelled() {
                    return None;
                }
                if state.active < self.registration.inner.concurrency_limit {
                    if let Some(winner) = best_admissible_entry_index_with_fairness(
                        &state.queued,
                        &state.active_projects,
                        state.priority_admissions_while_background_waits,
                    ) {
                        if state.queued[winner].id == self.registration.id {
                            let admissible_background_waiting = state.queued.iter().any(|entry| {
                                entry.priority == IndexingPriority::Background
                                    && !state.active_projects.contains(&entry.project_root)
                            });
                            let admitted = state.queued.swap_remove(winner);
                            let inserted =
                                state.active_projects.insert(admitted.project_root.clone());
                            assert!(
                                inserted,
                                "INVARIANT VIOLATED: indexing scheduler admitted two active generations for project {}. This is a bug because concurrent coordinators can replace facts in the same isolated engine. Fix: admit only queue entries whose project root is not active.",
                                admitted.project_root.display()
                            );
                            state.active += 1;
                            if admitted.priority == IndexingPriority::Background
                                || !admissible_background_waiting
                            {
                                state.priority_admissions_while_background_waits = 0;
                            } else {
                                state.priority_admissions_while_background_waits = state
                                    .priority_admissions_while_background_waits
                                    .checked_add(1)
                                    .expect(
                                        "INVARIANT VIOLATED: indexing scheduler fairness counter overflowed. This is a bug because the counter resets after a bounded priority burst. Fix: inspect admission accounting and background detection.",
                                    );
                                assert!(
                                    state.priority_admissions_while_background_waits
                                        <= MAX_PRIORITY_ADMISSIONS_WHILE_BACKGROUND_WAITS,
                                    "INVARIANT VIOLATED: indexing scheduler exceeded its bounded priority burst while background work was admissible. This is a bug because starvation resistance requires the next admission to select background work. Fix: route every admission through the fairness-aware selector."
                                    );
                            }
                            self.registration.admitted = true;
                            let permit = IndexingPermit {
                                inner: self.registration.inner.clone(),
                                project_root: admitted.project_root,
                            };
                            drop(state);
                            self.registration.inner.changed.notify_waiters();
                            return Some(permit);
                        }
                    }
                }
            }
            tokio::select! {
                biased;
                _ = self.cancellation.cancelled() => return None,
                _ = changed => {}
            }
        }
    }
}

impl IndexingScheduler {
    pub fn prioritize_active_project(&self, project_root: &Path) {
        let mut changed = false;
        {
            let mut state = self.inner.state.lock();
            if state.active_project.as_deref() != Some(project_root) {
                state.active_project = Some(project_root.to_path_buf());
                state.reprioritizations = state.reprioritizations.checked_add(1).expect(
                    "INVARIANT VIOLATED: scheduler active-project reprioritization count overflowed. \
                     This is a bug because one process cannot switch active projects 2^64 times. \
                     Fix: inspect the editor activity notification loop.",
                );
                changed = true;
            }
            let active_project = state.active_project.clone();
            for entry in &mut state.queued {
                let priority = effective_priority(
                    &entry.project_root,
                    entry.requested_priority,
                    active_project.as_deref(),
                );
                if entry.priority != priority {
                    entry.priority = priority;
                    changed = true;
                }
            }
        }
        if changed {
            self.inner.changed.notify_waiters();
        }
    }

    pub fn snapshot(&self) -> IndexingSchedulerSnapshot {
        let state = self.inner.state.lock();
        IndexingSchedulerSnapshot {
            active: state.active,
            queued: state.queued.len(),
            concurrency_limit: self.inner.concurrency_limit,
            active_project: state.active_project.clone(),
            reprioritizations: state.reprioritizations,
        }
    }
}

fn effective_priority(
    project_root: &Path,
    requested_priority: IndexingPriority,
    active_project: Option<&Path>,
) -> IndexingPriority {
    if active_project == Some(project_root) {
        IndexingPriority::ActiveDocument
    } else {
        requested_priority
    }
}

impl Default for IndexingScheduler {
    fn default() -> Self {
        // M0 profiling will replace this conservative internal policy with a
        // measured CPU/memory/disk-derived limit. It is intentionally not a
        // user-facing setting.
        Self::new(2)
    }
}

fn best_admissible_entry_index(
    entries: &[QueueEntry],
    active_projects: &BTreeSet<PathBuf>,
) -> Option<usize> {
    entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| !active_projects.contains(&entry.project_root))
        .min_by_key(|(_, entry)| {
            (
                entry.priority,
                entry.insertion_order,
                entry.project_root.as_os_str(),
            )
        })
        .map(|(index, _)| index)
}

fn best_admissible_entry_index_with_fairness(
    entries: &[QueueEntry],
    active_projects: &BTreeSet<PathBuf>,
    priority_admissions_while_background_waits: usize,
) -> Option<usize> {
    if priority_admissions_while_background_waits >= MAX_PRIORITY_ADMISSIONS_WHILE_BACKGROUND_WAITS
    {
        if let Some(background) = entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                entry.priority == IndexingPriority::Background
                    && !active_projects.contains(&entry.project_root)
            })
            .min_by_key(|(_, entry)| (entry.insertion_order, entry.project_root.as_os_str()))
            .map(|(index, _)| index)
        {
            return Some(background);
        }
    }
    best_admissible_entry_index(entries, active_projects)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexingSchedulerSnapshot {
    pub active: usize,
    pub queued: usize,
    pub concurrency_limit: usize,
    pub active_project: Option<PathBuf>,
    pub reprioritizations: u64,
}

struct QueueRegistration {
    inner: Arc<SchedulerInner>,
    id: u64,
    admitted: bool,
}

impl Drop for QueueRegistration {
    fn drop(&mut self) {
        if self.admitted {
            return;
        }
        let mut state = self.inner.state.lock();
        if let Some(index) = state.queued.iter().position(|entry| entry.id == self.id) {
            state.queued.swap_remove(index);
            drop(state);
            self.inner.changed.notify_waiters();
        }
    }
}

pub struct IndexingPermit {
    inner: Arc<SchedulerInner>,
    project_root: PathBuf,
}

impl Drop for IndexingPermit {
    fn drop(&mut self) {
        let mut state = self.inner.state.lock();
        assert!(
            state.active > 0,
            "INVARIANT VIOLATED: indexing scheduler released a permit with no active worker. This is a bug because every permit must increment active exactly once. Fix: inspect permit construction and drop ownership."
        );
        let removed = state.active_projects.remove(&self.project_root);
        assert!(
            removed,
            "INVARIANT VIOLATED: indexing scheduler released project {} without an active-project registration. This is a bug because every permit must own exactly one active project. Fix: preserve the admitted project root on the permit.",
            self.project_root.display()
        );
        state.active -= 1;
        drop(state);
        self.inner.changed.notify_waiters();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_util::sync::CancellationToken;

    #[test]
    fn queue_order_prefers_priority_then_insertion_order() {
        let entries = vec![
            QueueEntry {
                id: 0,
                project_root: PathBuf::from("/workspace/background"),
                requested_priority: IndexingPriority::Background,
                priority: IndexingPriority::Background,
                insertion_order: 0,
            },
            QueueEntry {
                id: 1,
                project_root: PathBuf::from("/workspace/open"),
                requested_priority: IndexingPriority::OpenDocument,
                priority: IndexingPriority::OpenDocument,
                insertion_order: 1,
            },
            QueueEntry {
                id: 2,
                project_root: PathBuf::from("/workspace/active"),
                requested_priority: IndexingPriority::ActiveDocument,
                priority: IndexingPriority::ActiveDocument,
                insertion_order: 2,
            },
        ];

        assert_eq!(
            best_admissible_entry_index(&entries, &BTreeSet::new()),
            Some(2)
        );
    }

    #[test]
    fn bounded_priority_burst_forces_oldest_admissible_background_project() {
        let entries = vec![
            QueueEntry {
                id: 0,
                project_root: PathBuf::from("/workspace/background"),
                requested_priority: IndexingPriority::Background,
                priority: IndexingPriority::Background,
                insertion_order: 0,
            },
            QueueEntry {
                id: 1,
                project_root: PathBuf::from("/workspace/active"),
                requested_priority: IndexingPriority::ActiveDocument,
                priority: IndexingPriority::ActiveDocument,
                insertion_order: 1,
            },
        ];

        assert_eq!(
            best_admissible_entry_index_with_fairness(
                &entries,
                &BTreeSet::new(),
                MAX_PRIORITY_ADMISSIONS_WHILE_BACKGROUND_WAITS - 1,
            ),
            Some(1),
            "active-document work must retain priority below the fairness bound"
        );
        assert_eq!(
            best_admissible_entry_index_with_fairness(
                &entries,
                &BTreeSet::new(),
                MAX_PRIORITY_ADMISSIONS_WHILE_BACKGROUND_WAITS,
            ),
            Some(0),
            "an older background project must run after the bounded priority burst"
        );
    }

    #[tokio::test]
    async fn queued_active_project_runs_before_older_background_project() {
        let scheduler = IndexingScheduler::new(1);
        let first = scheduler
            .acquire(
                PathBuf::from("/workspace/running"),
                IndexingPriority::Background,
            )
            .await;

        let background_scheduler = scheduler.clone();
        let background = tokio::spawn(async move {
            let _permit = background_scheduler
                .acquire(
                    PathBuf::from("/workspace/background"),
                    IndexingPriority::Background,
                )
                .await;
            "background"
        });
        let active_scheduler = scheduler.clone();
        let active = tokio::spawn(async move {
            let _permit = active_scheduler
                .acquire(
                    PathBuf::from("/workspace/active"),
                    IndexingPriority::ActiveDocument,
                )
                .await;
            "active"
        });

        tokio::task::yield_now().await;
        assert_eq!(scheduler.snapshot().queued, 2);
        drop(first);
        assert_eq!(active.await.unwrap(), "active");
        assert_eq!(background.await.unwrap(), "background");
        assert_eq!(scheduler.snapshot().active, 0);
    }

    #[tokio::test]
    async fn synchronously_registered_batch_cannot_admit_background_before_active() {
        let scheduler = IndexingScheduler::new(1);
        let active_root = PathBuf::from("/workspace/active");
        scheduler.prioritize_active_project(&active_root);

        let background = scheduler.register_cancellable(
            PathBuf::from("/workspace/background"),
            IndexingPriority::Background,
            CancellationToken::new(),
        );
        let active = scheduler.register_cancellable(
            active_root.clone(),
            IndexingPriority::Background,
            CancellationToken::new(),
        );

        let background_waiter = tokio::spawn(async move { background.wait().await.unwrap() });
        tokio::task::yield_now().await;
        assert!(
            !background_waiter.is_finished(),
            "background admission must wait while the registered active project is eligible"
        );

        let active_permit = tokio::time::timeout(std::time::Duration::from_secs(1), active.wait())
            .await
            .expect("active admission timed out")
            .expect("active admission was cancelled");
        assert_eq!(active_permit.project_root, active_root);
        drop(active_permit);

        let background_permit =
            tokio::time::timeout(std::time::Duration::from_secs(1), background_waiter)
                .await
                .expect("background admission timed out")
                .expect("background waiter panicked");
        assert_eq!(
            background_permit.project_root,
            PathBuf::from("/workspace/background")
        );
    }

    #[tokio::test]
    async fn priority_admission_wakes_a_sleeping_sibling_for_free_capacity() {
        let scheduler = IndexingScheduler::new(2);
        let active_root = PathBuf::from("/workspace/active");
        let background_root = PathBuf::from("/workspace/background");
        scheduler.prioritize_active_project(&active_root);

        let background = scheduler.register_cancellable(
            background_root.clone(),
            IndexingPriority::Background,
            CancellationToken::new(),
        );
        let active = scheduler.register_cancellable(
            active_root.clone(),
            IndexingPriority::Background,
            CancellationToken::new(),
        );

        let background_waiter = tokio::spawn(async move { background.wait().await.unwrap() });
        tokio::task::yield_now().await;
        assert!(
            !background_waiter.is_finished(),
            "the background waiter must initially defer to the registered active project"
        );

        let active_permit = active.wait().await.unwrap();
        assert_eq!(active_permit.project_root, active_root);
        let background_permit =
            tokio::time::timeout(std::time::Duration::from_millis(100), background_waiter)
                .await
                .expect(
                    "admitting the active project left free capacity but did not wake the \
                     already-sleeping sibling waiter",
                )
                .unwrap();
        assert_eq!(background_permit.project_root, background_root);
        assert_eq!(scheduler.snapshot().active, 2);
        drop(background_permit);
        drop(active_permit);
    }

    #[tokio::test]
    async fn active_intent_set_before_enqueue_is_honored() {
        let scheduler = IndexingScheduler::new(1);
        let running = scheduler
            .acquire(
                PathBuf::from("/workspace/running"),
                IndexingPriority::Background,
            )
            .await;
        scheduler.prioritize_active_project(Path::new("/workspace/active"));

        let (admitted_tx, mut admitted_rx) = tokio::sync::mpsc::unbounded_channel();
        let background_scheduler = scheduler.clone();
        let background_tx = admitted_tx.clone();
        let background = tokio::spawn(async move {
            let _permit = background_scheduler
                .acquire(
                    PathBuf::from("/workspace/background"),
                    IndexingPriority::Background,
                )
                .await;
            background_tx.send("background").unwrap();
        });
        wait_for_queued(&scheduler, 1).await;

        let active_scheduler = scheduler.clone();
        let active_tx = admitted_tx.clone();
        let active = tokio::spawn(async move {
            let _permit = active_scheduler
                .acquire(
                    PathBuf::from("/workspace/active"),
                    IndexingPriority::Background,
                )
                .await;
            active_tx.send("active").unwrap();
        });
        drop(admitted_tx);
        wait_for_queued(&scheduler, 2).await;
        drop(running);

        assert_eq!(
            admitted_rx.recv().await,
            Some("active"),
            "active intent recorded before queue insertion must promote that project"
        );
        assert_eq!(admitted_rx.recv().await, Some("background"));
        active.await.unwrap();
        background.await.unwrap();
    }

    #[tokio::test]
    async fn continuous_active_work_cannot_starve_an_admissible_background_project() {
        let scheduler = IndexingScheduler::new(1);
        let running = scheduler
            .acquire(
                PathBuf::from("/workspace/running"),
                IndexingPriority::Background,
            )
            .await;
        let (completed_tx, mut completed_rx) = tokio::sync::mpsc::unbounded_channel();
        let background_scheduler = scheduler.clone();
        let background_tx = completed_tx.clone();
        let background = tokio::spawn(async move {
            let _permit = background_scheduler
                .acquire(
                    PathBuf::from("/workspace/background"),
                    IndexingPriority::Background,
                )
                .await;
            background_tx.send("background").unwrap();
        });
        wait_for_queued(&scheduler, 1).await;

        let mut active_tasks = Vec::new();
        for index in 0..4 {
            let active_scheduler = scheduler.clone();
            let active_tx = completed_tx.clone();
            active_tasks.push(tokio::spawn(async move {
                let _permit = active_scheduler
                    .acquire(
                        PathBuf::from(format!("/workspace/active-{index}")),
                        IndexingPriority::ActiveDocument,
                    )
                    .await;
                active_tx.send("active").unwrap();
            }));
            wait_for_queued(&scheduler, index + 2).await;
        }
        drop(completed_tx);
        drop(running);

        assert_eq!(completed_rx.recv().await, Some("active"));
        assert_eq!(
            completed_rx.recv().await,
            Some("background"),
            "the oldest background project must run after one active-project bypass"
        );
        background.await.unwrap();
        for task in active_tasks {
            task.await.unwrap();
        }
        assert_eq!(scheduler.snapshot().active, 0);
        assert_eq!(scheduler.snapshot().queued, 0);
    }

    #[tokio::test]
    async fn cancelled_waiter_leaves_no_stale_queue_entry() {
        let scheduler = IndexingScheduler::new(1);
        let permit = scheduler
            .acquire(
                PathBuf::from("/workspace/running"),
                IndexingPriority::Background,
            )
            .await;
        let waiting_scheduler = scheduler.clone();
        let waiting = tokio::spawn(async move {
            let _permit = waiting_scheduler
                .acquire(
                    PathBuf::from("/workspace/cancelled"),
                    IndexingPriority::Background,
                )
                .await;
        });
        tokio::task::yield_now().await;
        assert_eq!(scheduler.snapshot().queued, 1);

        waiting.abort();
        let _ = waiting.await;
        assert_eq!(scheduler.snapshot().queued, 0);
        drop(permit);
    }

    #[tokio::test]
    async fn project_cancellation_wakes_queued_waiter_without_admission() {
        let scheduler = IndexingScheduler::new(1);
        let running = scheduler
            .acquire(
                PathBuf::from("/workspace/running"),
                IndexingPriority::Background,
            )
            .await;
        let cancellation = CancellationToken::new();
        let waiting_scheduler = scheduler.clone();
        let waiting_cancellation = cancellation.clone();
        let waiting = tokio::spawn(async move {
            waiting_scheduler
                .acquire_cancellable(
                    PathBuf::from("/workspace/cancelled"),
                    IndexingPriority::Background,
                    waiting_cancellation,
                )
                .await
        });
        tokio::task::yield_now().await;
        assert_eq!(scheduler.snapshot().queued, 1);

        cancellation.cancel();
        assert!(
            waiting
                .await
                .expect("cancelled scheduler waiter task must complete")
                .is_none(),
            "cancelled queued work must never receive an indexing permit"
        );
        assert_eq!(scheduler.snapshot().queued, 0);
        assert_eq!(scheduler.snapshot().active, 1);
        drop(running);
    }

    #[tokio::test]
    async fn same_project_generations_never_run_concurrently() {
        let scheduler = IndexingScheduler::new(2);
        let first = scheduler
            .acquire(
                PathBuf::from("/workspace/admin"),
                IndexingPriority::Background,
            )
            .await;
        let next_scheduler = scheduler.clone();
        let next = tokio::spawn(async move {
            let _permit = next_scheduler
                .acquire(
                    PathBuf::from("/workspace/admin"),
                    IndexingPriority::ActiveDocument,
                )
                .await;
        });
        tokio::task::yield_now().await;

        assert_eq!(
            scheduler.snapshot().active,
            1,
            "a second generation of the same project must wait even when global capacity remains"
        );
        assert_eq!(scheduler.snapshot().queued, 1);
        drop(first);
        next.await
            .expect("replacement project generation must run after its predecessor");
        assert_eq!(scheduler.snapshot().active, 0);
    }

    async fn wait_for_queued(scheduler: &IndexingScheduler, expected: usize) {
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while scheduler.snapshot().queued != expected {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect(
            "INVARIANT VIOLATED: scheduler test queue did not reach the expected size. This is a bug because every spawned waiter must register before admission. Fix: inspect queue registration and notification ownership.",
        );
    }
}
