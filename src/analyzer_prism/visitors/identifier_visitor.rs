use crate::analyzer_prism::position::lsp_pos_to_prism_loc;
use crate::indexer::types::fully_qualified_name::FullyQualifiedName;
use crate::indexer::types::ruby_constant::RubyConstant;
use crate::indexer::types::ruby_method::RubyMethod;
use crate::indexer::types::ruby_namespace::RubyNamespace;
use lsp_types::Position;
use ruby_prism::{
    visit_class_node, visit_module_node, CallNode, ClassNode, ConstantPathNode, ConstantReadNode,
    Location, ModuleNode, Visit,
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

    fn visit_call_node(&mut self, node: &CallNode) {
        // First, check if the cursor is on the receiver (if any)
        if let Some(receiver) = node.receiver() {
            // Check if the receiver is a constant or constant path
            if let Some(constant_node) = receiver.as_constant_read_node() {
                if self.is_position_in_location(&constant_node.location())
                    && self.identifier.is_none()
                {
                    // Cursor is on the constant part of a method call (e.g., Foo.bar)
                    let constant_name =
                        String::from_utf8_lossy(constant_node.name().as_slice()).to_string();

                    match RubyConstant::new(&constant_name) {
                        Ok(constant) => {
                            self.identifier =
                                Some(FullyQualifiedName::constant(Vec::new(), constant));
                        }
                        Err(_) => {
                            let namespace = RubyNamespace::new(constant_name.as_str()).unwrap();
                            self.identifier = Some(FullyQualifiedName::namespace(vec![namespace]));
                        }
                    }

                    self.ancestors = self.namespace_stack.clone();
                    return;
                }
            } else if let Some(constant_path) = receiver.as_constant_path_node() {
                // Check if the cursor is anywhere within the constant path
                if self.is_position_in_location(&constant_path.location())
                    && self.identifier.is_none()
                {
                    // Cursor is on the constant path part of a method call (e.g., Foo::Bar.baz)
                    // Use the existing constant path handling logic to determine which part of the path the cursor is on
                    let (mut namespaces, is_root_constant) =
                        self.determine_const_path_target(&constant_path);

                    // If namespaces is empty, the cursor is on a scope resolution operator (::)
                    // In that case, we don't want to set an identifier
                    if !namespaces.is_empty() {
                        if let Some(last_part) = namespaces.last() {
                            let last_part_str = last_part.to_string();

                            match RubyConstant::new(&last_part_str) {
                                Ok(constant) => {
                                    namespaces.pop(); // Remove the last part (constant name)
                                    self.identifier =
                                        Some(FullyQualifiedName::constant(namespaces, constant));
                                }
                                Err(_) => {
                                    self.identifier =
                                        Some(FullyQualifiedName::namespace(namespaces));
                                }
                            }

                            if is_root_constant {
                                self.ancestors = vec![];
                            } else {
                                self.ancestors = self.namespace_stack.clone();
                            }
                        }
                    }
                    return;
                }
            }
        }

        // If we're not on the receiver, check if the cursor is on the method name
        if let Some(message_loc) = node.message_loc() {
            if self.is_position_in_location(&message_loc) && self.identifier.is_none() {
                // Extract the method name
                let method_name_id = node.name();
                let method_name_bytes = method_name_id.as_slice();
                let method_name_str = String::from_utf8_lossy(method_name_bytes).to_string();

                // Try to create a RubyMethod from the name
                if let Ok(method_name) = RubyMethod::try_from(method_name_str.as_ref()) {
                    // Determine the method type based on the receiver and context
                    if let Some(receiver) = node.receiver() {
                        // Check if the receiver is a constant (class/module)
                        if let Some(constant_node) = receiver.as_constant_read_node() {
                            // It's a class method call on a constant
                            let constant_name =
                                String::from_utf8_lossy(constant_node.name().as_slice())
                                    .to_string();
                            let namespace = RubyNamespace::new(&constant_name).unwrap();

                            // Check if this might be a module function
                            // For now, we'll use class_method as the default for constant receivers
                            // The actual determination of ModuleFunc will happen in the indexer
                            self.identifier = Some(FullyQualifiedName::module_method(
                                vec![namespace.clone()],
                                method_name.clone(),
                            ));

                            self.ancestors = vec![]; // Class/module methods are absolute references
                        } else if let Some(constant_path) = receiver.as_constant_path_node() {
                            // It's a class method call on a namespaced constant
                            let (namespaces, is_root_constant) =
                                self.determine_const_path_target(&constant_path);

                            // Check if this might be a module function
                            // For now, we'll use module_method as the default for constant path receivers
                            // The actual determination of ModuleFunc will happen in the indexer
                            self.identifier = Some(FullyQualifiedName::module_method(
                                namespaces.clone(),
                                method_name.clone(),
                            ));

                            if is_root_constant {
                                self.ancestors = vec![];
                            } else {
                                self.ancestors = self.namespace_stack.clone();
                            }
                        } else {
                            // It's an instance method call on some other expression
                            // For now, we'll use the current namespace
                            self.identifier = Some(FullyQualifiedName::instance_method(
                                self.namespace_stack.clone(),
                                method_name,
                            ));
                            self.ancestors = self.namespace_stack.clone();
                        }
                    } else {
                        // No receiver, it's a local method call in the current context
                        // This could be either an instance method or a module function
                        // For now, we'll use instance_method as the default
                        self.identifier = Some(FullyQualifiedName::instance_method(
                            self.namespace_stack.clone(),
                            method_name.clone(),
                        ));

                        // Also check if it might be a module function in the current namespace
                        if !self.namespace_stack.is_empty() {
                            self.identifier = Some(FullyQualifiedName::module_method(
                                self.namespace_stack.clone(),
                                method_name,
                            ));
                        }

                        self.ancestors = self.namespace_stack.clone();
                    }
                }
            }
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

    #[test]
    fn test_constant_in_method_call() {
        // Test case: Foo.bar with cursor at Foo
        let mut visitor = IdentifierVisitor::new("Foo.bar".to_string(), Position::new(0, 1));
        let parse_result = ruby_prism::parse("Foo.bar".as_bytes());
        visitor.visit(&parse_result.node());

        // Ensure we found an identifier
        let identifier = visitor.identifier.expect("Expected to find an identifier");

        match identifier {
            FullyQualifiedName::Namespace(parts) => {
                assert_eq!(parts.len(), 1);
                assert_eq!(parts[0].to_string(), "Foo");
            }
            _ => panic!("Expected Namespace FQN, got {:?}", identifier),
        }
    }

    #[test]
    fn test_constant_path_in_method_call() {
        // Test case: Foo::Bar.baz with cursor at Bar
        let mut visitor = IdentifierVisitor::new("Foo::Bar.baz".to_string(), Position::new(0, 6));
        let parse_result = ruby_prism::parse("Foo::Bar.baz".as_bytes());
        visitor.visit(&parse_result.node());

        // Ensure we found an identifier
        let identifier = visitor.identifier.expect("Expected to find an identifier");

        match identifier {
            FullyQualifiedName::Namespace(parts) => {
                assert_eq!(parts.len(), 2);
                assert_eq!(parts[0].to_string(), "Foo");
                assert_eq!(parts[1].to_string(), "Bar");
            }
            _ => panic!("Expected Namespace FQN, got {:?}", identifier),
        }
    }

    #[test]
    fn test_module_method_call() {
        // Test case: Foo::Bar.baz with cursor at baz (module method call)
        let mut visitor = IdentifierVisitor::new("Foo::Bar.baz".to_string(), Position::new(0, 10));
        let parse_result = ruby_prism::parse("Foo::Bar.baz".as_bytes());
        visitor.visit(&parse_result.node());

        // Ensure we found an identifier
        let identifier = visitor.identifier.expect("Expected to find an identifier");

        match identifier {
            FullyQualifiedName::ModuleMethod(parts, method) => {
                assert_eq!(parts.len(), 2);
                assert_eq!(parts[0].to_string(), "Foo");
                assert_eq!(parts[1].to_string(), "Bar");
                assert_eq!(method.to_string(), "baz");
            }
            _ => panic!("Expected ModuleMethod FQN, got {:?}", identifier),
        }
    }

    #[test]
    fn test_namespace_in_method_call() {
        // Test case: Foo::Bar::Baz.foo with cursor at Bar
        let mut visitor =
            IdentifierVisitor::new("Foo::Bar::Baz.foo".to_string(), Position::new(0, 6));
        let parse_result = ruby_prism::parse("Foo::Bar::Baz.foo".as_bytes());
        visitor.visit(&parse_result.node());

        // Ensure we found an identifier
        let identifier = visitor.identifier.expect("Expected to find an identifier");

        match identifier {
            FullyQualifiedName::Namespace(parts) => {
                assert_eq!(parts.len(), 2);
                assert_eq!(parts[0].to_string(), "Foo");
                assert_eq!(parts[1].to_string(), "Bar");
            }
            _ => panic!("Expected Namespace FQN, got {:?}", identifier),
        }
    }

    #[test]
    fn test_constant_in_nested_expression() {
        // Test case: Foo::Bar::Baz::ABC with cursor at ABC
        let mut visitor =
            IdentifierVisitor::new("Foo::Bar::Baz::ABC".to_string(), Position::new(0, 15));
        let parse_result = ruby_prism::parse("Foo::Bar::Baz::ABC".as_bytes());
        visitor.visit(&parse_result.node());

        // Ensure we found an identifier
        let identifier = visitor.identifier.expect("Expected to find an identifier");

        match identifier {
            FullyQualifiedName::Constant(parts, constant) => {
                assert_eq!(parts.len(), 3);
                assert_eq!(parts[0].to_string(), "Foo");
                assert_eq!(parts[1].to_string(), "Bar");
                assert_eq!(parts[2].to_string(), "Baz");
                assert_eq!(constant.to_string(), "ABC");
            }
            _ => panic!("Expected Constant FQN, got {:?}", identifier),
        }
    }
}
