use crate::indexer::{
    entry::{EntryBuilder, EntryType},
    RubyIndexer,
};
use log::info;
use lsp_types::{Location, Url};
use tree_sitter::Node;

use super::{utils::node_to_range, TraversalContext};

pub fn process(
    indexer: &mut RubyIndexer,
    node: Node,
    uri: &Url,
    source_code: &str,
    context: &mut TraversalContext,
) -> Result<(), String> {
    // Find the module name node
    let name_node = match node.child_by_field_name("name") {
        Some(node) => node,
        None => {
            // Skip anonymous or dynamically defined modules instead of failing
            if indexer.debug_mode {
                info!(
                    "Skipping module without a name at {}:{}",
                    node.start_position().row + 1,
                    node.start_position().column + 1
                );
            }

            // Still traverse children for any defined methods or constants
            for i in 0..node.child_count() {
                if let Some(child) = node.child(i) {
                    indexer.traverse_node(child, uri, source_code, context)?;
                }
            }

            return Ok(());
        }
    };

    // Extract the name text
    let name = indexer.get_node_text(name_node, source_code);

    // Skip modules with empty names or just whitespace
    if name.trim().is_empty() {
        if indexer.debug_mode {
            info!(
                "Skipping module with empty name at {}:{}",
                node.start_position().row + 1,
                node.start_position().column + 1
            );
        }

        // Still traverse children
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                indexer.traverse_node(child, uri, source_code, context)?;
            }
        }

        return Ok(());
    }

    // Create a fully qualified name by joining the namespace stack
    let current_namespace = context.current_namespace();

    let fqn = if current_namespace.is_empty() {
        name.clone()
    } else {
        format!("{}::{}", current_namespace, name)
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
        .entry_type(EntryType::Module)
        .build()?;

    indexer.index.add_entry(entry);

    // Add to namespace tree
    let parent_namespace = if context.namespace_stack.is_empty() {
        String::new()
    } else {
        current_namespace
    };

    let children = indexer
        .index
        .namespace_tree
        .entry(parent_namespace)
        .or_insert_with(Vec::new);

    if !children.contains(&name) {
        children.push(name.clone());
    }

    // Push the module name onto the namespace stack
    context.namespace_stack.push(name);

    // Process the body of the module
    if let Some(body_node) = node.child_by_field_name("body") {
        indexer.traverse_node(body_node, uri, source_code, context)?;
    }

    // Pop the namespace when done
    context.namespace_stack.pop();

    Ok(())
}
