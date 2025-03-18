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
    // Iterate through all parameter nodes
    for i in 0..parameters.named_child_count() {
        if let Some(param) = parameters.named_child(i) {
            let param_text = match param.kind() {
                "identifier" => Some(indexer.get_node_text(param, source_code)),
                "optional_parameter"
                | "keyword_parameter"
                | "rest_parameter"
                | "hash_splat_parameter"
                | "block_parameter" => param
                    .child_by_field_name("name")
                    .map(|name_node| indexer.get_node_text(name_node, source_code)),
                _ => None,
            };

            if let Some(param_text) = param_text {
                if param_text.trim().is_empty() {
                    continue;
                }

                // Create a range for the parameter
                let range = node_to_range(param);

                // Find the method that contains this block
                let mut current = parameters.clone();
                let mut method_name = None;

                while let Some(p) = current.parent() {
                    if p.kind() == "method" || p.kind() == "singleton_method" {
                        if let Some(method_name_node) = p.child_by_field_name("name") {
                            method_name =
                                Some(indexer.get_node_text(method_name_node, source_code));
                        }
                        break;
                    }
                    current = p;
                }

                // Build the FQN based on context
                let fqn = if let Some(method_name) =
                    method_name.as_ref().or(context.current_method.as_ref())
                {
                    if context.namespace_stack.is_empty() {
                        format!("{}$block${}", method_name, param_text)
                    } else {
                        format!(
                            "{}#{}$block${}",
                            context.current_namespace(),
                            method_name,
                            param_text
                        )
                    }
                } else {
                    if context.namespace_stack.is_empty() {
                        format!("$block${}", param_text)
                    } else {
                        format!("{}#$block${}", context.current_namespace(), param_text)
                    }
                };

                // Create and add the entry
                let entry = EntryBuilder::new(&param_text)
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
    }
    Ok(())
}
