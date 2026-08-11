use ruby_analysis::indexer::selection_range_chains;
use tower_lsp::lsp_types::{SelectionRange, SelectionRangeParams};

use crate::server::RubyLanguageServer;
use crate::utils::lsp::{lsp_text_range, source_position};

pub async fn handle_selection_ranges(
    server: &RubyLanguageServer,
    params: SelectionRangeParams,
) -> Option<Vec<SelectionRange>> {
    let document = {
        let docs = server.docs.lock();
        docs.get(&params.text_document.uri)?.clone()
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

    Some(
        chains
            .into_iter()
            .map(|chain| selection_range_from_chain(&document, chain))
            .collect(),
    )
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
    nested.expect(
        "INVARIANT VIOLATED: indexer returned an empty selection range chain. \
         This is a bug because every requested position requires an LSP response. \
         Fix: return a zero-width fallback range when no Prism node contains the position.",
    )
}
