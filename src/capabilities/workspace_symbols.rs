//! Workspace Symbols capability — thin adapter over the query layer.
//!
//! Delegates symbol search and top-level listing to `EngineQuery`.

use crate::query::EngineQuery;
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
    let mut symbols = Vec::new();
    for analysis_engine in lang_server.analysis_engines() {
        let engine_query = EngineQuery::with_engine(analysis_engine);
        if query_text.is_empty() {
            symbols.extend(engine_query.get_top_level_symbols());
        } else {
            symbols.extend(engine_query.search_workspace_symbols(&query_text));
        }
    }
    symbols.sort_by(|left, right| {
        (
            left.name.as_str(),
            left.location.uri.as_str(),
            left.location.range.start,
            left.location.range.end,
        )
            .cmp(&(
                right.name.as_str(),
                right.location.uri.as_str(),
                right.location.range.start,
                right.location.range.end,
            ))
    });
    symbols.dedup_by(|left, right| {
        left.name == right.name
            && left.kind == right.kind
            && left.location == right.location
            && left.container_name == right.container_name
    });

    info!(
        "Workspace symbols search completed in {:?} - found {} symbols",
        start_time.elapsed(),
        symbols.len()
    );

    Some(symbols)
}
