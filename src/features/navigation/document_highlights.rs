//! Document highlights: same-document semantic occurrences of the symbol at
//! the cursor, built on the reference query.

use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::{
    DocumentHighlight, DocumentHighlightKind, DocumentHighlightParams, Position, Url,
};

use crate::features::cursor::EngineQuery;
use crate::server::RubyLanguageServer;

/// Handle `textDocument/documentHighlight`.
pub async fn handle(
    server: &RubyLanguageServer,
    params: DocumentHighlightParams,
) -> LspResult<Option<Vec<DocumentHighlight>>> {
    let uri = &params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;
    Ok(find_document_highlights(server, uri, position))
}

fn find_document_highlights(
    server: &RubyLanguageServer,
    uri: &Url,
    position: Position,
) -> Option<Vec<DocumentHighlight>> {
    let (content, doc_arc) = {
        let docs_guard = server.documents.read();
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
