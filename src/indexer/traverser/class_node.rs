use lsp_types::{Location, Url};
use tree_sitter::Node;

use crate::indexer::{
    entry::{EntryBuilder, EntryType},
    RubyIndexer,
};

use super::{
    utils::{get_fqn, get_node_text, node_to_range},
    TraversalContext,
};

pub fn process(
    indexer: &mut RubyIndexer,
    node: Node,
    uri: &Url,
    content: &str,
    context: &mut TraversalContext,
) -> Result<(), String> {
    let current_namespace = context.current_namespace();
    let class_name = get_class_name(node, content);
    let class_fqn = get_fqn(&current_namespace, &class_name);
    let range = node_to_range(node);
    let location = Location {
        uri: uri.clone(),
        range,
    };

    let entry = EntryBuilder::new(&class_name)
        .fully_qualified_name(&class_fqn)
        .location(location)
        .entry_type(EntryType::Class)
        .build()?;

    // Add entry to index
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

    if !children.contains(&class_name) {
        children.push(class_name.clone());
    }

    // Push the class name onto the namespace stack
    context.namespace_stack.push(class_name);

    // Process the body of the class
    if let Some(body_node) = node.child_by_field_name("body") {
        indexer.traverse_node(body_node, uri, content, context)?;
    }

    // Pop the namespace when done
    context.namespace_stack.pop();

    Ok(())
}

fn get_class_name(node: Node, content: &str) -> String {
    let name_node = match node.child_by_field_name("name") {
        Some(node) => node,
        None => return String::new(),
    };
    get_node_text(name_node, content)
}
