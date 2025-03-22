use log::info;
use lsp_types::{Location, Position, Url};

use crate::analyzer::RubyAnalyzer;
use crate::indexer::RubyIndexer;

/// Find the definition of a symbol at the given position
pub async fn find_definition_at_position(
    indexer: &RubyIndexer,
    uri: &Url,
    position: Position,
    content: &str,
) -> Option<Location> {
    // Use the analyzer to find the identifier at the position
    let mut analyzer = RubyAnalyzer::new();
    let identifier = match analyzer.find_identifier_at_position(content, position) {
        Some(name) => name,
        None => {
            info!("No identifier found at position {:?}", position);
            return None;
        }
    };

    info!("Looking for definition of: {}", identifier);

    // Use the indexer to find entries matching the identifier
    let index_arc = indexer.index();
    let index = index_arc.lock().unwrap();

    // Debug print the contents of the definitions index
    for (fqn, entries) in &index.definitions {
        info!("  FQN: {}, Entries: {}", fqn, entries.len());
        for (i, entry) in entries.iter().enumerate() {
            info!(
                "    Entry {}: {} at {:?}",
                i, entry.entry_type, entry.location
            );
        }
    }

    // Try to find an exact match first
    for (fqn, entries) in &index.definitions {
        let fqn_str = fqn.to_string();
        info!("Comparing {} with {}", fqn_str, identifier);

        // Direct match for class/module/constant
        if fqn_str == identifier {
            info!("Found match: {} for {}", fqn_str, identifier);
            if !entries.is_empty() {
                // Return the first matching definition
                let location = Location {
                    uri: entries[0].location.uri.clone(),
                    range: entries[0].location.range,
                };
                info!("Found definition at {:?}", location);
                return Some(location);
            }
        }

        // Check for method identifier (with or without class prefix)
        let is_method_call = identifier.contains('#') || identifier.contains('.');
        if is_method_call {
            // Extract method name for comparison
            let method_name = if let Some(pos) = identifier.rfind('#') {
                &identifier[pos + 1..]
            } else if let Some(pos) = identifier.rfind('.') {
                &identifier[pos + 1..]
            } else {
                continue;
            };

            // Check if the FQN ends with the method name
            if fqn_str.ends_with(&format!("#{}", method_name)) {
                info!("Found match for method: {} -> {}", identifier, fqn_str);
                if !entries.is_empty() {
                    let location = Location {
                        uri: entries[0].location.uri.clone(),
                        range: entries[0].location.range,
                    };
                    info!("Found method definition at {:?}", location);
                    return Some(location);
                }
            }
        } else {
            // For non-method identifiers, also try with '#' if we didn't find direct matches
            if fqn_str == format!("#{}", identifier) {
                info!("Found match: {} for {}", fqn_str, identifier);
                if !entries.is_empty() {
                    let location = Location {
                        uri: entries[0].location.uri.clone(),
                        range: entries[0].location.range,
                    };
                    info!("Found definition at {:?}", location);
                    return Some(location);
                }
            }
        }
    }

    // If we haven't found anything by exact match, try partial matching for methods
    if identifier.contains('#') || !identifier.contains(':') {
        let method_name = if let Some(pos) = identifier.rfind('#') {
            &identifier[pos + 1..]
        } else {
            identifier.as_str()
        };

        // Look for any definition with this method name
        for (fqn, entries) in &index.definitions {
            let fqn_str = fqn.to_string();
            if fqn_str.ends_with(&format!("#{}", method_name)) {
                if !entries.is_empty() {
                    let location = Location {
                        uri: entries[0].location.uri.clone(),
                        range: entries[0].location.range,
                    };
                    info!("Found definition at {:?} via method name match", location);
                    return Some(location);
                }
            }
        }

        // Special case: also check for unqualified method names in the index
        let unqualified_key = format!("#{}", method_name);
        for (fqn, entries) in &index.definitions {
            if fqn.to_string() == unqualified_key {
                if !entries.is_empty() {
                    let location = Location {
                        uri: entries[0].location.uri.clone(),
                        range: entries[0].location.range,
                    };
                    info!(
                        "Found definition at {:?} via unqualified method name",
                        location
                    );
                    return Some(location);
                }
            }
        }
    }

    info!("No definition found for {}", identifier);
    None
}
