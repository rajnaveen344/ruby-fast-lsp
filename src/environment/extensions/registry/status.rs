use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionStatusReport {
    pub id: String,
    pub name: Option<String>,
    pub version: Option<String>,
    pub status: String,
    pub last_error: Option<String>,
    pub capabilities: Vec<String>,
    pub permissions: Vec<String>,
    pub watched_files: Vec<String>,
    pub process_commands: Vec<String>,
    pub indexed_call_names: Vec<String>,
    #[serde(default)]
    pub telemetry: ExtensionTelemetryReport,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionTelemetryReport {
    pub guest_calls: u64,
    pub lifecycle_calls: u64,
    pub index_calls: u64,
    pub event_calls: u64,
    pub guest_failures: u64,
    pub guest_traps: u64,
    pub resource_limit_failures: u64,
    pub disablements: u64,
    pub rejected_outputs: u64,
    pub patch_conflicts: u64,
    pub emitted_index_patches: u64,
    pub emitted_execution_contexts: u64,
    pub emitted_response_patches: u64,
    pub emitted_command_patches: u64,
    pub emitted_process_requests: u64,
    pub requested_reindex_files: u64,
    pub total_guest_time_ns: u64,
    pub max_guest_time_ns: u64,
    pub project_instance_creations: u64,
    pub project_instance_failures: u64,
    pub total_project_instance_time_ns: u64,
    pub max_project_instance_time_ns: u64,
    pub project_instances: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ExtensionStatusParams {}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExtensionStatusResponse {
    pub extensions: Vec<ExtensionStatusReport>,
}

#[derive(Debug, Default)]
pub(in crate::environment::extensions) struct ExtensionTelemetry {
    guest_calls: AtomicU64,
    lifecycle_calls: AtomicU64,
    index_calls: AtomicU64,
    event_calls: AtomicU64,
    guest_failures: AtomicU64,
    guest_traps: AtomicU64,
    resource_limit_failures: AtomicU64,
    disablements: AtomicU64,
    rejected_outputs: AtomicU64,
    patch_conflicts: AtomicU64,
    emitted_index_patches: AtomicU64,
    emitted_execution_contexts: AtomicU64,
    emitted_response_patches: AtomicU64,
    emitted_command_patches: AtomicU64,
    emitted_process_requests: AtomicU64,
    requested_reindex_files: AtomicU64,
    total_guest_time_ns: AtomicU64,
    max_guest_time_ns: AtomicU64,
    project_instance_creations: AtomicU64,
    project_instance_failures: AtomicU64,
    total_project_instance_time_ns: AtomicU64,
    max_project_instance_time_ns: AtomicU64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::environment::extensions) enum GuestCallKind {
    Lifecycle,
    Index,
    Event,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::environment::extensions) enum ExtensionStatus {
    Discovered,
    Loaded,
    Deactivated,
    Slow { reason: String },
    Failed { reason: String },
}

impl ExtensionStatus {
    pub(in crate::environment::extensions) fn from_failure(reason: impl Into<String>) -> Self {
        let reason = reason.into();
        if reason.contains("wall-clock deadline") {
            Self::Slow { reason }
        } else {
            Self::Failed { reason }
        }
    }
}

impl ExtensionTelemetry {
    pub(in crate::environment::extensions) fn record_call(
        &self,
        kind: GuestCallKind,
        elapsed: Duration,
        output: Option<&ruby_fast_lsp_extension_api::ExtensionOutput>,
        failure: Option<&str>,
    ) {
        saturating_increment(&self.guest_calls, 1);
        match kind {
            GuestCallKind::Lifecycle => saturating_increment(&self.lifecycle_calls, 1),
            GuestCallKind::Index => saturating_increment(&self.index_calls, 1),
            GuestCallKind::Event => saturating_increment(&self.event_calls, 1),
        }
        if let Some(reason) = failure {
            self.record_guest_failure(reason);
        }
        if let Some(output) = output {
            saturating_increment(
                &self.emitted_index_patches,
                u64::try_from(output.index_patches.len()).expect(
                    "INVARIANT VIOLATED: index patch count does not fit in u64. This is a bug because extension output is bounded far below u64::MAX. Fix: enforce output bounds before telemetry recording.",
                ),
            );
            saturating_increment(
                &self.emitted_execution_contexts,
                u64::try_from(output.execution_contexts.len()).expect(
                    "INVARIANT VIOLATED: execution-context count does not fit in u64. This is a bug because extension output is bounded far below u64::MAX. Fix: enforce output bounds before telemetry recording.",
                ),
            );
            saturating_increment(
                &self.emitted_response_patches,
                u64::try_from(output.response_patches.len()).expect(
                    "INVARIANT VIOLATED: response patch count does not fit in u64. This is a bug because extension output is bounded far below u64::MAX. Fix: enforce output bounds before telemetry recording.",
                ),
            );
            saturating_increment(
                &self.emitted_command_patches,
                u64::try_from(output.command_patches.len()).expect(
                    "INVARIANT VIOLATED: command patch count does not fit in u64. This is a bug because extension output is bounded far below u64::MAX. Fix: enforce output bounds before telemetry recording.",
                ),
            );
            saturating_increment(
                &self.emitted_process_requests,
                u64::try_from(output.process_requests.len()).expect(
                    "INVARIANT VIOLATED: process request count does not fit in u64. This is a bug because extension output is bounded far below u64::MAX. Fix: enforce output bounds before telemetry recording.",
                ),
            );
            saturating_increment(
                &self.requested_reindex_files,
                u64::try_from(output.reindex_files.len()).expect(
                    "INVARIANT VIOLATED: reindex file count does not fit in u64. This is a bug because extension output is bounded far below u64::MAX. Fix: enforce output bounds before telemetry recording.",
                ),
            );
        }
        let elapsed_ns = u64::try_from(elapsed.as_nanos()).unwrap_or(u64::MAX);
        saturating_increment(&self.total_guest_time_ns, elapsed_ns);
        self.max_guest_time_ns
            .fetch_max(elapsed_ns, Ordering::Relaxed);
    }

    pub(in crate::environment::extensions) fn record_disablement(&self) {
        saturating_increment(&self.disablements, 1);
    }

    pub(in crate::environment::extensions) fn record_rejected_output(&self) {
        saturating_increment(&self.rejected_outputs, 1);
    }

    pub(in crate::environment::extensions) fn record_patch_conflict(&self) {
        saturating_increment(&self.patch_conflicts, 1);
    }

    fn record_guest_failure(&self, reason: &str) {
        saturating_increment(&self.guest_failures, 1);
        let reason = reason.to_ascii_lowercase();
        let trapped = reason.contains("wasm trap")
            || reason.contains("unreachable")
            || reason.contains("fuel")
            || reason.contains("wall-clock deadline");
        if trapped {
            saturating_increment(&self.guest_traps, 1);
        }
        let resource_limited = reason.contains("fuel")
            || reason.contains("wall-clock deadline")
            || (reason.contains("payload") && reason.contains("exceeds max"))
            || (reason.contains("memory")
                && (reason.contains("limit")
                    || reason.contains("grow")
                    || reason.contains("out of bounds")));
        if resource_limited {
            saturating_increment(&self.resource_limit_failures, 1);
        }
    }

    pub(super) fn record_project_instance_creation(
        &self,
        elapsed: Duration,
        failure: Option<&str>,
    ) {
        saturating_increment(&self.project_instance_creations, 1);
        if let Some(reason) = failure {
            saturating_increment(&self.project_instance_failures, 1);
            self.record_guest_failure(reason);
        }
        let elapsed_ns = u64::try_from(elapsed.as_nanos()).unwrap_or(u64::MAX);
        saturating_increment(&self.total_project_instance_time_ns, elapsed_ns);
        self.max_project_instance_time_ns
            .fetch_max(elapsed_ns, Ordering::Relaxed);
    }

    pub(in crate::environment::extensions) fn report(
        &self,
        project_instances: usize,
    ) -> ExtensionTelemetryReport {
        ExtensionTelemetryReport {
            guest_calls: self.guest_calls.load(Ordering::Relaxed),
            lifecycle_calls: self.lifecycle_calls.load(Ordering::Relaxed),
            index_calls: self.index_calls.load(Ordering::Relaxed),
            event_calls: self.event_calls.load(Ordering::Relaxed),
            guest_failures: self.guest_failures.load(Ordering::Relaxed),
            guest_traps: self.guest_traps.load(Ordering::Relaxed),
            resource_limit_failures: self.resource_limit_failures.load(Ordering::Relaxed),
            disablements: self.disablements.load(Ordering::Relaxed),
            rejected_outputs: self.rejected_outputs.load(Ordering::Relaxed),
            patch_conflicts: self.patch_conflicts.load(Ordering::Relaxed),
            emitted_index_patches: self.emitted_index_patches.load(Ordering::Relaxed),
            emitted_execution_contexts: self.emitted_execution_contexts.load(Ordering::Relaxed),
            emitted_response_patches: self.emitted_response_patches.load(Ordering::Relaxed),
            emitted_command_patches: self.emitted_command_patches.load(Ordering::Relaxed),
            emitted_process_requests: self.emitted_process_requests.load(Ordering::Relaxed),
            requested_reindex_files: self.requested_reindex_files.load(Ordering::Relaxed),
            total_guest_time_ns: self.total_guest_time_ns.load(Ordering::Relaxed),
            max_guest_time_ns: self.max_guest_time_ns.load(Ordering::Relaxed),
            project_instance_creations: self
                .project_instance_creations
                .load(Ordering::Relaxed),
            project_instance_failures: self.project_instance_failures.load(Ordering::Relaxed),
            total_project_instance_time_ns: self
                .total_project_instance_time_ns
                .load(Ordering::Relaxed),
            max_project_instance_time_ns: self
                .max_project_instance_time_ns
                .load(Ordering::Relaxed),
            project_instances: u64::try_from(project_instances).expect(
                "INVARIANT VIOLATED: project extension instance count does not fit in u64. This is a bug because process address space cannot contain that many Wasm instances. Fix: keep project instance accounting bounded by host memory limits.",
            ),
        }
    }
}

fn saturating_increment(counter: &AtomicU64, amount: u64) {
    let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
        Some(current.saturating_add(amount))
    });
}
