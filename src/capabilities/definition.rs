use log::info;
use lsp_types::{GotoDefinitionResponse, Location, Position, Url};

use crate::analyzer::RubyAnalyzer;
use crate::indexer::{entry::EntryType, RubyIndexer};

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

    for (fqn, entries) in &index.definitions {
        info!("  FQN: {}, Entries: {}", fqn, entries.len());
        for (idx, entry) in entries.iter().enumerate() {
            info!(
                "    Entry {}: {:?} at {:?}",
                idx, entry.entry_type, entry.location
            );
        }
    }

    // Search for entries with the same string representation
    let mut found_entries = Vec::new();
    for (fqn, entries) in &index.definitions {
        info!("Comparing {} with {}", fqn.to_string(), identifier);
        if fqn.to_string() == identifier && !entries.is_empty() {
            info!("Found match: {} == {}", fqn.to_string(), identifier);
            // For classes and modules, collect all definitions
            // For other entry types, just use the first one
            let first_entry_type = &entries[0].entry_type;

            if *first_entry_type == EntryType::Class || *first_entry_type == EntryType::Module {
                // Class or module may be defined in multiple files or reopened
                info!("Adding all {} entries for class/module", entries.len());
                found_entries.extend(entries.iter().cloned());
            } else {
                // For methods and other types, just take the first definition
                info!("Adding first entry for method/other");
                found_entries.push(entries[0].clone());
                break;
            }
        }
    }

    if found_entries.is_empty() {
        info!("No definition found for {}", identifier);
        return None;
    }

    // If we found only one entry, return it as a scalar response
    if found_entries.len() == 1 {
        let entry = &found_entries[0];
        info!("Found single definition at {:?}", entry.location);
        return Some(GotoDefinitionResponse::Scalar(Location {
            uri: entry.location.uri.clone(),
            range: entry.location.range,
        }));
    }

    // Multiple entries found, return them as an array
    let locations: Vec<Location> = found_entries
        .iter()
        .map(|entry| Location {
            uri: entry.location.uri.clone(),
            range: entry.location.range,
        })
        .collect();

    info!("Found {} definitions for {}", locations.len(), identifier);
    Some(GotoDefinitionResponse::Array(locations))
}
