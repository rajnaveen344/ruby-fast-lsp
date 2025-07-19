use lsp_types::Location;
use log::debug;

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

        let search_fqn = Identifier::RubyConstant(combined_ns);

        debug!("Searching for constant: {:?}", search_fqn);

        if let Some(entries) = index.definitions.get(&search_fqn.clone().into()) {
            if !entries.is_empty() {
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

    if !found_locations.is_empty() {
        Some(found_locations)
    } else {
        None
    }
}
