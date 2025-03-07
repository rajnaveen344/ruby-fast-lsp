use log::info;
use lsp_types::{Location, Position, Url};

use crate::analyzer::RubyAnalyzer;
use crate::indexer::traverser::RubyIndexer;

/// Find all references to a symbol at the given position.
pub async fn find_references_at_position(
    indexer: &RubyIndexer,
    uri: &Url,
    position: Position,
    content: &str,
    include_declaration: bool,
) -> Option<Vec<Location>> {
    // Use the analyzer to find the identifier at the position and get its fully qualified name
    let mut analyzer = RubyAnalyzer::new();
    let fully_qualified_name = analyzer.find_identifier_at_position(content, position)?;

    info!("Looking for references to: {}", fully_qualified_name);

    // Use the indexer to find all references
    let mut locations = indexer.index().find_references(&fully_qualified_name);

    // Optionally include the declaration
    if include_declaration {
        if let Some(entry) = indexer.index().find_definition(&fully_qualified_name) {
            let declaration_location = Location {
                uri: entry.location.uri.clone(),
                range: entry.location.range,
            };

            // Avoid duplicates if the declaration is already in the references
            if !locations.iter().any(|loc| {
                loc.uri == declaration_location.uri && loc.range == declaration_location.range
            }) {
                locations.push(declaration_location);
            }
        }
    }

    if locations.is_empty() {
        return None;
    }

    Some(locations)
}
