//! Machine-readable indexing timing and aggregate summary JSON.

use crate::invariant::ExpectInvariant;
use ruby_analysis::core::{InferenceTelemetry, SourceKind};
use ruby_analysis::engine::{AnalysisStat, ResolveStat};
use ruby_analysis::stats::{Stat, StatsSnapshot};
use ruby_fast_lsp::server::RubyLanguageServer;
use sha2::{Digest, Sha256};
use std::time::Duration;
use tower_lsp::lsp_types::Url;

use crate::evidence::{
    build_evidence, dataset_fingerprint_sha256, hash_length_prefixed, machine_evidence,
    stable_fingerprint_hex, ProcessResourceUsage,
};

pub(crate) fn duration_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

pub(crate) fn indexing_timing_json(
    uri: &Url,
    timings: ruby_fast_lsp::loader::coordinator::IndexingTimings,
) -> serde_json::Value {
    serde_json::json!({
        "project": uri,
        "runtime_ms": duration_ms(timings.runtime),
        "discovery_ms": duration_ms(timings.discovery),
        "core_ms": duration_ms(timings.core),
        "project_ms": duration_ms(timings.project),
        "dependencies_ms": duration_ms(timings.dependencies),
        "resolve_ms": duration_ms(timings.resolve),
        "publish_ms": duration_ms(timings.publish),
        "total_ms": duration_ms(timings.total),
    })
}

pub(crate) fn indexing_summary_json(
    server: &RubyLanguageServer,
    completed: &[(Url, ruby_fast_lsp::loader::coordinator::IndexingTimings)],
    wall: Duration,
    resources_started: Option<ProcessResourceUsage>,
    resources_finished: Option<ProcessResourceUsage>,
    live_definition_evidence: &[serde_json::Value],
) -> serde_json::Value {
    let mut runtime = Duration::ZERO;
    let mut discovery = Duration::ZERO;
    let mut core = Duration::ZERO;
    let mut project = Duration::ZERO;
    let mut dependencies = Duration::ZERO;
    let mut resolve = Duration::ZERO;
    let mut publish = Duration::ZERO;
    let mut project_cpu_wall = Duration::ZERO;
    for (_, timings) in completed {
        runtime += timings.runtime;
        discovery += timings.discovery;
        core += timings.core;
        project += timings.project;
        dependencies += timings.dependencies;
        resolve += timings.resolve;
        publish += timings.publish;
        project_cpu_wall += timings.total;
    }

    let mut analysis = StatsSnapshot::<AnalysisStat>::default();
    let mut resolve_pass = StatsSnapshot::<ResolveStat>::default();
    let mut estimated_engine_heap_bytes = 0usize;
    let mut inference_telemetry = InferenceTelemetry::default();
    let mut project_evidence = Vec::new();
    let status_by_root = server
        .indexing_status_snapshot()
        .projects
        .into_iter()
        .map(|status| (status.root.clone(), status))
        .collect::<std::collections::HashMap<_, _>>();
    let mut project_navigation_ready_ms = Vec::new();
    let mut dependency_navigation_ready_ms = Vec::new();
    let mut semantic_complete_ms = Vec::new();
    for workspace in server.list_workspaces() {
        let engine = workspace.analysis_engine.read();
        inference_telemetry.merge(&engine.view().inference_telemetry());
        analysis.merge(&engine.view().stats());
        resolve_pass.merge(engine.view().last_resolve_stats());
        estimated_engine_heap_bytes = estimated_engine_heap_bytes
            .checked_add(engine.view().estimated_memory_stats().total())
            .expect_invariant(
                "profiler aggregate engine heap overflowed usize",
                "estimated live engine memory must fit the process address space",
                "inspect memory accounting",
            );
        let mut project_sources = engine
            .view()
            .files()
            .filter(|file| file.kind == SourceKind::Project)
            .collect::<Vec<_>>();
        project_sources.sort_by(|left, right| left.path.cmp(&right.path));
        let mut source_fingerprint = Sha256::new();
        let mut project_source_bytes = 0usize;
        for file in &project_sources {
            let disk_source;
            let source = if let Some(source) = file.source.as_deref() {
                source.as_bytes()
            } else {
                disk_source = std::fs::read(&file.path).unwrap_or_else(|error| {
                    unreachable_invariant!(
                        what =
                            "profiler cannot read project-owned evidence file {} (error: {error})",
                        why = "exact dataset evidence must hash every indexed project byte",
                        fix = "keep the indexed file readable for the measurement",
                        file.path.display(),
                        error = error,
                    )
                });
                disk_source.as_slice()
            };
            hash_length_prefixed(
                &mut source_fingerprint,
                file.path.to_string_lossy().as_bytes(),
            );
            hash_length_prefixed(&mut source_fingerprint, source);
            project_source_bytes = project_source_bytes
                .checked_add(source.len())
                .expect_invariant(
                    "profiler project source byte count overflowed usize",
                    "indexed source must fit the process address space",
                    "inspect corrupt file metadata",
                );
        }
        let status = status_by_root.get(&workspace.root_path).unwrap_or_else(|| {
            unreachable_invariant!(
                what = "profiler completed project {} without an indexing status snapshot",
                why = "every scheduled project must retain its authoritative readiness state",
                fix = "register project status before scheduling indexing",
                workspace.root_path.display(),
            )
        });
        let project_ready = status.project_navigation_ready_ms.unwrap_or_else(|| {
            unreachable_invariant!(
                what = "profiler completed project {} without a project-navigation readiness milestone",
                why = "the coordinator must publish staged readiness before dependencies",
                fix = "transition through ProjectNavigationReady in every successful indexing run",
                workspace.root_path.display(),
            )
        });
        let dependencies_ready = status.dependency_navigation_ready_ms.unwrap_or_else(|| {
            unreachable_invariant!(
                what = "profiler completed project {} without a dependency-navigation readiness milestone",
                why = "the coordinator must publish staged readiness before semantic completion",
                fix = "transition through DependencyNavigationReady in every successful indexing run",
                workspace.root_path.display(),
            )
        });
        project_navigation_ready_ms.push(project_ready);
        dependency_navigation_ready_ms.push(dependencies_ready);
        semantic_complete_ms.push(status.elapsed_ms);
        let semantic_result_fingerprint_hex =
            stable_fingerprint_hex(engine.view().semantic_result_fingerprint().stable_bytes());
        project_evidence.push(serde_json::json!({
            "root": workspace.root_path,
            "runtime": workspace.runtime.selected().read().clone(),
            "detected_ruby_version": workspace.runtime.ruby_version().read().clone(),
            "runtime_classpath_fingerprint_sha256": workspace.runtime.classpath_fingerprint(),
            "project_files": project_sources.len(),
            "project_source_bytes": project_source_bytes,
            "project_source_fingerprint_sha256": format!("{:x}", source_fingerprint.finalize()),
            "semantic_result_fingerprint_hex": semantic_result_fingerprint_hex,
            "project_navigation_ready_ms": project_ready,
            "dependency_navigation_ready_ms": dependencies_ready,
            "semantic_complete_ms": status.elapsed_ms,
        }));
    }
    project_evidence.sort_by(|left, right| left["root"].as_str().cmp(&right["root"].as_str()));
    let dataset_fingerprint_sha256 = dataset_fingerprint_sha256(&project_evidence);
    let scheduler = server.indexing_scheduler_snapshot();
    let resources = server.indexing_resource_snapshot();
    let resource_delta = ProcessResourceUsage::delta(resources_started, resources_finished);
    let products = server.runtime_product_snapshot();
    let mut gem_dependency_products = stats_json(&products.gem_dependencies);
    gem_dependency_products["binding"] = stats_json(&products.gem_bindings);

    serde_json::json!({
        "schema_version": 15,
        "ruby_fast_lsp_version": ruby_fast_lsp::SERVER_VERSION,
        "target_os": std::env::consts::OS,
        "target_arch": std::env::consts::ARCH,
        "logical_cpus": std::thread::available_parallelism().map(usize::from).unwrap_or(1),
        "machine": machine_evidence(),
        "build": build_evidence(),
        "projects": completed.len(),
        "dataset_fingerprint_sha256": dataset_fingerprint_sha256,
        "project_evidence": project_evidence,
        "readiness_ms": {
            "project_navigation": millisecond_summary(&project_navigation_ready_ms),
            "dependency_navigation": millisecond_summary(&dependency_navigation_ready_ms),
            "semantic_complete": millisecond_summary(&semantic_complete_ms),
        },
        "live_definition_probes": live_definition_evidence,
        "scheduler": {
            "concurrency_limit": scheduler.concurrency_limit,
            "active_at_end": scheduler.active,
            "queued_at_end": scheduler.queued,
            "active_project": scheduler.active_project,
            "reprioritizations": scheduler.reprioritizations,
        },
        "resource_budget": {
            "cpu_lanes": resources.cpu_lane_limit,
            "top_level_tasks": resources.top_level_task_limit,
            "transient_memory_bytes": resources.transient_memory_limit_bytes,
            "io_slots": resources.io_slot_limit,
        },
        "resource_usage": {
            "queued_tasks_at_end": resources.queued_tasks,
            "active_tasks_at_end": resources.active_tasks,
            "peak_active_tasks": resources.peak_active_tasks,
            "active_cpu_lanes_at_end": resources.active_cpu_lanes,
            "peak_active_cpu_lanes": resources.peak_active_cpu_lanes,
            "active_transient_memory_bytes_at_end": resources.active_transient_memory_bytes,
            "peak_active_transient_memory_bytes": resources.peak_active_transient_memory_bytes,
            "active_io_slots_at_end": resources.active_io_slots,
            "peak_active_io_slots": resources.peak_active_io_slots,
            "completed_tasks": resources.completed_tasks,
            "panicked_tasks": resources.panicked_tasks,
            "cancelled_before_start": resources.cancelled_before_start,
            "cancelled_after_start": resources.cancelled_after_start,
            "active_project": resources.active_project,
            "reprioritizations": resources.reprioritizations,
        },
        "wall_ms": duration_ms(wall),
        "summed_project_wall_ms": duration_ms(project_cpu_wall),
        "phase_sum_ms": {
            "runtime": duration_ms(runtime),
            "discovery": duration_ms(discovery),
            "core": duration_ms(core),
            "project": duration_ms(project),
            "dependencies": duration_ms(dependencies),
            "resolve": duration_ms(resolve),
            "publish": duration_ms(publish),
        },
        "engine": {
            "files": analysis.get(AnalysisStat::Files),
            "source_bytes": analysis.get(AnalysisStat::SourceBytes),
            "estimated_heap_bytes": estimated_engine_heap_bytes,
            "reference_candidates": analysis.get(AnalysisStat::ReferenceCandidates),
            "constant_reference_candidates": analysis.get(AnalysisStat::ConstantReferenceCandidates),
            "method_reference_candidates": analysis.get(AnalysisStat::MethodReferenceCandidates),
            "resolved_reference_candidates": analysis.get(AnalysisStat::ResolvedReferenceCandidates),
            "resolve_pass": stats_json_without(&resolve_pass, &[ResolveStat::MethodReturnEquationSolveRuns]),
        },
        "inference_telemetry": inference_telemetry,
        "process": resource_delta,
        "process_local_core_templates": stats_json(&products.core_templates),
        "process_local_runtime_stdlib_paths": stats_json(&products.stdlib_paths),
        "process_local_gem_dependency_products": gem_dependency_products,
        "process_local_classpath_file_products": stats_json(&products.classpath_files),
        "process_local_java_artifact_products": stats_json(&products.java_artifacts),
        "persistent_gem_dependency_products": stats_json(&products.persistent_gems),
        "persistent_java_artifact_products": stats_json(&products.persistent_java),
        "persistent_compiled_wasm_products": stats_json(&products.compiled_wasm),
    })
}

pub(crate) fn millisecond_summary(values: &[u64]) -> serde_json::Value {
    invariant!(
        !values.is_empty(),
        what = "profiler readiness summary received no measurements",
        why =
            "an indexing aggregate is emitted only after at least one registered project completes",
        fix = "retain every project's staged readiness milestones",
    );
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let p50_index = sorted.len().div_ceil(2) - 1;
    let p95_index = (sorted.len() * 95).div_ceil(100) - 1;
    serde_json::json!({
        "samples": sorted.len(),
        "min": sorted[0],
        "p50": sorted[p50_index],
        "p95": sorted[p95_index],
        "max": sorted[sorted.len() - 1],
    })
}

/// One statistics snapshot as a JSON object keyed by stat name.
pub(crate) fn stats_json<S: Stat>(snapshot: &StatsSnapshot<S>) -> serde_json::Value {
    stats_json_without(snapshot, &[])
}

/// One statistics snapshot as a JSON object, omitting `excluded` stats.
pub(crate) fn stats_json_without<S: Stat>(
    snapshot: &StatsSnapshot<S>,
    excluded: &[S],
) -> serde_json::Value {
    serde_json::Value::Object(
        snapshot
            .iter()
            .filter(|(stat, _, _)| !excluded.contains(stat))
            .map(|(_, name, value)| (name.to_owned(), serde_json::Value::from(value)))
            .collect(),
    )
}
