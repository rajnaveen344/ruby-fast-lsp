use dhat::Profiler;
use log::{info, LevelFilter};
use ruby_fast_lsp::capabilities::indexing;
use ruby_fast_lsp::server::RubyLanguageServer;
use std::env;
use tokio::runtime::Runtime;
use tower_lsp::lsp_types::Url;

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

fn main() -> anyhow::Result<()> {
    // Enable the profiler
    let _profiler = Profiler::new_heap();

    // Initialize logger
    env_logger::Builder::new()
        .filter_level(LevelFilter::Info)
        .init();

    // Parse command line arguments
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <folder_path>", args[0]);
        std::process::exit(1);
    }

    let folder_path = &args[1];
    let absolute_path = std::fs::canonicalize(folder_path)?;
    let workspace_uri =
        Url::from_file_path(&absolute_path).map_err(|_| anyhow::anyhow!("Invalid folder path"))?;

    info!(
        "Starting profiling harness for: {}",
        absolute_path.display()
    );

    // Create a Tokio runtime
    let rt = Runtime::new()?;

    rt.block_on(async {
        // Initialize the server
        let server = RubyLanguageServer::default();

        // Configure the workspace URI
        server.set_workspace_uri(Some(workspace_uri.clone()));

        // We don't have a real client, so we can't easily capture progress notifications
        // but the server logs should show what's happening.

        // Trigger indexing directly
        info!("Taking snapshot of heap before indexing...");

        // Use the indexing module directly if possible, or via server
        // server.index.clone() returns the index, but we want to trigger the process.
        // `indexing::init_workspace` is what we want.

        info!("Starting workspace initialization...");
        let start_time = std::time::Instant::now();

        match indexing::init_workspace(&server, workspace_uri).await {
            Ok(_) => {
                info!("Indexing completed successfully!");
                info!(
                    "Total definitions: {}",
                    server.index.lock().definitions_len()
                );
            }
            Err(e) => info!("Indexing failed: {}", e),
        }

        info!("Total time: {:?}", start_time.elapsed());
    });

    Ok(())
}
