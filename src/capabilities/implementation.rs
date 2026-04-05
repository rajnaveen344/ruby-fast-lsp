//! Implementation Capability - Find implementations of methods and modules
//!
//! Answers "textDocument/implementation":
//! - For a method: find all overrides in descendant classes and including classes
//! - For a module/class: find all classes that include/prepend/extend it

use tower_lsp::lsp_types::{Location, Position, Url};

use crate::query::IndexQuery;
use crate::server::RubyLanguageServer;

/// Find implementations at position using the unified IndexQuery layer
pub async fn find_implementation_at_position(
    server: &RubyLanguageServer,
    uri: Url,
    position: Position,
) -> Option<Vec<Location>> {
    let (content, doc_arc) = {
        let doc_guard = server.docs.lock();
        let doc_arc = doc_guard.get(&uri)?.clone();
        let doc = doc_arc.read();
        (doc.content.clone(), doc_arc.clone())
    };

    let query = IndexQuery::with_doc(server.index.clone(), doc_arc);
    query.find_implementations_at_position(&uri, position, &content)
}
