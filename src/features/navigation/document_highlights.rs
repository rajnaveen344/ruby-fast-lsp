//! Document highlights: same-document semantic occurrences of the symbol at
//! the cursor, built on the reference query.

use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::{
    DocumentHighlight, DocumentHighlightKind, DocumentHighlightParams, Position, Url,
};

use crate::features::navigation::references;
use crate::server::Server;

/// Handle `textDocument/documentHighlight`.
pub async fn handle(
    server: &Server,
    params: DocumentHighlightParams,
) -> LspResult<Option<Vec<DocumentHighlight>>> {
    let uri = &params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;
    server.await_document_semantic_commit(uri).await;
    Ok(find_document_highlights(server, uri, position))
}

fn find_document_highlights(
    server: &Server,
    uri: &Url,
    position: Position,
) -> Option<Vec<DocumentHighlight>> {
    let mut highlights = references::read_open_document(server, uri, |cursor| {
        references::highlights_at(cursor, position)
    })?
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
