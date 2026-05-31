use std::env;
use std::path::PathBuf;
use std::time::Instant;

use anyhow::Result;
use log::{info, LevelFilter};
use ruby_fast_lsp::config::RubyFastLspConfig;
use ruby_fast_lsp::extensions::ExtensionRegistryHandle;
use ruby_fast_lsp::indexer::file_processor::FileProcessor;
use ruby_fast_lsp::server::RubyLanguageServer;
use tower_lsp::lsp_types::Url;

fn main() -> Result<()> {
    env_logger::Builder::new()
        .filter_level(LevelFilter::Info)
        .init();

    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <file_to_process> [workspace_root]", args[0]);
        std::process::exit(1);
    }

    let file_path = std::fs::canonicalize(&args[1])?;
    let workspace_path = if args.len() >= 3 {
        std::fs::canonicalize(&args[2])?
    } else {
        file_path
            .parent()
            .map(PathBuf::from)
            .expect("file path must have a parent")
    };
    let content = std::fs::read_to_string(&file_path)?;
    let file_uri =
        Url::from_file_path(&file_path).map_err(|()| anyhow::anyhow!("invalid file path"))?;
    let workspace_uri = Url::from_file_path(&workspace_path)
        .map_err(|()| anyhow::anyhow!("invalid workspace path"))?;

    let mut config = RubyFastLspConfig::default();
    let bundled_rspec = PathBuf::from("extensions/rspec-ruby");
    if bundled_rspec.join("extension.toml").is_file() {
        config
            .extension_packages
            .push(bundled_rspec.canonicalize()?.to_string_lossy().to_string());
    }
    let extension_registry = ExtensionRegistryHandle::from_config(&config);

    let server = RubyLanguageServer::default();
    server.add_workspace(workspace_uri);
    let processor = FileProcessor::with_extension_registry(extension_registry);

    info!("Profile single file: {}", file_path.display());
    info!("File size: {} bytes", content.len());
    let start = Instant::now();
    processor.process_file_current_file_resolution(&file_uri, &content, &server)?;
    info!("Profile single file total: {:?}", start.elapsed());

    Ok(())
}
