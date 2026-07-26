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
//!   --check-budgets      Fail when a production budget is exceeded
//!   --diagnostics-file <relative-path>  Open a file and print its user-visible diagnostics
//!   --definition-at <path:line:character>  Open a file and print definitions at an LSP position
//!   --references-at <path:line:character>  Open a file and print references at an LSP position
//!   --help               Show help

mod sample_project;

use log::{info, LevelFilter};
use ruby_analysis::core::{
    DiagnosticCandidate, DiagnosticFact, FullyQualifiedName, GraphEdgeFact, GraphNodeFact,
    MethodFact, ReferenceCandidate, ReferenceFact, StoredConstantReferenceCandidate,
    StoredMethodReferenceCandidate, StoredReferenceCandidate, StoredResolvedReferenceCandidate,
    SymbolFact, TypeFact, TypeSubject,
};
use ruby_fast_lsp::capabilities::indexing;
use ruby_fast_lsp::capabilities::{completion, definitions, hover, references};
use ruby_fast_lsp::config::RubyFastLspConfig;
use ruby_fast_lsp::perf::metrics::{LatencySummary, ProductionBudget, ProductionMeasurements};
use ruby_fast_lsp::query::EngineQuery;
use ruby_fast_lsp::server::RubyLanguageServer;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;
use tower_lsp::lsp_types::{
    CompletionContext, CompletionResponse, CompletionTriggerKind, DidChangeTextDocumentParams,
    DidOpenTextDocumentParams, HoverParams, Position, TextDocumentContentChangeEvent,
    TextDocumentIdentifier, TextDocumentItem, TextDocumentPositionParams, Url,
    VersionedTextDocumentIdentifier, WorkDoneProgressParams,
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
    check_budgets: bool,
    diagnostics_files: Vec<PathBuf>,
    definition_probes: Vec<ReferenceProbe>,
    reference_probes: Vec<ReferenceProbe>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ReferenceProbe {
    path: PathBuf,
    line: u32,
    character: u32,
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
        check_budgets: false,
        diagnostics_files: Vec::new(),
        definition_probes: Vec::new(),
        reference_probes: Vec::new(),
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
            "--check-budgets" => {
                config.check_budgets = true;
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
    --check-budgets          Exit unsuccessfully when a production budget is exceeded
    --diagnostics-file <PATH>
                             Open a workspace-relative file through didOpen and print diagnostics as JSON; repeatable
    --definition-at <PATH:LINE:CHARACTER>
                             Open a workspace-relative file and print definitions as JSON; repeatable, zero-indexed
    --references-at <PATH:LINE:CHARACTER>
                             Open a workspace-relative file and print resolved references as JSON; repeatable, zero-indexed
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
        let abs_path = std::fs::canonicalize(&path)?;
        info!("Using workspace: {}", abs_path.display());
        abs_path
    } else {
        info!("Creating sample Ruby project for profiling...");
        let sample_path = sample_project::create_sample_project()?;
        info!("Sample project created at: {}", sample_path.display());
        sample_path
    };

    let workspace_uri = Url::from_file_path(&workspace_path)
        .map_err(|_| anyhow::anyhow!("Invalid workspace path"))?;

    // Create runtime
    let rt = Runtime::new()?;

    let benchmark_result = rt.block_on(async {
        let server = RubyLanguageServer::default();
        configure_server(
            &server,
            config.config_path.as_ref(),
            config.extension_path.as_ref(),
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

        let cold_indexing = match config.phase {
            Phase::All => {
                // Full indexing (includes type inference)
                info!("=== PROFILING: Full Indexing (with type inference) ===");
                run_full_indexing(&server).await
            }
            Phase::Index => {
                // Index only (no type inference)
                info!("=== PROFILING: Indexing Only (no type inference) ===");
                run_indexing_only(&server).await
            }
            Phase::Infer => {
                // Index first, then profile inference separately
                info!("=== PROFILING: Type Inference Only ===");
                info!("Step 1: Indexing (not profiled focus)...");
                let indexing = run_indexing_only(&server).await;

                info!("Step 2: Type Inference (profiled)...");
                run_type_inference_only(&server).await;
                indexing
            }
        };

        info!("=== TOTAL TIME: {:?} ===", total_start.elapsed());

        // Print stats
        print_stats(&server);
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
) {
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
    server.extension_registry.configure_from_config(&lsp_config);
    *server.config.lock() = lsp_config;
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

async fn run_full_indexing(server: &RubyLanguageServer) -> Duration {
    let start = Instant::now();
    run_registered_workspace_indexing(server).await;
    info!("Full indexing completed in {:?}", start.elapsed());
    start.elapsed()
}

async fn run_indexing_only(server: &RubyLanguageServer) -> Duration {
    let start = Instant::now();
    run_registered_workspace_indexing(server).await;
    info!("Indexing completed in {:?}", start.elapsed());
    start.elapsed()
}

async fn run_registered_workspace_indexing(server: &RubyLanguageServer) {
    let workspaces = server.list_workspaces();
    assert!(
        !workspaces.is_empty(),
        "INVARIANT VIOLATED: profiler reached indexing without registered projects. This is a profiler lifecycle bug because workspace containers must be expanded before indexing. Fix: call add_workspace_folder before run_registered_workspace_indexing."
    );
    let mut tasks = tokio::task::JoinSet::new();
    for workspace in workspaces {
        let server = server.clone();
        tasks.spawn(async move {
            let uri = workspace.root_uri.clone();
            let result = indexing::init_workspace(&server, uri.clone()).await;
            (uri, result)
        });
    }
    while let Some(joined) = tasks.join_next().await {
        let (uri, result) = joined.expect(
            "INVARIANT VIOLATED: profiler workspace indexing task panicked. This is a bug because a production measurement cannot omit one isolated project. Fix: inspect the indexing task panic and keep every discovered project in the gate.",
        );
        if let Err(error) = result {
            panic!(
                "INVARIANT VIOLATED: profiler project `{uri}` indexing failed. This is a bug because performance measurements require every isolated project to complete. Fix: repair the corpus or indexing failure before benchmarking. Error: {error}"
            );
        }
    }
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
}

fn bytes_to_mb(bytes: usize) -> f64 {
    bytes as f64 / 1_048_576.0
}
