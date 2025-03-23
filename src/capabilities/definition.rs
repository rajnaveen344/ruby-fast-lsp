use log::info;
use lsp_types::{Location, Position, Url};

use crate::analyzer::RubyAnalyzer;
use crate::indexer::RubyIndexer;

/// Find the definition of a symbol at the given position
pub async fn find_definition_at_position(
    indexer: &RubyIndexer,
    _uri: &Url,
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

    // Check if this is a method call with a class prefix (either Class.method or instance.method)
    if identifier.contains('#') || identifier.contains('.') {
        // Parse the class and method parts
        let (class_part, method_part) = if let Some(pos) = identifier.rfind('#') {
            (&identifier[0..pos], &identifier[pos + 1..])
        } else if let Some(pos) = identifier.rfind('.') {
            (&identifier[0..pos], &identifier[pos + 1..])
        } else {
            ("", identifier.as_str()) // Should not happen given the if condition
        };

        info!("Class part: {}, Method part: {}", class_part, method_part);

        // If we have a class name, try to find the method in that class through the index
        if !class_part.is_empty() {
            // Use string-based lookups to avoid using the private types directly
            for (fqn, entries) in &index.definitions {
                let fqn_str = fqn.to_string();

                // Check if this is the method we're looking for in the right class
                if fqn_str == format!("{}#{}", class_part, method_part) {
                    if !entries.is_empty() {
                        let location = Location {
                            uri: entries[0].location.uri.clone(),
                            range: entries[0].location.range,
                        };
                        info!(
                            "Found method definition at {:?} in class namespace",
                            location
                        );
                        return Some(location);
                    }
                }
            }

            // If we didn't find the exact FQN match, try to match method entries by name
            // This is needed because methods might be indexed with just #method_name
            // for (fqn, entries) in &index.definitions {
            //     let fqn_str = fqn.to_string();

            //     // Check for a method with this name (prefixed with # for instance methods)
            //     if fqn_str == format!("#{}", method_part) {
            //         for entry in entries {
            //             // Check if the entry is for a method and add it (we'll handle class-specific matching later)
            //             if entry.kind == EntryKind::Method {
            //                 let location = Location {
            //                     uri: entry.location.uri.clone(),
            //                     range: entry.location.range,
            //                 };
            //                 info!(
            //                     "Found method definition at {:?} via general matching",
            //                     location
            //                 );
            //                 return Some(location);
            //             }
            //         }
            //     }
            // }
        } else {
            // Handle unqualified method names (e.g., #method)
            for (fqn, entries) in &index.definitions {
                let fqn_str = fqn.to_string();

                // Check if this is the method we're looking for
                if fqn_str.ends_with(&format!("#{}", method_part)) {
                    if !entries.is_empty() {
                        let location = Location {
                            uri: entries[0].location.uri.clone(),
                            range: entries[0].location.range,
                        };
                        info!("Found method definition at {:?} from all methods", location);
                        return Some(location);
                    }
                }
            }
        }
    } else {
        // For non-method identifiers (classes, constants, etc.)
        // Try to find an exact match by name
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
        }

        // For simple identifiers that might be methods without a class prefix
        // Try to find any method with this name
        for (fqn, entries) in &index.definitions {
            let fqn_str = fqn.to_string();

            // Check if this is a method definition with the name we're looking for
            if fqn_str.ends_with(&format!("#{}", identifier)) {
                if !entries.is_empty() {
                    let location = Location {
                        uri: entries[0].location.uri.clone(),
                        range: entries[0].location.range,
                    };
                    info!(
                        "Found method definition at {:?} for simple identifier",
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
