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

    if let None = identifier {
        info!("No identifier found at position {:?}", position);
        return None;
    }

    let fqn = identifier.unwrap();

    info!("Looking for definition of: {}", fqn);

    let index = indexer.index();
    let index_guard = index.lock().unwrap();

    match fqn.clone() {
        FullyQualifiedName::Constant(ns, constant) => {
            // Start with the current namespace and ancestors
            let mut search_namespaces = ancestors.clone();

            // Search through ancestor namespaces
            while !search_namespaces.is_empty() {
                let mut combined_ns = search_namespaces.clone();
                combined_ns.extend(ns.iter().cloned());

                let search_fqn = FullyQualifiedName::Namespace(combined_ns);

                if let Some(entries) = index_guard.definitions.get(&search_fqn) {
                    if !entries.is_empty() {
                        info!(
                            "Found constant definition in ancestor namespace for: {}",
                            search_fqn
                        );
                        return Some(entries[0].location.clone());
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
                        "Found constant definition at top level for: {}",
                        top_level_fqn
                    );
                    return Some(entries[0].location.clone());
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
        _ => {
            info!("Unsupported identifier type: {:?}", fqn);
            return None;
        }
    }

    info!("No definition found for {}", fqn);
    None
}
