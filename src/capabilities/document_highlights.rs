//! Same-document semantic highlights.

use tower_lsp::lsp_types::{DocumentHighlight, DocumentHighlightKind, Position, Url};

use crate::capabilities::references;
use crate::server::RubyLanguageServer;

pub async fn find_document_highlights(
    server: &RubyLanguageServer,
    uri: &Url,
    position: Position,
) -> Option<Vec<DocumentHighlight>> {
    let mut highlights = references::find_references_at_position(server, uri, position)
        .await?
        .into_iter()
        .filter(|location| &location.uri == uri)
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
