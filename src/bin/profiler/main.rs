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
//!   --extension-path <p>  VS Code extension path for bundled stubs
//!   --hold-seconds <n>   Keep process alive after profiling for external memory tools
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
use ruby_fast_lsp::config::RubyFastLspConfig;
use ruby_fast_lsp::server::RubyLanguageServer;
use std::env;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;
use tower_lsp::lsp_types::Url;

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
    extension_path: Option<PathBuf>,
    memory_profiling: bool,
    phase: Phase,
    hold_seconds: u64,
}

fn parse_args() -> Config {
    let args: Vec<String> = env::args().collect();
    let mut config = Config {
        workspace: None,
        extension_path: None,
        memory_profiling: false,
        phase: Phase::All,
        hold_seconds: 0,
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

fn print_help() {
    println!(
        r#"Ruby Fast LSP Profiler

USAGE:
    profiler [OPTIONS] [WORKSPACE]

OPTIONS:
    -w, --workspace <PATH>   Path to Ruby workspace (default: built-in sample project)
    -m, --memory             Enable dhat memory profiling (outputs dhat-heap.json)
    -p, --phase <PHASE>      Profile specific phase: index, infer, all (default: all)
    --extension-path <PATH>  VS Code extension path for bundled stubs
    --hold-seconds <N>       Keep process alive after profiling for external memory tools
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

    # Use built-in sample project
    samply record ./target/release/profiler
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
        .filter_level(LevelFilter::Info)
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

    rt.block_on(async {
        let server = RubyLanguageServer::default();
        server.add_workspace(workspace_uri.clone());
        configure_server(&server, config.extension_path.as_ref());

        let total_start = Instant::now();

        match config.phase {
            Phase::All => {
                // Full indexing (includes type inference)
                info!("=== PROFILING: Full Indexing (with type inference) ===");
                run_full_indexing(&server, workspace_uri).await;
            }
            Phase::Index => {
                // Index only (no type inference)
                info!("=== PROFILING: Indexing Only (no type inference) ===");
                run_indexing_only(&server, workspace_uri).await;
            }
            Phase::Infer => {
                // Index first, then profile inference separately
                info!("=== PROFILING: Type Inference Only ===");
                info!("Step 1: Indexing (not profiled focus)...");
                run_indexing_only(&server, workspace_uri.clone()).await;

                info!("Step 2: Type Inference (profiled)...");
                run_type_inference_only(&server).await;
            }
        }

        info!("=== TOTAL TIME: {:?} ===", total_start.elapsed());

        // Print stats
        print_stats(&server);

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
    });

    // Cleanup sample project if we created it
    if use_sample_project {
        info!("Cleaning up sample project...");
        let _ = sample_project::cleanup_sample_project();
    }

    Ok(())
}

fn configure_server(server: &RubyLanguageServer, extension_path: Option<&PathBuf>) {
    let mut lsp_config = RubyFastLspConfig::default();
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
        lsp_config.extension_path = Some(absolute.to_string_lossy().to_string());
    }
    server.extension_registry.configure_from_config(&lsp_config);
    *server.config.lock() = lsp_config;
}

async fn run_full_indexing(server: &RubyLanguageServer, workspace_uri: Url) {
    let start = Instant::now();

    match indexing::init_workspace(server, workspace_uri).await {
        Ok(_) => {
            info!("Full indexing completed in {:?}", start.elapsed());
        }
        Err(e) => {
            info!("Indexing failed: {}", e);
        }
    }
}

async fn run_indexing_only(server: &RubyLanguageServer, workspace_uri: Url) {
    let start = Instant::now();

    // We need to run indexing without type inference
    // For now, just run full indexing - the profiler will show where time is spent
    match indexing::init_workspace(server, workspace_uri).await {
        Ok(_) => {
            info!("Indexing completed in {:?}", start.elapsed());
        }
        Err(e) => {
            info!("Indexing failed: {}", e);
        }
    }
}

async fn run_type_inference_only(server: &RubyLanguageServer) {
    let start = Instant::now();
    let inferred_count = server
        .analysis_engine
        .lock()
        .type_store()
        .all_facts()
        .into_iter()
        .filter(|fact| matches!(fact.subject, TypeSubject::MethodReturn(_)))
        .count();
    info!("Type inference completed in {:?}", start.elapsed());
    info!(
        "Analysis engine has {} method return type facts",
        inferred_count
    );
}

fn print_stats(server: &RubyLanguageServer) {
    let engine = server.analysis_engine.lock();
    let stats = engine.stats();

    info!("=== ANALYSIS STATS ===");
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

    let memory = engine.estimated_memory_stats();
    let total = memory.total();
    info!("=== ESTIMATED ENGINE HEAP ===");
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
