use log::info;
use lsp_types::{Location, Position};

use crate::analyzer_prism::RubyPrismAnalyzer;
use crate::indexer::types::fully_qualified_name::FullyQualifiedName;
use crate::indexer::RubyIndexer;

/// Find the definition of a symbol at the given position
pub async fn find_definition_at_position(
    indexer: &RubyIndexer,
    position: Position,
    content: &str,
) -> Option<Location> {
    // Use the analyzer to find the identifier at the position
    let analyzer = RubyPrismAnalyzer::new(content.to_string());
    let (identifier, ancestors) = analyzer.get_identifier(position);

    let fqn = match identifier {
        Some(fqn) => {
            info!("Looking for definition of: {}", fqn);
            fqn
        }
        None => {
            info!("No identifier found at position {:?}", position);
            return None;
        }
    };

    // Get the index
    let index = indexer.index();
    let index_guard = index.lock().unwrap();

    // First, try to find the definition directly
    // if let Some(entries) = index_guard.definitions.get(&fqn) {
    //     if !entries.is_empty() {
    //         info!("Found definition directly for: {}", fqn);
    //         return Some(entries[0].location.clone());
    //     }
    // }

    // If not found directly, try to find it in the ancestor namespaces
    // This is primarily for constants where we need to check parent namespaces
    match fqn {
        FullyQualifiedName::Constant(_ns, constant) => {
            // Start with the current namespace and ancestors
            let mut search_namespaces = ancestors.clone();

            // Keep searching up the namespace hierarchy
            while !search_namespaces.is_empty() {
                // Create a new FQN with the current search namespace
                let search_fqn =
                    FullyQualifiedName::Constant(search_namespaces.clone(), constant.clone());

                // Look for the definition
                if let Some(entries) = index_guard.definitions.get(&search_fqn) {
                    if !entries.is_empty() {
                        info!("Found definition in ancestor namespace for: {}", search_fqn);
                        return Some(entries[0].location.clone());
                    }
                }

                // Pop the last namespace and try again
                search_namespaces.pop();
            }

            // Try at the top level (empty namespace)
            let top_level_fqn = FullyQualifiedName::Constant(Vec::new(), constant.clone());
            if let Some(entries) = index_guard.definitions.get(&top_level_fqn) {
                if !entries.is_empty() {
                    info!("Found definition at top level for: {}", top_level_fqn);
                    return Some(entries[0].location.clone());
                }
            }
        }
        FullyQualifiedName::Namespace(ref ns) => {
            // Similar approach for namespaces
            let mut search_namespaces = ancestors.clone();

            while !search_namespaces.is_empty() {
                // For each ancestor, try to find the namespace
                let mut combined_ns = search_namespaces.clone();
                combined_ns.extend(ns.clone());

                let search_fqn = FullyQualifiedName::Namespace(combined_ns);

                if let Some(entries) = index_guard.definitions.get(&search_fqn) {
                    if !entries.is_empty() {
                        info!(
                            "Found namespace definition in ancestor namespace for: {}",
                            search_fqn
                        );
                        return Some(entries[0].location.clone());
                    }
                }

                // Pop the last namespace and try again
                search_namespaces.pop();
            }

            // Try at the top level
            let top_level_fqn = FullyQualifiedName::Namespace(ns.clone());
            if let Some(entries) = index_guard.definitions.get(&top_level_fqn) {
                if !entries.is_empty() {
                    info!(
                        "Found namespace definition at top level for: {}",
                        top_level_fqn
                    );
                    return Some(entries[0].location.clone());
                }
            }
        }
        // For now, we're only handling constants and namespaces as specified
        _ => {}
    }

    info!("No definition found");
    None
}
