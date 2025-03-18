use crate::indexer::{
    entry::{EntryBuilder, EntryType},
    RubyIndexer,
};
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
    // Check parent node to determine if these are method or block parameters
    if let Some(parent) = node.parent() {
        if parent.kind() == "method" || parent.kind() == "singleton_method" {
            process_method_parameters(indexer, node, uri, source_code, context)?;
        } else if parent.kind() == "block" {
            super::block_node::process_block_parameters(indexer, node, uri, source_code, context)?;
        }
    }

    Ok(())
}

pub fn process_method_parameters(
    indexer: &mut RubyIndexer,
    node: Node,
    uri: &Url,
    source_code: &str,
    context: &mut TraversalContext,
) -> Result<(), String> {
    // Iterate through all parameter nodes
    for i in 0..node.named_child_count() {
        if let Some(param_node) = node.named_child(i) {
            let param_kind = param_node.kind();
            let param_name = match param_kind {
                "identifier" => indexer.get_node_text(param_node, source_code),
                "optional_parameter"
                | "keyword_parameter"
                | "rest_parameter"
                | "hash_splat_parameter"
                | "block_parameter" => {
                    if let Some(name_node) = param_node.child_by_field_name("name") {
                        indexer.get_node_text(name_node, source_code)
                    } else {
                        continue;
                    }
                }
                _ => continue,
            };

            if param_name.trim().is_empty() {
                continue;
            }

            // Create a range for the definition
            let range = node_to_range(param_node);

            // Create a fully qualified name for the parameter
            let current_namespace = context.current_namespace();
            let current_method = context
                .current_method
                .as_ref()
                .ok_or_else(|| "Method parameter outside of method context".to_string())?;

            let fqn = if current_namespace.is_empty() {
                format!("{}${}", current_method, param_name)
            } else {
                format!("{}#{}${}", current_namespace, current_method, param_name)
            };

            // Create and add the entry
            let entry = EntryBuilder::new(&param_name)
                .fully_qualified_name(&fqn)
                .location(Location {
                    uri: uri.clone(),
                    range,
                })
                .entry_type(EntryType::LocalVariable)
                .metadata("kind", "parameter")
                .build()
                .map_err(|e| e.to_string())?;

            indexer.index.add_entry(entry);
        }
    }

    Ok(())
}
