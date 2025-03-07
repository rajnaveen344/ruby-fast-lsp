use lsp_types::Position;
use tree_sitter::{Node, Parser, Point, Tree, TreeCursor};

/// Central analyzer for Ruby code
///
/// Responsible for parsing Ruby code and providing analysis capabilities
/// like finding identifiers, determining contexts, and extracting semantic information.
pub struct RubyAnalyzer {
    parser: Parser,
    document: String,
}

impl RubyAnalyzer {
    /// Create a new RubyAnalyzer instance
    pub fn new() -> Self {
        let mut parser = Parser::new();
        let language = tree_sitter_ruby::LANGUAGE;
        let _ = parser
            .set_language(&language.into())
            .map_err(|_| "Failed to load Ruby grammar".to_string());

        RubyAnalyzer {
            parser,
            document: String::new(),
        }
    }

    /// Parse a document string into a tree-sitter Tree
    pub fn parse_document(&mut self, document: &str) -> Tree {
        self.document = document.to_string();
        self.parser
            .parse(document, None)
            .expect("Failed to parse document")
    }

    /// Extract text from a node in the document
    pub fn get_node_text(&self, node: Node) -> String {
        let start_byte = node.start_byte();
        let end_byte = node.end_byte();

        if start_byte >= self.document.len() || end_byte > self.document.len() {
            return String::new();
        }

        self.document[start_byte..end_byte].to_string()
    }

    /// Find a node at a specific point in the tree
    pub fn find_node_at_point<'a>(
        &self,
        cursor: &mut TreeCursor<'a>,
        point: Point,
    ) -> Option<Node<'a>> {
        let node = cursor.node();

        // Check if point is within node bounds
        if !is_point_within_node(point, node) {
            return None;
        }

        // First check if any of the children contain the point
        if cursor.goto_first_child() {
            loop {
                if let Some(matching_node) = self.find_node_at_point(cursor, point) {
                    return Some(matching_node);
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
            cursor.goto_parent();
        }

        // If no child contains the point, return this node
        Some(node)
    }
}

/// Check if a point is within a node's range
fn is_point_within_node(point: Point, node: Node) -> bool {
    let start_position = node.start_position();
    let end_position = node.end_position();

    (start_position.row < point.row
        || (start_position.row == point.row && start_position.column <= point.column))
        && (end_position.row > point.row
            || (end_position.row == point.row && end_position.column >= point.column))
}
