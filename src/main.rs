use ruby_fast_lsp::server::RubyLanguageServer;
use std::process::exit;

use anyhow::Result;
use log::{error, info};
use tower_lsp::{LspService, Server};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize the logger with debug level enabled (actual filtering is done via log::set_max_level)
    // This allows runtime log level changes without restarting the server
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();

    // Start with info level - can be changed at runtime via configuration
    log::set_max_level(log::LevelFilter::Info);

    info!("Starting Ruby Fast LSP server");

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::build(|client| {
        RubyLanguageServer::new(client).unwrap_or_else(|e| {
            error!("Failed to initialize Ruby LSP server: {}", e);
            exit(1)
        })
    })
    .custom_method(
        "ruby/namespaceTree",
        RubyLanguageServer::handle_namespace_tree_request,
    )
    // Debug commands for lsp-repl
    .custom_method("$/listCommands", RubyLanguageServer::handle_list_commands)
    .custom_method(
        "ruby-fast-lsp/debug/lookup",
        RubyLanguageServer::handle_debug_lookup,
    )
    .custom_method(
        "ruby-fast-lsp/debug/stats",
        RubyLanguageServer::handle_debug_stats,
    )
    .custom_method(
        "ruby-fast-lsp/debug/ancestors",
        RubyLanguageServer::handle_debug_ancestors,
    )
    .custom_method(
        "ruby-fast-lsp/debug/methods",
        RubyLanguageServer::handle_debug_methods,
    )
    .custom_method(
        "ruby-fast-lsp/debug/inference-stats",
        RubyLanguageServer::handle_debug_inference_stats,
    )
    .finish();

    info!("Ruby LSP server initialized, waiting for client connections");

    Server::new(stdin, stdout, socket).serve(service).await;

    Ok(())
}
