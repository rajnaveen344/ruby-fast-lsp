//! Same-document semantic highlights.

use tower_lsp::lsp_types::{DocumentHighlight, DocumentHighlightKind, Position, Url};

use crate::query::EngineQuery;
use crate::server::RubyLanguageServer;

pub async fn find_document_highlights(
    server: &RubyLanguageServer,
    uri: &Url,
    position: Position,
) -> Option<Vec<DocumentHighlight>> {
    let (content, doc_arc) = {
        let docs_guard = server.docs.lock();
        let doc_arc = docs_guard.get(uri)?.clone();
        let doc = doc_arc.read();
        (doc.content.clone(), doc_arc.clone())
    };

    let query = EngineQuery::with_doc_and_engine(doc_arc, server.analysis_engine_for_uri(uri));
    let mut highlights = query
        .find_document_highlight_locations_at_position(uri, position, &content)?
        .into_iter()
        .map(|location| DocumentHighlight {
            range: location.range,
            kind: Some(DocumentHighlightKind::TEXT),
        })
        .collect::<Vec<_>>();
    highlights.sort_by_key(|highlight| {
        (
            highlight.range.start.line,
            highlight.range.start.character,
            highlight.range.end.line,
            highlight.range.end.character,
        )
    });
    highlights.dedup_by_key(|highlight| highlight.range);
    (!highlights.is_empty()).then_some(highlights)
}
