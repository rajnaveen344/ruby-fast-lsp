//! Weighted, fairness-aware admission decisions over the locked queue state.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::{checked_add_u64, checked_add_usize, IndexingResourceState};
use super::{IndexingResourcePolicy, IndexingResourcePriority, IndexingWorkSpec};

pub(super) const MAX_PRIORITY_ADMISSIONS_WHILE_BACKGROUND_WAITS: usize = 1;

#[derive(Debug, Clone)]
pub(super) struct QueuedWork {
    pub(super) id: u64,
    pub(super) spec: IndexingWorkSpec,
    pub(super) priority: IndexingResourcePriority,
    pub(super) insertion_order: u64,
}

#[derive(Debug)]
pub(super) struct AdmissionState {
    pub(super) queued: Vec<QueuedWork>,
    pub(super) active_tasks: usize,
    pub(super) peak_active_tasks: usize,
    pub(super) active_cpu_lanes: usize,
    pub(super) peak_active_cpu_lanes: usize,
    pub(super) active_transient_memory_bytes: usize,
    pub(super) peak_active_transient_memory_bytes: usize,
    pub(super) active_io_slots: usize,
    pub(super) peak_active_io_slots: usize,
    pub(super) completed_tasks: u64,
    pub(super) panicked_tasks: u64,
    pub(super) cancelled_before_start: u64,
    pub(super) cancelled_after_start: u64,
    pub(super) active_project: Option<PathBuf>,
    pub(super) reprioritizations: u64,
    pub(super) priority_admissions_while_background_waits: usize,
    pub(super) active_project_navigation_pending: bool,
}

pub(super) fn effective_priority(
    spec: &IndexingWorkSpec,
    active_project: Option<&Path>,
) -> IndexingResourcePriority {
    if active_project.is_some() && spec.project_root() == active_project {
        IndexingResourcePriority::ActiveDocument
    } else {
        spec.requested_priority
    }
}

pub(super) fn validate_request_fits_policy(
    spec: &IndexingWorkSpec,
    policy: IndexingResourcePolicy,
) {
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

pub(super) fn request_fits_available(
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

pub(super) fn best_admissible_entry_index(
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

pub(super) fn reserve_resources(
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

pub(super) fn record_cancelled_before_start(state: &Arc<IndexingResourceState>) {
    let mut admission = state.admission.lock();
    admission.cancelled_before_start = checked_add_u64(
        admission.cancelled_before_start,
        1,
        "indexing resource pre-admission cancellation count",
    );
}
