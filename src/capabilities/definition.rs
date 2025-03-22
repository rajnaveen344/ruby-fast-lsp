use log::info;
use lsp_types::{GotoDefinitionResponse, Location, Position, Url};

use crate::analyzer::RubyAnalyzer;
use crate::indexer::RubyIndexer;

/// Find definition of a symbol at the given position in a file.
pub async fn find_definition_at_position(
    indexer: &RubyIndexer,
    uri: &Url,
    position: Position,
    content: &str,
) -> Option<GotoDefinitionResponse> {
    // Use the analyzer to find the identifier at the position and get its fully qualified name
    let mut analyzer = RubyAnalyzer::new();
    let identifier = match analyzer.find_identifier_at_position(content, position) {
        Some(name) => name,
        None => {
            info!("No identifier found at position {:?}", position);
            return None;
        }
    };

    info!("Looking for definition of: {}", identifier);

    // Use the indexer to find the definition
    let i = indexer.index();
    let index = i.lock().unwrap();

    // Search for entries with the same string representation
    let mut found_entry = None;
    for (fqn, entries) in &index.definitions {
        if fqn.to_string() == identifier && !entries.is_empty() {
            found_entry = Some(entries[0].clone());
            break;
        }
    }

    let entry = match found_entry {
        Some(entry) => entry,
        None => {
            info!("No definition found for {}", identifier);
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
