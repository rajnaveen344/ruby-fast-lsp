//! Analyze the composition of entries in the index
//!
//! This binary helps understand what types of entries are being stored.

use log::{info, LevelFilter};
use ruby_fast_lsp::capabilities::indexing;
use ruby_fast_lsp::server::RubyLanguageServer;
use std::env;
use tokio::runtime::Runtime;
use tower_lsp::lsp_types::Url;

fn main() -> anyhow::Result<()> {
    // Initialize logging
    env_logger::Builder::new()
        .filter_level(LevelFilter::Info)
        .init();

    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <workspace_path>", args[0]);
        std::process::exit(1);
    }

    let folder_path = &args[1];
    let absolute_path = std::fs::canonicalize(folder_path)?;
    let workspace_uri =
        Url::from_file_path(&absolute_path).map_err(|_| anyhow::anyhow!("Invalid folder path"))?;

    info!(
        "Analyzing entries for workspace: {}",
        absolute_path.display()
    );

    let rt = Runtime::new()?;
    rt.block_on(async {
        let server = RubyLanguageServer::default();
        server.set_workspace_uri(Some(workspace_uri.clone()));

        info!("Starting workspace indexing...");
        let start_time = std::time::Instant::now();

        match indexing::init_workspace(&server, workspace_uri).await {
            Ok(_) => {
                info!("Indexing completed in {:?}", start_time.elapsed());

                // Now analyze the entries
                let index = server.index.lock();

                let total_entries = index.entries_len();
                let total_definitions = index.definitions_len();

                info!("=== Entry Analysis ===");
                info!("Total entries in slotmap: {}", total_entries);
                info!("Total definitions (unique FQNs): {}", total_definitions);

                // Count entries by type using the built-in method
                let type_counts = index.count_entries_by_type();

                info!("\n=== Entries by Type ===");
                let mut sorted_counts: Vec<_> = type_counts.iter().collect();
                sorted_counts.sort_by(|a, b| b.1.cmp(a.1));
                for (type_name, count) in sorted_counts {
                    info!("  {}: {}", type_name, count);
                }

                // Calculate reference percentage
                let reference_count = type_counts.get("Reference").copied().unwrap_or(0);
                let definition_entries = total_entries - reference_count;
                info!("\n=== Summary ===");
                info!("Definition entries: {}", definition_entries);
                info!("Reference entries: {}", reference_count);
                info!(
                    "Reference percentage: {:.1}%",
                    (reference_count as f64 / total_entries as f64) * 100.0
                );

                // Check the discrepancy
                info!("\n=== Storage Analysis ===");
                info!("Unique FQNs (definitions index): {}", total_definitions);
                info!("Total entries (slotmap): {}", total_entries);
                info!(
                    "This means on average {:.1} entries per unique FQN",
                    total_entries as f64 / total_definitions as f64
                );
            }
            Err(e) => info!("Indexing failed: {}", e),
        }
    });

    Ok(())
}
