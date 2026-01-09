//! References capability - Find all usages of a symbol
//!
//! This module now delegates to the unified query layer for most operations.

use tower_lsp::lsp_types::{Location, Position, Url};

use crate::query::IndexQuery;
use crate::server::RubyLanguageServer;

/// Find all references to a symbol at the given position.
pub async fn find_references_at_position(
    server: &RubyLanguageServer,
    uri: &Url,
    position: Position,
) -> Option<Vec<Location>> {
    // Get document content and Arc (no lock held after this block)
    let (content, doc_arc) = {
        let docs_guard = server.docs.lock();
        let doc_arc = docs_guard.get(uri)?.clone();
        let doc = doc_arc.read();
        (doc.content.clone(), doc_arc.clone())
    };

    // Create unified query with document context
    let query = IndexQuery::with_doc(server.index.clone(), doc_arc);

    query.find_references_at_position(uri, position, &content)
}
