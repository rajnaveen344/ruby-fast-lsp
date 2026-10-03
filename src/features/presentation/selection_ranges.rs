//! Selection ranges: the enclosing syntax chain at each requested position.

use crate::invariant::ExpectInvariant;
use ruby_analysis::indexer::selection_range_chains;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::{SelectionRange, SelectionRangeParams};

use crate::server::Server;
use crate::utils::lsp::{lsp_text_range, source_position};

/// Handle `textDocument/selectionRange`.
pub async fn handle(
    server: &Server,
    params: SelectionRangeParams,
) -> LspResult<Option<Vec<SelectionRange>>> {
    let Some(document) = server
        .documents
        .read()
        .get(&params.text_document.uri)
        .cloned()
    else {
        return Ok(None);
    };
    let document = document.read();
    let offsets = params
        .positions
        .iter()
        .map(|position| document.position_to_analysis_offset(source_position(*position)))
        .collect::<Vec<_>>();
    let chains = selection_range_chains(
        document.analysis_file_id(),
        document.analysis_content(),
        &offsets,
    );

    Ok(Some(
        chains
            .into_iter()
            .map(|chain| selection_range_from_chain(&document, chain))
            .collect(),
    ))
}

fn selection_range_from_chain(
    document: &ruby_analysis::indexer::RubyDocument,
    chain: Vec<ruby_analysis::core::TextRange>,
) -> SelectionRange {
    let mut nested = None;
    for range in chain.into_iter().rev() {
        nested = Some(SelectionRange {
            range: lsp_text_range(document, range),
            parent: nested.map(Box::new),
        });
    }
    nested.expect_invariant(
        "indexer returned an empty selection range chain",
        "every requested position requires an LSP response",
        "return a zero-width fallback range when no Prism node contains the position",
    )
}
