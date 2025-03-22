use log::info;
use lsp_types::{Location, Position, Url};

use crate::analyzer::RubyAnalyzer;
use crate::indexer::RubyIndexer;

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

    // Use the indexer to find the matching fully qualified name
    let index_arc = indexer.index();
    let index = index_arc.lock().unwrap();

    // Search for the corresponding fully qualified name in the index
    let mut references_key = None;
    for (fqn, _) in &index.references {
        if fqn.to_string() == fully_qualified_name {
            references_key = Some(fqn.clone());
            break;
        }
    }

    // Use the key we found to get the references
    let mut locations = match references_key {
        Some(key) => index.find_references(&key),
        None => Vec::new(),
    };

    // Optionally include the declaration
    if include_declaration {
        // Find the matching definition
        let mut definition_key = None;
        for (fqn, entries) in &index.definitions {
            if fqn.to_string() == fully_qualified_name && !entries.is_empty() {
                definition_key = Some(fqn.clone());
                break;
            }
        }

        if let Some(key) = definition_key {
            if let Some(entries) = index.definitions.get(&key) {
                if let Some(entry) = entries.first() {
                    let declaration_location = Location {
                        uri: entry.location.uri.clone(),
                        range: entry.location.range,
                    };

                    // Avoid duplicates if the declaration is already in the references
                    if !locations.iter().any(|loc| {
                        loc.uri == declaration_location.uri
                            && loc.range == declaration_location.range
                    }) {
                        locations.push(declaration_location);
                    }
                }
            }
        }
    }

    if locations.is_empty() {
        return None;
    }

    Some(locations)
}
