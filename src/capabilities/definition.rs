use log::info;
use lsp_types::{GotoDefinitionResponse, Location, Position, Url};

use crate::analyzer::RubyAnalyzer;
use crate::indexer::traverser::RubyIndexer;

/// Find definition of a symbol at the given position in a file.
pub async fn find_definition_at_position(
    indexer: &RubyIndexer,
    uri: &Url,
    position: Position,
    content: &str,
) -> Option<GotoDefinitionResponse> {
    // Use the analyzer to find the identifier at the position and get its fully qualified name
    let mut analyzer = RubyAnalyzer::new();
    let fully_qualified_name = match analyzer.find_identifier_at_position(content, position) {
        Some(name) => name,
        None => {
            info!("No identifier found at position {:?}", position);
            return None;
        }
    };

    info!("Looking for definition of: {}", fully_qualified_name);

    // Use the indexer to find the definition
    let entry = match indexer.index().find_definition(&fully_qualified_name) {
        Some(entry) => entry,
        None => {
            info!("No definition found for {}", fully_qualified_name);
            return None;
        }
    };

    info!("Found definition at {:?}", entry.location);

    // Return the location of the definition
    Some(GotoDefinitionResponse::Scalar(Location {
        uri: entry.location.uri.clone(),
        range: entry.location.range,
    }))
}
