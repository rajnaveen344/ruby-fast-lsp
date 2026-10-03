//! Protocol lifecycle handlers: initialize (server capabilities), initialized,
//! shutdown, configuration, workspace folders, and watched files. Document
//! notifications delegate to `super::indexing`.

mod configuration;
mod initialize;
mod watched_files;
mod workspace_folders;

pub use configuration::handle_did_change_configuration;
pub use initialize::{handle_initialize, handle_initialized};
pub use watched_files::handle_did_change_watched_files;
pub use workspace_folders::handle_did_change_workspace_folders;

use crate::lsp::lifecycle::indexing;
use crate::server::Server;
use log::info;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::*;

pub async fn handle_did_open(server: &Server, params: DidOpenTextDocumentParams) {
    indexing::handle_did_open(server, params).await;
}

pub async fn handle_did_change(server: &Server, params: DidChangeTextDocumentParams) {
    indexing::handle_did_change(server, params).await;
}

pub async fn handle_did_close(server: &Server, params: DidCloseTextDocumentParams) {
    indexing::handle_did_close(server, params).await;
}

pub async fn handle_did_save(server: &Server, params: DidSaveTextDocumentParams) {
    indexing::handle_did_save(server, params).await;
}

pub async fn handle_shutdown(server: &Server) -> LspResult<()> {
    info!("Shutting down Ruby LSP server");
    server.cancel_watched_file_changes();
    server.cancel_all_indexing();
    server.extension_registry().shutdown();
    Ok(())
}

#[cfg(test)]
mod tests;
