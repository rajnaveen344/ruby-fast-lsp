use ruby_analysis::stats::{self, StatsRegistry, StatsSnapshot};
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
    pub telemetry: StatsSnapshot<ExtensionStat>,
}

ruby_analysis::stat_set! {
    /// Dimensionless per-extension guest telemetry. `ProjectInstances` is a
    /// gauge observed when the status report is built.
    pub enum ExtensionStat {
        GuestCalls = "guest_calls",
        LifecycleCalls = "lifecycle_calls",
        IndexCalls = "index_calls",
        EventCalls = "event_calls",
        GuestFailures = "guest_failures",
        GuestTraps = "guest_traps",
        ResourceLimitFailures = "resource_limit_failures",
        Disablements = "disablements",
        RejectedOutputs = "rejected_outputs",
        PatchConflicts = "patch_conflicts",
        EmittedIndexPatches = "emitted_index_patches",
        EmittedExecutionContexts = "emitted_execution_contexts",
        EmittedResponsePatches = "emitted_response_patches",
        EmittedCommandPatches = "emitted_command_patches",
        EmittedProcessRequests = "emitted_process_requests",
        RequestedReindexFiles = "requested_reindex_files",
        TotalGuestTimeNs = "total_guest_time_ns",
        MaxGuestTimeNs = "max_guest_time_ns": max,
        ProjectInstanceCreations = "project_instance_creations",
        ProjectInstanceFailures = "project_instance_failures",
        TotalProjectInstanceTimeNs = "total_project_instance_time_ns",
        MaxProjectInstanceTimeNs = "max_project_instance_time_ns": max,
        ProjectInstances = "project_instances",
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ExtensionStatusParams {}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExtensionStatusResponse {
    pub extensions: Vec<ExtensionStatusReport>,
}

#[derive(Debug, Default)]
pub(in crate::environment::extensions) struct ExtensionTelemetry {
    stats: StatsRegistry<ExtensionStat>,
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
        self.stats.increment(ExtensionStat::GuestCalls);
        self.stats.increment(match kind {
            GuestCallKind::Lifecycle => ExtensionStat::LifecycleCalls,
            GuestCallKind::Index => ExtensionStat::IndexCalls,
            GuestCallKind::Event => ExtensionStat::EventCalls,
        });
        if let Some(reason) = failure {
            self.record_guest_failure(reason);
        }
        if let Some(output) = output {
            for (stat, emitted) in [
                (
                    ExtensionStat::EmittedIndexPatches,
                    output.index_patches.len(),
                ),
                (
                    ExtensionStat::EmittedExecutionContexts,
                    output.execution_contexts.len(),
                ),
                (
                    ExtensionStat::EmittedResponsePatches,
                    output.response_patches.len(),
                ),
                (
                    ExtensionStat::EmittedCommandPatches,
                    output.command_patches.len(),
                ),
                (
                    ExtensionStat::EmittedProcessRequests,
                    output.process_requests.len(),
                ),
                (
                    ExtensionStat::RequestedReindexFiles,
                    output.reindex_files.len(),
                ),
            ] {
                self.stats.record(stat, stats::count(emitted));
            }
        }
        self.stats
            .record_duration(ExtensionStat::TotalGuestTimeNs, elapsed);
        self.stats
            .record_duration(ExtensionStat::MaxGuestTimeNs, elapsed);
    }

    pub(in crate::environment::extensions) fn record_disablement(&self) {
        self.stats.increment(ExtensionStat::Disablements);
    }

    pub(in crate::environment::extensions) fn record_rejected_output(&self) {
        self.stats.increment(ExtensionStat::RejectedOutputs);
    }

    pub(in crate::environment::extensions) fn record_patch_conflict(&self) {
        self.stats.increment(ExtensionStat::PatchConflicts);
    }

    fn record_guest_failure(&self, reason: &str) {
        self.stats.increment(ExtensionStat::GuestFailures);
        let reason = reason.to_ascii_lowercase();
        let trapped = reason.contains("wasm trap")
            || reason.contains("unreachable")
            || reason.contains("fuel")
            || reason.contains("wall-clock deadline");
        if trapped {
            self.stats.increment(ExtensionStat::GuestTraps);
        }
        let resource_limited = reason.contains("fuel")
            || reason.contains("wall-clock deadline")
            || (reason.contains("payload") && reason.contains("exceeds max"))
            || (reason.contains("memory")
                && (reason.contains("limit")
                    || reason.contains("grow")
                    || reason.contains("out of bounds")));
        if resource_limited {
            self.stats.increment(ExtensionStat::ResourceLimitFailures);
        }
    }

    pub(super) fn record_project_instance_creation(
        &self,
        elapsed: Duration,
        failure: Option<&str>,
    ) {
        self.stats
            .increment(ExtensionStat::ProjectInstanceCreations);
        if let Some(reason) = failure {
            self.stats.increment(ExtensionStat::ProjectInstanceFailures);
            self.record_guest_failure(reason);
        }
        self.stats
            .record_duration(ExtensionStat::TotalProjectInstanceTimeNs, elapsed);
        self.stats
            .record_duration(ExtensionStat::MaxProjectInstanceTimeNs, elapsed);
    }

    pub(in crate::environment::extensions) fn report(
        &self,
        project_instances: usize,
    ) -> StatsSnapshot<ExtensionStat> {
        let mut report = self.stats.snapshot();
        report.set(
            ExtensionStat::ProjectInstances,
            stats::count(project_instances),
        );
        report
    }
}
