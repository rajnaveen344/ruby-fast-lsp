#![recursion_limit = "256"]

//! Unified profiler for Ruby Fast LSP
//!
//! Combines CPU and memory profiling capabilities for:
//! - Indexing performance
//! - Type inference performance  
//! - File open/close operations
//!
//! Usage:
//!   # CPU profiling with samply (recommended)
//!   cargo build --release -p devtools --bin profiler
//!   samply record ./target/release/profiler [options]
//!
//!   # Memory profiling with dhat
//!   cargo build --release -p devtools --bin profiler --no-default-features --features memory-profiling
//!   ./target/release/profiler --memory [options]
//!
//! Options:
//!   --workspace <path>   Path to Ruby workspace (default: built-in sample project)
//!   --memory             Enable dhat memory profiling (outputs dhat-heap.json)
//!   --phase <name>       Profile specific phase: index, infer, all (default: all)
//!   --config <path>      Canonical Ruby Fast LSP JSON configuration
//!   --extension-path <p>  VS Code extension path for bundled stubs
//!   --hold-seconds <n>   Keep process alive after profiling for external memory tools
//!   --benchmark-iterations <n>  Measure editor operations after indexing
//!   --scheduler-concurrency <n>  Override bounded project workers for evidence
//!   --resource-cpu-lanes <n>  Override the process indexing CPU pool for evidence
//!   --resource-task-limit <n>  Override admitted top-level indexing tasks
//!   --resource-memory-mib <n>  Override transient-memory admission for evidence
//!   --resource-io-slots <n>    Override concurrent indexing I/O admission
//!   --check-budgets      Fail when a production budget is exceeded
//!   --diagnostics-file <relative-path>  Open a file and print its user-visible diagnostics
//!   --definition-at <path:line:character>  Probe first live and final definitions at an LSP position
//!   --references-at <path:line:character>  Open a file and print references at an LSP position
//!   --semantic-export-manifest  Print stable per-project-file export fingerprints
//!   --diagnostic-manifest  Print stable per-project resolved diagnostic facts
//!   --help               Show help

#[macro_use]
#[allow(unused_macros)]
#[path = "../../../../ruby-analysis/src/invariant.rs"]
mod invariant;

mod benchmark;
mod cli;
mod evidence;
mod indexing_summary;
mod navigation_probes;
mod reports;
mod sample_project;
#[cfg(test)]
mod tests;
mod workspace_indexing;

use crate::invariant::ExpectInvariant;
use devtools::metrics::ProductionBudget;
use log::info;
use ruby_fast_lsp::server::RubyLanguageServer;
use ruby_fast_lsp::utils::admission;
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;
use tower_lsp::lsp_types::Url;

use crate::benchmark::{print_production_measurements, run_production_benchmark};
use crate::cli::{parse_args, Phase};
use crate::indexing_summary::{duration_ms, stats_json_without};
use crate::navigation_probes::{
    prepare_live_definition_probes, sample_definitions, sample_open_file_diagnostics,
    sample_references,
};
use crate::reports::{print_diagnostic_manifest, print_semantic_export_manifest, print_stats};
use crate::workspace_indexing::{
    configure_server, run_full_indexing, run_indexing_only, run_type_inference_only,
};
use ruby_fast_lsp::loader::cache::persistent::PersistentProductStat;

// Conditionally use dhat for memory profiling
#[cfg(feature = "memory-profiling")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

#[cfg(all(
    feature = "jemalloc",
    not(feature = "memory-profiling"),
    not(target_env = "msvc")
))]
#[global_allocator]
static ALLOC: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

#[cfg(all(feature = "jemalloc", feature = "memory-profiling"))]
compile_error!(
    "invariant violated: jemalloc and memory-profiling both select a global allocator — bug: one binary owns one global allocator — fix: enable exactly one allocator feature"
);

fn main() -> anyhow::Result<()> {
    let config = parse_args();

    // Initialize memory profiler if enabled
    #[cfg(feature = "memory-profiling")]
    let _profiler = if config.memory_profiling {
        Some(dhat::Profiler::new_heap())
    } else {
        None
    };

    // Initialize logger
    let default_log_filter = if config.benchmark_iterations.is_some() {
        "warn"
    } else {
        "info"
    };
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(default_log_filter))
        .init();

    // Determine workspace path
    let use_sample_project = config.workspace.is_none();
    let workspace_path = if let Some(path) = config.workspace {
        path
    } else {
        info!("Creating sample Ruby project for profiling...");
        let sample_path = sample_project::create_sample_project()?;
        info!("Sample project created at: {}", sample_path.display());
        sample_path
    };
    let workspace_path = std::fs::canonicalize(&workspace_path)?;
    info!("Using canonical workspace: {}", workspace_path.display());

    let workspace_uri = Url::from_file_path(&workspace_path)
        .map_err(|_| anyhow::anyhow!("Invalid workspace path"))?;

    // Create runtime
    let rt = Runtime::new()?;

    let benchmark_result = rt.block_on(async {
        let mut server = RubyLanguageServer::default();
        server.set_indexing_concurrency(config.scheduler_concurrency);
        if config.resource_cpu_lanes.is_some()
            || config.resource_task_limit.is_some()
            || config.resource_memory_mib.is_some()
            || config.resource_io_slots.is_some()
        {
            let default_policy = server.indexing_resource_policy();
            let transient_memory_limit_bytes = config
                .resource_memory_mib
                .map(|memory_mib| {
                    memory_mib.checked_mul(1024 * 1024).expect_invariant(
                        "profiler transient-memory MiB overflowed usize",
                        "the requested evidence budget cannot fit the host address space",
                        "pass a smaller --resource-memory-mib value",
                    )
                })
                .unwrap_or_else(|| default_policy.transient_memory_limit_bytes());
            server.set_indexing_resource_policy(admission::IndexingResourcePolicy::with_limits(
                config
                    .resource_cpu_lanes
                    .unwrap_or_else(|| default_policy.cpu_lanes()),
                config
                    .resource_task_limit
                    .unwrap_or_else(|| default_policy.top_level_tasks()),
                transient_memory_limit_bytes,
                config
                    .resource_io_slots
                    .unwrap_or_else(|| default_policy.io_slots()),
            ));
        }
        let extension_load = configure_server(
            &mut server,
            config.config_path.as_ref(),
            config.extension_path.as_ref(),
        );
        println!(
            "{}",
            serde_json::json!({
                "extension_load_timing": {
                    "elapsed_ms": duration_ms(extension_load),
                    "loaded": server.extension_status_reports().len(),
                    "persistent_products": stats_json_without(
                        &server.compiled_wasm_cache_snapshot(),
                        &[
                            PersistentProductStat::Misses,
                            PersistentProductStat::LockWaits,
                            PersistentProductStat::Publications,
                            PersistentProductStat::PublicationFailures,
                            PersistentProductStat::Evictions,
                        ],
                    ),
                }
            })
        );
        let discovered = server.add_workspace_folder(workspace_uri.clone())?;
        anyhow::ensure!(
            !discovered.is_empty(),
            "workspace container discovered no Ruby projects: {}",
            workspace_path.display()
        );
        server.refresh_embedded_extensions();
        info!(
            "Discovered {} isolated Ruby project(s): {:?}",
            discovered.len(),
            server.workspace_root_paths()
        );

        let total_start = Instant::now();

        let live_definition_probes =
            prepare_live_definition_probes(&server, &workspace_path, &config.definition_probes)
                .await?;
        if let Some(active_probe) = live_definition_probes.first() {
            let workspace = server
                .workspace_for_uri(&active_probe.uri)
                .unwrap_or_else(|| {
                    unreachable_invariant!(
                        what = "live definition probe {} has no owning project",
                        why = "probes must remain inside one discovered Ruby project",
                        fix = "choose a project-owned source file",
                        active_probe.relative_path.display(),
                    )
                });
            server.prioritize_indexing_project(&workspace.root_path);
        }

        let cold_indexing = match config.phase {
            Phase::All => {
                // Full indexing (includes type inference)
                info!("=== PROFILING: Full Indexing (with type inference) ===");
                run_full_indexing(&server, &live_definition_probes).await
            }
            Phase::Index => {
                // Index only (no type inference)
                info!("=== PROFILING: Indexing Only (no type inference) ===");
                run_indexing_only(&server, &live_definition_probes).await
            }
            Phase::Infer => {
                // Index first, then profile inference separately
                info!("=== PROFILING: Type Inference Only ===");
                info!("Step 1: Indexing (not profiled focus)...");
                let indexing = run_indexing_only(&server, &live_definition_probes).await;

                info!("Step 2: Type Inference (profiled)...");
                run_type_inference_only(&server).await;
                indexing
            }
        };

        info!("=== TOTAL TIME: {:?} ===", total_start.elapsed());

        // Print stats
        print_stats(&server);
        if config.semantic_export_manifest {
            print_semantic_export_manifest(&server)?;
        }
        if config.diagnostic_manifest {
            print_diagnostic_manifest(&server)?;
        }
        println!(
            "{}",
            serde_json::to_string(&serde_json::json!({
                "extension_status": server.extension_status_reports(),
            }))?
        );

        sample_open_file_diagnostics(&server, &workspace_path, &config.diagnostics_files).await?;
        sample_definitions(&server, &workspace_path, &config.definition_probes).await?;
        sample_references(&server, &workspace_path, &config.reference_probes).await?;

        let benchmark_result = if let Some(iterations) = config.benchmark_iterations {
            invariant!(
                config.phase == Phase::All,
                what = "production benchmark requested with a partial profiler phase",
                why = "editor latency budgets require a fully indexed workspace",
                fix = "use --phase all or omit --phase",
            );
            let measurements =
                run_production_benchmark(&server, &workspace_path, cold_indexing, iterations)
                    .await?;
            print_production_measurements(&measurements);
            Some(measurements)
        } else {
            None
        };

        #[cfg(feature = "memory-profiling")]
        if config.memory_profiling {
            let stats = dhat::HeapStats::get();
            info!("=== MEMORY STATS ===");
            info!(
                "Peak memory: {:.1} MB",
                stats.max_bytes as f64 / 1_000_000.0
            );
            info!(
                "Current memory: {:.1} MB",
                stats.curr_bytes as f64 / 1_000_000.0
            );
            info!("Total allocations: {} blocks", stats.total_blocks);
        }

        if config.hold_seconds > 0 {
            info!(
                "Holding profiler process for {}s for external memory inspection",
                config.hold_seconds
            );
            tokio::time::sleep(Duration::from_secs(config.hold_seconds)).await;
        }
        anyhow::Ok(benchmark_result)
    })?;

    // Cleanup sample project if we created it
    if use_sample_project {
        info!("Cleaning up sample project...");
        let _ = sample_project::cleanup_sample_project();
    }

    if config.check_budgets {
        let measurements = benchmark_result.ok_or_else(|| {
            anyhow::anyhow!("--check-budgets requires --benchmark-iterations <N>")
        })?;
        let exceeded = ProductionBudget::default().exceeded_by(&measurements);
        if !exceeded.is_empty() {
            anyhow::bail!("production budgets exceeded: {}", exceeded.join(", "));
        }
        println!("production budgets: PASS");
    }

    Ok(())
}
