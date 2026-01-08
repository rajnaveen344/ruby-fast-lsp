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
//!   --help               Show help

mod sample_project;

use log::{info, LevelFilter};
use ruby_fast_lsp::capabilities::indexing;
use ruby_fast_lsp::inferrer::return_type::infer_return_type_for_node;
use ruby_fast_lsp::inferrer::RubyType;
use ruby_fast_lsp::server::RubyLanguageServer;
use std::collections::HashMap;
use std::env;
use std::path::PathBuf;
use std::time::Instant;
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
    memory_profiling: bool,
    phase: Phase,
}

fn parse_args() -> Config {
    let args: Vec<String> = env::args().collect();
    let mut config = Config {
        workspace: None,
        memory_profiling: false,
        phase: Phase::All,
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
        server.set_workspace_uri(Some(workspace_uri.clone()));

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
    });

    // Cleanup sample project if we created it
    if use_sample_project {
        info!("Cleaning up sample project...");
        let _ = sample_project::cleanup_sample_project();
    }

    Ok(())
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

    // Get all methods needing inference, grouped by file
    let methods_by_file: HashMap<Url, Vec<(ruby_fast_lsp::indexer::index::EntryId, u32)>> = {
        let index = server.index.lock();
        let methods = index.get_methods_needing_inference();
        let total = methods.len();
        info!("Found {} methods needing return type inference", total);

        let mut by_file: HashMap<Url, Vec<(ruby_fast_lsp::indexer::index::EntryId, u32)>> =
            HashMap::new();
        for (entry_id, file_id, line) in methods {
            if let Some(url) = index.get_file_url(file_id) {
                by_file
                    .entry(url.clone())
                    .or_default()
                    .push((entry_id, line));
            }
        }
        by_file
    };

    let total_files = methods_by_file.len();
    info!("Inferring types across {} files", total_files);

    let mut inferred_count = 0;

    // Process each file
    for (current, (file_url, methods)) in methods_by_file.into_iter().enumerate() {
        if current % 50 == 0 {
            info!("Progress: {}/{} files", current, total_files);
        }

        // Load file content
        let file_content = match file_url.to_file_path() {
            Ok(path) => match std::fs::read(&path) {
                Ok(content) => content,
                Err(_) => continue,
            },
            Err(_) => continue,
        };

        // Parse file once
        let parse_result = ruby_prism::parse(&file_content);
        let node = parse_result.node();

        // Infer each method in this file
        for (entry_id, line) in methods {
            if let Some(def_node) = find_def_node_at_line(&node, line, &file_content) {
                let mut index = server.index.lock();
                if let Some(inferred_ty) =
                    infer_return_type_for_node(&mut index, &file_content, &def_node, None, None)
                {
                    if inferred_ty != RubyType::Unknown {
                        index.update_method_return_type(entry_id, inferred_ty);
                        inferred_count += 1;
                    }
                }
            }
        }
    }

    info!("Type inference completed in {:?}", start.elapsed());
    info!(
        "Successfully inferred {} method return types",
        inferred_count
    );
}

fn find_def_node_at_line<'a>(
    node: &ruby_prism::Node<'a>,
    target_line: u32,
    content: &[u8],
) -> Option<ruby_prism::DefNode<'a>> {
    if let Some(def_node) = node.as_def_node() {
        let offset = def_node.location().start_offset();
        let line = content[..offset].iter().filter(|&&b| b == b'\n').count() as u32;
        if line == target_line {
            return Some(def_node);
        }
    }

    // Recurse into child nodes
    if let Some(program) = node.as_program_node() {
        for stmt in program.statements().body().iter() {
            if let Some(found) = find_def_node_at_line(&stmt, target_line, content) {
                return Some(found);
            }
        }
    }

    if let Some(class_node) = node.as_class_node() {
        if let Some(body) = class_node.body() {
            if let Some(stmts) = body.as_statements_node() {
                for stmt in stmts.body().iter() {
                    if let Some(found) = find_def_node_at_line(&stmt, target_line, content) {
                        return Some(found);
                    }
                }
            }
        }
    }

    if let Some(module_node) = node.as_module_node() {
        if let Some(body) = module_node.body() {
            if let Some(stmts) = body.as_statements_node() {
                for stmt in stmts.body().iter() {
                    if let Some(found) = find_def_node_at_line(&stmt, target_line, content) {
                        return Some(found);
                    }
                }
            }
        }
    }

    if let Some(stmts) = node.as_statements_node() {
        for stmt in stmts.body().iter() {
            if let Some(found) = find_def_node_at_line(&stmt, target_line, content) {
                return Some(found);
            }
        }
    }

    if let Some(sclass) = node.as_singleton_class_node() {
        if let Some(body) = sclass.body() {
            if let Some(stmts) = body.as_statements_node() {
                for stmt in stmts.body().iter() {
                    if let Some(found) = find_def_node_at_line(&stmt, target_line, content) {
                        return Some(found);
                    }
                }
            }
        }
    }

    None
}

fn print_stats(server: &RubyLanguageServer) {
    let index = server.index.lock();

    info!("=== INDEX STATS ===");
    info!("Total entries: {}", index.entries_len());
    info!("Total definitions: {}", index.definitions_len());
    info!("Total files: {}", index.files_count());

    let counts = index.count_entries_by_type();
    for (type_name, count) in counts {
        info!("  {}: {}", type_name, count);
    }
}
