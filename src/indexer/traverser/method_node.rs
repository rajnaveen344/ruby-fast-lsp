use crate::indexer::{
    entry::{EntryBuilder, EntryType},
    RubyIndexer,
};
use lsp_types::{Location, Url};
use tree_sitter::Node;

use super::{parameter_node, utils::node_to_range, TraversalContext};

pub fn process(
    indexer: &mut RubyIndexer,
    node: Node,
    uri: &Url,
    source_code: &str,
    context: &mut TraversalContext,
) -> Result<(), String> {
    // Find the method name node
    let name_node = node
        .child_by_field_name("name")
        .ok_or_else(|| "Method without a name".to_string())?;

    // Extract the name text
    let name = indexer.get_node_text(name_node, source_code);

    // Create a fully qualified name
    let current_namespace = context.current_namespace();
    let method_name = name.clone();

    let fqn = if current_namespace.is_empty() {
        method_name.clone()
    } else {
        format!("{}#{}", current_namespace, method_name)
    };

    // Create a range for the definition
    let range = node_to_range(node);

    // Create a range for the method name reference
    let name_range = node_to_range(name_node);

    // Create a location for this reference
    let location = Location {
        uri: uri.clone(),
        range: name_range,
    };

    // Add reference to the method name
    indexer.index.add_reference(&name, location.clone());

    // Add reference to the fully qualified name
    if name != fqn {
        indexer.index.add_reference(&fqn, location.clone());
    }

    // Also add a reference to the method declaration itself
    // This is important for finding references to method declarations
    let declaration_location = Location {
        uri: uri.clone(),
        range,
    };
    indexer
        .index
        .add_reference(&name, declaration_location.clone());
    if name != fqn {
        indexer.index.add_reference(&fqn, declaration_location);
    }

    // Create and add the entry
    let entry = EntryBuilder::new(&name)
        .fully_qualified_name(&fqn)
        .location(Location {
            uri: uri.clone(),
            range,
        })
        .entry_type(EntryType::Method)
        .visibility(context.visibility)
        .build()
        .map_err(|e| e.to_string())?;

    indexer.index.add_entry(entry);

    // Set the current method before processing the body
    context.current_method = Some(name.clone());

    // Process method parameters if they exist
    if let Some(parameters) = node.child_by_field_name("parameters") {
        parameter_node::process(indexer, parameters, uri, source_code, context)?;
    }

    // Process method body contents recursively
    if let Some(body) = node.child_by_field_name("body") {
        for i in 0..body.named_child_count() {
            if let Some(child) = body.named_child(i) {
                indexer.traverse_node(child, uri, source_code, context)?;
            }
        }
    }

    // Reset the current method after processing
    context.current_method = None;

    Ok(())
}
