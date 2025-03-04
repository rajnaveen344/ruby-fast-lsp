mod analysis;
mod parser;
mod server;

use anyhow::Result;
use log::{info, error};
use tower_lsp::{LspService, Server};

use crate::server::RubyLanguageServer;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize the logger
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    
    info!("Starting Ruby Fast LSP server");
    
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    
    let (service, socket) = LspService::new(|client| {
        match RubyLanguageServer::new(client.clone()) {
            Ok(server) => server,
            Err(e) => {
                error!("Failed to initialize Ruby LSP server: {}", e);
                // Create a fallback server with minimal functionality
                RubyLanguageServer::new_fallback(client.clone())
            }
        }
    });
    
    info!("Ruby LSP server initialized, waiting for client connections");
    
    Server::new(stdin, stdout, socket).serve(service).await;
    
    Ok(())
}
