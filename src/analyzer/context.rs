use lsp_types::Position;
use tree_sitter::{Node, Tree};

use super::core::RubyAnalyzer;
use super::position::position_to_point;

impl RubyAnalyzer {
    /// Find the current method name at the given position
    pub fn find_current_method(&self, tree: &Tree, position: Position) -> Option<String> {
        let point = position_to_point(position);
        let mut cursor = tree.root_node().walk();

        // Find the smallest node that contains the point
        let node = self.find_node_at_point(&mut cursor, point)?;

        // Walk up the tree to find a method definition
        let mut current = Some(node);
        while let Some(n) = current {
            if n.kind() == "method" {
                if let Some(name_node) = n.child_by_field_name("name") {
                    return Some(self.get_node_text(name_node));
                }
            }
            current = n.parent();
        }

        None
    }

    /// Find the current class/module context at the given position
    pub fn find_current_context(&self, tree: &Tree, position: Position) -> String {
        // We don't need to convert to Point since we're using row/column directly
        let root_node = tree.root_node();

        // Get all modules and classes in the file
        let mut module_nodes = Vec::new();
        self.find_modules_and_classes(root_node, &mut module_nodes);

        // Build a context hierarchy
        let mut contexts = Vec::new();

        // For each module/class, check if it contains the position
        // This includes the declaration line (unlike the naive point-in-range check)
        for node in &module_nodes {
            // Get the range of lines this module/class covers
            let start_line = node.start_position().row as u32;
            let end_line = node.end_position().row as u32;

            // Get the name of the module/class
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = self.get_node_text(name_node);

                // Check if the position is within this module/class
                // Include the declaration line (where position.line == start_line)
                if position.line >= start_line && position.line <= end_line {
                    // This is a module or class that contains our position
                    // Keep track of it along with its extent
                    contexts.push((name, node.start_byte(), node.end_byte()));
                }
            }
        }

        // Sort contexts by size (smallest to largest)
        // This will give us the most specific (innermost) context first
        contexts.sort_by_key(|&(_, start, end)| end - start);

        // Extract the names in order from most specific to least specific
        let mut result = Vec::new();
        for (name, _, _) in contexts {
            result.push(name);
        }

        // Reverse to get from outermost to innermost
        result.reverse();

        // Join with :: separator to form the context string
        if result.is_empty() {
            String::new()
        } else {
            result.join("::")
        }
    }

    /// Find all module and class nodes in the tree
    fn find_modules_and_classes<'a>(&self, node: Node<'a>, result: &mut Vec<Node<'a>>) {
        // If this is a module or class node, add it
        if node.kind() == "module" || node.kind() == "class" {
            result.push(node);
        }

        // Recursively check all children
        for i in 0..node.named_child_count() {
            if let Some(child) = node.named_child(i) {
                self.find_modules_and_classes(child, result);
            }
        }
    }

    /// Determine the namespace for a given node
    pub fn determine_namespace(&self, node: Node) -> String {
        let mut parts = Vec::new();
        let mut current = node.parent();

        while let Some(parent) = current {
            match parent.kind() {
                "class" | "module" => {
                    if let Some(name_node) = parent.child_by_field_name("name") {
                        parts.push(self.get_node_text(name_node));
                    }
                }
                _ => {}
            }
            current = parent.parent();
        }

        // Reverse to get outer-to-inner order
        parts.reverse();

        if parts.is_empty() {
            String::new()
        } else {
            parts.join("::")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_test(code: &str) -> (RubyAnalyzer, Tree) {
        let mut analyzer = RubyAnalyzer::new();
        let tree = analyzer.parse_document(code);
        (analyzer, tree)
    }

    #[test]
    fn test_find_current_method() {
        let code = r#"
        class User
          def save
            update_attributes(name: "test")
          end
        end
        "#;

        let (analyzer, tree) = setup_test(code);

        // Position inside the save method
        let result = analyzer.find_current_method(&tree, Position::new(3, 14));
        assert_eq!(result, Some("save".to_string()));

        // Position outside any method
        let result = analyzer.find_current_method(&tree, Position::new(1, 5));
        assert_eq!(result, None);
    }

    #[test]
    fn test_find_current_context() {
        let code = r#"
        module Admin
          class User
            def save
              update_attributes(name: "test")
            end
          end
        end
        "#;

        let (analyzer, tree) = setup_test(code);

        // Position inside the save method
        let result = analyzer.find_current_context(&tree, Position::new(4, 14));
        assert_eq!(result, "Admin::User");

        // Position at the "module" keyword
        let result = analyzer.find_current_context(&tree, Position::new(1, 5));
        assert_eq!(result, "Admin");

        // Additional test for "class" keyword
        let result = analyzer.find_current_context(&tree, Position::new(2, 5));
        assert_eq!(result, "Admin::User");
    }

    // This is a test that would verify our generic implementation
    // The implementation is complex because tree-sitter's AST structure
    // can vary between versions, so we need a robust approach
    #[test]
    fn test_find_context_by_traversal() {
        let code = r#"
        module Outer
          module Inner
            class Example
              def method
                # Some code here
              end
            end
          end
        end
        "#;

        let (analyzer, tree) = setup_test(code);

        // We'll collect all module/class nodes to check our traversal logic
        let mut nodes: Vec<Node> = Vec::new();
        analyzer.find_modules_and_classes(tree.root_node(), &mut nodes);

        // Verify we found 3 module/class nodes
        assert_eq!(nodes.len(), 3);

        // Print the nodes for debugging
        for node in &nodes {
            if let Some(name_node) = node.child_by_field_name("name") {
                println!(
                    "{} node: {}",
                    node.kind(),
                    analyzer.get_node_text(name_node)
                );
            }
        }
    }
}
