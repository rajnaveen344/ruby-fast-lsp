use crate::analyzer_prism::position::lsp_pos_to_prism_loc;
use crate::indexer::types::fully_qualified_name::FullyQualifiedName;
use crate::indexer::types::ruby_constant::RubyConstant;
use crate::indexer::types::ruby_namespace::RubyNamespace;
use lsp_types::Position;
use ruby_prism::{
    visit_class_node, visit_module_node, ClassNode, ConstantPathNode, ConstantReadNode, Location,
    ModuleNode, Visit,
};

/// Visitor for finding identifiers at a specific position
pub struct IdentifierVisitor {
    code: String,
    position: Position,
    pub identifier: Option<FullyQualifiedName>,
    pub ancestors: Vec<RubyNamespace>,
    pub namespace_stack: Vec<RubyNamespace>,
}

impl IdentifierVisitor {
    pub fn new(code: String, position: Position) -> Self {
        Self {
            code,
            position,
            identifier: None,
            ancestors: Vec::new(),
            namespace_stack: Vec::new(),
        }
    }

    pub fn is_position_in_location(&self, location: &Location) -> bool {
        let position_offset = lsp_pos_to_prism_loc(self.position, &self.code);

        let start_offset = location.start_offset();
        let end_offset = location.end_offset();

        position_offset >= start_offset && position_offset < end_offset
    }

    /// Based on a constant node target, a constant path node parent and a position, this method will find the exact
    /// portion of the constant path that matches the requested position, for higher precision in hover and
    /// definition. For example:
    ///
    /// ```ruby
    /// Foo::Bar::BAZ
    ///           ^ Going to definition here should go to Foo::Bar::Baz
    ///      ^ Going to definition here should go to Foo::Bar - Parent of ConstantPathNode BAZ
    /// ^ Going to definition here should go to Foo - Parent of ConstantPathNode Bar
    /// ```
    pub fn determine_const_path_target(&self, target: &ConstantPathNode) -> Vec<RubyNamespace> {
        // Get the position offset
        let position_offset = lsp_pos_to_prism_loc(self.position, &self.code);

        // Get the code being parsed
        let code = self.code.as_bytes();

        // For "Foo::Bar::Baz", we need to determine which part the cursor is on
        let start = target.location().start_offset();
        let end = target.location().end_offset();
        let target_text = &code[start..end];
        let target_str = String::from_utf8_lossy(target_text).to_string();

        // Check if we're dealing with a constant path with multiple parts
        if target_str.contains("::") {
            // Split the path by "::"
            let parts: Vec<&str> = target_str.split("::").collect();

            // Find which part the cursor is on
            let mut current_offset = target.location().start_offset();
            let mut cursor_part_index = 0;

            for (i, part) in parts.iter().enumerate() {
                let part_end = current_offset + part.len();

                // Check if cursor is within this part
                if position_offset >= current_offset && position_offset < part_end {
                    cursor_part_index = i;
                    break;
                }

                // Move past this part and the "::" separator
                current_offset = part_end + 2; // +2 for "::"
            }

            // Create namespaces for parts up to and including the cursor part
            let mut namespaces = Vec::new();
            for i in 0..=cursor_part_index {
                namespaces.push(RubyNamespace::new(parts[i]).unwrap());
            }

            return namespaces;
        }

        // For simple cases or when we can't determine the parts, fall back to the original behavior
        let mut namespaces = Vec::new();

        // Get the name of the target node
        let name = String::from_utf8_lossy(target.name().unwrap().as_slice()).to_string();

        // If this is a simple constant (not a path), just return it
        if !target_str.contains("::") {
            namespaces.push(RubyNamespace::new(&name).unwrap());
            return namespaces;
        }

        // For other cases, try to build the path
        let mut parts = Vec::new();
        parts.push(name);

        // Check for parent nodes
        if let Some(parent) = target.parent() {
            if let Some(cr) = parent.as_constant_read_node() {
                let parent_name = String::from_utf8_lossy(cr.name().as_slice()).to_string();
                parts.push(parent_name);
            }
        }

        // Reverse to get the correct order
        parts.reverse();

        // Convert to namespaces
        for part in parts {
            namespaces.push(RubyNamespace::new(&part).unwrap());
        }

        namespaces
    }
}

impl Visit<'_> for IdentifierVisitor {
    fn visit_class_node(&mut self, node: &ClassNode) {
        // Add the class name to the namespace stack regardless of cursor position
        let name = String::from_utf8_lossy(&node.name().as_slice());
        self.namespace_stack
            .push(RubyNamespace::new(&name.to_string()).unwrap());

        // Visit the class body
        visit_class_node(self, &node);

        // Remove the class name from the namespace stack
        self.namespace_stack.pop();
    }

    fn visit_module_node(&mut self, node: &ModuleNode) {
        // Add the module name to the namespace stack regardless of cursor position
        let name = String::from_utf8_lossy(&node.name().as_slice());
        self.namespace_stack
            .push(RubyNamespace::new(&name.to_string()).unwrap());

        // Visit the module body
        visit_module_node(self, &node);

        // Remove the module name from the namespace stack
        self.namespace_stack.pop();
    }

    fn visit_constant_path_node(&mut self, node: &ConstantPathNode) {
        if self.is_position_in_location(&node.location()) && self.identifier.is_none() {
            // Get all namespace parts
            let mut namespaces = self.determine_const_path_target(node);

            if let Some(last_part) = namespaces.last() {
                let last_part_str = last_part.to_string();

                match RubyConstant::new(&last_part_str) {
                    Ok(constant) => {
                        namespaces.pop(); // Remove the last part (constant name)
                        self.identifier = Some(FullyQualifiedName::constant(namespaces, constant));
                    }
                    Err(_) => {
                        self.identifier = Some(FullyQualifiedName::namespace(namespaces));
                    }
                }

                self.ancestors = self.namespace_stack.clone();
            }
        }
    }

    fn visit_constant_read_node(&mut self, node: &ConstantReadNode) {
        if self.is_position_in_location(&node.location()) && self.identifier.is_none() {
            let constant_name = String::from_utf8_lossy(node.name().as_slice()).to_string();

            match RubyConstant::new(&constant_name) {
                Ok(constant) => {
                    self.identifier = Some(FullyQualifiedName::constant(Vec::new(), constant));
                }
                Err(_) => {
                    let namespace = RubyNamespace::new(constant_name.as_str()).unwrap();
                    self.identifier = Some(FullyQualifiedName::namespace(vec![namespace]));
                }
            }

            self.ancestors = self.namespace_stack.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::Position;

    // Helper function to test the full visitor behavior
    fn test_visitor(code: &str, position: Position, expected_parts: Vec<&str>) {
        let mut visitor = IdentifierVisitor::new(code.to_string(), position);
        let parse_result = ruby_prism::parse(code.as_bytes());

        // Use the full visitor pattern
        visitor.visit(&parse_result.node());

        // Check the identifier was found
        assert!(
            visitor.identifier.is_some(),
            "Expected to find an identifier at position {:?}",
            position
        );

        // Get the parts from the identifier - could be either a namespace or a constant
        let parts = match &visitor.identifier.unwrap() {
            FullyQualifiedName::Namespace(parts) => parts.clone(),
            FullyQualifiedName::Constant(parts, _) => parts.clone(),
            _ => panic!("Expected a Namespace or Constant FQN"),
        };

        // Verify the parts match
        assert_eq!(
            parts.len(),
            expected_parts.len(),
            "Namespace parts count mismatch"
        );
        for (i, expected_part) in expected_parts.iter().enumerate() {
            assert_eq!(
                parts[i].to_string(),
                *expected_part,
                "Namespace part at index {} mismatch",
                i
            );
        }
    }

    #[test]
    fn test_simple_constant_path() {
        // Test case: Foo::Bar with cursor at Bar
        test_visitor("Foo::Bar", Position::new(0, 6), vec!["Foo", "Bar"]);
    }

    #[test]
    fn test_nested_constant_path_at_middle() {
        // Test case: Foo::Bar::Baz with cursor at Bar
        test_visitor("Foo::Bar::Baz", Position::new(0, 6), vec!["Foo", "Bar"]);
    }

    #[test]
    fn test_nested_constant_path_at_first() {
        // Test case: Foo::Bar::Baz with cursor at Foo
        test_visitor("Foo::Bar::Baz", Position::new(0, 1), vec!["Foo"]);
    }

    #[test]
    fn test_nested_constant_path_at_last() {
        // Test case: Foo::Bar::Baz with cursor at Baz
        test_visitor(
            "Foo::Bar::Baz",
            Position::new(0, 11),
            vec!["Foo", "Bar", "Baz"],
        );
    }
}
