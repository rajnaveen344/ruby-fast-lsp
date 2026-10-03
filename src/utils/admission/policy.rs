//! Resource budgets, work claims, priorities, and observable admission snapshots.

use std::path::{Path, PathBuf};

const MAX_DEFAULT_CPU_LANES: usize = 6;
const RESERVED_HOST_CPU_LANES: usize = 2;
const DEFAULT_TOP_LEVEL_TASKS: usize = 2;
const MIB: usize = 1024 * 1024;
const DEFAULT_TRANSIENT_MEMORY_LIMIT_BYTES: usize = 512 * MIB;
const DEFAULT_IO_SLOTS: usize = 2;
pub(super) const DEFAULT_PARALLEL_TASK_MEMORY_BYTES: usize = 256 * MIB;

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
        invariant!(
            cpu_lanes > 0,
            what = "the indexing CPU lane budget is zero",
            why = "no indexing work could make progress",
            fix = "configure at least one CPU lane",
        );
        invariant!(
            top_level_tasks > 0,
            what = "the indexing task admission budget is zero",
            why = "no coordinator phase could enter the worker pool",
            fix = "configure at least one top-level task",
        );
        invariant!(
            transient_memory_limit_bytes > 0,
            what = "the indexing transient-memory budget is zero",
            why = "every indexing task requires bounded temporary allocations",
            fix = "configure a positive transient-memory budget",
        );
        invariant!(
            io_slots > 0,
            what = "the indexing I/O budget is zero",
            why = "project discovery and source loading could never make progress",
            fix = "configure at least one I/O slot",
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
                unreachable_invariant!(
                    what = "cooperative indexing divided by a zero task limit",
                    why = "policy construction rejects zero top-level tasks",
                    fix = "preserve the validated resource policy when deriving cooperative lane partitions",
                )
            })
            .max(1)
    }

    pub(super) fn for_current_host() -> Self {
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
    pub(super) project_root: Option<PathBuf>,
    pub(super) requested_priority: IndexingResourcePriority,
    cpu_lanes: usize,
    transient_memory_bytes: usize,
    io_slots: usize,
    pub(super) project_parallel: bool,
}

impl IndexingWorkSpec {
    pub fn new(
        project_root: Option<PathBuf>,
        priority: IndexingResourcePriority,
        cpu_lanes: usize,
        transient_memory_bytes: usize,
        io_slots: usize,
    ) -> Self {
        invariant!(
            cpu_lanes > 0,
            what = "an indexing work request claims zero CPU lanes",
            why = "admitted work could execute without CPU accounting",
            fix = "reserve at least one CPU lane",
        );
        invariant!(
            transient_memory_bytes > 0,
            what = "an indexing work request claims zero transient-memory bytes",
            why = "admitted work could allocate outside memory accounting",
            fix = "provide a conservative positive transient-memory estimate",
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
        invariant!(
            self.project_root.is_some(),
            what = "project-parallel work has no project root",
            why = "the active-project reservation cannot distinguish its owner",
            fix = "attach the exact isolated project root before marking a work request project-parallel",
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
