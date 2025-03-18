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
    // Check for attr_* method calls like attr_accessor, attr_reader, attr_writer
    process_attribute_methods(indexer, node, uri, source_code, context)?;

    // Process method call references
    process_method_call(indexer, node, uri, source_code, context)?;

    // Continue traversing children
    let child_count = node.child_count();
    for i in 0..child_count {
        if let Some(child) = node.child(i) {
            indexer.traverse_node(child, uri, source_code, context)?;
        }
    }

    Ok(())
}

pub fn process_attribute_methods(
    indexer: &mut RubyIndexer,
    node: Node,
    uri: &Url,
    source_code: &str,
    context: &mut TraversalContext,
) -> Result<(), String> {
    // Check if this is a method call like attr_accessor, attr_reader, attr_writer
    if let Some(method_node) = node.child_by_field_name("method") {
        let method_name = indexer.get_node_text(method_node, source_code);

        // Only process specific attribute method calls
        if method_name != "attr_accessor"
            && method_name != "attr_reader"
            && method_name != "attr_writer"
        {
            return Ok(());
        }

        // Get the arguments (could be multiple symbol arguments)
        if let Some(args_node) = node.child_by_field_name("arguments") {
            let args_count = args_node.child_count();

            for i in 0..args_count {
                if let Some(arg_node) = args_node.child(i) {
                    // Skip non-symbol nodes (like commas)
                    if arg_node.kind() != "simple_symbol" {
                        continue;
                    }

                    // Extract the attribute name without the colon
                    let mut attr_name = indexer.get_node_text(arg_node, source_code);
                    if attr_name.starts_with(':') {
                        attr_name = attr_name[1..].to_string();
                    }

                    // Get the current namespace (class/module)
                    let current_namespace = context.current_namespace();
                    if current_namespace.is_empty() {
                        continue; // Skip if we're not in a class/module
                    }

                    // Create a range for the attribute definition
                    let range = node_to_range(arg_node);

                    // Create entries for the accessor methods
                    if method_name == "attr_accessor" || method_name == "attr_reader" {
                        // Add the getter method
                        let getter_fqn = format!("{}#{}", current_namespace, attr_name);
                        let getter_entry = EntryBuilder::new(&attr_name)
                            .fully_qualified_name(&getter_fqn)
                            .location(Location {
                                uri: uri.clone(),
                                range,
                            })
                            .entry_type(EntryType::Method)
                            .visibility(context.visibility)
                            .build()
                            .map_err(|e| e.to_string())?;

                        // Add the getter method to the index
                        indexer.index.add_entry(getter_entry);
                    }

                    if method_name == "attr_accessor" || method_name == "attr_writer" {
                        // Add the setter method (name=)
                        let setter_name = format!("{}=", attr_name);
                        let setter_fqn = format!("{}#{}", current_namespace, setter_name);
                        let setter_entry = EntryBuilder::new(&setter_name)
                            .fully_qualified_name(&setter_fqn)
                            .location(Location {
                                uri: uri.clone(),
                                range,
                            })
                            .entry_type(EntryType::Method)
                            .visibility(context.visibility)
                            .build()
                            .map_err(|e| e.to_string())?;

                        // Add the setter method to the index
                        indexer.index.add_entry(setter_entry);
                    }
                }
            }
        }
    }

    Ok(())
}

pub fn process_method_call(
    indexer: &mut RubyIndexer,
    node: Node,
    uri: &Url,
    source_code: &str,
    context: &mut TraversalContext,
) -> Result<(), String> {
    // Get the method name node
    if let Some(method_node) = node.child_by_field_name("method") {
        // Extract the method name
        let method_name = indexer.get_node_text(method_node, source_code);

        // Debug logging
        if indexer.debug_mode {
            info!(
                "Processing method call: {} at line {}:{}",
                method_name,
                node.start_position().row + 1,
                node.start_position().column + 1
            );
        }

        // Skip if method name is empty or is an attribute method
        if method_name.trim().is_empty()
            || method_name == "attr_accessor"
            || method_name == "attr_reader"
            || method_name == "attr_writer"
        {
            return Ok(());
        }

        // Create a range for the reference
        // For method calls without a receiver, we want to include the entire method call node
        // to match the expected range in tests
        let range = if node.child_by_field_name("receiver").is_none() {
            // For calls like 'bar' without a receiver, use the entire node range
            node_to_range(node)
        } else {
            // For calls with a receiver like 'foo.bar', use just the method name range
            node_to_range(method_node)
        };

        // Debug logging for the range
        if indexer.debug_mode {
            info!(
                "Method call range: {}:{} to {}:{}",
                range.start.line, range.start.character, range.end.line, range.end.character
            );
        }

        // Create a location for this reference
        let location = Location {
            uri: uri.clone(),
            range,
        };

        // Add reference with just the method name
        indexer.index.add_reference(&method_name, location.clone());

        // If there's a receiver, try to determine its type
        if let Some(receiver_node) = node.child_by_field_name("receiver") {
            let receiver_text = indexer.get_node_text(receiver_node, source_code);

            // If the receiver starts with uppercase, it's likely a class name
            if receiver_text
                .chars()
                .next()
                .map_or(false, |c| c.is_uppercase())
            {
                let fqn = format!("{}#{}", receiver_text, method_name);
                indexer.index.add_reference(&fqn, location.clone());
            }

            // Handle scope resolution operator for nested classes
            if receiver_node.kind() == "scope_resolution" {
                if let Some(scope_text) =
                    get_fully_qualified_scope(indexer, receiver_node, source_code)
                {
                    let fqn = format!("{}#{}", scope_text, method_name);
                    indexer.index.add_reference(&fqn, location.clone());
                }
            }

            // Add references for all possible class combinations
            // This helps with finding references in nested classes
            let current_namespace = context.current_namespace();
            if !current_namespace.is_empty() {
                // Try with current namespace as prefix
                let fqn = format!("{}::{}#{}", current_namespace, receiver_text, method_name);
                indexer.index.add_reference(&fqn, location.clone());
            }
        } else {
            // No explicit receiver, use current namespace as context
            let current_namespace = context.current_namespace();
            if !current_namespace.is_empty() {
                let fqn = format!("{}#{}", current_namespace, method_name);
                indexer.index.add_reference(&fqn, location);
            }
        }
    }

    Ok(())
}

// Helper method to get the fully qualified name from a scope resolution node
pub fn get_fully_qualified_scope(
    indexer: &RubyIndexer,
    node: Node,
    source_code: &str,
) -> Option<String> {
    if node.kind() != "scope_resolution" {
        return None;
    }

    let mut parts = Vec::new();

    // Get the constant part (right side of ::)
    if let Some(name_node) = node.child_by_field_name("name") {
        parts.push(indexer.get_node_text(name_node, source_code));
    }

    // Get the scope part (left side of ::)
    if let Some(scope_node) = node.child_by_field_name("scope") {
        if scope_node.kind() == "scope_resolution" {
            // Recursive case for nested scopes
            if let Some(parent_scope) = get_fully_qualified_scope(indexer, scope_node, source_code)
            {
                parts.insert(0, parent_scope);
            }
        } else {
            // Base case - just a constant
            parts.insert(0, indexer.get_node_text(scope_node, source_code));
        }
    }

    Some(parts.join("::"))
}
