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
                    // Handle nested constants like A::B
                    let parent = node.parent().unwrap();
                    let scope = parent.child(0);
                    if let Some(scope_node) = scope {
                        if scope_node.kind() == "constant" {
                            let scope_text = self.get_node_text(scope_node);
                            return format!("{}::{}", scope_text, node_text);
                        }
                    }
                }
                node_text
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
}
