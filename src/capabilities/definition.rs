use log::info;
use lsp_types::{Location, Position, Url};

use crate::analyzer_prism::RubyPrismAnalyzer;
use crate::indexer::RubyIndexer;

/// Find the definition of a symbol at the given position
pub async fn find_definition_at_position(
    _indexer: &RubyIndexer,
    _uri: &Url,
    position: Position,
    content: &str,
) -> Option<Location> {
    // Use the analyzer to find the identifier at the position
    let analyzer = RubyPrismAnalyzer::new(content.to_string());
    let (identifier, _namespace_stack) = analyzer.get_identifier(position);

    match identifier {
        Some(fqn) => {
            info!("Looking for definition of: {}", fqn);
        }
        None => {
            info!("No identifier found at position {:?}", position);
            return None;
        }
    };

    None
}
