use log::{debug, info};
use lsp_types::{Location, Position};

use crate::analyzer_prism::RubyPrismAnalyzer;
use crate::indexer::types::fully_qualified_name::FullyQualifiedName;
use crate::indexer::RubyIndexer;

/// Find the definition(s) of a symbol at the given position
///
/// Returns a vector of locations if definitions are found, or None if no definitions are found.
/// Multiple definitions can be returned when a symbol is defined in multiple places.
pub async fn find_definition_at_position(
    indexer: &RubyIndexer,
    position: Position,
    content: &str,
) -> Option<Vec<Location>> {
    // Use the analyzer to find the identifier at the position
    let analyzer = RubyPrismAnalyzer::new(content.to_string());
    let (identifier, ancestors) = analyzer.get_identifier(position);

    // Extract the fully qualified name if available
    if let None = identifier {
        info!("No identifier found at position {:?}", position);
        return None;
    }

    info!("Looking for definition of: {}", identifier.clone().unwrap());

    // Get the index and search for the definition
    let index = indexer.index();
    let index_guard = index.lock().unwrap();
    let fqn = identifier.unwrap();
    let mut found_locations = Vec::new();

    // If not found directly, try based on the FQN type
    match fqn.clone() {
        FullyQualifiedName::Constant(ns, constant) => {
            // Start with the current namespace and ancestors
            let mut search_namespaces = ancestors.clone();

            // Search through ancestor namespaces
            while !search_namespaces.is_empty() {
                let mut combined_ns = search_namespaces.clone();
                combined_ns.extend(ns.iter().cloned());

                let search_fqn = FullyQualifiedName::Constant(combined_ns, constant.clone());

                if let Some(entries) = index_guard.definitions.get(&search_fqn) {
                    if !entries.is_empty() {
                        info!(
                            "Found {} constant definition(s) in ancestor namespace for: {}",
                            entries.len(),
                            search_fqn
                        );
                        // Add all locations to our result
                        for entry in entries {
                            found_locations.push(entry.location.clone());
                        }
                        return Some(found_locations);
                    }
                }

                // Pop the last namespace and try again
                search_namespaces.pop();
            }

            // Try at the top level (empty namespace)
            let top_level_fqn = FullyQualifiedName::Constant(ns, constant.clone());
            if let Some(entries) = index_guard.definitions.get(&top_level_fqn) {
                if !entries.is_empty() {
                    info!(
                        "Found {} constant definition(s) at top level for: {}",
                        entries.len(),
                        top_level_fqn
                    );
                    // Add all locations to our result
                    for entry in entries {
                        found_locations.push(entry.location.clone());
                    }
                    return Some(found_locations);
                }
            }
        }
        FullyQualifiedName::Namespace(ref ns) => {
            // Start with the current namespace and ancestors
            let mut search_namespaces = ancestors.clone();

            // Search through ancestor namespaces
            while !search_namespaces.is_empty() {
                // For each ancestor, try to find the namespace
                let mut combined_ns = search_namespaces.clone();
                combined_ns.extend(ns.iter().cloned());

                let search_fqn = FullyQualifiedName::Namespace(combined_ns);

                if let Some(entries) = index_guard.definitions.get(&search_fqn) {
                    if !entries.is_empty() {
                        debug!(
                            "Found {} namespace definition(s) in ancestor namespace for: {}",
                            entries.len(),
                            search_fqn
                        );
                        // Add all locations to our result
                        for entry in entries {
                            found_locations.push(entry.location.clone());
                        }
                        return Some(found_locations);
                    }
                }

                // Pop the last namespace and try again
                search_namespaces.pop();
            }

            // Try at the top level
            let top_level_fqn = FullyQualifiedName::Namespace(ns.clone());
            if let Some(entries) = index_guard.definitions.get(&top_level_fqn) {
                if !entries.is_empty() {
                    debug!(
                        "Found {} namespace definition(s) at top level for: {}",
                        entries.len(),
                        top_level_fqn
                    );
                    // Add all locations to our result
                    for entry in entries {
                        found_locations.push(entry.location.clone());
                    }
                    return Some(found_locations);
                }
            }
        }
        // For now, we're only handling constants and namespaces as specified
        _ => {
            debug!("Unsupported identifier type: {:?}", fqn);
            return None;
        }
    }

    debug!("No definition found for {}", fqn);

    // If we found any locations during the search, return them
    if !found_locations.is_empty() {
        Some(found_locations)
    } else {
        None
    }
}
