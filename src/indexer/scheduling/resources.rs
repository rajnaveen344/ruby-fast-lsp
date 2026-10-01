use anyhow::{anyhow, Context, Result};
use parking_lot::Mutex;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

const MAX_DEFAULT_CPU_LANES: usize = 6;
const RESERVED_HOST_CPU_LANES: usize = 2;
const DEFAULT_TOP_LEVEL_TASKS: usize = 2;
const MIB: usize = 1024 * 1024;
const DEFAULT_TRANSIENT_MEMORY_LIMIT_BYTES: usize = 512 * MIB;
const DEFAULT_IO_SLOTS: usize = 2;
const DEFAULT_PARALLEL_TASK_MEMORY_BYTES: usize = 256 * MIB;
const MAX_PRIORITY_ADMISSIONS_WHILE_BACKGROUND_WAITS: usize = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IndexingResourcePolicy {
    cpu_lanes: usize,
    top_level_tasks: usize,
    transient_memory_limit_bytes: usize,
    io_slots: usize,
}

impl IndexingResourcePolicy {
    pub fn new(cpu_lanes: usize, top_level_tasks: usize) -> Self {
        Self::with_limits(
            cpu_lanes,
            top_level_tasks,
            DEFAULT_TRANSIENT_MEMORY_LIMIT_BYTES,
            DEFAULT_IO_SLOTS,
        )
    }

    pub fn with_limits(
        cpu_lanes: usize,
        top_level_tasks: usize,
        transient_memory_limit_bytes: usize,
        io_slots: usize,
    ) -> Self {
        assert!(
            cpu_lanes > 0,
            "INVARIANT VIOLATED: the indexing CPU lane budget is zero. This is a bug because no indexing work could make progress. Fix: configure at least one CPU lane."
        );
        assert!(
            top_level_tasks > 0,
            "INVARIANT VIOLATED: the indexing task admission budget is zero. This is a bug because no coordinator phase could enter the worker pool. Fix: configure at least one top-level task."
        );
        assert!(
            transient_memory_limit_bytes > 0,
            "INVARIANT VIOLATED: the indexing transient-memory budget is zero. This is a bug because every indexing task requires bounded temporary allocations. Fix: configure a positive transient-memory budget."
        );
        assert!(
            io_slots > 0,
            "INVARIANT VIOLATED: the indexing I/O budget is zero. This is a bug because project discovery and source loading could never make progress. Fix: configure at least one I/O slot."
        );
        Self {
            cpu_lanes,
            top_level_tasks,
            transient_memory_limit_bytes,
            io_slots,
        }
    }

    pub fn cpu_lanes(self) -> usize {
        self.cpu_lanes
    }

    pub fn top_level_tasks(self) -> usize {
        self.top_level_tasks
    }

    pub fn transient_memory_limit_bytes(self) -> usize {
        self.transient_memory_limit_bytes
    }

    pub fn io_slots(self) -> usize {
        self.io_slots
    }

    pub fn cooperative_parallel_cpu_lanes(self) -> usize {
        self.cpu_lanes
            .checked_div(self.top_level_tasks)
            .unwrap_or_else(|| {
                panic!(
                    "INVARIANT VIOLATED: cooperative indexing divided by a zero task limit. This is a bug because policy construction rejects zero top-level tasks. Fix: preserve the validated resource policy when deriving cooperative lane partitions."
                )
            })
            .max(1)
    }

    fn for_current_host() -> Self {
        let logical_cpus = std::thread::available_parallelism()
            .map(usize::from)
            .unwrap_or(1);
        let cpu_lanes = logical_cpus
            .saturating_sub(RESERVED_HOST_CPU_LANES)
            .clamp(1, MAX_DEFAULT_CPU_LANES);
        Self::new(cpu_lanes, DEFAULT_TOP_LEVEL_TASKS.min(cpu_lanes))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum IndexingResourcePriority {
    ActiveDocument,
    OpenDocument,
    Background,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexingWorkSpec {
    project_root: Option<PathBuf>,
    requested_priority: IndexingResourcePriority,
    cpu_lanes: usize,
    transient_memory_bytes: usize,
    io_slots: usize,
    project_parallel: bool,
}

impl IndexingWorkSpec {
    pub fn new(
        project_root: Option<PathBuf>,
        priority: IndexingResourcePriority,
        cpu_lanes: usize,
        transient_memory_bytes: usize,
        io_slots: usize,
    ) -> Self {
        assert!(
            cpu_lanes > 0,
            "INVARIANT VIOLATED: an indexing work request claims zero CPU lanes. This is a bug because admitted work could execute without CPU accounting. Fix: reserve at least one CPU lane."
        );
        assert!(
            transient_memory_bytes > 0,
            "INVARIANT VIOLATED: an indexing work request claims zero transient-memory bytes. This is a bug because admitted work could allocate outside memory accounting. Fix: provide a conservative positive transient-memory estimate."
        );
        Self {
            project_root,
            requested_priority: priority,
            cpu_lanes,
            transient_memory_bytes,
            io_slots,
            project_parallel: false,
        }
    }

    pub fn as_project_parallel(mut self) -> Self {
        assert!(
            self.project_root.is_some(),
            "INVARIANT VIOLATED: project-parallel work has no project root. This is a bug because \
             the active-project reservation cannot distinguish its owner. Fix: attach the exact \
             isolated project root before marking a work request project-parallel."
        );
        self.project_parallel = true;
        self
    }

    pub fn project_root(&self) -> Option<&Path> {
        self.project_root.as_deref()
    }

    pub fn cpu_lanes(&self) -> usize {
        self.cpu_lanes
    }

    pub fn transient_memory_bytes(&self) -> usize {
        self.transient_memory_bytes
    }

    pub fn io_slots(&self) -> usize {
        self.io_slots
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexingResourceSnapshot {
    pub cpu_lane_limit: usize,
    pub top_level_task_limit: usize,
    pub transient_memory_limit_bytes: usize,
    pub io_slot_limit: usize,
    pub queued_tasks: usize,
    pub active_tasks: usize,
    pub peak_active_tasks: usize,
    pub active_cpu_lanes: usize,
    pub peak_active_cpu_lanes: usize,
    pub active_transient_memory_bytes: usize,
    pub peak_active_transient_memory_bytes: usize,
    pub active_io_slots: usize,
    pub peak_active_io_slots: usize,
    pub completed_tasks: u64,
    pub panicked_tasks: u64,
    pub cancelled_before_start: u64,
    pub cancelled_after_start: u64,
    pub active_project: Option<PathBuf>,
    pub reprioritizations: u64,
    pub active_project_navigation_pending: bool,
}

#[derive(Debug, Clone)]
struct QueuedWork {
    id: u64,
    spec: IndexingWorkSpec,
    priority: IndexingResourcePriority,
    insertion_order: u64,
}

#[derive(Debug)]
struct AdmissionState {
    queued: Vec<QueuedWork>,
    active_tasks: usize,
    peak_active_tasks: usize,
    active_cpu_lanes: usize,
    peak_active_cpu_lanes: usize,
    active_transient_memory_bytes: usize,
    peak_active_transient_memory_bytes: usize,
    active_io_slots: usize,
    peak_active_io_slots: usize,
    completed_tasks: u64,
    panicked_tasks: u64,
    cancelled_before_start: u64,
    cancelled_after_start: u64,
    active_project: Option<PathBuf>,
    reprioritizations: u64,
    priority_admissions_while_background_waits: usize,
    active_project_navigation_pending: bool,
}

struct IndexingResourceState {
    policy: IndexingResourcePolicy,
    cpu_pool: rayon::ThreadPool,
    next_id: AtomicU64,
    admission: Mutex<AdmissionState>,
    changed: Notify,
}

#[derive(Clone)]
pub struct IndexingResourceGovernor {
    state: Arc<IndexingResourceState>,
}

impl fmt::Debug for IndexingResourceGovernor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("IndexingResourceGovernor")
            .field("snapshot", &self.snapshot())
            .finish()
    }
}

impl IndexingResourceGovernor {
    pub fn new(policy: IndexingResourcePolicy) -> Self {
        let cpu_pool = rayon::ThreadPoolBuilder::new()
            .num_threads(policy.cpu_lanes())
            .thread_name(|index| format!("ruby-fast-lsp-index-{index}"))
            .build()
            .expect(
                "INVARIANT VIOLATED: the server-owned indexing CPU pool could not be created. \
                 This is a bug because every background indexing phase must execute inside the \
                 bounded process pool. Fix: inspect the configured positive CPU lane budget and \
                 host thread availability.",
            );
        Self {
            state: Arc::new(IndexingResourceState {
                policy,
                cpu_pool,
                next_id: AtomicU64::new(0),
                admission: Mutex::new(AdmissionState {
                    queued: Vec::new(),
                    active_tasks: 0,
                    peak_active_tasks: 0,
                    active_cpu_lanes: 0,
                    peak_active_cpu_lanes: 0,
                    active_transient_memory_bytes: 0,
                    peak_active_transient_memory_bytes: 0,
                    active_io_slots: 0,
                    peak_active_io_slots: 0,
                    completed_tasks: 0,
                    panicked_tasks: 0,
                    cancelled_before_start: 0,
                    cancelled_after_start: 0,
                    active_project: None,
                    reprioritizations: 0,
                    priority_admissions_while_background_waits: 0,
                    active_project_navigation_pending: false,
                }),
                changed: Notify::new(),
            }),
        }
    }

    pub fn policy(&self) -> IndexingResourcePolicy {
        self.state.policy
    }

    pub fn snapshot(&self) -> IndexingResourceSnapshot {
        let admission = self.state.admission.lock();
        IndexingResourceSnapshot {
            cpu_lane_limit: self.state.policy.cpu_lanes(),
            top_level_task_limit: self.state.policy.top_level_tasks(),
            transient_memory_limit_bytes: self.state.policy.transient_memory_limit_bytes(),
            io_slot_limit: self.state.policy.io_slots(),
            queued_tasks: admission.queued.len(),
            active_tasks: admission.active_tasks,
            peak_active_tasks: admission.peak_active_tasks,
            active_cpu_lanes: admission.active_cpu_lanes,
            peak_active_cpu_lanes: admission.peak_active_cpu_lanes,
            active_transient_memory_bytes: admission.active_transient_memory_bytes,
            peak_active_transient_memory_bytes: admission.peak_active_transient_memory_bytes,
            active_io_slots: admission.active_io_slots,
            peak_active_io_slots: admission.peak_active_io_slots,
            completed_tasks: admission.completed_tasks,
            panicked_tasks: admission.panicked_tasks,
            cancelled_before_start: admission.cancelled_before_start,
            cancelled_after_start: admission.cancelled_after_start,
            active_project: admission.active_project.clone(),
            reprioritizations: admission.reprioritizations,
            active_project_navigation_pending: admission.active_project_navigation_pending,
        }
    }

    pub fn prioritize_active_project(&self, project_root: &Path) {
        self.prioritize_active_project_with_navigation_pending(project_root, false);
    }

    pub fn prioritize_active_project_with_navigation_pending(
        &self,
        project_root: &Path,
        navigation_pending: bool,
    ) {
        let mut changed = false;
        {
            let mut admission = self.state.admission.lock();
            if admission.active_project.as_deref() != Some(project_root) {
                admission.active_project = Some(project_root.to_path_buf());
                admission.reprioritizations = checked_add_u64(
                    admission.reprioritizations,
                    1,
                    "indexing resource reprioritization count",
                );
                changed = true;
            }
            if admission.active_project_navigation_pending != navigation_pending {
                admission.active_project_navigation_pending = navigation_pending;
                changed = true;
            }
            let active_project = admission.active_project.clone();
            for queued in &mut admission.queued {
                let priority = effective_priority(&queued.spec, active_project.as_deref());
                if queued.priority != priority {
                    queued.priority = priority;
                    changed = true;
                }
            }
        }
        if changed {
            self.state.changed.notify_waiters();
        }
    }

    pub fn mark_project_navigation_pending_if_active(&self, project_root: &Path) {
        let mut changed = false;
        {
            let mut admission = self.state.admission.lock();
            if admission.active_project.as_deref() == Some(project_root)
                && !admission.active_project_navigation_pending
            {
                admission.active_project_navigation_pending = true;
                changed = true;
            }
        }
        if changed {
            self.state.changed.notify_waiters();
        }
    }

    pub fn mark_project_navigation_complete_if_active(&self, project_root: &Path) {
        let mut changed = false;
        {
            let mut admission = self.state.admission.lock();
            if admission.active_project.as_deref() == Some(project_root)
                && admission.active_project_navigation_pending
            {
                admission.active_project_navigation_pending = false;
                changed = true;
            }
        }
        if changed {
            self.state.changed.notify_waiters();
        }
    }

    pub fn project_navigation_reservation(
        &self,
        project_root: PathBuf,
    ) -> ProjectNavigationReservation {
        ProjectNavigationReservation {
            state: self.state.clone(),
            project_root,
        }
    }

    pub fn project_parallel_cpu_lanes(&self, project_root: &Path) -> usize {
        let admission = self.state.admission.lock();
        if admission.active_project.as_deref() == Some(project_root)
            && admission.active_project_navigation_pending
        {
            self.state.policy.cpu_lanes().saturating_sub(1).max(1)
        } else {
            self.state.policy.cooperative_parallel_cpu_lanes()
        }
    }

    pub async fn run_cpu<T, F>(&self, label: &'static str, task: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce() -> T + Send + 'static,
    {
        self.run_cpu_cancellable(label, None, task).await
    }

    pub async fn run_cpu_cancellable<T, F>(
        &self,
        label: &'static str,
        cancellation: Option<CancellationToken>,
        task: F,
    ) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce() -> T + Send + 'static,
    {
        let spec = IndexingWorkSpec::new(
            None,
            IndexingResourcePriority::Background,
            self.state.policy.cpu_lanes(),
            DEFAULT_PARALLEL_TASK_MEMORY_BYTES
                .min(self.state.policy.transient_memory_limit_bytes()),
            1.min(self.state.policy.io_slots()),
        );
        self.run_parallel_with_resources(label, spec, cancellation, task)
            .await
    }

    pub async fn run_with_resources<T, F>(
        &self,
        label: &'static str,
        spec: IndexingWorkSpec,
        cancellation: Option<CancellationToken>,
        task: F,
    ) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce() -> T + Send + 'static,
    {
        self.run_with_resources_inner(label, spec, cancellation, false, task)
            .await
    }

    pub async fn run_parallel_with_resources<T, F>(
        &self,
        label: &'static str,
        spec: IndexingWorkSpec,
        cancellation: Option<CancellationToken>,
        task: F,
    ) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce() -> T + Send + 'static,
    {
        assert_eq!(
            spec.cpu_lanes(),
            self.state.policy.cpu_lanes(),
            "INVARIANT VIOLATED: parallel indexing work reserved {} CPU lanes but the owned Rayon pool has {} lanes. This is a bug because nested Rayon work could exceed its declared resource claim. Fix: reserve the complete process indexing CPU pool for parallel work.",
            spec.cpu_lanes(),
            self.state.policy.cpu_lanes(),
        );
        self.run_with_resources_inner(label, spec, cancellation, true, task)
            .await
    }

    pub async fn run_cooperative_parallel_with_resources<T, F>(
        &self,
        label: &'static str,
        spec: IndexingWorkSpec,
        cancellation: Option<CancellationToken>,
        task: F,
    ) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce() -> T + Send + 'static,
    {
        assert!(
            spec.cpu_lanes() <= self.state.policy.cooperative_parallel_cpu_lanes(),
            "INVARIANT VIOLATED: cooperative parallel indexing reserved {} CPU lanes but the policy's per-task partition is {} lanes. This is a bug because one cooperative task could serialize sibling project work despite a multi-task budget. Fix: derive the claim from IndexingResourcePolicy::cooperative_parallel_cpu_lanes.",
            spec.cpu_lanes(),
            self.state.policy.cooperative_parallel_cpu_lanes(),
        );
        self.run_owned_parallel_with_resources(label, spec, cancellation, task)
            .await
    }

    pub async fn run_partitioned_parallel_with_resources<T, F>(
        &self,
        label: &'static str,
        spec: IndexingWorkSpec,
        cancellation: Option<CancellationToken>,
        task: F,
    ) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce() -> T + Send + 'static,
    {
        assert!(
            spec.cpu_lanes() < self.state.policy.cpu_lanes(),
            "INVARIANT VIOLATED: partitioned parallel indexing reserved {} CPU lanes from a {}-lane process pool. This is a bug because a full-width task must use the server-owned shared pool instead of creating a redundant private pool. Fix: route full-width work through run_parallel_with_resources and reserve partitioned pools only for strict subsets.",
            spec.cpu_lanes(),
            self.state.policy.cpu_lanes(),
        );
        self.run_owned_parallel_with_resources(label, spec, cancellation, task)
            .await
    }

    async fn run_owned_parallel_with_resources<T, F>(
        &self,
        label: &'static str,
        spec: IndexingWorkSpec,
        cancellation: Option<CancellationToken>,
        task: F,
    ) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce() -> T + Send + 'static,
    {
        let lanes = spec.cpu_lanes();
        let lease = self.acquire(label, spec, cancellation).await?;
        tokio::task::spawn_blocking(move || {
            let mut lease = lease;
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(lanes)
                .thread_name(|index| format!("ruby-fast-lsp-partition-{index}"))
                .build()
                .expect(
                    "INVARIANT VIOLATED: a partitioned indexing Rayon pool could not be created. This is a bug because its positive lane count was admitted under the process resource budget. Fix: inspect host thread creation failure and the admitted lane accounting.",
                );
            let output = pool.install(task);
            lease.completed = true;
            output
        })
        .await
        .map_err(|join_error| {
            let panic_message = join_error
                .try_into_panic()
                .ok()
                .map(|payload| {
                    if let Some(message) = payload.downcast_ref::<&str>() {
                        (*message).to_string()
                    } else if let Some(message) = payload.downcast_ref::<String>() {
                        message.clone()
                    } else {
                        "non-string panic payload".to_string()
                    }
                })
                .map(|message| format!("; panic: {message}"))
                .unwrap_or_default();
            anyhow::anyhow!("{label} partitioned blocking worker failed{panic_message}")
        })
    }

    pub async fn run_async_with_resources<T, F>(
        &self,
        label: &'static str,
        spec: IndexingWorkSpec,
        cancellation: Option<CancellationToken>,
        task: F,
    ) -> Result<T>
    where
        F: std::future::Future<Output = T>,
    {
        let mut lease = self.acquire(label, spec, cancellation).await?;
        let output = task.await;
        lease.completed = true;
        drop(lease);
        Ok(output)
    }

    async fn run_with_resources_inner<T, F>(
        &self,
        label: &'static str,
        spec: IndexingWorkSpec,
        cancellation: Option<CancellationToken>,
        parallel: bool,
        task: F,
    ) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce() -> T + Send + 'static,
    {
        let lease = self.acquire(label, spec, cancellation).await?;
        let state = self.state.clone();
        tokio::task::spawn_blocking(move || {
            let mut lease = lease;
            let output = if parallel {
                state.cpu_pool.install(task)
            } else {
                task()
            };
            lease.completed = true;
            output
        })
        .await
        .with_context(|| format!("{label} blocking worker failed"))
    }

    async fn acquire(
        &self,
        label: &'static str,
        spec: IndexingWorkSpec,
        cancellation: Option<CancellationToken>,
    ) -> Result<ActiveResourceLease> {
        validate_request_fits_policy(&spec, self.state.policy);
        let id = self.state.next_id.fetch_add(1, Ordering::Relaxed);
        assert!(
            id != u64::MAX,
            "INVARIANT VIOLATED: indexing resource ticket overflowed. This is a bug because one server cannot enqueue 2^64 work items. Fix: inspect the loop continuously rebuilding indexing products."
        );
        {
            let mut admission = self.state.admission.lock();
            let priority = effective_priority(&spec, admission.active_project.as_deref());
            admission.queued.push(QueuedWork {
                id,
                spec,
                priority,
                insertion_order: id,
            });
        }
        let mut registration = QueuedTaskRegistration {
            state: self.state.clone(),
            id,
            admitted: false,
        };

        loop {
            if cancellation
                .as_ref()
                .is_some_and(CancellationToken::is_cancelled)
            {
                record_cancelled_before_start(&self.state);
                return Err(anyhow!(
                    "{label} was cancelled before entering the process indexing resource budget"
                ));
            }
            let changed = self.state.changed.notified();
            {
                let mut admission = self.state.admission.lock();
                if cancellation
                    .as_ref()
                    .is_some_and(CancellationToken::is_cancelled)
                {
                    drop(admission);
                    record_cancelled_before_start(&self.state);
                    return Err(anyhow!(
                        "{label} was cancelled before entering the process indexing resource budget"
                    ));
                }
                if let Some(winner) = best_admissible_entry_index(&admission, self.state.policy) {
                    if admission.queued[winner].id == id {
                        let background_waiting = admission.queued.iter().any(|queued| {
                            queued.id != id
                                && queued.priority == IndexingResourcePriority::Background
                                && request_fits_available(
                                    &queued.spec,
                                    &admission,
                                    self.state.policy,
                                )
                        });
                        let queued = admission.queued.swap_remove(winner);
                        reserve_resources(&mut admission, &queued.spec, self.state.policy);
                        if queued.priority == IndexingResourcePriority::Background
                            || !background_waiting
                        {
                            admission.priority_admissions_while_background_waits = 0;
                        } else {
                            admission.priority_admissions_while_background_waits =
                                checked_add_usize(
                                    admission.priority_admissions_while_background_waits,
                                    1,
                                    "indexing resource fairness counter",
                                );
                            assert!(
                                admission.priority_admissions_while_background_waits
                                    <= MAX_PRIORITY_ADMISSIONS_WHILE_BACKGROUND_WAITS,
                                "INVARIANT VIOLATED: weighted resource admission exceeded its bounded priority burst. This is a bug because an admitted background coordinator could starve behind active-project phases. Fix: route every resource admission through the fairness-aware selector."
                            );
                        }
                        registration.admitted = true;
                        return Ok(ActiveResourceLease {
                            state: self.state.clone(),
                            spec: queued.spec,
                            completed: false,
                        });
                    }
                }
            }
            match cancellation.as_ref() {
                Some(cancellation) => {
                    tokio::select! {
                        biased;
                        _ = cancellation.cancelled() => {
                            record_cancelled_before_start(&self.state);
                            return Err(anyhow!(
                                "{label} was cancelled before entering the process indexing resource budget"
                            ));
                        }
                        _ = changed => {}
                    }
                }
                None => changed.await,
            }
        }
    }
}

impl Default for IndexingResourceGovernor {
    fn default() -> Self {
        Self::new(IndexingResourcePolicy::for_current_host())
    }
}

fn effective_priority(
    spec: &IndexingWorkSpec,
    active_project: Option<&Path>,
) -> IndexingResourcePriority {
    if active_project.is_some() && spec.project_root() == active_project {
        IndexingResourcePriority::ActiveDocument
    } else {
        spec.requested_priority
    }
}

fn validate_request_fits_policy(spec: &IndexingWorkSpec, policy: IndexingResourcePolicy) {
    assert!(
        spec.cpu_lanes() <= policy.cpu_lanes(),
        "INVARIANT VIOLATED: indexing work requested {} CPU lanes from a {}-lane budget. This is a bug because impossible work would remain queued forever. Fix: split the work or cap its declared CPU claim.",
        spec.cpu_lanes(),
        policy.cpu_lanes(),
    );
    assert!(
        spec.transient_memory_bytes() <= policy.transient_memory_limit_bytes(),
        "INVARIANT VIOLATED: indexing work requested {} transient-memory bytes from a {}-byte budget. This is a bug because impossible work would remain queued forever. Fix: split the product or cap its bounded input before admission.",
        spec.transient_memory_bytes(),
        policy.transient_memory_limit_bytes(),
    );
    assert!(
        spec.io_slots() <= policy.io_slots(),
        "INVARIANT VIOLATED: indexing work requested {} I/O slots from a {}-slot budget. This is a bug because impossible work would remain queued forever. Fix: split the scan or cap its declared I/O claim.",
        spec.io_slots(),
        policy.io_slots(),
    );
}

fn request_fits_available(
    spec: &IndexingWorkSpec,
    admission: &AdmissionState,
    policy: IndexingResourcePolicy,
) -> bool {
    !project_parallel_blocked_by_active_navigation(spec, admission)
        && admission.active_tasks < policy.top_level_tasks()
        && admission
            .active_cpu_lanes
            .checked_add(spec.cpu_lanes())
            .is_some_and(|total| total <= policy.cpu_lanes())
        && admission
            .active_transient_memory_bytes
            .checked_add(spec.transient_memory_bytes())
            .is_some_and(|total| total <= policy.transient_memory_limit_bytes())
        && admission
            .active_io_slots
            .checked_add(spec.io_slots())
            .is_some_and(|total| total <= policy.io_slots())
}

fn project_parallel_blocked_by_active_navigation(
    spec: &IndexingWorkSpec,
    admission: &AdmissionState,
) -> bool {
    spec.project_parallel
        && admission.active_project_navigation_pending
        && admission.active_project.as_deref() != spec.project_root()
}

fn best_admissible_entry_index(
    admission: &AdmissionState,
    policy: IndexingResourcePolicy,
) -> Option<usize> {
    if admission.priority_admissions_while_background_waits
        >= MAX_PRIORITY_ADMISSIONS_WHILE_BACKGROUND_WAITS
    {
        if let Some((index, _)) = admission
            .queued
            .iter()
            .enumerate()
            .filter(|(_, queued)| {
                queued.priority == IndexingResourcePriority::Background
                    && request_fits_available(&queued.spec, admission, policy)
            })
            .min_by_key(|(_, queued)| (queued.insertion_order, queued.spec.project_root.as_deref()))
        {
            return Some(index);
        }
    }
    admission
        .queued
        .iter()
        .enumerate()
        .filter(|(_, queued)| request_fits_available(&queued.spec, admission, policy))
        .min_by_key(|(_, queued)| {
            (
                queued.priority,
                queued.insertion_order,
                queued.spec.project_root.as_deref(),
            )
        })
        .map(|(index, _)| index)
}

fn reserve_resources(
    admission: &mut AdmissionState,
    spec: &IndexingWorkSpec,
    policy: IndexingResourcePolicy,
) {
    assert!(
        request_fits_available(spec, admission, policy),
        "INVARIANT VIOLATED: weighted indexing resources were reserved after the request stopped fitting. This is a bug because CPU, memory, I/O, and task admission must be one atomic locked transition. Fix: never release the admission lock between selection and reservation."
    );
    admission.active_tasks = checked_add_usize(
        admission.active_tasks,
        1,
        "active indexing resource task count",
    );
    admission.active_cpu_lanes = checked_add_usize(
        admission.active_cpu_lanes,
        spec.cpu_lanes(),
        "active indexing CPU lane count",
    );
    admission.active_transient_memory_bytes = checked_add_usize(
        admission.active_transient_memory_bytes,
        spec.transient_memory_bytes(),
        "active indexing transient-memory byte count",
    );
    admission.active_io_slots = checked_add_usize(
        admission.active_io_slots,
        spec.io_slots(),
        "active indexing I/O slot count",
    );
    admission.peak_active_tasks = admission.peak_active_tasks.max(admission.active_tasks);
    admission.peak_active_cpu_lanes = admission
        .peak_active_cpu_lanes
        .max(admission.active_cpu_lanes);
    admission.peak_active_transient_memory_bytes = admission
        .peak_active_transient_memory_bytes
        .max(admission.active_transient_memory_bytes);
    admission.peak_active_io_slots = admission
        .peak_active_io_slots
        .max(admission.active_io_slots);
}

fn record_cancelled_before_start(state: &Arc<IndexingResourceState>) {
    let mut admission = state.admission.lock();
    admission.cancelled_before_start = checked_add_u64(
        admission.cancelled_before_start,
        1,
        "indexing resource pre-admission cancellation count",
    );
}

struct QueuedTaskRegistration {
    state: Arc<IndexingResourceState>,
    id: u64,
    admitted: bool,
}

impl Drop for QueuedTaskRegistration {
    fn drop(&mut self) {
        if self.admitted {
            return;
        }
        let mut admission = self.state.admission.lock();
        if let Some(index) = admission
            .queued
            .iter()
            .position(|queued| queued.id == self.id)
        {
            admission.queued.swap_remove(index);
            drop(admission);
            self.state.changed.notify_waiters();
        }
    }
}

struct ActiveResourceLease {
    state: Arc<IndexingResourceState>,
    spec: IndexingWorkSpec,
    completed: bool,
}

pub struct ProjectNavigationReservation {
    state: Arc<IndexingResourceState>,
    project_root: PathBuf,
}

impl Drop for ProjectNavigationReservation {
    fn drop(&mut self) {
        let mut changed = false;
        {
            let mut admission = self.state.admission.lock();
            if admission.active_project.as_deref() == Some(self.project_root.as_path())
                && admission.active_project_navigation_pending
            {
                admission.active_project_navigation_pending = false;
                changed = true;
            }
        }
        if changed {
            self.state.changed.notify_waiters();
        }
    }
}

impl Drop for ActiveResourceLease {
    fn drop(&mut self) {
        let mut admission = self.state.admission.lock();
        admission.active_tasks = checked_sub_usize(
            admission.active_tasks,
            1,
            "active indexing resource task count",
        );
        admission.active_cpu_lanes = checked_sub_usize(
            admission.active_cpu_lanes,
            self.spec.cpu_lanes(),
            "active indexing CPU lane count",
        );
        admission.active_transient_memory_bytes = checked_sub_usize(
            admission.active_transient_memory_bytes,
            self.spec.transient_memory_bytes(),
            "active indexing transient-memory byte count",
        );
        admission.active_io_slots = checked_sub_usize(
            admission.active_io_slots,
            self.spec.io_slots(),
            "active indexing I/O slot count",
        );
        if std::thread::panicking() {
            admission.panicked_tasks = checked_add_u64(
                admission.panicked_tasks,
                1,
                "indexing resource panicked task count",
            );
        } else if self.completed {
            admission.completed_tasks = checked_add_u64(
                admission.completed_tasks,
                1,
                "indexing resource completed task count",
            );
        } else {
            admission.cancelled_after_start = checked_add_u64(
                admission.cancelled_after_start,
                1,
                "indexing resource post-admission cancellation count",
            );
        }
        drop(admission);
        self.state.changed.notify_waiters();
    }
}

fn checked_add_usize(current: usize, amount: usize, label: &'static str) -> usize {
    current.checked_add(amount).unwrap_or_else(|| {
        panic!(
            "INVARIANT VIOLATED: {label} overflowed. This is a bug because one process cannot reserve more than usize::MAX resources. Fix: inspect corrupt work estimates or leaked resource registrations."
        )
    })
}

fn checked_sub_usize(current: usize, amount: usize, label: &'static str) -> usize {
    current.checked_sub(amount).unwrap_or_else(|| {
        panic!(
            "INVARIANT VIOLATED: {label} underflowed. This is a bug because a resource registration released more than it reserved. Fix: preserve one exact RAII lease per atomic admission."
        )
    })
}

fn checked_add_u64(current: u64, amount: u64, label: &'static str) -> u64 {
    current.checked_add(amount).unwrap_or_else(|| {
        panic!(
            "INVARIANT VIOLATED: {label} overflowed. This is a bug because one server cannot record 2^64 indexing events. Fix: inspect the runaway indexing loop."
        )
    })
}

#[cfg(test)]
mod tests {
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
        let governor =
            IndexingResourceGovernor::new(IndexingResourcePolicy::with_limits(4, 3, 100, 2));
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
        let governor =
            IndexingResourceGovernor::new(IndexingResourcePolicy::with_limits(1, 1, 100, 1));
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
        let governor =
            IndexingResourceGovernor::new(IndexingResourcePolicy::with_limits(1, 1, 100, 1));
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
        let governor =
            IndexingResourceGovernor::new(IndexingResourcePolicy::with_limits(1, 1, 100, 1));
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
        let governor =
            IndexingResourceGovernor::new(IndexingResourcePolicy::with_limits(2, 1, 100, 1));
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
}
