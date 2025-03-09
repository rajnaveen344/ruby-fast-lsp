use lsp_types::Position;
use tree_sitter::{Node, Point, Tree};

use super::core::RubyAnalyzer;
use super::position::position_to_point;

impl RubyAnalyzer {
    /// Finds the identifier at the given position and returns its fully qualified name
    pub fn find_identifier_at_position(
        &mut self,
        document: &str,
        position: Position,
    ) -> Option<String> {
        let tree = self.parse_document(document);
        let point = position_to_point(position);

        let mut cursor = tree.root_node().walk();
        let node = match self.find_node_at_point(&mut cursor, point) {
            Some(node) => node,
            None => return None,
        };

        // Check if we're on an instance variable node
        if let Some(instance_var_node) = self.check_for_instance_variable(&node, point) {
            return Some(self.get_node_text(instance_var_node));
        }

        // Check if we're on a method definition node or method name
        if let Some(method_name) = self.check_for_method_definition(&node, point) {
            let current_context = self.find_current_context(&tree, position);
            if current_context.is_empty() {
                return Some(method_name);
            } else {
                return Some(format!("{}#{}", current_context, method_name));
            }
        }

        // For normal method calls and identifiers
        let fully_qualified_name = self.determine_fully_qualified_name(&tree, node, position);

        if fully_qualified_name.is_empty() {
            None
        } else {
            Some(fully_qualified_name)
        }
    }

    /// Check if the node or its parent is an instance variable
    pub fn check_for_instance_variable<'a>(
        &self,
        node: &Node<'a>,
        point: Point,
    ) -> Option<Node<'a>> {
        let node_kind = node.kind();

        // Direct match for instance variable
        if node_kind == "instance_variable" {
            return Some(*node);
        }

        // Check if parent is an instance variable
        let parent = node.parent()?;
        let parent_kind = parent.kind();

        if parent_kind == "instance_variable" && is_point_within_node(point, parent) {
            return Some(parent);
        }

        None
    }

    /// Check if the node is part of a method definition
    pub fn check_for_method_definition<'a>(&self, node: &Node<'a>, point: Point) -> Option<String> {
        let node_kind = node.kind();

        // Check if we're directly on a method name node
        if node_kind == "identifier" {
            let parent = node.parent()?;
            if parent.kind() == "method" {
                let name_node = parent.child_by_field_name("name")?;
                if name_node == *node {
                    return Some(self.get_node_text(*node));
                }
            }
        }

        // Check if we're on a method node
        if node_kind == "method" {
            if let Some(name_node) = node.child_by_field_name("name") {
                if is_point_within_node(point, name_node) ||
                   // Also check if we're on the 'def' keyword
                   (node.start_position().row == point.row &&
                    node.start_position().column <= point.column &&
                    point.column <= name_node.start_position().column)
                {
                    return Some(self.get_node_text(name_node));
                }
            }
        }

        // Check if parent is a method
        let parent = node.parent()?;
        if parent.kind() == "method" {
            if let Some(name_node) = parent.child_by_field_name("name") {
                if is_point_within_node(point, name_node) {
                    return Some(self.get_node_text(name_node));
                }

                // Check if we're on the 'def' keyword
                let def_node = parent.child_by_field_name("def")?;
                if is_point_within_node(point, def_node) {
                    return Some(self.get_node_text(name_node));
                }
            }
        }

        None
    }

    /// Determine if a node is a local variable
    pub fn is_local_variable(&self, node: &Node, tree: &Tree) -> bool {
        // Skip nodes that are clearly not identifiers
        match node.kind() {
            "identifier" | "constant" => {}
            _ => return false,
        }

        // Get the node text
        let node_text = self.get_node_text(*node);

        // Check if this identifier is used as a method name
        let parent = match node.parent() {
            Some(parent) => parent,
            None => return false,
        };

        match parent.kind() {
            // If parent is a method definition, this is not a variable usage
            "method" => {
                let method_name_node = parent.child_by_field_name("name");
                match method_name_node {
                    Some(name_node) if name_node == *node => return false,
                    _ => {}
                }
            }
            // Exclude method parameter names
            "method_parameters" | "block_parameters" => return false,
            _ => {}
        }

        // For typical method calls, the receiver would be an identifier
        // If this identifier is a part of method call and is the method name, it's not a variable
        if parent.kind() == "call" {
            if let Some(method_node) = parent.child_by_field_name("method") {
                if method_node == *node {
                    return false;
                }
            }
        }

        // Examine the context to ensure this is a local variable
        let current_context = self.find_current_context(
            tree,
            Position::new(
                node.start_position().row as u32,
                node.start_position().column as u32,
            ),
        );

        // Check if this identifier is defined as a local variable somewhere
        // This is a simplified check - a real implementation would analyze scope
        true
    }

    /// Determine the fully qualified name of a node
    pub fn determine_fully_qualified_name(
        &self,
        tree: &Tree,
        node: Node,
        position: Position,
    ) -> String {
        match node.kind() {
            "constant" => {
                // For constants, return the fully qualified name
                let node_text = self.get_node_text(node);
                if node
                    .parent()
                    .map_or(false, |p| p.kind() == "scope_resolution")
                {
                    // Handle nested constants like A::B or A::B::C
                    let parent = node.parent().unwrap();

                    // Check if this node is the 'name' field of the scope_resolution
                    if parent
                        .child_by_field_name("name")
                        .map_or(false, |n| n == node)
                    {
                        // Get the scope part (which could be another scope_resolution or a constant)
                        if let Some(scope_node) = parent.child_by_field_name("scope") {
                            // Recursively determine the fully qualified name of the scope
                            let scope_fqn =
                                self.determine_fully_qualified_name(tree, scope_node, position);
                            return format!("{}::{}", scope_fqn, node_text);
                        }
                    }
                }
                node_text
            }
            "scope_resolution" => {
                // Handle scope resolution nodes directly
                if let Some(scope_node) = node.child_by_field_name("scope") {
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let scope_fqn =
                            self.determine_fully_qualified_name(tree, scope_node, position);
                        let name_text = self.get_node_text(name_node);
                        return format!("{}::{}", scope_fqn, name_text);
                    }
                }
                String::new()
            }
            "identifier" => {
                if self.is_local_variable(&node, tree) {
                    // For local variables, return just the name
                    self.get_node_text(node)
                } else {
                    // This might be a method call
                    self.determine_method_call_fqn(tree, node, position)
                }
            }
            "call" => {
                // For method calls, construct the fully qualified name
                self.determine_method_call_fqn(tree, node, position)
            }
            "method" => {
                // For method definitions, get the method name
                if let Some(name_node) = node.child_by_field_name("name") {
                    let method_name = self.get_node_text(name_node);
                    let current_context = self.find_current_context(tree, position);

                    if current_context.is_empty() {
                        method_name
                    } else {
                        format!("{}#{}", current_context, method_name)
                    }
                } else {
                    String::new()
                }
            }
            "instance_variable" => {
                // For instance variables, return the name with context
                let var_name = self.get_node_text(node);
                let current_context = self.find_current_context(tree, position);

                if current_context.is_empty() {
                    var_name
                } else {
                    format!("{}{}", current_context, var_name)
                }
            }
            _ => String::new(),
        }
    }

    /// Determine the fully qualified name of a method call
    pub fn determine_method_call_fqn(&self, tree: &Tree, node: Node, position: Position) -> String {
        let method_node = if node.kind() == "call" {
            node.child_by_field_name("method")
        } else {
            // If the node itself is an identifier that's part of a call
            let parent = node.parent();
            if let Some(p) = parent {
                if p.kind() == "call" && p.child_by_field_name("method") == Some(node) {
                    Some(node)
                } else {
                    None
                }
            } else {
                None
            }
        };

        if let Some(method_node) = method_node {
            let method_name = self.get_node_text(method_node);

            // Try to determine the receiver type/class
            // For simplicity, we'll just return the method name
            // In a real implementation, this would need type inference

            method_name
        } else {
            String::new()
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::Position;

    fn setup_test(code: &str) -> (RubyAnalyzer, Tree) {
        let mut analyzer = RubyAnalyzer::new();
        let tree = analyzer.parse_document(code);
        (analyzer, tree)
    }

    #[test]
    fn test_method_call_identification() {
        let code = r#"
        class User
          def save
            update_attributes(name: "test")
          end
        end
        "#;

        let (mut analyzer, _) = setup_test(code);

        // Position at "update_attributes"
        let result = analyzer.find_identifier_at_position(code, Position::new(3, 14));
        assert_eq!(result, Some("update_attributes".to_string()));
    }

    #[test]
    fn test_instance_variable_identification() {
        let code = r#"
        class User
          def initialize(name)
            @name = name
          end
        end
        "#;

        let (mut analyzer, _) = setup_test(code);

        // Position at "@name"
        let result = analyzer.find_identifier_at_position(code, Position::new(3, 14));
        assert_eq!(result, Some("@name".to_string()));
    }

    #[test]
    fn test_multi_level_scope_resolution() {
        let code = r#"
        module Outer
          module Inner
            class VeryInner
              def method
                puts "Hello"
              end
            end
          end
        end

        # Usage
        Outer::Inner::VeryInner.new.method
        "#;

        let (mut analyzer, tree) = setup_test(code);

        // Find the scope_resolution node for Outer::Inner::VeryInner
        let root_node = tree.root_node();

        // Helper function to recursively find a node of a specific kind
        fn find_node_by_kind<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
            if node.kind() == kind {
                return Some(node);
            }

            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if let Some(found) = find_node_by_kind(child, kind) {
                    return Some(found);
                }
            }

            None
        }

        // Find a scope_resolution node
        let scope_node = find_node_by_kind(root_node, "scope_resolution")
            .expect("Failed to find scope_resolution node in test code");

        // Test the fully qualified name determination
        let fqn = analyzer.determine_fully_qualified_name(&tree, scope_node, Position::new(12, 10));

        // The test might find any scope_resolution node, so we'll check if it contains "Outer::Inner"
        assert!(
            fqn.contains("Outer::Inner"),
            "FQN '{}' should contain 'Outer::Inner'",
            fqn
        );
    }

    #[test]
    fn test_multi_level_scope_resolution_exact() {
        let code = r#"
        module Outer
          module Inner
            class VeryInner
            end
          end
        end

        # Direct reference to the nested class
        x = Outer::Inner::VeryInner
        "#;

        let (mut analyzer, tree) = setup_test(code);
        let root_node = tree.root_node();

        // Helper function to recursively find a node with specific text
        fn find_node_with_text<'a>(
            node: Node<'a>,
            text: &str,
            analyzer: &RubyAnalyzer,
        ) -> Option<Node<'a>> {
            let node_text = analyzer.get_node_text(node);
            if node_text == text {
                return Some(node);
            }

            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if let Some(found) = find_node_with_text(child, text, analyzer) {
                    return Some(found);
                }
            }

            None
        }

        // Find the exact scope_resolution node for "Outer::Inner::VeryInner"
        let target_node = find_node_with_text(root_node, "Outer::Inner::VeryInner", &analyzer)
            .expect("Failed to find 'Outer::Inner::VeryInner' node");

        // Verify it's a scope_resolution node
        assert_eq!(
            target_node.kind(),
            "scope_resolution",
            "Node should be a scope_resolution"
        );

        // Test the fully qualified name determination
        let fqn = analyzer.determine_fully_qualified_name(&tree, target_node, Position::new(9, 15));

        // Verify the exact FQN
        assert_eq!(
            fqn, "Outer::Inner::VeryInner",
            "FQN should be exactly 'Outer::Inner::VeryInner'"
        );
    }
}
