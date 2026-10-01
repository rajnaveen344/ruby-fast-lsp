//! Machine-readable indexing timing and aggregate summary JSON.

use ruby_analysis::core::{InferenceTelemetry, SourceKind};
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
    timings: ruby_fast_lsp::indexer::coordinator::IndexingTimings,
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
    completed: &[(Url, ruby_fast_lsp::indexer::coordinator::IndexingTimings)],
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

    let mut files = 0usize;
    let mut source_bytes = 0usize;
    let mut estimated_engine_heap_bytes = 0usize;
    let mut reference_candidates = 0usize;
    let mut constant_reference_candidates = 0usize;
    let mut method_reference_candidates = 0usize;
    let mut resolved_reference_candidates = 0usize;
    let mut resolve_pass_graph_retry_ns = 0u64;
    let mut resolve_pass_diagnostic_seed_ns = 0u64;
    let mut resolve_pass_constant_candidates_ns = 0u64;
    let mut resolve_pass_method_candidates_ns = 0u64;
    let mut resolve_pass_sort_all_ns = 0u64;
    let mut resolve_pass_diagnostic_rebuild_ns = 0u64;
    let mut resolve_pass_constant_cache_hits = 0usize;
    let mut resolve_pass_constant_cache_misses = 0usize;
    let mut resolve_pass_constant_cache_unique_keys = 0usize;
    let mut resolve_pass_method_cache_hits = 0usize;
    let mut resolve_pass_method_cache_misses = 0usize;
    let mut resolve_pass_method_cache_unique_keys = 0usize;
    let mut resolve_pass_method_lookup_chain_cache_entries = 0usize;
    let mut resolve_pass_method_namespace_exists_cache_entries = 0usize;
    let mut resolve_pass_method_suggestion_cache_entries = 0usize;
    let mut resolve_pass_incomplete_method_chain_cache_entries = 0usize;
    let mut resolve_pass_deferred_receiver_candidates = 0usize;
    let mut resolve_pass_deferred_receiver_proven = 0usize;
    let mut resolve_pass_deferred_receiver_unknown = 0usize;
    let mut resolve_pass_method_return_cache_hits = 0usize;
    let mut resolve_pass_method_return_cache_misses = 0usize;
    let mut resolve_pass_method_return_cache_entries = 0usize;
    let mut resolve_pass_method_visibility_cache_hits = 0usize;
    let mut resolve_pass_method_visibility_cache_misses = 0usize;
    let mut resolve_pass_method_visibility_cache_entries = 0usize;
    let mut resolve_pass_ambiguous_method_return_cache_hits = 0usize;
    let mut resolve_pass_ambiguous_method_return_cache_misses = 0usize;
    let mut resolve_pass_ambiguous_method_return_cache_entries = 0usize;
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
        let stats = engine.stats();
        inference_telemetry.merge(&engine.inference_telemetry());
        files = files.checked_add(stats.files).expect(
            "INVARIANT VIOLATED: profiler aggregate file count overflowed usize. This is a bug because the measured process cannot contain more indexed files than addressable memory. Fix: inspect corrupt engine stats.",
        );
        source_bytes = source_bytes.checked_add(stats.source_bytes).expect(
            "INVARIANT VIOLATED: profiler aggregate source bytes overflowed usize. This is a bug because the measured process cannot retain more source than addressable memory. Fix: inspect corrupt engine stats.",
        );
        reference_candidates = reference_candidates
            .checked_add(stats.reference_candidates)
            .expect(
                "INVARIANT VIOLATED: profiler aggregate reference-candidate count overflowed usize. This is a bug because measured engine facts must fit the process address space. Fix: inspect corrupt engine stats.",
            );
        constant_reference_candidates = constant_reference_candidates
            .checked_add(stats.constant_reference_candidates)
            .expect(
                "INVARIANT VIOLATED: profiler aggregate constant-candidate count overflowed usize. This is a bug because measured engine facts must fit the process address space. Fix: inspect corrupt engine stats.",
            );
        method_reference_candidates = method_reference_candidates
            .checked_add(stats.method_reference_candidates)
            .expect(
                "INVARIANT VIOLATED: profiler aggregate method-candidate count overflowed usize. This is a bug because measured engine facts must fit the process address space. Fix: inspect corrupt engine stats.",
            );
        resolved_reference_candidates = resolved_reference_candidates
            .checked_add(stats.resolved_reference_candidates)
            .expect(
                "INVARIANT VIOLATED: profiler aggregate exact-resolved-candidate count overflowed usize. This is a bug because measured engine facts must fit the process address space. Fix: inspect corrupt engine stats.",
            );
        let resolve_pass = engine.last_resolve_stats();
        resolve_pass_graph_retry_ns = resolve_pass_graph_retry_ns
            .checked_add(resolve_pass.graph_retry_ns)
            .expect(
                "INVARIANT VIOLATED: profiler aggregate resolve graph-retry timing overflowed u64. This is a bug because measured resolve passes must fit u64 nanoseconds. Fix: inspect corrupt resolve instrumentation.",
            );
        resolve_pass_diagnostic_seed_ns = resolve_pass_diagnostic_seed_ns
            .checked_add(resolve_pass.diagnostic_seed_ns)
            .expect(
                "INVARIANT VIOLATED: profiler aggregate resolve diagnostic-seed timing overflowed u64. This is a bug because measured resolve passes must fit u64 nanoseconds. Fix: inspect corrupt resolve instrumentation.",
            );
        resolve_pass_constant_candidates_ns = resolve_pass_constant_candidates_ns
            .checked_add(resolve_pass.constant_candidates_ns)
            .expect(
                "INVARIANT VIOLATED: profiler aggregate resolve constant-candidate timing overflowed u64. This is a bug because measured resolve passes must fit u64 nanoseconds. Fix: inspect corrupt resolve instrumentation.",
            );
        resolve_pass_method_candidates_ns = resolve_pass_method_candidates_ns
            .checked_add(resolve_pass.method_candidates_ns)
            .expect(
                "INVARIANT VIOLATED: profiler aggregate resolve method-candidate timing overflowed u64. This is a bug because measured resolve passes must fit u64 nanoseconds. Fix: inspect corrupt resolve instrumentation.",
            );
        resolve_pass_sort_all_ns = resolve_pass_sort_all_ns
            .checked_add(resolve_pass.sort_all_ns)
            .expect(
                "INVARIANT VIOLATED: profiler aggregate resolve sort_all timing overflowed u64. This is a bug because measured resolve passes must fit u64 nanoseconds. Fix: inspect corrupt resolve instrumentation.",
            );
        resolve_pass_diagnostic_rebuild_ns = resolve_pass_diagnostic_rebuild_ns
            .checked_add(resolve_pass.diagnostic_rebuild_ns)
            .expect(
                "INVARIANT VIOLATED: profiler aggregate resolve diagnostic-rebuild timing overflowed u64. This is a bug because measured resolve passes must fit u64 nanoseconds. Fix: inspect corrupt resolve instrumentation.",
            );
        resolve_pass_constant_cache_hits = resolve_pass_constant_cache_hits
            .checked_add(resolve_pass.constant_cache_hits)
            .expect(
                "INVARIANT VIOLATED: profiler aggregate constant-cache hits overflowed usize. This is a bug because measured resolve passes must fit the process address space. Fix: inspect corrupt resolve instrumentation.",
            );
        resolve_pass_constant_cache_misses = resolve_pass_constant_cache_misses
            .checked_add(resolve_pass.constant_cache_misses)
            .expect(
                "INVARIANT VIOLATED: profiler aggregate constant-cache misses overflowed usize. This is a bug because measured resolve passes must fit the process address space. Fix: inspect corrupt resolve instrumentation.",
            );
        resolve_pass_constant_cache_unique_keys = resolve_pass_constant_cache_unique_keys
            .checked_add(resolve_pass.constant_cache_unique_keys)
            .expect(
                "INVARIANT VIOLATED: profiler aggregate constant-cache unique keys overflowed usize. This is a bug because measured resolve passes must fit the process address space. Fix: inspect corrupt resolve instrumentation.",
            );
        resolve_pass_method_cache_hits = resolve_pass_method_cache_hits
            .checked_add(resolve_pass.method_cache_hits)
            .expect(
                "INVARIANT VIOLATED: profiler aggregate method-cache hits overflowed usize. This is a bug because measured resolve passes must fit the process address space. Fix: inspect corrupt resolve instrumentation.",
            );
        resolve_pass_method_cache_misses = resolve_pass_method_cache_misses
            .checked_add(resolve_pass.method_cache_misses)
            .expect(
                "INVARIANT VIOLATED: profiler aggregate method-cache misses overflowed usize. This is a bug because measured resolve passes must fit the process address space. Fix: inspect corrupt resolve instrumentation.",
            );
        resolve_pass_method_cache_unique_keys = resolve_pass_method_cache_unique_keys
            .checked_add(resolve_pass.method_cache_unique_keys)
            .expect(
                "INVARIANT VIOLATED: profiler aggregate method-cache unique keys overflowed usize. This is a bug because measured resolve passes must fit the process address space. Fix: inspect corrupt resolve instrumentation.",
            );
        resolve_pass_method_lookup_chain_cache_entries =
            resolve_pass_method_lookup_chain_cache_entries
                .checked_add(resolve_pass.method_lookup_chain_cache_entries)
                .expect(
                    "INVARIANT VIOLATED: profiler aggregate method-lookup-chain cache entries overflowed usize. This is a bug because measured resolve passes must fit the process address space. Fix: inspect corrupt resolve instrumentation.",
                );
        resolve_pass_method_namespace_exists_cache_entries =
            resolve_pass_method_namespace_exists_cache_entries
                .checked_add(resolve_pass.method_namespace_exists_cache_entries)
                .expect(
                    "INVARIANT VIOLATED: profiler aggregate method-namespace-exists cache entries overflowed usize. This is a bug because measured resolve passes must fit the process address space. Fix: inspect corrupt resolve instrumentation.",
                );
        resolve_pass_method_suggestion_cache_entries = resolve_pass_method_suggestion_cache_entries
            .checked_add(resolve_pass.method_suggestion_cache_entries)
            .expect(
                "INVARIANT VIOLATED: profiler aggregate method-suggestion cache entries overflowed usize. This is a bug because measured resolve passes must fit the process address space. Fix: inspect corrupt resolve instrumentation.",
            );
        resolve_pass_incomplete_method_chain_cache_entries =
            resolve_pass_incomplete_method_chain_cache_entries
                .checked_add(resolve_pass.incomplete_method_chain_cache_entries)
                .expect(
                    "INVARIANT VIOLATED: profiler aggregate incomplete-method-chain cache entries overflowed usize. This is a bug because measured resolve passes must fit the process address space. Fix: inspect corrupt resolve instrumentation.",
                );
        resolve_pass_deferred_receiver_candidates = resolve_pass_deferred_receiver_candidates
            .checked_add(resolve_pass.deferred_receiver_candidates)
            .expect(
                "INVARIANT VIOLATED: profiler aggregate deferred-receiver candidates overflowed usize. This is a bug because measured resolve passes must fit the process address space. Fix: inspect corrupt resolve instrumentation.",
            );
        resolve_pass_deferred_receiver_proven = resolve_pass_deferred_receiver_proven
            .checked_add(resolve_pass.deferred_receiver_proven)
            .expect(
                "INVARIANT VIOLATED: profiler aggregate proven deferred receivers overflowed usize. This is a bug because measured resolve passes must fit the process address space. Fix: inspect corrupt resolve instrumentation.",
            );
        resolve_pass_deferred_receiver_unknown = resolve_pass_deferred_receiver_unknown
            .checked_add(resolve_pass.deferred_receiver_unknown)
            .expect(
                "INVARIANT VIOLATED: profiler aggregate Unknown deferred receivers overflowed usize. This is a bug because measured resolve passes must fit the process address space. Fix: inspect corrupt resolve instrumentation.",
            );
        resolve_pass_method_return_cache_hits = resolve_pass_method_return_cache_hits
            .checked_add(resolve_pass.method_return_cache_hits)
            .expect(
                "INVARIANT VIOLATED: profiler aggregate method-return cache hits overflowed usize. This is a bug because measured resolve passes must fit the process address space. Fix: inspect corrupt resolve instrumentation.",
            );
        resolve_pass_method_return_cache_misses = resolve_pass_method_return_cache_misses
            .checked_add(resolve_pass.method_return_cache_misses)
            .expect(
                "INVARIANT VIOLATED: profiler aggregate method-return cache misses overflowed usize. This is a bug because measured resolve passes must fit the process address space. Fix: inspect corrupt resolve instrumentation.",
            );
        resolve_pass_method_return_cache_entries = resolve_pass_method_return_cache_entries
            .checked_add(resolve_pass.method_return_cache_entries)
            .expect(
                "INVARIANT VIOLATED: profiler aggregate method-return cache entries overflowed usize. This is a bug because measured resolve passes must fit the process address space. Fix: inspect corrupt resolve instrumentation.",
            );
        resolve_pass_method_visibility_cache_hits = resolve_pass_method_visibility_cache_hits
            .checked_add(resolve_pass.method_visibility_cache_hits)
            .expect(
                "INVARIANT VIOLATED: profiler aggregate method-visibility cache hits overflowed usize. This is a bug because measured resolve passes must fit the process address space. Fix: inspect corrupt resolve instrumentation.",
            );
        resolve_pass_method_visibility_cache_misses = resolve_pass_method_visibility_cache_misses
            .checked_add(resolve_pass.method_visibility_cache_misses)
            .expect(
                "INVARIANT VIOLATED: profiler aggregate method-visibility cache misses overflowed usize. This is a bug because measured resolve passes must fit the process address space. Fix: inspect corrupt resolve instrumentation.",
            );
        resolve_pass_method_visibility_cache_entries =
            resolve_pass_method_visibility_cache_entries
                .checked_add(resolve_pass.method_visibility_cache_entries)
                .expect(
                    "INVARIANT VIOLATED: profiler aggregate method-visibility cache entries overflowed usize. This is a bug because measured resolve passes must fit the process address space. Fix: inspect corrupt resolve instrumentation.",
                );
        resolve_pass_ambiguous_method_return_cache_hits =
            resolve_pass_ambiguous_method_return_cache_hits
                .checked_add(resolve_pass.ambiguous_method_return_cache_hits)
                .expect(
                    "INVARIANT VIOLATED: profiler aggregate ambiguous method-return cache hits overflowed usize. This is a bug because measured resolve passes must fit the process address space. Fix: inspect corrupt resolve instrumentation.",
                );
        resolve_pass_ambiguous_method_return_cache_misses =
            resolve_pass_ambiguous_method_return_cache_misses
                .checked_add(resolve_pass.ambiguous_method_return_cache_misses)
                .expect(
                    "INVARIANT VIOLATED: profiler aggregate ambiguous method-return cache misses overflowed usize. This is a bug because measured resolve passes must fit the process address space. Fix: inspect corrupt resolve instrumentation.",
                );
        resolve_pass_ambiguous_method_return_cache_entries =
            resolve_pass_ambiguous_method_return_cache_entries
                .checked_add(resolve_pass.ambiguous_method_return_cache_entries)
                .expect(
                    "INVARIANT VIOLATED: profiler aggregate ambiguous method-return cache entries overflowed usize. This is a bug because measured resolve passes must fit the process address space. Fix: inspect corrupt resolve instrumentation.",
                );
        estimated_engine_heap_bytes = estimated_engine_heap_bytes
            .checked_add(engine.estimated_memory_stats().total())
            .expect(
                "INVARIANT VIOLATED: profiler aggregate engine heap overflowed usize. This is a bug because estimated live engine memory must fit the process address space. Fix: inspect memory accounting.",
            );
        let mut project_sources = engine
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
                    panic!(
                        "INVARIANT VIOLATED: profiler cannot read project-owned evidence file {}. This is a bug because exact dataset evidence must hash every indexed project byte. Fix: keep the indexed file readable for the measurement. Error: {error}",
                        file.path.display()
                    )
                });
                disk_source.as_slice()
            };
            hash_length_prefixed(
                &mut source_fingerprint,
                file.path.to_string_lossy().as_bytes(),
            );
            hash_length_prefixed(&mut source_fingerprint, source);
            project_source_bytes = project_source_bytes.checked_add(source.len()).expect(
                "INVARIANT VIOLATED: profiler project source byte count overflowed usize. This is a bug because indexed source must fit the process address space. Fix: inspect corrupt file metadata.",
            );
        }
        let status = status_by_root.get(&workspace.root_path).unwrap_or_else(|| {
            panic!(
                "INVARIANT VIOLATED: profiler completed project {} without an indexing status snapshot. This is a bug because every scheduled project must retain its authoritative readiness state. Fix: register project status before scheduling indexing.",
                workspace.root_path.display()
            )
        });
        let project_ready = status.project_navigation_ready_ms.unwrap_or_else(|| {
            panic!(
                "INVARIANT VIOLATED: profiler completed project {} without a project-navigation readiness milestone. This is a bug because the coordinator must publish staged readiness before dependencies. Fix: transition through ProjectNavigationReady in every successful indexing run.",
                workspace.root_path.display()
            )
        });
        let dependencies_ready = status.dependency_navigation_ready_ms.unwrap_or_else(|| {
            panic!(
                "INVARIANT VIOLATED: profiler completed project {} without a dependency-navigation readiness milestone. This is a bug because the coordinator must publish staged readiness before semantic completion. Fix: transition through DependencyNavigationReady in every successful indexing run.",
                workspace.root_path.display()
            )
        });
        project_navigation_ready_ms.push(project_ready);
        dependency_navigation_ready_ms.push(dependencies_ready);
        semantic_complete_ms.push(status.elapsed_ms);
        let semantic_result_fingerprint_hex =
            stable_fingerprint_hex(engine.semantic_result_fingerprint().stable_bytes());
        project_evidence.push(serde_json::json!({
            "root": workspace.root_path,
            "runtime": workspace.runtime.selected().read().clone(),
            "detected_ruby_version": workspace.runtime.ruby_version().read().clone(),
            "runtime_classpath_fingerprint_sha256": workspace.runtime.classpath_fingerprint().read().clone(),
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
    let core_cache = products.core_templates.reuse;
    let core_cache_retained_weight = products.core_templates.retained_weight_bytes;
    let runtime_stdlib_path_cache = products.stdlib_paths.reuse;
    let runtime_stdlib_path_cache_retained_weight = products.stdlib_paths.retained_weight_bytes;
    let gem_cache = products.gem_dependencies.reuse;
    let gem_cache_retained_weight = products.gem_dependencies.retained_weight_bytes;
    let classpath_file_cache = products.classpath_files.reuse;
    let classpath_file_cache_retained_weight = products.classpath_files.retained_weight_bytes;
    let java_artifact_product_cache = products.java_artifacts.reuse;
    let java_artifact_product_cache_retained_weight = products.java_artifacts.retained_weight_bytes;
    let persistent_gem_cache = products.persistent_gems;
    let persistent_java_artifact_cache = products.persistent_java;
    let persistent_compiled_wasm_cache = products.compiled_wasm;
    let gem_binding = products.gem_bindings;

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
            "files": files,
            "source_bytes": source_bytes,
            "estimated_heap_bytes": estimated_engine_heap_bytes,
            "reference_candidates": reference_candidates,
            "constant_reference_candidates": constant_reference_candidates,
            "method_reference_candidates": method_reference_candidates,
            "resolved_reference_candidates": resolved_reference_candidates,
            "resolve_pass": {
                "graph_retry_ns": resolve_pass_graph_retry_ns,
                "diagnostic_seed_ns": resolve_pass_diagnostic_seed_ns,
                "constant_candidates_ns": resolve_pass_constant_candidates_ns,
                "method_candidates_ns": resolve_pass_method_candidates_ns,
                "sort_all_ns": resolve_pass_sort_all_ns,
                "diagnostic_rebuild_ns": resolve_pass_diagnostic_rebuild_ns,
                "constant_cache_hits": resolve_pass_constant_cache_hits,
                "constant_cache_misses": resolve_pass_constant_cache_misses,
                "constant_cache_unique_keys": resolve_pass_constant_cache_unique_keys,
                "method_cache_hits": resolve_pass_method_cache_hits,
                "method_cache_misses": resolve_pass_method_cache_misses,
                "method_cache_unique_keys": resolve_pass_method_cache_unique_keys,
                "method_lookup_chain_cache_entries": resolve_pass_method_lookup_chain_cache_entries,
                "method_namespace_exists_cache_entries": resolve_pass_method_namespace_exists_cache_entries,
                "method_suggestion_cache_entries": resolve_pass_method_suggestion_cache_entries,
                "incomplete_method_chain_cache_entries": resolve_pass_incomplete_method_chain_cache_entries,
                "deferred_receiver_candidates": resolve_pass_deferred_receiver_candidates,
                "deferred_receiver_proven": resolve_pass_deferred_receiver_proven,
                "deferred_receiver_unknown": resolve_pass_deferred_receiver_unknown,
                "method_return_cache_hits": resolve_pass_method_return_cache_hits,
                "method_return_cache_misses": resolve_pass_method_return_cache_misses,
                "method_return_cache_entries": resolve_pass_method_return_cache_entries,
                "method_visibility_cache_hits": resolve_pass_method_visibility_cache_hits,
                "method_visibility_cache_misses": resolve_pass_method_visibility_cache_misses,
                "method_visibility_cache_entries": resolve_pass_method_visibility_cache_entries,
                "ambiguous_method_return_cache_hits": resolve_pass_ambiguous_method_return_cache_hits,
                "ambiguous_method_return_cache_misses": resolve_pass_ambiguous_method_return_cache_misses,
                "ambiguous_method_return_cache_entries": resolve_pass_ambiguous_method_return_cache_entries
            }
        },
        "inference_telemetry": inference_telemetry,
        "process": resource_delta,
        "process_local_core_templates": {
            "entries": core_cache.entries,
            "retained_weight_bytes": core_cache_retained_weight,
            "lookups": core_cache.lookups,
            "hits": core_cache.hits,
            "joined_flights": core_cache.joined_flights,
            "misses": core_cache.misses,
            "producers": core_cache.producers,
            "failures": core_cache.failures,
            "evictions": core_cache.evictions,
            "producer_wall_ns": core_cache.producer_wall_ns,
            "producer_max_wall_ns": core_cache.producer_max_wall_ns,
            "consumer_wait_wall_ns": core_cache.consumer_wait_wall_ns,
            "consumer_max_wait_wall_ns": core_cache.consumer_max_wait_wall_ns,
        },
        "process_local_runtime_stdlib_paths": {
            "entries": runtime_stdlib_path_cache.entries,
            "retained_weight_bytes": runtime_stdlib_path_cache_retained_weight,
            "lookups": runtime_stdlib_path_cache.lookups,
            "hits": runtime_stdlib_path_cache.hits,
            "joined_flights": runtime_stdlib_path_cache.joined_flights,
            "misses": runtime_stdlib_path_cache.misses,
            "producers": runtime_stdlib_path_cache.producers,
            "failures": runtime_stdlib_path_cache.failures,
            "evictions": runtime_stdlib_path_cache.evictions,
            "producer_wall_ns": runtime_stdlib_path_cache.producer_wall_ns,
            "producer_max_wall_ns": runtime_stdlib_path_cache.producer_max_wall_ns,
            "consumer_wait_wall_ns": runtime_stdlib_path_cache.consumer_wait_wall_ns,
            "consumer_max_wait_wall_ns": runtime_stdlib_path_cache.consumer_max_wait_wall_ns,
        },
        "process_local_gem_dependency_products": {
            "entries": gem_cache.entries,
            "retained_weight_bytes": gem_cache_retained_weight,
            "lookups": gem_cache.lookups,
            "hits": gem_cache.hits,
            "joined_flights": gem_cache.joined_flights,
            "misses": gem_cache.misses,
            "producers": gem_cache.producers,
            "failures": gem_cache.failures,
            "evictions": gem_cache.evictions,
            "producer_wall_ns": gem_cache.producer_wall_ns,
            "producer_max_wall_ns": gem_cache.producer_max_wall_ns,
            "consumer_wait_wall_ns": gem_cache.consumer_wait_wall_ns,
            "consumer_max_wait_wall_ns": gem_cache.consumer_max_wait_wall_ns,
            "binding": {
                "attempts": gem_binding.attempts,
                "successes": gem_binding.successes,
                "failures": gem_binding.failures,
                "files": gem_binding.files,
                "validation_wall_ns": gem_binding.validation_wall_ns,
                "validation_max_wall_ns": gem_binding.validation_max_wall_ns,
                "insertion_wall_ns": gem_binding.insertion_wall_ns,
                "insertion_max_wall_ns": gem_binding.insertion_max_wall_ns,
            },
        },
        "process_local_classpath_file_products": {
            "entries": classpath_file_cache.entries,
            "retained_weight_bytes": classpath_file_cache_retained_weight,
            "lookups": classpath_file_cache.lookups,
            "hits": classpath_file_cache.hits,
            "joined_flights": classpath_file_cache.joined_flights,
            "misses": classpath_file_cache.misses,
            "producers": classpath_file_cache.producers,
            "failures": classpath_file_cache.failures,
            "evictions": classpath_file_cache.evictions,
            "producer_wall_ns": classpath_file_cache.producer_wall_ns,
            "producer_max_wall_ns": classpath_file_cache.producer_max_wall_ns,
            "consumer_wait_wall_ns": classpath_file_cache.consumer_wait_wall_ns,
            "consumer_max_wait_wall_ns": classpath_file_cache.consumer_max_wait_wall_ns,
        },
        "process_local_java_artifact_products": {
            "entries": java_artifact_product_cache.entries,
            "retained_weight_bytes": java_artifact_product_cache_retained_weight,
            "lookups": java_artifact_product_cache.lookups,
            "hits": java_artifact_product_cache.hits,
            "joined_flights": java_artifact_product_cache.joined_flights,
            "misses": java_artifact_product_cache.misses,
            "producers": java_artifact_product_cache.producers,
            "failures": java_artifact_product_cache.failures,
            "evictions": java_artifact_product_cache.evictions,
            "producer_wall_ns": java_artifact_product_cache.producer_wall_ns,
            "producer_max_wall_ns": java_artifact_product_cache.producer_max_wall_ns,
            "consumer_wait_wall_ns": java_artifact_product_cache.consumer_wait_wall_ns,
            "consumer_max_wait_wall_ns": java_artifact_product_cache.consumer_max_wait_wall_ns,
        },
        "persistent_gem_dependency_products": {
            "lookups": persistent_gem_cache.lookups,
            "hits": persistent_gem_cache.hits,
            "misses": persistent_gem_cache.misses,
            "producers": persistent_gem_cache.producers,
            "corruptions": persistent_gem_cache.corruptions,
            "lock_waits": persistent_gem_cache.lock_waits,
            "publications": persistent_gem_cache.publications,
            "publication_failures": persistent_gem_cache.publication_failures,
            "evictions": persistent_gem_cache.evictions,
            "physical_read_bytes": persistent_gem_cache.physical_read_bytes,
            "logical_read_bytes": persistent_gem_cache.logical_read_bytes,
            "write_bytes": persistent_gem_cache.write_bytes,
        },
        "persistent_java_artifact_products": {
            "lookups": persistent_java_artifact_cache.lookups,
            "hits": persistent_java_artifact_cache.hits,
            "misses": persistent_java_artifact_cache.misses,
            "producers": persistent_java_artifact_cache.producers,
            "corruptions": persistent_java_artifact_cache.corruptions,
            "lock_waits": persistent_java_artifact_cache.lock_waits,
            "publications": persistent_java_artifact_cache.publications,
            "publication_failures": persistent_java_artifact_cache.publication_failures,
            "evictions": persistent_java_artifact_cache.evictions,
            "physical_read_bytes": persistent_java_artifact_cache.physical_read_bytes,
            "logical_read_bytes": persistent_java_artifact_cache.logical_read_bytes,
            "write_bytes": persistent_java_artifact_cache.write_bytes,
        },
        "persistent_compiled_wasm_products": {
            "lookups": persistent_compiled_wasm_cache.lookups,
            "hits": persistent_compiled_wasm_cache.hits,
            "misses": persistent_compiled_wasm_cache.misses,
            "producers": persistent_compiled_wasm_cache.producers,
            "corruptions": persistent_compiled_wasm_cache.corruptions,
            "lock_waits": persistent_compiled_wasm_cache.lock_waits,
            "publications": persistent_compiled_wasm_cache.publications,
            "publication_failures": persistent_compiled_wasm_cache.publication_failures,
            "evictions": persistent_compiled_wasm_cache.evictions,
            "physical_read_bytes": persistent_compiled_wasm_cache.physical_read_bytes,
            "logical_read_bytes": persistent_compiled_wasm_cache.logical_read_bytes,
            "write_bytes": persistent_compiled_wasm_cache.write_bytes,
        },
    })
}

pub(crate) fn millisecond_summary(values: &[u64]) -> serde_json::Value {
    assert!(
        !values.is_empty(),
        "INVARIANT VIOLATED: profiler readiness summary received no measurements. This is a bug because an indexing aggregate is emitted only after at least one registered project completes. Fix: retain every project's staged readiness milestones."
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
