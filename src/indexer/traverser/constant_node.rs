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
