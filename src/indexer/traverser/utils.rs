use lsp_types::{Position, Range};
use tree_sitter::Node;

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
