use tower_lsp::lsp_types::{CompletionItem, Position};

use crate::{analyzer_prism::RubyPrismAnalyzer, indexer::index::RubyIndex};

use super::constant_completion::ConstantCompletionEngine;

pub fn find_constant_completions(
    index: &RubyIndex,
    analyzer: &RubyPrismAnalyzer,
    position: Position,
    partial_name: String,
) -> Vec<CompletionItem> {
    let engine = ConstantCompletionEngine::new();

    engine.complete_constants(index, analyzer, position, partial_name)
}
