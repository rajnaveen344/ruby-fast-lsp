//! Same-document semantic highlights.

use log::info;
use std::time::Instant;
use tower_lsp::lsp_types::{DocumentHighlight, DocumentHighlightKind, Position, Url};

use crate::query::EngineQuery;
use crate::server::RubyLanguageServer;

pub async fn find_document_highlights(
    server: &RubyLanguageServer,
    uri: &Url,
    position: Position,
) -> Option<Vec<DocumentHighlight>> {
    let total_start = Instant::now();
    let (content, doc_arc) = {
        let docs_guard = server.docs.lock();
        let doc_arc = docs_guard.get(uri)?.clone();
        let doc = doc_arc.read();
        (doc.content.clone(), doc_arc.clone())
    };

    let query = EngineQuery::with_doc_and_engine(doc_arc, server.analysis_engine_for_uri(uri));
    let lookup_start = Instant::now();
    let locations = query.find_document_highlight_locations_at_position(uri, position, &content);
    let lookup_elapsed = lookup_start.elapsed();
    let Some(locations) = locations else {
        info!(
            "[PERF][documentHighlight waterfall] file={} total={:?} lookup=none@{:?} highlights=0",
            uri.path(),
            total_start.elapsed(),
            lookup_elapsed
        );
        return None;
    };

    let mut highlights = locations
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
    let count = highlights.len();
    info!(
        "[PERF][documentHighlight waterfall] file={} total={:?} lookup={:?} highlights={}",
        uri.path(),
        total_start.elapsed(),
        lookup_elapsed,
        count
    );
    (!highlights.is_empty()).then_some(highlights)
}
