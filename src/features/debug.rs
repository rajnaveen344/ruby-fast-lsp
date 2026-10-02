//! Debug and status requests: FQN lookup for the VS Code index view, graph
//! export, and extension status.

use log::{debug, info};
pub use ruby_analysis::engine::{ExportGraphResponse, LookupResponse};
use serde::{Deserialize, Serialize};

use crate::environment::extensions::{ExtensionStatusParams, ExtensionStatusResponse};
use crate::server::RubyLanguageServer;
use parking_lot::RwLock;
use ruby_analysis::engine::{AnalysisEngine, AnalysisQuery};
use std::sync::Arc;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::Url;

fn project_engine(server: &RubyLanguageServer, uri: Option<&str>) -> Arc<RwLock<AnalysisEngine>> {
    uri.and_then(|value| Url::parse(value).ok())
        .map(|uri| server.analysis_engine_for_uri(&uri))
        .or_else(|| {
            let projects = server.list_workspaces();
            (projects.len() == 1).then(|| projects[0].analysis_engine.clone())
        })
        .unwrap_or_else(|| server.orphan_engine().clone())
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
pub async fn handle_lookup(
    server: &RubyLanguageServer,
    params: LookupParams,
) -> LspResult<LookupResponse> {
    info!("Debug lookup request received for: {}", params.fqn);
    debug!("[DEBUG] Looking up FQN: {}", params.fqn);
    let engine = project_engine(server, params.uri.as_deref());
    let engine = engine.read();
    Ok(AnalysisQuery::new(&engine).debug_lookup(&params.fqn))
}

/// Handle `ruby/exportGraph` - export the inheritance graph as JSON.
pub async fn handle_export_graph(
    server: &RubyLanguageServer,
    params: ExportGraphParams,
) -> LspResult<ExportGraphResponse> {
    info!("Export graph request received");
    debug!("[DEBUG] Exporting inheritance graph");
    let engine = project_engine(server, params.uri.as_deref());
    let engine = engine.read();
    Ok(AnalysisQuery::new(&engine).debug_export_graph())
}

/// Handle `ruby-fast-lsp/extensions/status` - list loaded extension states.
pub async fn handle_extension_status(
    server: &RubyLanguageServer,
    _params: ExtensionStatusParams,
) -> LspResult<ExtensionStatusResponse> {
    info!("Extension status request received");
    Ok(ExtensionStatusResponse {
        extensions: server.extensions.registry().status_reports(),
    })
}
