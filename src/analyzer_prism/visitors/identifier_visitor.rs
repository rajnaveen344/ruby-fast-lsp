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
    pub fn determine_const_path_target(
        &self,
        target: &ConstantPathNode,
    ) -> (Vec<RubyNamespace>, bool) {
        // Extract the full constant path text and cursor position
        let position_offset = lsp_pos_to_prism_loc(self.position, &self.code);
        let code = self.code.as_bytes();
        let start = target.location().start_offset();
        let end = target.location().end_offset();
        let target_str = String::from_utf8_lossy(&code[start..end]).to_string();

        // Check if this is a root constant path (starts with ::)
        let is_root_constant = target_str.starts_with("::");

        // Handle constant paths with multiple parts (e.g., Foo::Bar::Baz or ::GLOBAL_CONSTANT)
        if target_str.contains("::") {
            // Split the path into parts
            let parts: Vec<&str> = target_str.split("::").collect();

            // Find which part the cursor is on
            let mut current_offset = start;
            let mut cursor_part_index = parts.len() - 1; // Default to last part
            let mut on_scope_resolution = false;

            for (i, part) in parts.iter().enumerate() {
                let part_end = current_offset + part.len();

                // Check if cursor is within this part
                if position_offset >= current_offset && position_offset < part_end {
                    cursor_part_index = i;
                    break;
                }

                // Check if cursor is on the scope resolution operator ("::") between parts
                if i < parts.len() - 1 {
                    let scope_start = part_end;
                    let scope_end = scope_start + 2; // "::" is 2 characters

                    if position_offset >= scope_start && position_offset < scope_end {
                        // Mark that we're on a scope resolution operator
                        on_scope_resolution = true;
                        break;
                    }
                }

                current_offset = part_end + 2; // +2 for "::"
            }

            // If cursor is on scope resolution operator, return empty vector
            if on_scope_resolution {
                return (Vec::new(), false);
            }

            // Handle root constants (starting with ::)
            if is_root_constant {
                // If we're on the constant part (after ::), we need special handling
                if parts[0].is_empty() {
                    // For root constants, we need to skip the empty first segment
                    // and return the remaining parts
                    let result = parts[1..=cursor_part_index]
                        .iter()
                        .filter(|part| !part.is_empty()) // Skip empty parts
                        .map(|part| RubyNamespace::new(part).unwrap())
                        .collect::<Vec<RubyNamespace>>();

                    return (result, true);
                }
            }

            // For regular constants (not root), or if we're on a part before the constant
            let result = parts[0..=cursor_part_index]
                .iter()
                .filter(|part| !part.is_empty()) // Skip empty parts
                .map(|part| RubyNamespace::new(part).unwrap())
                .collect::<Vec<RubyNamespace>>();

            return (result, is_root_constant);
        }

        // Handle simple constant (not a path)
        let name = String::from_utf8_lossy(target.name().unwrap().as_slice()).to_string();
        (vec![RubyNamespace::new(&name).unwrap()], is_root_constant)
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
            let (mut namespaces, is_root_constant) = self.determine_const_path_target(node);

            // Handle the case when cursor is on scope resolution operator
            if namespaces.is_empty() {
                self.identifier = None;
                self.ancestors = vec![];
                return;
            }

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

                if is_root_constant {
                    self.ancestors = vec![];
                } else {
                    self.ancestors = self.namespace_stack.clone();
                }
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

        // If expected_parts is empty and we're on a scope resolution operator,
        // we expect identifier to be None
        if expected_parts.is_empty() {
            assert!(
                visitor.identifier.is_none(),
                "Expected identifier to be None at position {:?}",
                position
            );
            return;
        }

        // Otherwise, check the identifier was found
        assert!(
            visitor.identifier.is_some(),
            "Expected to find an identifier at position {:?}",
            position
        );

        // Get the identifier for further processing
        let identifier = visitor.identifier.as_ref().unwrap();

        // Special case for root constants
        if code.starts_with("::") {
            match identifier {
                FullyQualifiedName::Constant(parts, constant) => {
                    // For root constants, we expect an empty namespace vector
                    if expected_parts.len() == 1 {
                        // For direct root constants like ::GLOBAL_CONSTANT
                        assert_eq!(
                            parts.len(),
                            0,
                            "Expected empty namespace vector for root constant"
                        );
                        assert_eq!(
                            constant.to_string(),
                            expected_parts[0],
                            "Expected constant name to match"
                        );
                    } else {
                        // For nested root constants like ::Foo::Bar::CONSTANT
                        assert_eq!(
                            parts.len(),
                            expected_parts.len() - 1,
                            "Namespace parts count mismatch for root constant path"
                        );
                        for (i, expected_part) in expected_parts
                            .iter()
                            .take(expected_parts.len() - 1)
                            .enumerate()
                        {
                            assert_eq!(
                                parts[i].to_string(),
                                *expected_part,
                                "Namespace part at index {} mismatch",
                                i
                            );
                        }
                        assert_eq!(
                            constant.to_string(),
                            expected_parts[expected_parts.len() - 1],
                            "Expected constant name to match"
                        );
                    }
                    return;
                }
                _ => {}
            }
        }

        // Get the parts from the identifier - could be either a namespace or a constant
        let parts = match identifier {
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

    #[test]
    fn test_nested_constant_path_at_scope_resolution() {
        // Test case: Foo::Bar::Baz with cursor at first "::"
        // Empty vector indicates we expect identifier to be None
        test_visitor("Foo::Bar::Baz", Position::new(0, 3), vec![]);
    }

    #[test]
    fn test_nested_constant_path_at_scope_resolution_2() {
        // Test case: Foo::Bar::Baz with cursor at second "::"
        // Empty vector indicates we expect identifier to be None
        test_visitor("Foo::Bar::Baz", Position::new(0, 8), vec![]);
    }

    #[test]
    fn test_root_constant_read_node() {
        test_visitor(
            "::GLOBAL_CONSTANT",
            Position::new(0, 2),
            vec!["GLOBAL_CONSTANT"],
        );
    }

    #[test]
    fn test_root_constant_path_node() {
        test_visitor(
            "::Foo::Bar::GLOBAL_CONSTANT",
            Position::new(0, 12),
            vec!["Foo", "Bar", "GLOBAL_CONSTANT"],
        );
    }
}
