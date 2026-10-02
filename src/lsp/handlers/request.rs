//! LSP Request Handlers
//!
//! This module contains handlers for LSP requests (messages that require a response).
//! Each handler delegates to the appropriate capability module for the actual logic.

use crate::environment::extensions::{ExtensionStatusParams, ExtensionStatusResponse};
use crate::lsp::capabilities::debug;
use crate::server::RubyLanguageServer;
use log::info;
use tower_lsp::jsonrpc::Result as LspResult;

// ============================================================================
// Debug Handlers
// ============================================================================

pub async fn handle_debug_lookup(
    lang_server: &RubyLanguageServer,
    params: debug::LookupParams,
) -> LspResult<debug::LookupResponse> {
    info!("Debug lookup request received for: {}", params.fqn);
    Ok(debug::handle_lookup(lang_server, params))
}

pub async fn handle_export_graph(
    lang_server: &RubyLanguageServer,
    params: debug::ExportGraphParams,
) -> LspResult<debug::ExportGraphResponse> {
    info!("Export graph request received");
    Ok(debug::handle_export_graph(lang_server, params))
}

pub async fn handle_extension_status(
    lang_server: &RubyLanguageServer,
    _params: ExtensionStatusParams,
) -> LspResult<ExtensionStatusResponse> {
    info!("Extension status request received");
    Ok(ExtensionStatusResponse {
        extensions: lang_server.extensions.registry().status_reports(),
    })
}
