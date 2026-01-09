//! References capability - Find all usages of a symbol
//!
//! This module now delegates to the unified query layer for most operations.

// use log::info;
use tower_lsp::lsp_types::{Location, Position, Url};

use crate::query::IndexQuery;
use crate::server::RubyLanguageServer;

/// Find all references to a symbol at the given position.
pub async fn find_references_at_position(
    server: &RubyLanguageServer,
    uri: &Url,
    position: Position,
) -> Option<Vec<Location>> {
    // Get the document content from the server
    let docs_guard = server.docs.lock();
    let doc_arc = docs_guard.get(uri)?;
    let doc = doc_arc.read();
    let content = doc.content.clone();

    let index = server.index.lock();
    let query = IndexQuery::with_doc(&index, &doc);

    query.find_references_at_position(uri, position, &content)
}
