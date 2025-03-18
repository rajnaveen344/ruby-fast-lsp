use lsp_types::{Location, Position, Range, Url};
use tree_sitter::Node;

use crate::indexer::RubyIndexer;

pub fn get_node_text(node: Node, content: &str) -> String {
    let start_byte = node.start_byte();
    let end_byte = node.end_byte();

    if start_byte <= end_byte && end_byte <= content.len() {
        content[start_byte..end_byte].to_string()
    } else {
        String::new()
    }
}

pub fn node_to_range(node: Node) -> Range {
    let start_point = node.start_position();
    let end_point = node.end_position();

    Range {
        start: Position {
            line: start_point.row as u32,
            character: start_point.column as u32,
        },
        end: Position {
            line: end_point.row as u32,
            character: end_point.column as u32,
        },
    }
}

pub fn get_fqn(namespace: &str, name: &str) -> String {
    if namespace.is_empty() {
        name.to_string()
    } else {
        format!("{}::{}", namespace, name)
    }
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
        parts.push(get_indexer_node_text(indexer, name_node, source_code));
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
            parts.insert(0, get_indexer_node_text(indexer, scope_node, source_code));
        }
    }

    Some(parts.join("::"))
}

// Helper method to create a Location from a node
pub fn create_location(uri: &Url, node: Node) -> Location {
    Location {
        uri: uri.clone(),
        range: node_to_range(node),
    }
}

// Helper method to create a reference Location and add it to the index
pub fn add_reference(indexer: &mut RubyIndexer, name: &str, uri: &Url, node: Node) {
    let location = create_location(uri, node);
    indexer.index.add_reference(name, location);
}

// Helper method to create multiple references for a method call
pub fn add_method_call_references(
    indexer: &mut RubyIndexer,
    method_name: &str,
    method_node: Node,
    call_node: Node,
    uri: &Url,
    current_namespace: &str,
) {
    // Create locations for both method name and full call node
    let method_location = Location {
        uri: uri.clone(),
        range: node_to_range(method_node),
    };

    let full_call_location = Location {
        uri: uri.clone(),
        range: node_to_range(call_node),
    };

    // Add references with just the method name
    indexer
        .index
        .add_reference(method_name, method_location.clone());
    indexer
        .index
        .add_reference(method_name, full_call_location.clone());

    // Add reference with namespace context if available
    if !current_namespace.is_empty() {
        let fqn = format!("{}#{}", current_namespace, method_name);
        indexer.index.add_reference(&fqn, method_location);
        indexer.index.add_reference(&fqn, full_call_location);
    }
}

// Helper method to get node text using the indexer's method
pub fn get_indexer_node_text(indexer: &RubyIndexer, node: Node, source_code: &str) -> String {
    indexer.get_node_text(node, source_code)
}
