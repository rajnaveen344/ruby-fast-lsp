//! RAII registrations and leases that release admission state on drop.

use std::path::PathBuf;
use std::sync::Arc;

use super::{checked_add_u64, checked_sub_usize, IndexingResourceState, IndexingWorkSpec};

pub(super) struct QueuedTaskRegistration {
    pub(super) state: Arc<IndexingResourceState>,
    pub(super) id: u64,
    pub(super) admitted: bool,
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

pub(super) struct ActiveResourceLease {
    pub(super) state: Arc<IndexingResourceState>,
    pub(super) spec: IndexingWorkSpec,
    pub(super) completed: bool,
}

pub struct ProjectNavigationReservation {
    pub(super) state: Arc<IndexingResourceState>,
    pub(super) project_root: PathBuf,
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
