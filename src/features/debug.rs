//! Debug and status requests: FQN lookup for the VS Code index view, graph
//! export, and extension status.

use log::{debug, info};
pub use ruby_analysis::engine::{ExportGraphResponse, LookupResponse};
use serde::{Deserialize, Serialize};

use crate::environment::extensions::{ExtensionStatusParams, ExtensionStatusResponse};
use crate::server::{ProjectHandle, Server};
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::Url;

fn project(server: &Server, uri: Option<&str>) -> ProjectHandle {
    uri.and_then(|value| Url::parse(value).ok())
        .map(|uri| server.project_for_uri(&uri))
        .or_else(|| {
            let projects = server.list_workspaces();
            (projects.len() == 1).then(|| projects[0].handle().clone())
        })
        .unwrap_or_else(|| server.orphan_project().clone())
}

// ============================================================================
// Lookup Types
// ============================================================================

/// Parameters for `ruby-fast-lsp/debug/lookup`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LookupParams {
    /// The fully qualified name to look up (e.g., "User#find", "Foo::Bar")
    pub fqn: String,
    #[serde(default)]
    pub uri: Option<String>,
}

// ============================================================================
// Export Graph Types
// ============================================================================

/// Parameters for `ruby/exportGraph`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExportGraphParams {
    #[serde(default)]
    pub uri: Option<String>,
}

// ============================================================================
// Handlers
// ============================================================================

/// Handle `ruby-fast-lsp/debug/lookup` - query analysis state for an FQN.
pub async fn handle_lookup(server: &Server, params: LookupParams) -> LspResult<LookupResponse> {
    info!("Debug lookup request received for: {}", params.fqn);
    debug!("[DEBUG] Looking up FQN: {}", params.fqn);
    Ok(project(server, params.uri.as_deref()).view(|view| view.debug_lookup(&params.fqn)))
}

/// Handle `ruby/exportGraph` - export the inheritance graph as JSON.
pub async fn handle_export_graph(
    server: &Server,
    params: ExportGraphParams,
) -> LspResult<ExportGraphResponse> {
    info!("Export graph request received");
    debug!("[DEBUG] Exporting inheritance graph");
    Ok(project(server, params.uri.as_deref()).view(|view| view.debug_export_graph()))
}

/// Handle `ruby-fast-lsp/extensions/status` - list loaded extension states.
pub async fn handle_extension_status(
    server: &Server,
    _params: ExtensionStatusParams,
) -> LspResult<ExtensionStatusResponse> {
    info!("Extension status request received");
    Ok(ExtensionStatusResponse {
        extensions: server.extension_registry().status_reports(),
    })
}
