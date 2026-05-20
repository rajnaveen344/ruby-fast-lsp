//! Definitions Capability - Go to definition support
//!
//! Handles definition requests by dispatching to:
//! - `EngineQuery` for constants, methods (via method resolution), and globals
//! - Document analysis for local variables
//! - YARD parser for type comments

use tower_lsp::lsp_types::{Location, Position, Url};

use crate::query::EngineQuery;
use crate::server::RubyLanguageServer;

/// Find definition at position using the unified EngineQuery layer
pub async fn find_definition_at_position(
    server: &RubyLanguageServer,
    uri: Url,
    position: Position,
) -> Option<Vec<Location>> {
    // Get document content and Arc
    let (content, doc_arc) = {
        let doc_guard = server.docs.lock();
        let doc_arc = doc_guard.get(&uri)?.clone();
        let doc = doc_arc.read();
        (doc.content.clone(), doc_arc.clone())
    };

    // Create unified query with document context (no lock held here).
    let query = EngineQuery::with_doc_and_engine(doc_arc, server.analysis_engine.clone());

    // query.find_definitions_at_position already checks YARD and uses analyzer
    // AND now handles local variables via self.doc
    query.find_definitions_at_position(&uri, position, &content)
}
