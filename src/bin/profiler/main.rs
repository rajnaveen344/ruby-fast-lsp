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
//!   cargo build --release --bin profiler
//!   samply record ./target/release/profiler [options]
//!
//!   # Memory profiling with dhat
//!   cargo build --release --bin profiler --features memory-profiling
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

mod sample_project;

use log::{info, LevelFilter};
use ruby_analysis::core::{
    DiagnosticCandidate, DiagnosticFact, FullyQualifiedName, GraphEdgeFact, GraphNodeFact,
    MethodFact, ReferenceCandidate, ReferenceFact, SourceKind, StoredConstantReferenceCandidate,
    StoredMethodReferenceCandidate, StoredReferenceCandidate, StoredResolvedReferenceCandidate,
    SymbolFact, TypeFact, TypeSubject,
};
use ruby_fast_lsp::capabilities::indexing;
use ruby_fast_lsp::capabilities::{completion, definitions, hover, references};
use ruby_fast_lsp::config::RubyFastLspConfig;
use ruby_fast_lsp::handlers::request;
use ruby_fast_lsp::perf::metrics::{LatencySummary, ProductionBudget, ProductionMeasurements};
use ruby_fast_lsp::query::EngineQuery;
use ruby_fast_lsp::server::RubyLanguageServer;
use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;
use tower_lsp::lsp_types::{
    CompletionContext, CompletionResponse, CompletionTriggerKind, DidChangeTextDocumentParams,
    DidOpenTextDocumentParams, GotoDefinitionParams, GotoDefinitionResponse, HoverParams,
    PartialResultParams, Position, TextDocumentContentChangeEvent, TextDocumentIdentifier,
    TextDocumentItem, TextDocumentPositionParams, Url, VersionedTextDocumentIdentifier,
    WorkDoneProgressParams,
};

// Conditionally use dhat for memory profiling
#[cfg(feature = "memory-profiling")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

#[derive(Debug, Clone, PartialEq)]
enum Phase {
    All,
    Index,
    Infer,
}

struct Config {
    workspace: Option<PathBuf>,
    config_path: Option<PathBuf>,
    extension_path: Option<PathBuf>,
    memory_profiling: bool,
    phase: Phase,
    hold_seconds: u64,
    benchmark_iterations: Option<usize>,
    scheduler_concurrency: usize,
    resource_cpu_lanes: Option<usize>,
    resource_task_limit: Option<usize>,
    resource_memory_mib: Option<usize>,
    resource_io_slots: Option<usize>,
    check_budgets: bool,
    diagnostics_files: Vec<PathBuf>,
    definition_probes: Vec<ReferenceProbe>,
    reference_probes: Vec<ReferenceProbe>,
    semantic_export_manifest: bool,
    diagnostic_manifest: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ReferenceProbe {
    path: PathBuf,
    line: u32,
    character: u32,
}

#[derive(Clone, Debug)]
struct PreparedDefinitionProbe {
    relative_path: PathBuf,
    uri: Url,
    position: Position,
}

fn parse_args() -> Config {
    parse_args_from(env::args())
}

fn parse_args_from<I, S>(args: I) -> Config
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let args = args.into_iter().map(Into::into).collect::<Vec<_>>();
    let mut config = Config {
        workspace: None,
        config_path: None,
        extension_path: None,
        memory_profiling: false,
        phase: Phase::All,
        hold_seconds: 0,
        benchmark_iterations: None,
        scheduler_concurrency: 2,
        resource_cpu_lanes: None,
        resource_task_limit: None,
        resource_memory_mib: None,
        resource_io_slots: None,
        check_budgets: false,
        diagnostics_files: Vec::new(),
        definition_probes: Vec::new(),
        reference_probes: Vec::new(),
        semantic_export_manifest: false,
        diagnostic_manifest: false,
    };

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--workspace" | "-w" => {
                if i + 1 < args.len() {
                    config.workspace = Some(PathBuf::from(&args[i + 1]));
                    i += 1;
                }
            }
            "--memory" | "-m" => {
                config.memory_profiling = true;
            }
            "--config" => {
                assert!(
                    i + 1 < args.len(),
                    "INVARIANT VIOLATED: profiler --config has no path. This is a bug because a \
                     configured profiling run requires an explicit JSON file. Fix: pass \
                     --config /path/to/ruby-fast-lsp.json."
                );
                config.config_path = Some(PathBuf::from(&args[i + 1]));
                i += 1;
            }
            "--extension-path" => {
                if i + 1 < args.len() {
                    config.extension_path = Some(PathBuf::from(&args[i + 1]));
                    i += 1;
                }
            }
            "--phase" | "-p" => {
                if i + 1 < args.len() {
                    config.phase = match args[i + 1].as_str() {
                        "index" => Phase::Index,
                        "infer" => Phase::Infer,
                        "all" => Phase::All,
                        _ => {
                            eprintln!("Unknown phase: {}. Using 'all'", args[i + 1]);
                            Phase::All
                        }
                    };
                    i += 1;
                }
            }
            "--hold-seconds" => {
                if i + 1 < args.len() {
                    config.hold_seconds = args[i + 1].parse().unwrap_or_else(|error| {
                        panic!(
                            "INVARIANT VIOLATED: --hold-seconds must be an unsigned integer. \
                             This is a bug because profiler hold duration must be parseable seconds. \
                             Fix: pass a numeric value like --hold-seconds 30. Error: {error}"
                        )
                    });
                    i += 1;
                }
            }
            "--benchmark-iterations" => {
                if i + 1 < args.len() {
                    let iterations = args[i + 1].parse().unwrap_or_else(|error| {
                        panic!(
                            "INVARIANT VIOLATED: --benchmark-iterations must be a positive integer. This is a bug because p95 measurement requires a fixed nonzero sample count. Fix: pass a numeric value like --benchmark-iterations 100. Error: {error}"
                        )
                    });
                    assert!(
                        iterations > 0,
                        "INVARIANT VIOLATED: --benchmark-iterations is zero. This is a bug because p95 measurement requires observations. Fix: pass a positive iteration count."
                    );
                    config.benchmark_iterations = Some(iterations);
                    i += 1;
                }
            }
            "--scheduler-concurrency" => {
                assert!(
                    i + 1 < args.len(),
                    "INVARIANT VIOLATED: profiler --scheduler-concurrency has no value. This is a bug because scheduling evidence requires an explicit positive worker limit. Fix: pass --scheduler-concurrency 1."
                );
                let concurrency = args[i + 1].parse().unwrap_or_else(|error| {
                    panic!(
                        "INVARIANT VIOLATED: --scheduler-concurrency must be a positive integer. This is a bug because profiler scheduling must be reproducible. Fix: pass a numeric value such as 1 or 2. Error: {error}"
                    )
                });
                assert!(
                    concurrency > 0,
                    "INVARIANT VIOLATED: --scheduler-concurrency is zero. This is a bug because no project could be admitted. Fix: pass a positive worker count."
                );
                config.scheduler_concurrency = concurrency;
                i += 1;
            }
            "--resource-cpu-lanes" => {
                assert!(
                    i + 1 < args.len(),
                    "INVARIANT VIOLATED: profiler --resource-cpu-lanes has no value. This is a bug because resource evidence requires an explicit positive lane limit. Fix: pass --resource-cpu-lanes 2."
                );
                let lanes = args[i + 1].parse().unwrap_or_else(|error| {
                    panic!(
                        "INVARIANT VIOLATED: --resource-cpu-lanes must be a positive integer. This is a bug because profiler resource evidence must be reproducible. Fix: pass a numeric value such as 2 or 6. Error: {error}"
                    )
                });
                assert!(
                    lanes > 0,
                    "INVARIANT VIOLATED: --resource-cpu-lanes is zero. This is a bug because no indexing CPU work could progress. Fix: pass a positive lane count."
                );
                config.resource_cpu_lanes = Some(lanes);
                i += 1;
            }
            "--resource-task-limit" => {
                assert!(
                    i + 1 < args.len(),
                    "INVARIANT VIOLATED: profiler --resource-task-limit has no value. This is a bug because resource evidence requires an explicit positive admission limit. Fix: pass --resource-task-limit 2."
                );
                let tasks = args[i + 1].parse().unwrap_or_else(|error| {
                    panic!(
                        "INVARIANT VIOLATED: --resource-task-limit must be a positive integer. This is a bug because profiler resource evidence must be reproducible. Fix: pass a numeric value such as 1 or 2. Error: {error}"
                    )
                });
                assert!(
                    tasks > 0,
                    "INVARIANT VIOLATED: --resource-task-limit is zero. This is a bug because no indexing task could enter the worker pool. Fix: pass a positive task limit."
                );
                config.resource_task_limit = Some(tasks);
                i += 1;
            }
            "--resource-memory-mib" => {
                assert!(
                    i + 1 < args.len(),
                    "INVARIANT VIOLATED: profiler --resource-memory-mib has no value. This is a bug because memory evidence requires an explicit positive admission limit. Fix: pass --resource-memory-mib 512."
                );
                let memory_mib = args[i + 1].parse::<usize>().unwrap_or_else(|error| {
                    panic!(
                        "INVARIANT VIOLATED: --resource-memory-mib must be a positive integer. This is a bug because profiler resource evidence must be reproducible. Fix: pass a numeric value such as 256 or 512. Error: {error}"
                    )
                });
                assert!(
                    memory_mib > 0,
                    "INVARIANT VIOLATED: --resource-memory-mib is zero. This is a bug because no indexing work could reserve temporary memory. Fix: pass a positive MiB limit."
                );
                config.resource_memory_mib = Some(memory_mib);
                i += 1;
            }
            "--resource-io-slots" => {
                assert!(
                    i + 1 < args.len(),
                    "INVARIANT VIOLATED: profiler --resource-io-slots has no value. This is a bug because I/O evidence requires an explicit positive admission limit. Fix: pass --resource-io-slots 2."
                );
                let io_slots = args[i + 1].parse::<usize>().unwrap_or_else(|error| {
                    panic!(
                        "INVARIANT VIOLATED: --resource-io-slots must be a positive integer. This is a bug because profiler resource evidence must be reproducible. Fix: pass a numeric value such as 1 or 2. Error: {error}"
                    )
                });
                assert!(
                    io_slots > 0,
                    "INVARIANT VIOLATED: --resource-io-slots is zero. This is a bug because source discovery could never enter the I/O budget. Fix: pass a positive slot count."
                );
                config.resource_io_slots = Some(io_slots);
                i += 1;
            }
            "--check-budgets" => {
                config.check_budgets = true;
            }
            "--semantic-export-manifest" => {
                config.semantic_export_manifest = true;
            }
            "--diagnostic-manifest" => {
                config.diagnostic_manifest = true;
            }
            "--diagnostics-file" => {
                assert!(
                    i + 1 < args.len(),
                    "INVARIANT VIOLATED: --diagnostics-file has no path. This is a bug because diagnostic sampling requires an explicit workspace-relative file. Fix: pass --diagnostics-file path/to/file.rb."
                );
                config.diagnostics_files.push(PathBuf::from(&args[i + 1]));
                i += 1;
            }
            "--references-at" => {
                assert!(
                    i + 1 < args.len(),
                    "INVARIANT VIOLATED: --references-at has no path and position. This is a bug because reference sampling requires path:line:character. Fix: pass --references-at spec/example_spec.rb:29:10."
                );
                config
                    .reference_probes
                    .push(parse_reference_probe(&args[i + 1]));
                i += 1;
            }
            "--definition-at" => {
                assert!(
                    i + 1 < args.len(),
                    "INVARIANT VIOLATED: --definition-at has no path and position. This is a bug because definition sampling requires path:line:character. Fix: pass --definition-at lib/example.rb:4:10."
                );
                config
                    .definition_probes
                    .push(parse_position_probe("--definition-at", &args[i + 1]));
                i += 1;
            }
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            _ => {
                // Treat as workspace path if no flag
                if !args[i].starts_with('-') {
                    config.workspace = Some(PathBuf::from(&args[i]));
                }
            }
        }
        i += 1;
    }

    config
}

fn parse_reference_probe(value: &str) -> ReferenceProbe {
    parse_position_probe("--references-at", value)
}

fn parse_position_probe(flag: &str, value: &str) -> ReferenceProbe {
    let (path_and_line, character) = value.rsplit_once(':').unwrap_or_else(|| {
        panic!("INVARIANT VIOLATED: {flag} `{value}` has no character component. This is a bug because profiler query positions must be explicit. Fix: use path:line:character with zero-indexed LSP coordinates.")
    });
    let (path, line) = path_and_line.rsplit_once(':').unwrap_or_else(|| {
        panic!("INVARIANT VIOLATED: {flag} `{value}` has no line component. This is a bug because profiler query positions must be explicit. Fix: use path:line:character with zero-indexed LSP coordinates.")
    });
    let line = line.parse().unwrap_or_else(|error| {
        panic!("INVARIANT VIOLATED: {flag} line `{line}` is invalid. This is a bug because LSP lines are unsigned integers. Fix: pass a zero-indexed numeric line. Error: {error}")
    });
    let character = character.parse().unwrap_or_else(|error| {
        panic!("INVARIANT VIOLATED: {flag} character `{character}` is invalid. This is a bug because LSP characters are unsigned integers. Fix: pass a zero-indexed numeric character. Error: {error}")
    });
    let path = PathBuf::from(path);
    assert!(
        path.is_relative(),
        "INVARIANT VIOLATED: {flag} path `{}` is absolute. This is a bug because profiler probes must remain inside the selected workspace. Fix: pass a workspace-relative path.",
        path.display()
    );
    ReferenceProbe {
        path,
        line,
        character,
    }
}

fn print_help() {
    println!(
        r#"Ruby Fast LSP Profiler

USAGE:
    profiler [OPTIONS] [WORKSPACE]

OPTIONS:
    -w, --workspace <PATH>   Path to Ruby workspace (default: built-in sample project)
    -m, --memory             Enable dhat memory profiling (outputs dhat-heap.json)
    -p, --phase <PHASE>      Profile specific phase: index, infer, all (default: all)
    --config <PATH>          Load canonical Ruby Fast LSP JSON configuration
    --extension-path <PATH>  VS Code extension path for bundled stubs
    --hold-seconds <N>       Keep process alive after profiling for external memory tools
    --benchmark-iterations <N>
                             Measure edit and query p95 latency after indexing
    --resource-cpu-lanes <N>
                             Override the server-owned indexing CPU pool width for evidence
    --resource-task-limit <N>
                             Override admitted top-level indexing tasks for evidence
    --resource-memory-mib <N>
                             Override transient-memory admission for evidence
    --resource-io-slots <N>
                             Override concurrent indexing I/O admission for evidence
    --check-budgets          Exit unsuccessfully when a production budget is exceeded
    --diagnostics-file <PATH>
                             Open a workspace-relative file through didOpen and print diagnostics as JSON; repeatable
    --definition-at <PATH:LINE:CHARACTER>
                             Open a workspace-relative file and print definitions as JSON; repeatable, zero-indexed
    --references-at <PATH:LINE:CHARACTER>
                             Open a workspace-relative file and print resolved references as JSON; repeatable, zero-indexed
    --semantic-export-manifest
                             Print stable per-project-file semantic export fingerprints as JSON
    --diagnostic-manifest
                             Print stable per-project resolved diagnostic facts as JSON
    -h, --help               Show this help message

EXAMPLES:
    # Profile with samply (CPU)
    cargo build --release --bin profiler
    samply record ./target/release/profiler /path/to/ruby/project

    # Profile specific phase
    samply record ./target/release/profiler --phase infer /path/to/project

    # Memory profiling (requires --features memory-profiling)
    cargo build --release --bin profiler --features memory-profiling
    ./target/release/profiler --memory /path/to/project

    # Check deterministic built-in production budgets
    ./target/release/profiler --benchmark-iterations 100 --check-budgets

    # Prove warm-process dependency reuse with projects admitted sequentially
    ./target/release/profiler --workspace /path/to/umbrella --scheduler-concurrency 1
"#
    );
}

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
    env_logger::Builder::new()
        .filter_level(if config.benchmark_iterations.is_some() {
            LevelFilter::Warn
        } else {
            LevelFilter::Info
        })
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
        server.indexing_scheduler =
            ruby_fast_lsp::indexing_scheduler::IndexingScheduler::new(
                config.scheduler_concurrency,
            );
        if config.resource_cpu_lanes.is_some()
            || config.resource_task_limit.is_some()
            || config.resource_memory_mib.is_some()
            || config.resource_io_slots.is_some()
        {
            let default_policy = server.indexing_resources.policy();
            let transient_memory_limit_bytes = config
                .resource_memory_mib
                .map(|memory_mib| {
                    memory_mib.checked_mul(1024 * 1024).expect(
                        "INVARIANT VIOLATED: profiler transient-memory MiB overflowed usize. This is a bug because the requested evidence budget cannot fit the host address space. Fix: pass a smaller --resource-memory-mib value.",
                    )
                })
                .unwrap_or_else(|| default_policy.transient_memory_limit_bytes());
            server.indexing_resources =
                ruby_fast_lsp::indexing_resources::IndexingResourceGovernor::new(
                    ruby_fast_lsp::indexing_resources::IndexingResourcePolicy::with_limits(
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
                    ),
                );
        }
        let extension_load = configure_server(
            &server,
            config.config_path.as_ref(),
            config.extension_path.as_ref(),
        );
        let extension_cache = server
            .persistent_derived_product_cache
            .compiled_wasm_snapshot();
        println!(
            "{}",
            serde_json::json!({
                "extension_load_timing": {
                    "elapsed_ms": duration_ms(extension_load),
                    "loaded": server.extension_registry.status_reports().len(),
                    "persistent_products": {
                        "lookups": extension_cache.lookups,
                        "hits": extension_cache.hits,
                        "producers": extension_cache.producers,
                        "corruptions": extension_cache.corruptions,
                        "physical_read_bytes": extension_cache.physical_read_bytes,
                        "logical_read_bytes": extension_cache.logical_read_bytes,
                        "write_bytes": extension_cache.write_bytes,
                    }
                }
            })
        );
        let discovered = server.add_workspace_folder(workspace_uri.clone())?;
        anyhow::ensure!(
            !discovered.is_empty(),
            "workspace container discovered no Ruby projects: {}",
            workspace_path.display()
        );
        let lsp_config = server.config.lock().clone();
        server
            .extension_registry
            .configure_from_config_and_workspace_roots(
                &lsp_config,
                &server.workspace_root_paths(),
            );
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
            let workspace = server.workspace_for_uri(&active_probe.uri).unwrap_or_else(|| {
                panic!(
                    "INVARIANT VIOLATED: live definition probe {} has no owning project. This is a profiler setup bug because probes must remain inside one discovered Ruby project. Fix: choose a project-owned source file.",
                    active_probe.relative_path.display()
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
                "extension_status": server.extension_registry.status_reports(),
            }))?
        );

        sample_open_file_diagnostics(&server, &workspace_path, &config.diagnostics_files).await?;
        sample_definitions(&server, &workspace_path, &config.definition_probes).await?;
        sample_references(&server, &workspace_path, &config.reference_probes).await?;

        let benchmark_result = if let Some(iterations) = config.benchmark_iterations {
            assert!(
                config.phase == Phase::All,
                "INVARIANT VIOLATED: production benchmark requested with a partial profiler phase. This is a bug because editor latency budgets require a fully indexed workspace. Fix: use --phase all or omit --phase."
            );
            let measurements = run_production_benchmark(
                &server,
                &workspace_path,
                cold_indexing,
                iterations,
            )
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

fn print_semantic_export_manifest(server: &RubyLanguageServer) -> anyhow::Result<()> {
    let mut workspaces = server.list_workspaces();
    workspaces.sort_by(|left, right| left.root_path.cmp(&right.root_path));
    for workspace in workspaces {
        let engine = workspace.analysis_engine.read();
        let mut files = engine
            .files()
            .map(|file| {
                let path = if file.kind == SourceKind::Project {
                    file.path
                        .strip_prefix(&workspace.root_path)
                        .unwrap_or_else(|_| {
                            panic!(
                                "INVARIANT VIOLATED: project semantic export source {} is outside owning root {}. This is a bug because project source ownership must remain workspace-contained. Fix: route registration through the deepest owning project before exporting evidence.",
                                file.path.display(),
                                workspace.root_path.display()
                            )
                        })
                        .to_path_buf()
                } else {
                    file.path.clone()
                };
                let fingerprint = engine
                    .semantic_export_fingerprint(file.id)
                    .map(|fingerprint| stable_fingerprint_hex(fingerprint.stable_bytes()));
                serde_json::json!({
                    "path": path,
                    "source_kind": format!("{:?}", file.kind),
                    "fingerprint_hex": fingerprint,
                })
            })
            .collect::<Vec<_>>();
        files.sort_by(|left, right| {
            left["source_kind"]
                .as_str()
                .cmp(&right["source_kind"].as_str())
                .then_with(|| left["path"].as_str().cmp(&right["path"].as_str()))
        });
        println!(
            "{}",
            serde_json::to_string(&serde_json::json!({
                "semantic_export_manifest": {
                    "project": workspace.root_path,
                    "files": files,
                }
            }))?
        );
    }
    Ok(())
}

fn print_diagnostic_manifest(server: &RubyLanguageServer) -> anyhow::Result<()> {
    let mut workspaces = server.list_workspaces();
    workspaces.sort_by(|left, right| left.root_path.cmp(&right.root_path));
    for workspace in workspaces {
        let engine = workspace.analysis_engine.read();
        let query = engine.query();
        let mut diagnostics = query
            .all_diagnostic_facts()
            .into_iter()
            .map(|diagnostic| {
                let file = query.file(diagnostic.range.file_id).unwrap_or_else(|| {
                    panic!(
                        "INVARIANT VIOLATED: diagnostic {} references unknown file {:?}. This is a bug because resolved diagnostic facts must remain owned by a registered source. Fix: remove diagnostic facts through the ordinary per-file replacement lifecycle before unregistering their source.",
                        diagnostic.code,
                        diagnostic.range.file_id
                    )
                });
                let path = if file.kind == SourceKind::Project {
                    file.path
                        .strip_prefix(&workspace.root_path)
                        .unwrap_or_else(|_| {
                            panic!(
                                "INVARIANT VIOLATED: project diagnostic source {} is outside owning root {}. This is a bug because project diagnostic ownership must remain workspace-contained. Fix: route registration through the deepest owning project before exporting evidence.",
                                file.path.display(),
                                workspace.root_path.display()
                            )
                        })
                        .to_path_buf()
                } else {
                    file.path.clone()
                };
                let (start_line, start_character) = file
                    .byte_offset_to_line_character(diagnostic.range.start_byte)
                    .unwrap_or_else(|| {
                        panic!(
                            "INVARIANT VIOLATED: diagnostic {} start byte {} is outside source {}. This is a bug because resolved facts must retain valid source ranges. Fix: validate fact ranges before engine ingestion.",
                            diagnostic.code,
                            diagnostic.range.start_byte,
                            file.path.display()
                        )
                    });
                let (end_line, end_character) = file
                    .byte_offset_to_line_character(diagnostic.range.end_byte)
                    .unwrap_or_else(|| {
                        panic!(
                            "INVARIANT VIOLATED: diagnostic {} end byte {} is outside source {}. This is a bug because resolved facts must retain valid source ranges. Fix: validate fact ranges before engine ingestion.",
                            diagnostic.code,
                            diagnostic.range.end_byte,
                            file.path.display()
                        )
                    });
                serde_json::json!({
                    "path": path,
                    "source_kind": format!("{:?}", file.kind),
                    "start_byte": diagnostic.range.start_byte,
                    "end_byte": diagnostic.range.end_byte,
                    "start_line": start_line,
                    "start_character": start_character,
                    "end_line": end_line,
                    "end_character": end_character,
                    "severity": format!("{:?}", diagnostic.severity),
                    "code": diagnostic.code,
                    "message": diagnostic.message,
                })
            })
            .collect::<Vec<_>>();
        diagnostics.sort_by(|left, right| {
            (
                left["source_kind"].as_str(),
                left["path"].as_str(),
                left["start_byte"].as_u64(),
                left["end_byte"].as_u64(),
                left["severity"].as_str(),
                left["code"].as_str(),
                left["message"].as_str(),
            )
                .cmp(&(
                    right["source_kind"].as_str(),
                    right["path"].as_str(),
                    right["start_byte"].as_u64(),
                    right["end_byte"].as_u64(),
                    right["severity"].as_str(),
                    right["code"].as_str(),
                    right["message"].as_str(),
                ))
        });
        let encoded = serde_json::to_vec(&diagnostics)?;
        let fingerprint_sha256 = format!("{:x}", Sha256::digest(&encoded));
        println!(
            "{}",
            serde_json::to_string(&serde_json::json!({
                "diagnostic_manifest": {
                    "project": workspace.root_path,
                    "fingerprint_sha256": fingerprint_sha256,
                    "diagnostics": diagnostics,
                }
            }))?
        );
    }
    Ok(())
}

async fn prepare_live_definition_probes(
    server: &RubyLanguageServer,
    workspace_path: &std::path::Path,
    probes: &[ReferenceProbe],
) -> anyhow::Result<Vec<PreparedDefinitionProbe>> {
    let workspace_path = std::fs::canonicalize(workspace_path)?;
    let mut prepared = Vec::with_capacity(probes.len());
    let mut opened = std::collections::HashSet::new();
    for probe in probes {
        anyhow::ensure!(
            probe.path.is_relative(),
            "--definition-at must be workspace-relative: {}",
            probe.path.display()
        );
        let path = std::fs::canonicalize(workspace_path.join(&probe.path))?;
        anyhow::ensure!(
            path.starts_with(&workspace_path),
            "--definition-at escapes the workspace: {}",
            probe.path.display()
        );
        let uri = Url::from_file_path(&path)
            .map_err(|()| anyhow::anyhow!("invalid definition file path: {}", path.display()))?;
        if opened.insert(uri.clone()) {
            let content = fs::read_to_string(&path)?;
            indexing::handle_did_open(
                server,
                DidOpenTextDocumentParams {
                    text_document: TextDocumentItem {
                        uri: uri.clone(),
                        language_id: "ruby".to_string(),
                        version: 1,
                        text: content,
                    },
                },
            )
            .await;
        }
        prepared.push(PreparedDefinitionProbe {
            relative_path: probe.path.clone(),
            uri,
            position: Position {
                line: probe.line,
                character: probe.character,
            },
        });
    }
    Ok(prepared)
}

async fn observe_first_live_definition(
    server: &RubyLanguageServer,
    probe: PreparedDefinitionProbe,
    indexing_started: Instant,
) -> serde_json::Value {
    let workspace = server.workspace_for_uri(&probe.uri).unwrap_or_else(|| {
        panic!(
            "INVARIANT VIOLATED: live definition probe {} has no owning project. This is a profiler setup bug because live navigation evidence requires one isolated engine. Fix: choose a project-owned file.",
            probe.relative_path.display()
        )
    });
    loop {
        let query_started = Instant::now();
        let locations = match request::handle_goto_definition(
            server,
            GotoDefinitionParams {
                text_document_position_params: TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier {
                        uri: probe.uri.clone(),
                    },
                    position: probe.position,
                },
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: PartialResultParams::default(),
            },
        )
        .await
        {
            Ok(Some(GotoDefinitionResponse::Array(locations))) => locations,
            Ok(Some(GotoDefinitionResponse::Scalar(location))) => vec![location],
            Ok(Some(GotoDefinitionResponse::Link(links))) => links
                .into_iter()
                .map(|link| tower_lsp::lsp_types::Location {
                    uri: link.target_uri,
                    range: link.target_selection_range,
                })
                .collect(),
            Ok(None) | Err(_) => Vec::new(),
        };
        let query_elapsed = query_started.elapsed();
        let status = workspace.indexing_status.snapshot();
        if !locations.is_empty() {
            let engine = workspace.analysis_engine.read();
            let target_source_kinds = locations
                .iter()
                .map(|location| {
                    location
                        .uri
                        .to_file_path()
                        .ok()
                        .and_then(|path| engine.file_id(path))
                        .and_then(|file_id| engine.file(file_id))
                        .map(|file| format!("{:?}", file.kind))
                        .unwrap_or_else(|| "Unknown".to_string())
                })
                .collect::<Vec<_>>();
            assert!(
                target_source_kinds.iter().all(|kind| kind != "Unknown"),
                "INVARIANT VIOLATED: live definition probe {} resolved to a location absent from its originating project engine. This is a profiler or provenance bug because successful staged navigation must retain exact semantic ownership. Fix: preserve the originating engine for external locations and register every returned source.",
                probe.relative_path.display()
            );
            return serde_json::json!({
                "file": probe.relative_path,
                "line": probe.position.line,
                "character": probe.position.character,
                "first_success_elapsed_ms": u64::try_from(indexing_started.elapsed().as_millis()).unwrap_or(u64::MAX),
                "query_elapsed_ns": u64::try_from(query_elapsed.as_nanos()).unwrap_or(u64::MAX),
                "project_root": workspace.root_path,
                "generation": status.generation,
                "sequence": status.sequence,
                "phase": status.phase,
                "target_source_kinds": target_source_kinds,
                "locations": locations,
            });
        }
        if matches!(
            status.phase,
            ruby_fast_lsp::indexing_status::IndexingPhase::Ready
                | ruby_fast_lsp::indexing_status::IndexingPhase::Failed
                | ruby_fast_lsp::indexing_status::IndexingPhase::Cancelled
        ) {
            panic!(
                "INVARIANT VIOLATED: live definition probe {}:{}:{} never resolved before project {} reached terminal phase {:?}. This is a profiler acceptance failure because readiness timestamps without a successful semantic query are not evidence. Fix: repair the selected probe or the staged indexing lifecycle. Failure: {:?}",
                probe.relative_path.display(),
                probe.position.line,
                probe.position.character,
                workspace.root_path.display(),
                status.phase,
                status.failure
            );
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

async fn sample_references(
    server: &RubyLanguageServer,
    workspace_path: &std::path::Path,
    probes: &[ReferenceProbe],
) -> anyhow::Result<()> {
    for probe in probes {
        let path = std::fs::canonicalize(workspace_path.join(&probe.path))?;
        anyhow::ensure!(
            path.starts_with(workspace_path),
            "--references-at escapes the workspace: {}",
            probe.path.display()
        );
        let uri = Url::from_file_path(&path)
            .map_err(|()| anyhow::anyhow!("invalid references file path: {}", path.display()))?;
        let content = fs::read_to_string(&path)?;
        indexing::handle_did_open(
            server,
            DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri: uri.clone(),
                    language_id: "ruby".to_string(),
                    version: 1,
                    text: content,
                },
            },
        )
        .await;
        let position = Position {
            line: probe.line,
            character: probe.character,
        };
        let started = Instant::now();
        let locations = references::find_references_at_position(server, &uri, position)
            .await
            .unwrap_or_default();
        println!(
            "{}",
            serde_json::to_string(&serde_json::json!({
                "references_file": probe.path,
                "position": position,
                "elapsed_ns": u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX),
                "count": locations.len(),
                "locations": locations,
            }))?
        );
    }
    Ok(())
}

async fn sample_definitions(
    server: &RubyLanguageServer,
    workspace_path: &std::path::Path,
    probes: &[ReferenceProbe],
) -> anyhow::Result<()> {
    for probe in probes {
        let path = std::fs::canonicalize(workspace_path.join(&probe.path))?;
        anyhow::ensure!(
            path.starts_with(workspace_path),
            "--definition-at escapes the workspace: {}",
            probe.path.display()
        );
        let uri = Url::from_file_path(&path)
            .map_err(|()| anyhow::anyhow!("invalid definition file path: {}", path.display()))?;
        let content = fs::read_to_string(&path)?;
        indexing::handle_did_open(
            server,
            DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri: uri.clone(),
                    language_id: "ruby".to_string(),
                    version: 1,
                    text: content,
                },
            },
        )
        .await;
        let position = Position {
            line: probe.line,
            character: probe.character,
        };
        let started = Instant::now();
        let locations = definitions::find_definition_at_position(server, uri.clone(), position)
            .await
            .unwrap_or_default();
        println!(
            "{}",
            serde_json::to_string(&serde_json::json!({
                "definition_file": probe.path,
                "position": position,
                "elapsed_ns": u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX),
                "count": locations.len(),
                "locations": locations,
            }))?
        );
    }
    Ok(())
}

async fn sample_open_file_diagnostics(
    server: &RubyLanguageServer,
    workspace_path: &std::path::Path,
    relative_paths: &[PathBuf],
) -> anyhow::Result<()> {
    for relative_path in relative_paths {
        anyhow::ensure!(
            relative_path.is_relative(),
            "--diagnostics-file must be workspace-relative: {}",
            relative_path.display()
        );
        let path = std::fs::canonicalize(workspace_path.join(relative_path))?;
        anyhow::ensure!(
            path.starts_with(workspace_path),
            "--diagnostics-file escapes the workspace: {}",
            relative_path.display()
        );
        let uri = Url::from_file_path(&path)
            .map_err(|()| anyhow::anyhow!("invalid diagnostics file path: {}", path.display()))?;
        let content = fs::read_to_string(&path)?;
        indexing::handle_did_open(
            server,
            DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri: uri.clone(),
                    language_id: "ruby".to_string(),
                    version: 1,
                    text: content,
                },
            },
        )
        .await;
        let diagnostics = EngineQuery::with_engine(server.analysis_engine_for_uri(&uri))
            .get_unresolved_diagnostics(&uri);

        println!(
            "{}",
            serde_json::to_string(&serde_json::json!({
                "diagnostics_file": relative_path,
                "semantic_diagnostics": diagnostics,
            }))?
        );
    }
    Ok(())
}

fn configure_server(
    server: &RubyLanguageServer,
    config_path: Option<&PathBuf>,
    extension_path: Option<&PathBuf>,
) -> Duration {
    let mut lsp_config = config_path
        .map(|path| load_profiler_config(path))
        .unwrap_or_default();
    if let Some(path) = extension_path {
        let absolute = std::fs::canonicalize(path).unwrap_or_else(|error| {
            panic!(
                "INVARIANT VIOLATED: profiler --extension-path must point to an existing path. \
                 This is a bug because VS Code parity profiling requires real bundled stubs. \
                 Fix: pass the installed extension directory. Path: {}. Error: {error}",
                path.display()
            )
        });
        info!("Using extension path: {}", absolute.display());
        let bundled_extensions = absolute.join("extensions");
        assert!(
            bundled_extensions.is_dir(),
            "INVARIANT VIOLATED: profiler --extension-path has no bundled extensions directory. This is a bug because VS Code parity profiling must load the same framework guests as the installed editor package. Fix: pass the extracted or installed VS Code extension root. Missing: {}",
            bundled_extensions.display()
        );
        lsp_config.extension_path = Some(absolute.to_string_lossy().to_string());
        lsp_config
            .extension_dirs
            .push(bundled_extensions.to_string_lossy().to_string());
    }
    let extension_load_started = Instant::now();
    server.extension_registry.configure_from_config(&lsp_config);
    let extension_load = extension_load_started.elapsed();
    *server.config.lock() = lsp_config;
    extension_load
}

fn load_profiler_config(path: &PathBuf) -> RubyFastLspConfig {
    const MAX_CONFIG_BYTES: u64 = 1024 * 1024;
    let absolute = std::fs::canonicalize(path).unwrap_or_else(|error| {
        panic!(
            "INVARIANT VIOLATED: profiler --config path cannot be canonicalized. This is a bug \
             because production evidence must record one exact configuration file. Fix: pass an \
             existing readable JSON file. Path: {}. Error: {error}",
            path.display()
        )
    });
    let metadata = std::fs::metadata(&absolute).unwrap_or_else(|error| {
        panic!(
            "INVARIANT VIOLATED: profiler --config metadata is unreadable. This is a bug because \
             configuration input must be bounded before reading. Fix: make the file readable. \
             Path: {}. Error: {error}",
            absolute.display()
        )
    });
    assert!(
        metadata.is_file() && metadata.len() <= MAX_CONFIG_BYTES,
        "INVARIANT VIOLATED: profiler --config must be a regular JSON file no larger than 1 MiB. \
         This is a bug because profiler configuration must remain bounded. Fix: pass a small \
         canonical configuration file. Path: {}, bytes: {}",
        absolute.display(),
        metadata.len()
    );
    let bytes = std::fs::read(&absolute).unwrap_or_else(|error| {
        panic!(
            "INVARIANT VIOLATED: profiler --config cannot be read. This is a bug because the \
             selected evidence configuration must be reproducible. Fix: make the file readable. \
             Path: {}. Error: {error}",
            absolute.display()
        )
    });
    let config: RubyFastLspConfig = serde_json::from_slice(&bytes).unwrap_or_else(|error| {
        panic!(
            "INVARIANT VIOLATED: profiler --config is not canonical Ruby Fast LSP JSON. This is a \
             bug because measurements cannot silently use defaults after malformed input. Fix: \
             correct the JSON configuration. Path: {}. Error: {error}",
            absolute.display()
        )
    });
    config
        .validate_runtime_configuration()
        .unwrap_or_else(|error| {
            panic!(
                "INVARIANT VIOLATED: profiler --config runtime selection is invalid. This is a \
                 bug because evidence must use a defensible runtime identity. Fix: correct the \
                 runtime/JRuby project configuration. Path: {}. Error: {error}",
                absolute.display()
            )
        });
    config
}

async fn run_full_indexing(
    server: &RubyLanguageServer,
    definition_probes: &[PreparedDefinitionProbe],
) -> Duration {
    let start = Instant::now();
    run_registered_workspace_indexing(server, definition_probes).await;
    info!("Full indexing completed in {:?}", start.elapsed());
    start.elapsed()
}

async fn run_indexing_only(
    server: &RubyLanguageServer,
    definition_probes: &[PreparedDefinitionProbe],
) -> Duration {
    let start = Instant::now();
    run_registered_workspace_indexing(server, definition_probes).await;
    info!("Indexing completed in {:?}", start.elapsed());
    start.elapsed()
}

async fn run_registered_workspace_indexing(
    server: &RubyLanguageServer,
    definition_probes: &[PreparedDefinitionProbe],
) {
    let workspaces = server.list_workspaces();
    assert!(
        !workspaces.is_empty(),
        "INVARIANT VIOLATED: profiler reached indexing without registered projects. This is a profiler lifecycle bug because workspace containers must be expanded before indexing. Fix: call add_workspace_folder before run_registered_workspace_indexing."
    );
    let wall_started = Instant::now();
    let resources_started = ProcessResourceUsage::capture();
    let mut live_probe_tasks = tokio::task::JoinSet::new();
    for probe in definition_probes.iter().cloned() {
        let server = server.clone();
        live_probe_tasks.spawn(async move {
            observe_first_live_definition(&server, probe, wall_started).await
        });
    }
    let scheduled = workspaces
        .into_iter()
        .map(|workspace| {
            let run = workspace.begin_indexing_run();
            let admission = server.indexing_scheduler.register_cancellable(
                workspace.root_path.clone(),
                ruby_fast_lsp::indexing_scheduler::IndexingPriority::Background,
                run.cancellation(),
            );
            (workspace, run, admission)
        })
        .collect::<Vec<_>>();
    let mut tasks = tokio::task::JoinSet::new();
    for (workspace, run, admission) in scheduled {
        let server = server.clone();
        tasks.spawn(async move {
            let uri = workspace.root_uri.clone();
            let Some(_permit) = admission.wait().await else {
                return (
                    uri,
                    Err(anyhow::anyhow!(
                        "profiler indexing generation {} was cancelled before admission",
                        run.generation()
                    )),
                );
            };
            let _ = workspace.indexing_status.transition(
                run.generation(),
                ruby_fast_lsp::indexing_status::IndexingPhase::ResolvingRuntime,
                None,
                None,
            );
            let result = indexing::init_workspace_for_run(&server, uri.clone(), run.clone()).await;
            match &result {
                Ok(_) => {
                    let _ = workspace.indexing_status.transition(
                        run.generation(),
                        ruby_fast_lsp::indexing_status::IndexingPhase::Ready,
                        None,
                        None,
                    );
                }
                Err(error) => {
                    let _ = workspace
                        .indexing_status
                        .fail(run.generation(), error.to_string());
                }
            }
            (uri, result)
        });
    }
    let mut completed = Vec::new();
    while let Some(joined) = tasks.join_next().await {
        let (uri, result) = joined.expect(
            "INVARIANT VIOLATED: profiler workspace indexing task panicked. This is a bug because a production measurement cannot omit one isolated project. Fix: inspect the indexing task panic and keep every discovered project in the gate.",
        );
        match result {
            Ok(timings) => completed.push((uri, timings)),
            Err(error) => {
                panic!(
                    "INVARIANT VIOLATED: profiler project `{uri}` indexing failed. This is a bug because performance measurements require every isolated project to complete. Fix: repair the corpus or indexing failure before benchmarking. Error: {error}"
                );
            }
        }
    }
    completed.sort_by(|(left, _), (right, _)| left.as_str().cmp(right.as_str()));
    let mut live_definition_evidence = Vec::new();
    while let Some(joined) = live_probe_tasks.join_next().await {
        live_definition_evidence.push(joined.expect(
            "INVARIANT VIOLATED: live navigation probe task panicked. This is a bug because staged readiness evidence cannot silently omit a configured probe. Fix: inspect the probe panic and keep every configured position in the profiler result.",
        ));
    }
    live_definition_evidence.sort_by(|left, right| {
        left["file"]
            .as_str()
            .cmp(&right["file"].as_str())
            .then_with(|| left["line"].as_u64().cmp(&right["line"].as_u64()))
            .then_with(|| left["character"].as_u64().cmp(&right["character"].as_u64()))
    });
    for (uri, timings) in &completed {
        println!(
            "{}",
            serde_json::json!({
                "indexing_timing": indexing_timing_json(uri, *timings)
            })
        );
    }

    let wall = wall_started.elapsed();
    let resources_finished = ProcessResourceUsage::capture();
    println!(
        "{}",
        serde_json::json!({
            "indexing_summary": indexing_summary_json(
                server,
                &completed,
                wall,
                resources_started,
                resources_finished,
                &live_definition_evidence,
            )
        })
    );
}

fn duration_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn indexing_timing_json(
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

fn indexing_summary_json(
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
            "runtime": workspace.effective_runtime.read().clone(),
            "detected_ruby_version": workspace.detected_ruby_version.read().clone(),
            "runtime_classpath_fingerprint_sha256": workspace.runtime_classpath_fingerprint_sha256.read().clone(),
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
    let scheduler = server.indexing_scheduler.snapshot();
    let resources = server.indexing_resources.snapshot();
    let resource_delta = ProcessResourceUsage::delta(resources_started, resources_finished);
    let core_cache = server.core_engine_cache.snapshot();
    let core_cache_retained_weight = server.core_engine_cache.retained_weight();
    let runtime_stdlib_path_cache = server.runtime_stdlib_path_cache_snapshot();
    let runtime_stdlib_path_cache_retained_weight =
        server.runtime_stdlib_path_cache_retained_weight();
    let gem_cache = server.gem_dependency_cache.snapshot();
    let gem_cache_retained_weight = server.gem_dependency_cache.retained_weight();
    let classpath_file_cache = server.classpath_file_product_cache.snapshot();
    let classpath_file_cache_retained_weight =
        server.classpath_file_product_cache.retained_weight_bytes();
    let java_artifact_product_cache = server.java_artifact_product_cache_snapshot();
    let java_artifact_product_cache_retained_weight =
        server.java_artifact_product_cache_retained_weight();
    let persistent_gem_cache = server
        .persistent_derived_product_cache
        .gem_product_snapshot();
    let persistent_java_artifact_cache = server
        .persistent_derived_product_cache
        .java_artifact_snapshot();
    let persistent_compiled_wasm_cache = server
        .persistent_derived_product_cache
        .compiled_wasm_snapshot();
    let gem_binding = server.gem_dependency_binding_counters.snapshot();

    serde_json::json!({
        "schema_version": 13,
        "ruby_fast_lsp_version": env!("CARGO_PKG_VERSION"),
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
        },
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

fn hash_length_prefixed(hasher: &mut Sha256, bytes: &[u8]) {
    let length = u64::try_from(bytes.len()).expect(
        "INVARIANT VIOLATED: profiler fingerprint input length exceeds u64. This is a bug because one source path or file cannot be that large in the process address space. Fix: inspect corrupt source metadata.",
    );
    hasher.update(length.to_le_bytes());
    hasher.update(bytes);
}

fn stable_fingerprint_hex(bytes: [u8; 16]) -> String {
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        write!(&mut encoded, "{byte:02x}").expect(
            "INVARIANT VIOLATED: writing a byte to an in-memory String failed. This is a bug because String formatting is infallible. Fix: inspect the formatter implementation before emitting profiler evidence.",
        );
    }
    encoded
}

fn dataset_fingerprint_sha256(project_evidence: &[serde_json::Value]) -> String {
    const IDENTITY_FIELDS: [&str; 7] = [
        "root",
        "runtime",
        "detected_ruby_version",
        "runtime_classpath_fingerprint_sha256",
        "project_files",
        "project_source_bytes",
        "project_source_fingerprint_sha256",
    ];

    let mut projects = project_evidence.iter().collect::<Vec<_>>();
    projects.sort_by(|left, right| {
        let left_root = left
            .get("root")
            .and_then(serde_json::Value::as_str)
            .expect(
                "INVARIANT VIOLATED: profiler project evidence has no string root. This is a bug because dataset identity requires one canonical project owner. Fix: construct project evidence with its canonical root before fingerprinting.",
            );
        let right_root = right
            .get("root")
            .and_then(serde_json::Value::as_str)
            .expect(
                "INVARIANT VIOLATED: profiler project evidence has no string root. This is a bug because dataset identity requires one canonical project owner. Fix: construct project evidence with its canonical root before fingerprinting.",
            );
        left_root.cmp(right_root)
    });

    let mut fingerprint = Sha256::new();
    hash_length_prefixed(&mut fingerprint, b"ruby-fast-lsp-profiler-dataset-v1");
    for project in projects {
        for field in IDENTITY_FIELDS {
            let value = project.get(field).unwrap_or_else(|| {
                panic!(
                    "INVARIANT VIOLATED: profiler project evidence is missing dataset identity field `{field}`. This is a bug because comparable runs must hash the same complete input identity. Fix: add the field before computing the dataset fingerprint."
                )
            });
            let encoded = serde_json::to_vec(value).expect(
                "INVARIANT VIOLATED: profiler dataset identity could not serialize to JSON. This is a bug because every evidence field is already JSON-compatible. Fix: keep dataset identity fields serializable.",
            );
            hash_length_prefixed(&mut fingerprint, field.as_bytes());
            hash_length_prefixed(&mut fingerprint, &encoded);
        }
    }
    format!("{:x}", fingerprint.finalize())
}

fn machine_evidence() -> serde_json::Value {
    serde_json::json!({
        "os_release": bounded_command_output("uname", &["-sr"]),
        "cpu_model": bounded_command_output("sysctl", &["-n", "machdep.cpu.brand_string"]),
        "physical_memory_bytes": bounded_command_output("sysctl", &["-n", "hw.memsize"]),
    })
}

fn build_evidence() -> serde_json::Value {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let source_revision = bounded_command_output_in(manifest_dir, "git", &["rev-parse", "HEAD"]);
    let source_worktree_status =
        bounded_command_output_in(manifest_dir, "git", &["status", "--short"]);
    let tracked_diff_sha256 = command_stdout_sha256_in(
        manifest_dir,
        "git",
        &["diff", "--binary", "HEAD", "--", "."],
    );
    let executable = std::env::current_exe().ok();
    let executable_sha256 = executable.as_deref().and_then(sha256_file);
    serde_json::json!({
        "source_revision": source_revision,
        "source_worktree_status": source_worktree_status,
        "tracked_diff_sha256": tracked_diff_sha256,
        "profiler_executable": executable,
        "profiler_executable_sha256": executable_sha256,
    })
}

fn millisecond_summary(values: &[u64]) -> serde_json::Value {
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

fn bounded_command_output(program: &str, args: &[&str]) -> Option<String> {
    bounded_command_output_in(".", program, args)
}

fn bounded_command_output_in(
    current_dir: impl AsRef<std::path::Path>,
    program: &str,
    args: &[&str],
) -> Option<String> {
    const MAX_OUTPUT_BYTES: usize = 4096;
    let output = std::process::Command::new(program)
        .current_dir(current_dir)
        .args(args)
        .output()
        .ok()?;
    if !output.status.success() || output.stdout.len() > MAX_OUTPUT_BYTES {
        return None;
    }
    let value = std::str::from_utf8(&output.stdout).ok()?.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn command_stdout_sha256_in(
    current_dir: impl AsRef<std::path::Path>,
    program: &str,
    args: &[&str],
) -> Option<String> {
    const MAX_OUTPUT_BYTES: usize = 256 * 1024 * 1024;
    use std::io::Read;
    use std::process::Stdio;

    let mut child = std::process::Command::new(program)
        .current_dir(current_dir)
        .args(args)
        .stdout(Stdio::piped())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let mut hasher = Sha256::new();
    let mut total = 0usize;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = stdout.read(&mut buffer).ok()?;
        if read == 0 {
            break;
        }
        total = total.checked_add(read)?;
        if total > MAX_OUTPUT_BYTES {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        hasher.update(&buffer[..read]);
    }
    let status = child.wait().ok()?;
    status.success().then(|| format!("{:x}", hasher.finalize()))
}

fn sha256_file(path: &std::path::Path) -> Option<String> {
    use std::io::Read;

    let mut file = std::fs::File::open(path).ok()?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).ok()?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Some(format!("{:x}", hasher.finalize()))
}

#[derive(Clone, Copy, Debug)]
struct ProcessResourceUsage {
    user_cpu_us: u64,
    system_cpu_us: u64,
    peak_rss_bytes: u64,
    input_blocks: u64,
    output_blocks: u64,
}

impl ProcessResourceUsage {
    #[cfg(unix)]
    fn capture() -> Option<Self> {
        let mut usage = std::mem::MaybeUninit::<libc::rusage>::zeroed();
        // SAFETY: `usage` points to writable storage for one `rusage`; libc
        // initializes it on a successful `getrusage` call.
        let result = unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) };
        if result != 0 {
            return None;
        }
        // SAFETY: successful `getrusage` initialized every field.
        let usage = unsafe { usage.assume_init() };
        let peak_rss = u64::try_from(usage.ru_maxrss).ok()?;
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        let peak_rss_bytes = peak_rss;
        #[cfg(not(any(target_os = "macos", target_os = "ios")))]
        let peak_rss_bytes = peak_rss.checked_mul(1024)?;
        Some(Self {
            user_cpu_us: timeval_us(usage.ru_utime)?,
            system_cpu_us: timeval_us(usage.ru_stime)?,
            peak_rss_bytes,
            input_blocks: u64::try_from(usage.ru_inblock).ok()?,
            output_blocks: u64::try_from(usage.ru_oublock).ok()?,
        })
    }

    #[cfg(not(unix))]
    fn capture() -> Option<Self> {
        None
    }

    fn delta(start: Option<Self>, end: Option<Self>) -> serde_json::Value {
        let (Some(start), Some(end)) = (start, end) else {
            return serde_json::Value::Null;
        };
        serde_json::json!({
            "user_cpu_ms": (end.user_cpu_us.saturating_sub(start.user_cpu_us)) as f64 / 1000.0,
            "system_cpu_ms": (end.system_cpu_us.saturating_sub(start.system_cpu_us)) as f64 / 1000.0,
            "peak_rss_bytes": end.peak_rss_bytes,
            "input_blocks": end.input_blocks.saturating_sub(start.input_blocks),
            "output_blocks": end.output_blocks.saturating_sub(start.output_blocks),
        })
    }
}

#[cfg(unix)]
fn timeval_us(value: libc::timeval) -> Option<u64> {
    let seconds = u64::try_from(value.tv_sec).ok()?;
    let micros = u64::try_from(value.tv_usec).ok()?;
    seconds.checked_mul(1_000_000)?.checked_add(micros)
}

async fn run_type_inference_only(server: &RubyLanguageServer) {
    let start = Instant::now();
    let inferred_count = server
        .list_workspaces()
        .into_iter()
        .map(|workspace| {
            workspace
                .analysis_engine
                .read()
                .type_store()
                .all_facts()
                .into_iter()
                .filter(|fact| matches!(fact.subject, TypeSubject::MethodReturn(_)))
                .count()
        })
        .sum::<usize>();
    info!("Type inference completed in {:?}", start.elapsed());
    info!(
        "Analysis engine has {} method return type facts",
        inferred_count
    );
}

async fn run_production_benchmark(
    server: &RubyLanguageServer,
    workspace_path: &std::path::Path,
    cold_indexing: Duration,
    iterations: usize,
) -> anyhow::Result<ProductionMeasurements> {
    let file_path = workspace_path.join("app/controllers/users_controller.rb");
    let original = fs::read_to_string(&file_path).map_err(|error| {
        anyhow::anyhow!(
            "production benchmark requires {} from the deterministic sample corpus: {error}",
            file_path.display()
        )
    })?;
    let uri = Url::from_file_path(&file_path)
        .map_err(|_| anyhow::anyhow!("invalid benchmark file path: {}", file_path.display()))?;

    indexing::handle_did_open(
        server,
        DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: "ruby".to_string(),
                version: 1,
                text: original.clone(),
            },
        },
    )
    .await;

    let completion_position = position_after(&original, "@service.")?;
    let method_position = position_inside(&original, "list_users")?;
    let completion_context = Some(CompletionContext {
        trigger_kind: CompletionTriggerKind::TRIGGER_CHARACTER,
        trigger_character: Some(".".to_string()),
    });

    for _ in 0..5 {
        let _ = completion::find_completion_at_position(
            server,
            uri.clone(),
            completion_position,
            completion_context.clone(),
        )
        .await;
        let _ =
            definitions::find_definition_at_position(server, uri.clone(), method_position).await;
    }

    let mut completion_samples = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let start = Instant::now();
        let result = completion::find_completion_at_position(
            server,
            uri.clone(),
            completion_position,
            completion_context.clone(),
        )
        .await;
        completion_samples.push(start.elapsed());
        assert!(
            matches!(result, CompletionResponse::Array(ref items) if items.iter().any(|item| item.label == "list_users")),
            "INVARIANT VIOLATED: benchmark completion did not include list_users. This is a bug because timing an empty or semantically broken query would produce misleading evidence. Fix: repair the deterministic corpus or completion query position."
        );
    }

    let hover_params = || HoverParams {
        text_document_position_params: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            position: method_position,
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
    };
    let mut hover_samples = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let start = Instant::now();
        let result = hover::handle_hover(server, hover_params()).await;
        hover_samples.push(start.elapsed());
        assert!(
            result.is_some(),
            "INVARIANT VIOLATED: benchmark hover returned no result. This is a bug because timing an empty query would produce misleading evidence. Fix: repair the deterministic corpus or hover position."
        );
    }

    let mut definition_samples = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let start = Instant::now();
        let result =
            definitions::find_definition_at_position(server, uri.clone(), method_position).await;
        definition_samples.push(start.elapsed());
        assert!(
            result.as_ref().is_some_and(|locations| !locations.is_empty()),
            "INVARIANT VIOLATED: benchmark definition returned no locations. This is a bug because timing an empty query would produce misleading evidence. Fix: repair the deterministic corpus or definition position."
        );
    }

    let mut reference_samples = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let start = Instant::now();
        let result = references::find_references_at_position(server, &uri, method_position).await;
        reference_samples.push(start.elapsed());
        assert!(
            result.as_ref().is_some_and(|locations| !locations.is_empty()),
            "INVARIANT VIOLATED: benchmark references returned no locations. This is a bug because timing an empty query would produce misleading evidence. Fix: repair the deterministic corpus or reference position."
        );
    }

    let analysis_engine = server.analysis_engine_for_uri(&uri);
    let diagnostic_query = EngineQuery::with_engine(analysis_engine.clone());
    let mut diagnostic_samples = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let start = Instant::now();
        let _ = diagnostic_query.get_unresolved_diagnostics(&uri);
        diagnostic_samples.push(start.elapsed());
    }

    let variants = [
        format!("{original}\n# benchmark body edit a\n"),
        format!("{original}\n# benchmark body edit b\n"),
    ];
    let mut edit_samples = Vec::with_capacity(iterations);
    for iteration in 0..iterations {
        let start = Instant::now();
        indexing::handle_did_change(
            server,
            DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier {
                    uri: uri.clone(),
                    version: i32::try_from(iteration + 2).expect(
                        "INVARIANT VIOLATED: benchmark iteration count exceeds LSP document versions. This is a bug because the benchmark cannot represent that many edits. Fix: use fewer than i32::MAX iterations.",
                    ),
                },
                content_changes: vec![TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text: variants[iteration % variants.len()].clone(),
                }],
            },
        )
        .await;
        edit_samples.push(start.elapsed());
    }

    let engine_heap_bytes = analysis_engine.read().estimated_memory_stats().total();
    Ok(ProductionMeasurements {
        cold_indexing,
        edit: LatencySummary::from_samples(&edit_samples),
        completion: LatencySummary::from_samples(&completion_samples),
        hover: LatencySummary::from_samples(&hover_samples),
        definition: LatencySummary::from_samples(&definition_samples),
        references: LatencySummary::from_samples(&reference_samples),
        diagnostics: LatencySummary::from_samples(&diagnostic_samples),
        engine_heap_bytes,
    })
}

fn position_after(content: &str, needle: &str) -> anyhow::Result<Position> {
    let offset = content
        .find(needle)
        .map(|start| start + needle.len())
        .ok_or_else(|| anyhow::anyhow!("benchmark corpus is missing {needle:?}"))?;
    Ok(position_at_byte_offset(content, offset))
}

fn position_inside(content: &str, needle: &str) -> anyhow::Result<Position> {
    let offset = content
        .find(needle)
        .map(|start| start + 1)
        .ok_or_else(|| anyhow::anyhow!("benchmark corpus is missing {needle:?}"))?;
    Ok(position_at_byte_offset(content, offset))
}

fn position_at_byte_offset(content: &str, offset: usize) -> Position {
    assert!(
        content.is_char_boundary(offset),
        "INVARIANT VIOLATED: benchmark byte offset {offset} is not a UTF-8 character boundary. This is a bug because LSP positions must be derived from valid source boundaries. Fix: choose a complete source token."
    );
    let prefix = &content[..offset];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count();
    let line_start = prefix.rfind('\n').map_or(0, |newline| newline + 1);
    let character = content[line_start..offset].encode_utf16().count();
    Position::new(
        u32::try_from(line).expect("INVARIANT VIOLATED: benchmark line count exceeds u32"),
        u32::try_from(character).expect("INVARIANT VIOLATED: benchmark UTF-16 column exceeds u32"),
    )
}

fn print_production_measurements(measurements: &ProductionMeasurements) {
    let budget = ProductionBudget::default();
    println!("\n=== PRODUCTION BENCHMARK ===");
    println!(
        "cold_indexing: {:?} (budget {:?})",
        measurements.cold_indexing, budget.cold_indexing
    );
    print_latency("edit", measurements.edit, budget.edit);
    print_latency("completion", measurements.completion, budget.completion);
    print_latency("hover", measurements.hover, budget.hover);
    print_latency("definition", measurements.definition, budget.definition);
    print_latency("references", measurements.references, budget.references);
    print_latency("diagnostics", measurements.diagnostics, budget.diagnostics);
    println!(
        "engine_heap: {:.1} MB (budget {:.1} MB)",
        bytes_to_mb(measurements.engine_heap_bytes),
        bytes_to_mb(budget.engine_heap_bytes)
    );
}

fn print_latency(name: &str, summary: LatencySummary, budget: Duration) {
    println!(
        "{name}: n={} min={:?} p50={:?} p95={:?} max={:?} (p95 budget {:?})",
        summary.samples, summary.min, summary.p50, summary.p95, summary.max, budget
    );
}

fn print_stats(server: &RubyLanguageServer) {
    info!("=== SHALLOW TYPE SIZES ===");
    info!(
        "FullyQualifiedName: {} bytes",
        std::mem::size_of::<FullyQualifiedName>()
    );
    info!("SymbolFact: {} bytes", std::mem::size_of::<SymbolFact>());
    info!("MethodFact: {} bytes", std::mem::size_of::<MethodFact>());
    info!(
        "ReferenceCandidate: {} bytes",
        std::mem::size_of::<ReferenceCandidate>()
    );
    info!(
        "StoredReferenceCandidate: {} bytes",
        std::mem::size_of::<StoredReferenceCandidate>()
    );
    info!(
        "StoredConstantReferenceCandidate: {} bytes",
        std::mem::size_of::<StoredConstantReferenceCandidate>()
    );
    info!(
        "StoredMethodReferenceCandidate: {} bytes",
        std::mem::size_of::<StoredMethodReferenceCandidate>()
    );
    info!(
        "StoredResolvedReferenceCandidate: {} bytes",
        std::mem::size_of::<StoredResolvedReferenceCandidate>()
    );
    info!(
        "ReferenceFact: {} bytes",
        std::mem::size_of::<ReferenceFact>()
    );
    info!("TypeFact: {} bytes", std::mem::size_of::<TypeFact>());
    info!(
        "DiagnosticCandidate: {} bytes",
        std::mem::size_of::<DiagnosticCandidate>()
    );
    info!(
        "DiagnosticFact: {} bytes",
        std::mem::size_of::<DiagnosticFact>()
    );
    info!(
        "GraphNodeFact: {} bytes",
        std::mem::size_of::<GraphNodeFact>()
    );
    info!(
        "GraphEdgeFact: {} bytes",
        std::mem::size_of::<GraphEdgeFact>()
    );

    for workspace in server.list_workspaces() {
        let engine = workspace.analysis_engine.read();
        let stats = engine.stats();
        info!("=== ANALYSIS STATS: {} ===", workspace.root_path.display());
        info!("Files: {}", stats.files);
        info!("Source bytes indexed: {}", stats.source_bytes);
        info!("Symbols: {}", stats.symbols);
        info!("Methods: {}", stats.methods);
        info!("Reference candidates: {}", stats.reference_candidates);
        info!(
            "Reference candidates by kind: constants={}, methods={}, resolved={}",
            stats.constant_reference_candidates,
            stats.method_reference_candidates,
            stats.resolved_reference_candidates
        );
        info!("Resolved references: {}", stats.references);
        info!("Type facts: {}", stats.types);
        info!("Diagnostic candidates: {}", stats.diagnostic_candidates);
        info!("Diagnostics: {}", stats.diagnostics);
        info!("Graph nodes: {}", stats.graph_nodes);
        info!("Graph edges: {}", stats.graph_edges);
        info!("Unresolved graph edges: {}", stats.unresolved_graph_edges);

        let memory = engine.estimated_memory_stats();
        let total = memory.total();
        info!(
            "=== ESTIMATED ENGINE HEAP: {} ===",
            workspace.root_path.display()
        );
        info!("Estimated total: {:.1} MB", bytes_to_mb(total));
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

fn log_memory_bucket(name: &str, bytes: usize, total: usize) {
    let percent = if total == 0 {
        0.0
    } else {
        bytes as f64 * 100.0 / total as f64
    };
    info!("{name}: {:.1} MB ({percent:.1}%)", bytes_to_mb(bytes));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostics_files_are_collected_in_command_line_order() {
        let config = parse_args_from([
            "profiler",
            "--workspace",
            "/tmp/project",
            "--diagnostics-file",
            "lib/app.rb",
            "--diagnostics-file",
            "routes.rb",
        ]);

        assert_eq!(
            config.diagnostics_files,
            vec![PathBuf::from("lib/app.rb"), PathBuf::from("routes.rb")]
        );
    }

    #[test]
    fn canonical_configuration_path_is_collected_without_editor_translation() {
        let config = parse_args_from([
            "profiler",
            "--workspace",
            "/tmp/project",
            "--config",
            "/tmp/ruby-fast-lsp.json",
        ]);

        assert_eq!(
            config.config_path,
            Some(PathBuf::from("/tmp/ruby-fast-lsp.json"))
        );
    }

    #[test]
    fn semantic_export_manifest_is_explicit_and_disabled_by_default() {
        assert!(!parse_args_from(["profiler"]).semantic_export_manifest);
        assert!(
            parse_args_from(["profiler", "--semantic-export-manifest"]).semantic_export_manifest
        );
    }

    #[test]
    fn diagnostic_manifest_is_explicit_and_disabled_by_default() {
        assert!(!parse_args_from(["profiler"]).diagnostic_manifest);
        assert!(parse_args_from(["profiler", "--diagnostic-manifest"]).diagnostic_manifest);
    }

    #[test]
    fn scheduler_concurrency_is_explicit_and_positive() {
        let config = parse_args_from([
            "profiler",
            "--workspace",
            "/tmp/project",
            "--scheduler-concurrency",
            "1",
        ]);

        assert_eq!(config.scheduler_concurrency, 1);
    }

    #[test]
    fn indexing_resource_budget_is_explicit_and_positive() {
        let config = parse_args_from([
            "profiler",
            "--workspace",
            "/tmp/project",
            "--resource-cpu-lanes",
            "3",
            "--resource-task-limit",
            "2",
            "--resource-memory-mib",
            "384",
            "--resource-io-slots",
            "1",
        ]);

        assert_eq!(config.resource_cpu_lanes, Some(3));
        assert_eq!(config.resource_task_limit, Some(2));
        assert_eq!(config.resource_memory_mib, Some(384));
        assert_eq!(config.resource_io_slots, Some(1));
    }

    #[test]
    fn process_resource_delta_is_machine_readable_and_saturating() {
        let start = ProcessResourceUsage {
            user_cpu_us: 10_000,
            system_cpu_us: 5_000,
            peak_rss_bytes: 100,
            input_blocks: 8,
            output_blocks: 4,
        };
        let end = ProcessResourceUsage {
            user_cpu_us: 13_500,
            system_cpu_us: 7_250,
            peak_rss_bytes: 4096,
            input_blocks: 11,
            output_blocks: 3,
        };

        let delta = ProcessResourceUsage::delta(Some(start), Some(end));

        assert_eq!(delta["user_cpu_ms"], 3.5);
        assert_eq!(delta["system_cpu_ms"], 2.25);
        assert_eq!(delta["peak_rss_bytes"], 4096);
        assert_eq!(delta["input_blocks"], 3);
        assert_eq!(delta["output_blocks"], 0);
    }

    #[test]
    fn readiness_summary_uses_nearest_rank_percentiles() {
        let summary = millisecond_summary(&[100, 200, 300, 400, 500]);

        assert_eq!(summary["samples"], 5);
        assert_eq!(summary["min"], 100);
        assert_eq!(summary["p50"], 300);
        assert_eq!(summary["p95"], 500);
        assert_eq!(summary["max"], 500);
    }

    #[test]
    fn semantic_fingerprint_hex_is_fixed_width_and_byte_exact() {
        assert_eq!(
            stable_fingerprint_hex([
                0x00, 0x01, 0x0f, 0x10, 0xab, 0xcd, 0xef, 0xff, 0x12, 0x34, 0x56, 0x78, 0x90, 0xaa,
                0xbb, 0xcc,
            ]),
            "00010f10abcdefff1234567890aabbcc"
        );
    }

    #[test]
    fn dataset_fingerprint_excludes_measurement_results() {
        let first = serde_json::json!([{
            "root": "/workspace/app",
            "runtime": {"implementation": "jruby", "engineVersion": "9.2.21.0"},
            "detected_ruby_version": "2.5",
            "runtime_classpath_fingerprint_sha256": "classpath",
            "project_files": 2,
            "project_source_bytes": 42,
            "project_source_fingerprint_sha256": "source",
            "semantic_result_fingerprint_hex": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "project_navigation_ready_ms": 100,
            "dependency_navigation_ready_ms": 200,
            "semantic_complete_ms": 300
        }]);
        let second = serde_json::json!([{
            "root": "/workspace/app",
            "runtime": {"implementation": "jruby", "engineVersion": "9.2.21.0"},
            "detected_ruby_version": "2.5",
            "runtime_classpath_fingerprint_sha256": "classpath",
            "project_files": 2,
            "project_source_bytes": 42,
            "project_source_fingerprint_sha256": "source",
            "semantic_result_fingerprint_hex": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "project_navigation_ready_ms": 900,
            "dependency_navigation_ready_ms": 1000,
            "semantic_complete_ms": 1100
        }]);

        assert_eq!(
            dataset_fingerprint_sha256(first.as_array().unwrap()),
            dataset_fingerprint_sha256(second.as_array().unwrap()),
            "readiness measurements and semantic outputs must not mutate input dataset identity"
        );
    }

    #[test]
    fn loads_and_validates_bounded_canonical_configuration() {
        let fixture = tempfile::tempdir().expect("profiler config fixture must be created");
        let path = fixture.path().join("config.json");
        std::fs::write(
            &path,
            br#"{
                "runtime": {
                    "mode": "auto",
                    "projects": [{
                        "root": "/workspace/admin",
                        "selection": {
                            "implementation": "jruby",
                            "family": "9.2",
                            "engineVersion": "9.2.21.0",
                            "compatibilityVersion": "2.5",
                            "executable": "/runtimes/jruby/bin/jruby",
                            "discoverySource": "rvm",
                            "javaHome": "/jdks/17"
                        }
                    }]
                }
            }"#,
        )
        .expect("profiler config fixture must be written");

        let config = load_profiler_config(&path);
        assert_eq!(config.runtime.projects.len(), 1);
        assert_eq!(config.runtime.projects[0].root, "/workspace/admin");
    }

    #[test]
    fn reference_probes_parse_zero_indexed_positions_in_command_line_order() {
        let config = parse_args_from([
            "profiler",
            "--workspace",
            "/tmp/project",
            "--references-at",
            "spec/user_spec.rb:29:10",
            "--references-at",
            "lib/user.rb:4:2",
        ]);

        assert_eq!(
            config.reference_probes,
            vec![
                ReferenceProbe {
                    path: PathBuf::from("spec/user_spec.rb"),
                    line: 29,
                    character: 10,
                },
                ReferenceProbe {
                    path: PathBuf::from("lib/user.rb"),
                    line: 4,
                    character: 2,
                },
            ]
        );
    }

    #[test]
    fn definition_probes_parse_zero_indexed_positions_in_command_line_order() {
        let config = parse_args_from([
            "profiler",
            "--workspace",
            "/tmp/project",
            "--definition-at",
            "lib/runtime.rb:1:12",
            "--definition-at",
            "lib/runtime.rb:14:45",
        ]);

        assert_eq!(
            config.definition_probes,
            vec![
                ReferenceProbe {
                    path: PathBuf::from("lib/runtime.rb"),
                    line: 1,
                    character: 12,
                },
                ReferenceProbe {
                    path: PathBuf::from("lib/runtime.rb"),
                    line: 14,
                    character: 45,
                },
            ]
        );
    }

    #[tokio::test]
    async fn live_definition_probe_requires_a_real_semantic_answer() {
        let fixture = tempfile::tempdir().expect("live probe fixture must be created");
        std::fs::write(
            fixture.path().join("Gemfile"),
            "source 'https://rubygems.org'\n",
        )
        .expect("Gemfile must be written");
        std::fs::write(
            fixture.path().join("live.rb"),
            "class Live\n  def target; end\n  def call; target; end\nend\n",
        )
        .expect("live Ruby source must be written");
        let server = RubyLanguageServer::default();
        let canonical_root =
            std::fs::canonicalize(fixture.path()).expect("fixture root must canonicalize");
        server.add_workspace(Url::from_directory_path(&canonical_root).unwrap());
        let prepared = prepare_live_definition_probes(
            &server,
            &canonical_root,
            &[ReferenceProbe {
                path: PathBuf::from("live.rb"),
                line: 2,
                character: 14,
            }],
        )
        .await
        .expect("live definition probe must be prepared");

        let evidence =
            observe_first_live_definition(&server, prepared[0].clone(), Instant::now()).await;

        assert_eq!(evidence["file"], "live.rb");
        assert_eq!(evidence["phase"], "discovered");
        assert_eq!(evidence["target_source_kinds"][0], "Project");
        assert_eq!(evidence["locations"].as_array().unwrap().len(), 1);
    }
}

fn bytes_to_mb(bytes: usize) -> f64 {
    bytes as f64 / 1_048_576.0
}
