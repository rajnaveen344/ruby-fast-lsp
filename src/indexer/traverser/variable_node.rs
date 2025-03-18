use crate::indexer::{
    entry::{EntryBuilder, EntryType},
    RubyIndexer,
};
use lsp_types::{Location, Url};
use tree_sitter::Node;

use super::{utils::node_to_range, TraversalContext};

pub fn process_local_variable(
    indexer: &mut RubyIndexer,
    node: Node,
    uri: &Url,
    source_code: &str,
    context: &mut TraversalContext,
) -> Result<(), String> {
    // For local variable assignments, the name is in the "left" field
    let name_node = node
        .child_by_field_name("left")
        .ok_or_else(|| "Variable assignment without a name".to_string())?;

    // Extract the variable name
    let name = indexer.get_node_text(name_node, source_code);

    // Skip if name is empty
    if name.trim().is_empty() {
        return Ok(());
    }

    // Create a fully qualified name that includes the current method scope
    // This is important to prevent collisions between variables in different methods
    let current_namespace = context.current_namespace();

    // Determine if we're in a method context
    let current_method = context.current_method.as_ref();

    let fqn = if let Some(method_name) = current_method {
        // If we're in a method, include it in the FQN
        if current_namespace.is_empty() {
            format!("{}#${}", method_name, name)
        } else {
            format!("{}#${}${}", current_namespace, method_name, name)
        }
    } else {
        // Otherwise, just use the namespace and name
        if current_namespace.is_empty() {
            format!("${}", name)
        } else {
            format!("{}#${}", current_namespace, name)
        }
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
        .entry_type(EntryType::LocalVariable)
        .build()
        .map_err(|e| e.to_string())?;

    indexer.index.add_entry(entry);

    // Continue traversing the right side of the assignment
    if let Some(right) = node.child_by_field_name("right") {
        indexer.traverse_node(right, uri, source_code, context)?;
    }

    Ok(())
}

pub fn process_instance_variable(
    indexer: &mut RubyIndexer,
    node: Node,
    uri: &Url,
    source_code: &str,
    context: &mut TraversalContext,
) -> Result<(), String> {
    // For instance variable assignments, the name is in the "left" field
    let name_node = node
        .child_by_field_name("left")
        .ok_or_else(|| "Instance variable assignment without a name".to_string())?;

    // Extract the variable name
    let name = indexer.get_node_text(name_node, source_code);

    // Skip if name is empty
    if name.trim().is_empty() {
        return Ok(());
    }

    // Create a fully qualified name that includes the current class/module context
    let current_namespace = context.current_namespace();

    // Determine the FQN for the instance variable
    let fqn = if current_namespace.is_empty() {
        name.clone()
    } else {
        format!("{}#{}", current_namespace, name)
    };

    // Create a range for the definition
    let range = node_to_range(name_node);

    // Create and add the entry
    let entry = EntryBuilder::new(&name)
        .fully_qualified_name(&fqn)
        .location(Location {
            uri: uri.clone(),
            range,
        })
        .entry_type(EntryType::InstanceVariable)
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

pub fn process_instance_variable_reference(
    indexer: &mut RubyIndexer,
    node: Node,
    uri: &Url,
    source_code: &str,
    context: &mut TraversalContext,
) -> Result<(), String> {
    // Extract the variable name
    let name = indexer.get_node_text(node, source_code);

    // Skip if name is empty
    if name.trim().is_empty() {
        return Ok(());
    }

    // Create a range for the reference
    let range = node_to_range(node);

    // Create a location for this reference
    let location = Location {
        uri: uri.clone(),
        range,
    };

    // Add reference with just the variable name (e.g., @name)
    indexer.index.add_reference(&name, location.clone());

    // Also add reference with class context if available
    let current_namespace = context.current_namespace();
    if !current_namespace.is_empty() {
        let fqn = format!("{}#{}", current_namespace, name);
        indexer.index.add_reference(&fqn, location);
    }

    Ok(())
}

pub fn process_class_variable(
    indexer: &mut RubyIndexer,
    node: Node,
    uri: &Url,
    source_code: &str,
    _context: &mut TraversalContext,
) -> Result<(), String> {
    let left = node
        .child_by_field_name("left")
        .ok_or_else(|| "Failed to get left node of class variable assignment".to_string())?;

    let name = indexer.get_node_text(left, source_code);

    // Add reference for the class variable
    let location = Location::new(uri.clone(), node_to_range(left));
    indexer.index.add_reference(&name, location);

    Ok(())
}

pub fn process_class_variable_reference(
    indexer: &mut RubyIndexer,
    node: Node,
    uri: &Url,
    source_code: &str,
    _context: &mut TraversalContext,
) -> Result<(), String> {
    // Get the variable name
    let name = indexer.get_node_text(node, source_code);

    // Add reference for the class variable
    let range = node_to_range(node);
    let location = Location {
        uri: uri.clone(),
        range,
    };

    indexer.index.add_reference(&name, location);

    Ok(())
}
