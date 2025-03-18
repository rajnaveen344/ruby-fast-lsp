use crate::indexer::{
    entry::{EntryBuilder, EntryType},
    RubyIndexer,
};
use log::info;
use lsp_types::{Location, Url};
use tree_sitter::Node;

use super::{
    utils::{add_reference, create_location, get_indexer_node_text, node_to_range},
    TraversalContext,
};

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
    let name = get_indexer_node_text(indexer, name_node, source_code);
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

    // Create a fully qualified name that includes the current namespace
    let mut fqn = name.clone();
    let current_namespace = context.current_namespace();
    if !current_namespace.is_empty() {
        fqn = format!("{}::{}", current_namespace, name);
    }

    // Create a range for the definition
    let range = node_to_range(name_node);

    // Create and add the entry
    let entry = EntryBuilder::new(&name)
        .fully_qualified_name(&fqn)
        .location(Location {
            uri: uri.clone(),
            range,
        })
        .entry_type(EntryType::Constant)
        .visibility(context.visibility)
        .build()
        .map_err(|e| e.to_string())?;

    indexer.index.add_entry(entry);

    // Process the right-hand side of the assignment
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
    // Extract the constant name
    let name = get_indexer_node_text(indexer, node, source_code);

    // Skip if name is empty
    if name.trim().is_empty() {
        return Ok(());
    }

    // Create a range for the reference
    let range = node_to_range(node);

    // Add reference with just the constant name
    add_reference(indexer, &name, uri, node);

    // Also add reference with namespace context if available
    let current_namespace = context.current_namespace();
    if !current_namespace.is_empty() {
        let fqn = format!("{}::{}", current_namespace, name);
        let location = create_location(uri, node);
        indexer.index.add_reference(&fqn, location);
    }

    // Add reference for all possible parent classes
    // This helps with finding references to nested constants
    let parent = node.parent();
    if let Some(p) = parent {
        if p.kind() == "scope_resolution" {
            // This is a nested constant access like A::B
            // We need to add references for both A and A::B
            if let Some(scope) = p.child_by_field_name("scope") {
                let scope_name = get_indexer_node_text(indexer, scope, source_code);
                let fqn = format!("{}::{}", scope_name, name);
                let location = create_location(uri, node);
                indexer.index.add_reference(&fqn, location);
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use tempfile::NamedTempFile;

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
        # Top-level constant
        VERSION = "1.0"

        module Namespace
          # Nested constant
          TIMEOUT = 30
        end

        class Config
          # Class constant with calculation
          MAX_RETRIES = 5 * 2
        end
        "#;

        let (file, uri) = create_temp_ruby_file(ruby_code);

        let result = indexer.index_file_with_uri(uri.clone(), ruby_code);
        assert!(result.is_ok(), "Should be able to index the file");

        // Check for top-level constant
        let version_entries = indexer.index().constants_by_name.get("VERSION");
        assert!(
            version_entries.is_some(),
            "VERSION constant should be indexed"
        );

        // Check for nested constant
        let timeout_entries = indexer.index().constants_by_name.get("TIMEOUT");
        assert!(
            timeout_entries.is_some(),
            "TIMEOUT constant should be indexed"
        );

        // Check for class constant
        let retries_entries = indexer.index().constants_by_name.get("MAX_RETRIES");
        assert!(
            retries_entries.is_some(),
            "MAX_RETRIES constant should be indexed"
        );

        // Clean up
        drop(file);
    }

    #[test]
    fn test_constant_references() {
        let mut indexer = RubyIndexer::new().expect("Failed to create indexer");
        let ruby_code = r#"
        # Define constants
        PI = 3.14159
        module Math
          E = 2.71828
        end

        # Reference constants
        circumference = 2 * PI * radius
        exp_value = Math::E ** x
        "#;

        let (file, uri) = create_temp_ruby_file(ruby_code);

        let result = indexer.index_file_with_uri(uri.clone(), ruby_code);
        assert!(result.is_ok(), "Should be able to index the file");

        // Verify references were added
        let pi_refs = indexer.index().find_references("PI");
        assert!(!pi_refs.is_empty(), "Should have references to PI");

        let e_refs = indexer.index().find_references("Math::E");
        assert!(!e_refs.is_empty(), "Should have references to Math::E");

        // Clean up
        drop(file);
    }
}
