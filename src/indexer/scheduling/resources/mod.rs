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
mod tests;
