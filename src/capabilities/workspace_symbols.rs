//! Workspace Symbols capability — thin adapter over the query layer.
//!
//! Delegates symbol search and top-level listing to `IndexQuery`.

use crate::query::IndexQuery;
use crate::server::RubyLanguageServer;
use log::info;
use std::time::Instant;
use tower_lsp::lsp_types::{SymbolInformation, WorkspaceSymbolParams};

/// Handle workspace symbol requests.
pub async fn handle_workspace_symbols(
    lang_server: &RubyLanguageServer,
    params: WorkspaceSymbolParams,
) -> Option<Vec<SymbolInformation>> {
    let query_text = params.query;
    info!("Workspace symbols request for query: '{}'", query_text);

    let start_time = Instant::now();
    let query = IndexQuery::new(lang_server.index.clone());

    let symbols = if query_text.is_empty() {
        query.get_top_level_symbols()
    } else {
        query.search_workspace_symbols(&query_text)
    };

    info!(
        "Workspace symbols search completed in {:?} - found {} symbols",
        start_time.elapsed(),
        symbols.len()
    );

    Some(symbols)
}
