use crate::indexer::{
    entry::{EntryBuilder, EntryType},
    RubyIndexer,
};
use log::info;
use lsp_types::{Location, Url};
use tree_sitter::Node;

use super::{utils::node_to_range, TraversalContext};

pub fn process_constant(
    indexer: &mut RubyIndexer,
    node: Node,
    uri: &Url,
    source_code: &str,
    context: &mut TraversalContext,
) -> Result<(), String> {
    // For constant assignments, the name is in the "left" field
    let name_node = match node.child_by_field_name("left") {
        Some(node) => node,
        None => {
            // If we encounter a constant node without a left field, it's likely part of another
            // construct, like a class name or module. Instead of failing, just continue traversal.
            if indexer.debug_mode {
                info!(
                    "Skipping constant without a name field at {}:{}",
                    node.start_position().row + 1,
                    node.start_position().column + 1
                );
            }

            // Recursively traverse child nodes
            let child_count = node.child_count();
            for i in 0..child_count {
                if let Some(child) = node.child(i) {
                    indexer.traverse_node(child, uri, source_code, context)?;
                }
            }

            return Ok(());
        }
    };

    // Make sure it's a constant (starts with capital letter)
    let name = indexer.get_node_text(name_node, source_code);
    if name.trim().is_empty() || !name.starts_with(|c: char| c.is_uppercase()) {
        // Not a valid constant, just continue traversal
        // Recursively traverse child nodes
        let child_count = node.child_count();
        for i in 0..child_count {
            if let Some(child) = node.child(i) {
                indexer.traverse_node(child, uri, source_code, context)?;
            }
        }
        return Ok(());
    }

    // Create a fully qualified name
    let current_namespace = context.current_namespace();
    let constant_name = name.clone();

    let fqn = if current_namespace.is_empty() {
        constant_name.clone()
    } else {
        format!("{}::{}", current_namespace, constant_name)
    };

    // Create a range for the definition
    let range = node_to_range(node);

    // Create and add the entry
    let entry = EntryBuilder::new(&name)
        .fully_qualified_name(&fqn)
        .location(Location {
            uri: uri.clone(),
            range,
        })
        .entry_type(EntryType::Constant)
        .build()
        .map_err(|e| e.to_string())?;

    indexer.index.add_entry(entry);

    // Process the right side of the assignment
    if let Some(right) = node.child_by_field_name("right") {
        indexer.traverse_node(right, uri, source_code, context)?;
    }

    Ok(())
}

pub fn process_constant_reference(
    indexer: &mut RubyIndexer,
    node: Node,
    uri: &Url,
    source_code: &str,
    context: &mut TraversalContext,
) -> Result<(), String> {
    // Get the constant name
    let name = indexer.get_node_text(node, source_code);

    // Skip if name is empty or not a valid constant (should start with uppercase)
    if name.trim().is_empty() || !name.starts_with(|c: char| c.is_uppercase()) {
        return Ok(());
    }

    // Create a range for the reference
    let range = node_to_range(node);

    // Create a location for this reference
    let location = Location {
        uri: uri.clone(),
        range,
    };

    // Add reference with just the constant name
    indexer.index.add_reference(&name, location.clone());

    // Also add reference with namespace context if available
    let current_namespace = context.current_namespace();
    if !current_namespace.is_empty() {
        let fqn = format!("{}::{}", current_namespace, name);
        indexer.index.add_reference(&fqn, location);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use tempfile::NamedTempFile;

    use crate::indexer::entry::EntryType;

    use super::*;

    // Helper function to create a temporary Ruby file with given content
    fn create_temp_ruby_file(content: &str) -> (NamedTempFile, Url) {
        let mut file = NamedTempFile::new().expect("Failed to create temp file");
        file.write_all(content.as_bytes())
            .expect("Failed to write to temp file");
        let path = file.path().to_path_buf();
        let uri = Url::from_file_path(path).unwrap();
        (file, uri)
    }

    #[test]
    fn test_index_constants() {
        let mut indexer = RubyIndexer::new().expect("Failed to create indexer");
        let ruby_code = r#"
        module Config
          VERSION = "1.0.0"

          class Settings
            DEFAULT_TIMEOUT = 30
          end
        end
        "#;

        let (file, uri) = create_temp_ruby_file(ruby_code);

        let result = indexer.index_file_with_uri(uri, ruby_code);
        assert!(result.is_ok(), "Should be able to index the file");

        let index = indexer.index();

        // Verify constants were indexed
        let version_entries = index.entries.get("Config::VERSION");
        assert!(
            version_entries.is_some(),
            "VERSION constant should be indexed"
        );
        assert_eq!(
            version_entries.unwrap()[0].entry_type,
            EntryType::Constant,
            "VERSION should be a constant"
        );

        let timeout_entries = index.entries.get("Config::Settings::DEFAULT_TIMEOUT");
        assert!(
            timeout_entries.is_some(),
            "DEFAULT_TIMEOUT constant should be indexed"
        );
        assert_eq!(
            timeout_entries.unwrap()[0].entry_type,
            EntryType::Constant,
            "DEFAULT_TIMEOUT should be a constant"
        );

        // Keep file in scope until end of test
        drop(file);
    }

    #[test]
    fn test_constant_references() {
        let mut indexer = RubyIndexer::new().expect("Failed to create indexer");
        let ruby_code = r##"
        module Colors
          RED = "FF0000"
          GREEN = "00FF00"
          BLUE = "0000FF"
        end

        # Reference constants
        puts Colors::RED
        background = Colors::BLUE
        puts Colors::GREEN
        "##;

        let (file, uri) = create_temp_ruby_file(ruby_code);

        let result = indexer.index_file_with_uri(uri.clone(), ruby_code);
        assert!(result.is_ok(), "Should be able to index the file");

        let index = indexer.index();

        // Verify constants were indexed
        let red_entries = index.entries.get("Colors::RED");
        assert!(red_entries.is_some(), "RED constant should be indexed");

        // Verify references were indexed
        let red_refs = index.find_references("RED");
        assert!(!red_refs.is_empty(), "Should have references to RED");

        let colors_red_refs = index.find_references("Colors::RED");
        assert!(
            !colors_red_refs.is_empty(),
            "Should have references to Colors::RED"
        );

        // Verify at least one reference to each constant with the correct URI
        let has_red_uri_ref = red_refs.iter().any(|loc| loc.uri == uri);
        assert!(
            has_red_uri_ref,
            "Should have a RED reference with the correct URI"
        );

        let blue_refs = index.find_references("Colors::BLUE");
        assert!(
            !blue_refs.is_empty(),
            "Should have references to Colors::BLUE"
        );

        // Keep file in scope until end of test
        drop(file);
    }
}
