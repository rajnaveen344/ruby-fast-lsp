//! Debug capabilities exposed through custom LSP requests: FQN lookup for the
//! VS Code index view and graph export.

use log::debug;
pub use ruby_analysis::engine::{ExportGraphResponse, LookupResponse};
use serde::{Deserialize, Serialize};

use crate::lsp::query::EngineQuery;
use crate::server::RubyLanguageServer;
use parking_lot::RwLock;
use ruby_analysis::engine::AnalysisEngine;
use std::sync::Arc;
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
pub fn handle_lookup(server: &RubyLanguageServer, params: LookupParams) -> LookupResponse {
    debug!("[DEBUG] Looking up FQN: {}", params.fqn);
    let query = EngineQuery::with_engine(project_engine(server, params.uri.as_deref()));
    query.debug_lookup(&params.fqn)
}

/// Handle `ruby/exportGraph` - export the inheritance graph as JSON.
pub fn handle_export_graph(
    server: &RubyLanguageServer,
    params: ExportGraphParams,
) -> ExportGraphResponse {
    debug!("[DEBUG] Exporting inheritance graph");
    let query = EngineQuery::with_engine(project_engine(server, params.uri.as_deref()));
    query.debug_export_graph()
}
