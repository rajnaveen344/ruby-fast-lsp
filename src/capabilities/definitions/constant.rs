use log::{debug, info};
use tower_lsp::lsp_types::Location;

use crate::analyzer_prism::Identifier;
use crate::indexer::index::RubyIndex;
use crate::types::ruby_namespace::RubyConstant;

/// Find definitions for a Ruby constant
pub fn find_constant_definitions(
    ns: &[RubyConstant],
    index: &RubyIndex,
    ancestors: &[RubyConstant],
) -> Option<Vec<Location>> {
    let mut found_locations = Vec::new();

    // Start with the current namespace and ancestors
    let mut search_namespaces = ancestors.to_vec();

    // Search through ancestor namespaces
    while !search_namespaces.is_empty() {
        // For each ancestor, try to find the namespace
        let mut combined_ns = search_namespaces.clone();
        combined_ns.extend(ns.iter().cloned());

        let search_fqn = Identifier::RubyConstant {
            namespace: vec![],
            iden: combined_ns,
        };

        debug!("Searching for constant: {:?}", search_fqn);

        if let Some(entries) = index.get(&search_fqn.clone().into()) {
            if !entries.is_empty() {
                // Add all locations to our result
                for entry in entries {
                    if let Some(loc) = index.to_lsp_location(&entry.location) {
                        found_locations.push(loc);
                    }
                }
                return Some(found_locations);
            }
        }

        // Pop the last namespace and try again
        search_namespaces.pop();
    }

    // If not found in any ancestor namespace, search at the root level
    // This handles built-in constants like String, Array, Hash, etc.
    let root_search_fqn = Identifier::RubyConstant {
        namespace: vec![],
        iden: ns.to_vec(),
    };

    info!(
        "Searching for constant at root level: {:?}",
        root_search_fqn
    );

    if let Some(entries) = index.get(&root_search_fqn.into()) {
        if !entries.is_empty() {
            for entry in entries {
                if let Some(loc) = index.to_lsp_location(&entry.location) {
                    found_locations.push(loc);
                }
            }
            return Some(found_locations);
        }
    }

    if !found_locations.is_empty() {
        Some(found_locations)
    } else {
        None
    }
}
