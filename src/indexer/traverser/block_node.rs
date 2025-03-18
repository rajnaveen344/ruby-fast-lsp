use crate::indexer::{
    entry::{EntryBuilder, EntryType},
    RubyIndexer,
};
use lsp_types::{Location, Url};
use tree_sitter::Node;

use super::{
    utils::{get_indexer_node_text, node_to_range},
    TraversalContext,
};

pub fn process(
    indexer: &mut RubyIndexer,
    node: Node,
    uri: &Url,
    source_code: &str,
    context: &mut TraversalContext,
) -> Result<(), String> {
    // Process block parameters if they exist
    if let Some(parameters) = node.child_by_field_name("parameters") {
        process_block_parameters(indexer, parameters, uri, source_code, context)?;
    }

    // Process block body contents recursively
    if let Some(body) = node.child_by_field_name("body") {
        for i in 0..body.named_child_count() {
            if let Some(child) = body.named_child(i) {
                indexer.traverse_node(child, uri, source_code, context)?;
            }
        }
    } else {
        // If there's no explicit body field, traverse all children
        for i in 0..node.named_child_count() {
            if let Some(child) = node.named_child(i) {
                if child.kind() != "parameters" {
                    // Skip parameters as we already processed them
                    indexer.traverse_node(child, uri, source_code, context)?;
                }
            }
        }
    }

    Ok(())
}

pub fn process_block_parameters(
    indexer: &mut RubyIndexer,
    parameters: Node,
    uri: &Url,
    source_code: &str,
    context: &mut TraversalContext,
) -> Result<(), String> {
    // Process block parameters as local variables
    for i in 0..parameters.named_child_count() {
        if let Some(param) = parameters.named_child(i) {
            // Extract parameter name
            let name = get_indexer_node_text(indexer, param, source_code);
            if name.trim().is_empty() {
                continue;
            }

            // Create a range for the parameter declaration
            let range = node_to_range(param);

            // Use the current namespace and method to create a fully qualified name
            let current_namespace = context.current_namespace();
            let block_var_prefix = if let Some(method_name) = &context.current_method {
                format!(
                    "{}{}#block-{}",
                    if current_namespace.is_empty() {
                        ""
                    } else {
                        &current_namespace
                    },
                    if current_namespace.is_empty() {
                        ""
                    } else {
                        "::"
                    },
                    method_name
                )
            } else {
                format!(
                    "{}block",
                    if current_namespace.is_empty() {
                        ""
                    } else {
                        &current_namespace
                    }
                )
            };

            // Create the fully qualified name
            let fqn = format!("{}${}", block_var_prefix, name);

            // Create and add the entry
            let entry = EntryBuilder::new(&name)
                .fully_qualified_name(&fqn)
                .location(Location {
                    uri: uri.clone(),
                    range,
                })
                .entry_type(EntryType::LocalVariable)
                .metadata("kind", "block_parameter")
                .build()
                .map_err(|e| e.to_string())?;

            indexer.index.add_entry(entry);
        }
    }

    Ok(())
}
