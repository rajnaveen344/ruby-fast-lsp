//! Workspace Symbols capability — thin adapter over the query layer.
//!
//! Delegates symbol search and top-level listing to `IndexQuery`.

use crate::query::IndexQuery;
use crate::server::RubyLanguageServer;
use log::info;
use std::time::Instant;
use tower_lsp::lsp_types::{SymbolInformation, WorkspaceSymbolParams};

/// Handle workspace symbol requests.
///
/// `workspace/symbol` has no anchor URI, so we query every registered
/// workspace index plus the orphan index and merge the results. Multi-root
/// workspaces see symbols from every folder, with the per-workspace indices
/// remaining isolated for all other queries.
pub async fn handle_workspace_symbols(
    lang_server: &RubyLanguageServer,
    params: WorkspaceSymbolParams,
) -> Option<Vec<SymbolInformation>> {
    let query_text = params.query;
    info!("Workspace symbols request for query: '{}'", query_text);

    let start_time = Instant::now();
    let mut symbols: Vec<SymbolInformation> = Vec::new();
    for index in lang_server.all_indices() {
        let query = IndexQuery::new(index);
        let part = if query_text.is_empty() {
            query.get_top_level_symbols()
        } else {
            query.search_workspace_symbols(&query_text)
        };
        symbols.extend(part);
    }

    info!(
        "Workspace symbols search completed in {:?} - found {} symbols",
        start_time.elapsed(),
        symbols.len()
    );

    Some(symbols)
}
