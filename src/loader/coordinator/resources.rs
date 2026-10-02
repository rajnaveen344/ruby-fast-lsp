//! Resource admission, memory logging, and allocator release for indexing work.

use super::IndexingCoordinator;
use crate::invariant::ExpectInvariant;
use crate::loader::context::LoadContext;
use crate::utils::admission::{
    IndexingResourceGovernor, IndexingResourcePriority, IndexingWorkSpec,
};
use anyhow::Result;
use log::info;
use ruby_analysis::engine::AnalysisStat;
use std::path::PathBuf;
use tokio_util::sync::CancellationToken;

pub(super) const MIB: usize = 1024 * 1024;

#[derive(Clone, Copy)]
pub(super) enum IndexingWorkClass {
    LightCpu,
    Io,
    HeavyCpu,
    HeavyIo,
    ParallelIo,
    RuntimeCompanionParallelIo,
    ProjectCompanionIo,
    ProjectParallelIo,
}

pub(super) async fn run_cpu_indexing_task<T, F>(
    resources: &IndexingResourceGovernor,
    project_root: Option<PathBuf>,
    cancellation: Option<CancellationToken>,
    work_class: IndexingWorkClass,
    label: &'static str,
    task: F,
) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    let policy = resources.policy();
    let (cpu_lanes, transient_memory_bytes, io_slots, parallel, partitioned) = match work_class {
        IndexingWorkClass::LightCpu => (1, 16 * MIB, 0, false, false),
        IndexingWorkClass::Io => (1, 64 * MIB, 1, false, false),
        IndexingWorkClass::HeavyCpu => (1, 256 * MIB, 0, false, false),
        IndexingWorkClass::HeavyIo => (1, 256 * MIB, 1, false, false),
        IndexingWorkClass::ParallelIo => (policy.cpu_lanes(), 256 * MIB, 1, true, false),
        IndexingWorkClass::RuntimeCompanionParallelIo => {
            (1, 256 * MIB, 1, true, policy.cpu_lanes() > 1)
        }
        IndexingWorkClass::ProjectCompanionIo => (1, 256 * MIB, 1, false, false),
        IndexingWorkClass::ProjectParallelIo => {
            let project_root = project_root.as_deref().expect_invariant(
                "project-parallel indexing has no project root",
                "lane ownership needs the isolated project identity",
                "pass the canonical project root for every project-parallel phase",
            );
            let cpu_lanes = resources.project_parallel_cpu_lanes(project_root);
            (
                cpu_lanes,
                256 * MIB,
                1,
                true,
                cpu_lanes != policy.cpu_lanes(),
            )
        }
    };
    let spec = IndexingWorkSpec::new(
        project_root,
        IndexingResourcePriority::Background,
        cpu_lanes,
        transient_memory_bytes,
        io_slots,
    );
    let spec = if matches!(
        work_class,
        IndexingWorkClass::RuntimeCompanionParallelIo
            | IndexingWorkClass::ProjectCompanionIo
            | IndexingWorkClass::ProjectParallelIo
    ) {
        spec.as_project_parallel()
    } else {
        spec
    };
    if partitioned {
        resources
            .run_partitioned_parallel_with_resources(label, spec, cancellation, task)
            .await
    } else if parallel {
        resources
            .run_parallel_with_resources(label, spec, cancellation, task)
            .await
    } else {
        resources
            .run_with_resources(label, spec, cancellation, task)
            .await
    }
}

fn log_memory_bucket(name: &str, bytes: usize, total: usize) {
    let percent = if total == 0 {
        0.0
    } else {
        bytes as f64 * 100.0 / total as f64
    };
    info!("{name}: {:.1} MB ({percent:.1}%)", bytes_to_mb(bytes));
}

fn bytes_to_mb(bytes: usize) -> f64 {
    bytes as f64 / 1_048_576.0
}

#[cfg(target_os = "macos")]
pub(super) fn release_allocator_free_pages() {
    unsafe extern "C" {
        fn malloc_default_zone() -> *mut libc::c_void;
        fn malloc_zone_pressure_relief(zone: *mut libc::c_void, goal: usize) -> usize;
    }

    unsafe {
        let zone = malloc_default_zone();
        if !zone.is_null() {
            malloc_zone_pressure_relief(zone, 0);
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub(super) fn release_allocator_free_pages() {}

impl IndexingCoordinator {
    pub(super) fn log_analysis_memory_stats(&self, ctx: &LoadContext) {
        let (stats, memory) = self
            .load_target(ctx)
            .view(|view| (view.stats(), view.estimated_memory_stats()));
        let total = memory.total();

        info!(
            "Analysis stats: files={}, source_bytes={}, symbols={}, methods={}, ref_candidates={}, refs={}, types={}, diagnostic_candidates={}, diagnostics={}, graph_nodes={}, graph_edges={}, unresolved_graph_edges={}",
            stats.get(AnalysisStat::Files),
            stats.get(AnalysisStat::SourceBytes),
            stats.get(AnalysisStat::Symbols),
            stats.get(AnalysisStat::Methods),
            stats.get(AnalysisStat::ReferenceCandidates),
            stats.get(AnalysisStat::References),
            stats.get(AnalysisStat::Types),
            stats.get(AnalysisStat::DiagnosticCandidates),
            stats.get(AnalysisStat::Diagnostics),
            stats.get(AnalysisStat::GraphNodes),
            stats.get(AnalysisStat::GraphEdges),
            stats.get(AnalysisStat::UnresolvedGraphEdges)
        );
        info!("Estimated engine heap: {:.1} MB", bytes_to_mb(total));
        log_memory_bucket("names", memory.names, total);
        log_memory_bucket("files", memory.files, total);
        log_memory_bucket("symbols", memory.symbols, total);
        log_memory_bucket("methods", memory.methods, total);
        log_memory_bucket("types", memory.types, total);
        log_memory_bucket("reference candidates", memory.reference_candidates, total);
        log_memory_bucket("references", memory.references, total);
        log_memory_bucket("diagnostics", memory.diagnostics, total);
        log_memory_bucket("diagnostic candidates", memory.diagnostic_candidates, total);
        log_memory_bucket("graph", memory.graph, total);
        log_memory_bucket(
            "unresolved graph edges",
            memory.unresolved_graph_edges,
            total,
        );
    }
}
