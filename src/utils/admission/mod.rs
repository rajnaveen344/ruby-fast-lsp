//! Process-wide indexing resource governor: CPU, memory, I/O, and task admission.

mod admission;
mod lease;
mod policy;

use crate::invariant::ExpectInvariant;
use admission::{
    best_admissible_entry_index, effective_priority, record_cancelled_before_start,
    request_fits_available, reserve_resources, validate_request_fits_policy, AdmissionState,
    QueuedWork, MAX_PRIORITY_ADMISSIONS_WHILE_BACKGROUND_WAITS,
};
pub use lease::ProjectNavigationReservation;
use lease::{ActiveResourceLease, QueuedTaskRegistration};
use policy::DEFAULT_PARALLEL_TASK_MEMORY_BYTES;
pub use policy::{
    IndexingResourcePolicy, IndexingResourcePriority, IndexingResourceSnapshot, IndexingWorkSpec,
};

use anyhow::{anyhow, Context, Result};
use parking_lot::Mutex;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

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
            .expect_invariant(
                "the server-owned indexing CPU pool could not be created",
                "every background indexing phase must execute inside the bounded process pool",
                "inspect the configured positive CPU lane budget and host thread availability",
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
        invariant_eq!(
            spec.cpu_lanes(),
            self.state.policy.cpu_lanes(),
            what = "parallel indexing work reserved {} CPU lanes but the owned Rayon pool has {} lanes",
            why = "nested Rayon work could exceed its declared resource claim",
            fix = "reserve the complete process indexing CPU pool for parallel work",
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
        invariant!(
            spec.cpu_lanes() <= self.state.policy.cooperative_parallel_cpu_lanes(),
            what = "cooperative indexing reserved {} CPU lanes but the per-task partition is {}",
            why = "one task could serialize sibling project work",
            fix = "derive the claim from cooperative_parallel_cpu_lanes",
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
        invariant!(
            spec.cpu_lanes() < self.state.policy.cpu_lanes(),
            what = "partitioned indexing reserved {} CPU lanes from a {}-lane pool",
            why = "full-width work must use the shared server pool",
            fix = "route full-width work through run_parallel_with_resources",
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
                .expect_invariant(
                    "a partitioned indexing Rayon pool could not be created",
                    "its positive lane count was admitted under the process resource budget",
                    "inspect host thread creation failure and the admitted lane accounting",
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
        invariant!(
            id != u64::MAX,
            what = "indexing resource ticket overflowed",
            why = "one server cannot enqueue 2^64 work items",
            fix = "inspect the loop continuously rebuilding indexing products",
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
                            invariant!(
                                admission.priority_admissions_while_background_waits
                                    <= MAX_PRIORITY_ADMISSIONS_WHILE_BACKGROUND_WAITS,
                                what = "weighted resource admission exceeded its bounded priority burst",
                                why = "an admitted background coordinator could starve behind active-project phases",
                                fix = "route every resource admission through the fairness-aware selector",
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

fn checked_add_usize(current: usize, amount: usize, label: &'static str) -> usize {
    current.checked_add(amount).unwrap_or_else(|| {
        unreachable_invariant!(
            what = "{label} overflowed",
            why = "one process cannot reserve more than usize::MAX resources",
            fix = "inspect corrupt work estimates or leaked resource registrations",
            label = label,
        )
    })
}

fn checked_sub_usize(current: usize, amount: usize, label: &'static str) -> usize {
    current.checked_sub(amount).unwrap_or_else(|| {
        unreachable_invariant!(
            what = "{label} underflowed",
            why = "a resource registration released more than it reserved",
            fix = "preserve one exact RAII lease per atomic admission",
            label = label,
        )
    })
}

fn checked_add_u64(current: u64, amount: u64, label: &'static str) -> u64 {
    current.checked_add(amount).unwrap_or_else(|| {
        unreachable_invariant!(
            what = "{label} overflowed",
            why = "one server cannot record 2^64 indexing events",
            fix = "inspect the runaway indexing loop",
            label = label,
        )
    })
}

#[cfg(test)]
mod tests;
