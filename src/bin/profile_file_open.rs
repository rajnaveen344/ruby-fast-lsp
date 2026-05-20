//! Profile memory usage when opening a file
//!
//! Usage: cargo run --release --bin profile_file_open -- <workspace_path> <file_to_open>
//!
//! This simulates:
//! 1. Indexing the workspace
//! 2. Opening a specific file (like did_open)
//! 3. Measuring memory before/after

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

use log::{info, LevelFilter};
use parking_lot::RwLock;
use ruby_fast_lsp::capabilities::indexing;
use ruby_fast_lsp::indexer::file_processor::FileProcessor;
use ruby_fast_lsp::server::RubyLanguageServer;
use ruby_fast_lsp::types::ruby_document::RubyDocument;
use std::env;
use std::sync::Arc;
use tokio::runtime::Runtime;
use tower_lsp::lsp_types::Url;

fn main() {
    let _profiler = dhat::Profiler::new_heap();

    env_logger::Builder::new()
        .filter_level(LevelFilter::Info)
        .init();

    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: {} <workspace_path> <file_to_open>", args[0]);
        eprintln!(
            "Example: {} /path/to/project /path/to/project/app/models/user.rb",
            args[0]
        );
        std::process::exit(1);
    }

    let workspace_path = &args[1];
    let file_to_open = &args[2];

    let workspace_absolute = match std::fs::canonicalize(workspace_path) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Invalid workspace path: {}", e);
            std::process::exit(1);
        }
    };
    let file_absolute = match std::fs::canonicalize(file_to_open) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Invalid file path: {}", e);
            std::process::exit(1);
        }
    };

    let workspace_uri = match Url::from_file_path(&workspace_absolute) {
        Ok(u) => u,
        Err(_) => {
            eprintln!("Failed to create workspace URI");
            std::process::exit(1);
        }
    };
    let file_uri = match Url::from_file_path(&file_absolute) {
        Ok(u) => u,
        Err(_) => {
            eprintln!("Failed to create file URI");
            std::process::exit(1);
        }
    };

    info!("Workspace: {}", workspace_absolute.display());
    info!("File to open: {}", file_absolute.display());

    let rt = Runtime::new().expect("Failed to create runtime");
    rt.block_on(async {
        let server = RubyLanguageServer::default();
        server.add_workspace(workspace_uri.clone());

        // Phase 1: index workspace
        info!("\n=== PHASE 1: Indexing Workspace ===");
        let start = std::time::Instant::now();
        if let Err(e) = indexing::init_workspace(&server, workspace_uri.clone()).await {
            info!("Indexing failed: {}", e);
            return;
        }
        info!("Indexing completed in {:?}", start.elapsed());

        let stats_after_index = dhat::HeapStats::get();
        info!("After indexing:");
        info!(
            "  Peak memory: {:.1} MB",
            stats_after_index.max_bytes as f64 / 1_000_000.0
        );
        info!(
            "  Current memory: {:.1} MB",
            stats_after_index.curr_bytes as f64 / 1_000_000.0
        );
        info!(
            "  Total allocations: {} blocks",
            stats_after_index.total_blocks
        );
        info!("  Analysis facts: {}", analysis_fact_count(&server));

        // Phase 2: Simulate file open
        info!("\n=== PHASE 2: Opening File ===");
        let content = match std::fs::read_to_string(&file_absolute) {
            Ok(c) => c,
            Err(e) => {
                info!("Failed to read file: {}", e);
                return;
            }
        };
        let file_size = content.len();
        info!(
            "File size: {} bytes ({:.1} KB)",
            file_size,
            file_size as f64 / 1024.0
        );

        let stats_before_open = dhat::HeapStats::get();
        let facts_before = analysis_fact_count(&server);

        // Simulate handle_did_open
        {
            // Create document
            let document = RubyDocument::new(file_uri.clone(), content.clone(), 1);
            server
                .docs
                .lock()
                .insert(file_uri.clone(), Arc::new(RwLock::new(document)));

            // Process file (collect facts/references)
            let indexer = FileProcessor::with_extension_registry(server.extension_registry.clone());
            let _ = indexer.process_file(&file_uri, &content, &server);
        }

        let stats_after_open = dhat::HeapStats::get();
        let facts_after_open = analysis_fact_count(&server);

        info!("\n=== RESULTS ===");
        info!("Before file open:");
        info!(
            "  Current memory: {:.1} MB",
            stats_before_open.curr_bytes as f64 / 1_000_000.0
        );
        info!("  Analysis facts: {}", facts_before);
        info!("After file open:");
        info!(
            "  Current memory: {:.1} MB",
            stats_after_open.curr_bytes as f64 / 1_000_000.0
        );
        info!("  Analysis facts: {}", facts_after_open);

        let memory_jump = stats_after_open.curr_bytes as i64 - stats_before_open.curr_bytes as i64;
        let facts_added = facts_after_open as i64 - facts_before as i64;
        info!(
            "Memory jump: {:.2} MB ({} bytes)",
            memory_jump as f64 / 1_000_000.0,
            memory_jump
        );
        info!("Facts added: {}", facts_added);

        // Phase 3: Simulate file close
        info!("\n=== PHASE 3: Closing File ===");
        {
            server.docs.lock().remove(&file_uri);
            // Note: Analysis facts are NOT removed (intentional for cross-file navigation)
        }

        let stats_after_close = dhat::HeapStats::get();
        let facts_after_close = analysis_fact_count(&server);
        info!("After file close:");
        info!(
            "  Current memory: {:.1} MB",
            stats_after_close.curr_bytes as f64 / 1_000_000.0
        );
        info!(
            "  Analysis facts: {} (should be same as after open)",
            facts_after_close
        );

        let memory_after_close =
            stats_after_close.curr_bytes as i64 - stats_before_open.curr_bytes as i64;
        info!(
            "Memory still retained: {:.2} MB",
            memory_after_close as f64 / 1_000_000.0
        );

        // Phase 4: Reopen same file
        info!("\n=== PHASE 4: Reopening Same File ===");
        let stats_before_reopen = dhat::HeapStats::get();
        {
            let mut docs = server.docs.lock();
            if let Some(existing_doc) = docs.get(&file_uri) {
                let mut doc_guard = existing_doc.write();
                doc_guard.update(content.clone(), 2);
                info!("Updated existing document");
            } else {
                let document = RubyDocument::new(file_uri.clone(), content.clone(), 2);
                docs.insert(file_uri.clone(), Arc::new(RwLock::new(document)));
                info!("Created new document");
            }
            drop(docs);

            let indexer = FileProcessor::with_extension_registry(server.extension_registry.clone());
            let _ = indexer.process_file(&file_uri, &content, &server);
        }

        let stats_after_reopen = dhat::HeapStats::get();
        let facts_after_reopen = analysis_fact_count(&server);
        let reopen_jump =
            stats_after_reopen.curr_bytes as i64 - stats_before_reopen.curr_bytes as i64;
        info!(
            "Memory jump on reopen: {:.2} MB",
            reopen_jump as f64 / 1_000_000.0
        );
        info!("Facts after reopen: {}", facts_after_reopen);

        info!("\n=== SUMMARY ===");
        info!(
            "First open memory jump: {:.2} MB",
            memory_jump as f64 / 1_000_000.0
        );
        info!(
            "Memory retained after close: {:.2} MB",
            memory_after_close as f64 / 1_000_000.0
        );
        info!(
            "Reopen memory jump: {:.2} MB",
            reopen_jump as f64 / 1_000_000.0
        );
        info!("Facts added on first open: {}", facts_added);
        info!(
            "Facts after reopen (should be same): {}",
            facts_after_reopen
        );
    });
}

fn analysis_fact_count(server: &RubyLanguageServer) -> usize {
    let engine = server.analysis_engine.lock();
    engine.all_symbol_facts().len()
        + engine.all_method_facts().len()
        + engine.reference_store().all_facts().len()
        + engine.type_store().all_facts().len()
}
