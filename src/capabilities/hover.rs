//! Hover capability for displaying type information.
//!
//! Provides hover information for:
//! - Local variables (shows inferred type)
//! - Methods (shows return type)
//! - Classes/Modules (shows class/module name)
//! - Constants (shows type or value info)
//!
//! All logic is delegated to the query layer.

use tower_lsp::lsp_types::{
    Hover, HoverContents, HoverParams, HoverProviderCapability, MarkupContent, MarkupKind,
};

use crate::query::IndexQuery;
use crate::server::RubyLanguageServer;

/// Return the hover capability.
pub fn get_hover_capability() -> HoverProviderCapability {
    HoverProviderCapability::Simple(true)
}

/// Handle hover request using the unified IndexQuery layer.
pub async fn handle_hover(server: &RubyLanguageServer, params: HoverParams) -> Option<Hover> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;

    // Get document content and Arc (no lock held after this block)
    let (content, doc_arc) = {
        let docs = server.docs.lock();
        let doc_arc = docs.get(&uri)?.clone();
        let doc = doc_arc.read();
        (doc.content.clone(), doc_arc.clone())
    };

    // Create unified query with document context
    let query = IndexQuery::with_doc(server.index.clone(), doc_arc);

    // Get hover info from query layer
    let hover_info = query.get_hover_at_position(&uri, position, &content)?;

    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: hover_info.content,
        }),
        range: hover_info.range,
    })
}
