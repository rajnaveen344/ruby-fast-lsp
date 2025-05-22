use log::debug;
use lsp_types::{Location, Position, Url};

use crate::analyzer_prism::RubyPrismAnalyzer;
use crate::indexer::RubyIndexer;

/// Find all references to a symbol at the given position.
pub async fn find_references_at_position(
    indexer: &RubyIndexer,
    _uri: &Url,
    position: Position,
    content: &str,
    include_declaration: bool,
) -> Option<Vec<Location>> {
    // Use the analyzer to find the identifier at the position and get its fully qualified name
    let analyzer = RubyPrismAnalyzer::new(content.to_string());
    let (identifier_opt, _) = analyzer.get_identifier(position);

    let identifier = match identifier_opt {
        Some(fqn) => format!("{:?}", fqn), // Use Debug format
        None => {
            debug!("No identifier found at position {:?}", position);
            return None;
        }
    };

    debug!("Looking for references to: {}", identifier);

    // Use the indexer to find the matching fully qualified name
    let index_arc = indexer.index();
    let index = index_arc.lock().unwrap();

    // Check if this is a method call (starts with # or .)
    let is_method =
        identifier.starts_with('#') || identifier.contains(".") || identifier.contains("#");
    let method_name = if is_method {
        // Extract method name from something like "Class#method" or "Class.method" or just "#method"
        if let Some(pos) = identifier.rfind('#') {
            Some(&identifier[pos + 1..])
        } else if let Some(pos) = identifier.rfind('.') {
            Some(&identifier[pos + 1..])
        } else {
            None
        }
    } else {
        None
    };

    // Collect references from all matching FQNs
    let mut all_locations = Vec::new();

    // First pass: collect all exact FQN matches for references
    for (fqn, locations) in &index.references {
        let fqn_str = fqn.to_string();

        // Check for exact matches or method name matches
        let exact_match = fqn_str == identifier;
        let method_name_match = if let Some(method) = method_name {
            fqn_str.ends_with(&format!("#{}", method)) || fqn_str.ends_with(&format!(".{}", method))
        } else {
            false
        };

        if exact_match || method_name_match {
            debug!("Found reference match: {} for {}", fqn_str, identifier);
            all_locations.extend(locations.iter().cloned());
        }
    }

    // Optionally include the declarations
    if include_declaration {
        // Find all matching definitions
        for (fqn, entries) in &index.definitions {
            let fqn_str = fqn.to_string();

            // Check for exact matches or method name matches
            let exact_match = fqn_str == identifier;
            let method_name_match = if let Some(method) = method_name {
                fqn_str.ends_with(&format!("#{}", method))
                    || fqn_str.ends_with(&format!(".{}", method))
            } else {
                false
            };

            if (exact_match || method_name_match) && !entries.is_empty() {
                debug!("Including declarations for: {}", fqn_str);

                for entry in entries {
                    let declaration_location = Location {
                        uri: entry.location.uri.clone(),
                        range: entry.location.range,
                    };

                    // Avoid duplicates
                    if !all_locations.iter().any(|loc| {
                        loc.uri == declaration_location.uri
                            && loc.range == declaration_location.range
                    }) {
                        all_locations.push(declaration_location);
                    }
                }
            }
        }
    }

    if all_locations.is_empty() {
        debug!("No references found for {}", identifier);
        return None;
    }

    debug!(
        "Found {} total references for {}",
        all_locations.len(),
        identifier
    );
    Some(all_locations)
}
