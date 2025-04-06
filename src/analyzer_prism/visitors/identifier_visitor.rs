use crate::analyzer_prism::position::lsp_pos_to_prism_loc;
use crate::analyzer_prism::visitors::call_node;
use crate::indexer::types::fully_qualified_name::FullyQualifiedName;
use crate::indexer::types::ruby_constant::RubyConstant;
use crate::indexer::types::ruby_namespace::RubyNamespace;
use lsp_types::Position;
use ruby_prism::{
    visit_call_node, visit_class_node, visit_module_node, ArgumentsNode, CallNode, ClassNode,
    ConstantPathNode, ConstantReadNode, Location, ModuleNode, Visit,
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
                // Even if we don't set an identifier, we still need to visit child nodes
                // to ensure proper traversal of the AST
            } else if let Some(last_part) = namespaces.last() {
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

        // No need to call a default visit method here as ConstantReadNode is a leaf node
        // and doesn't have any children to visit
    }

    fn visit_call_node(&mut self, node: &CallNode) {
        // First, check if the cursor is in the receiver
        if call_node::handle_receiver(self, node) {
            return;
        }

        // Then, check if the cursor is in the arguments
        if call_node::handle_arguments(self, node) {
            return;
        }

        // Finally, check if the cursor is on the method name
        call_node::handle_method_name(self, node);

        visit_call_node(self, node);
    }

    fn visit_arguments_node(&mut self, node: &ArgumentsNode) {
        // Visit each argument to check if the cursor is on a constant within the arguments
        for arg in node.arguments().iter() {
            // First, check if the cursor is directly on this argument
            if self.is_position_in_location(&arg.location()) {
                // Visit the argument node to handle constants within it
                // We need to check what type of node it is and call the appropriate visitor method
                if let Some(constant_node) = arg.as_constant_read_node() {
                    self.visit_constant_read_node(&constant_node);
                } else if let Some(constant_path_node) = arg.as_constant_path_node() {
                    // Use our specialized handler for constant paths in arguments
                    call_node::handle_constant_path_in_argument(self, &constant_path_node);
                } else if let Some(call_node) = arg.as_call_node() {
                    // Handle call nodes in arguments (like Error::Type.new(...))
                    // First check if the cursor is on the receiver
                    if let Some(receiver) = call_node.receiver() {
                        if let Some(constant_path) = receiver.as_constant_path_node() {
                            if self.is_position_in_location(&constant_path.location()) {
                                call_node::handle_constant_path_receiver(self, &constant_path);
                                if self.identifier.is_some() {
                                    break;
                                }
                            }
                        }
                    }

                    // Then check if the cursor is in the arguments of this call node
                    if let Some(nested_args) = call_node.arguments() {
                        if self.is_position_in_location(&nested_args.location()) {
                            self.visit_arguments_node(&nested_args);
                            if self.identifier.is_some() {
                                break;
                            }
                        }
                    }
                }
                // Add more node types as needed

                // If we found an identifier, we can stop processing
                if self.identifier.is_some() {
                    break;
                }
            } else {
                // If the cursor is not directly on this argument, check if it's on a nested constant path
                // This is important for complex constant paths like GoshPosh::Platform::Shows::ShowActions::SEND_AUCTION_REQUEST
                if let Some(constant_path_node) = arg.as_constant_path_node() {
                    // Check if the cursor is within this constant path
                    if self.is_position_in_location(&constant_path_node.location()) {
                        // Use our specialized handler for constant paths in arguments
                        if call_node::handle_constant_path_in_argument(self, &constant_path_node) {
                            break;
                        }
                    }
                }
                // Also check for nested call nodes with constant paths in arguments
                else if let Some(call_node) = arg.as_call_node() {
                    // Check if the cursor is in the arguments of this call node
                    if let Some(nested_args) = call_node.arguments() {
                        for nested_arg in nested_args.arguments().iter() {
                            if let Some(constant_path) = nested_arg.as_constant_path_node() {
                                if self.is_position_in_location(&constant_path.location()) {
                                    if call_node::handle_constant_path_in_argument(
                                        self,
                                        &constant_path,
                                    ) {
                                        break;
                                    }
                                }
                            }
                        }

                        // If we found an identifier, we can stop processing
                        if self.identifier.is_some() {
                            break;
                        }
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

    #[test]
    fn test_constant_in_method_arguments() {
        // Test case: method(Foo::Bar) with cursor at Bar
        let mut visitor =
            IdentifierVisitor::new("method(Foo::Bar)".to_string(), Position::new(0, 12));
        let parse_result = ruby_prism::parse("method(Foo::Bar)".as_bytes());
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
    fn test_nested_constant_in_method_arguments() {
        // Test case: method(A::B::C::D::CONST) with cursor at CONST
        let mut visitor = IdentifierVisitor::new(
            "method(A::B::C::D::CONST)".to_string(),
            Position::new(0, 19),
        );
        let parse_result = ruby_prism::parse("method(A::B::C::D::CONST)".as_bytes());
        visitor.visit(&parse_result.node());

        // Ensure we found an identifier
        let identifier = visitor.identifier.expect("Expected to find an identifier");

        match identifier {
            FullyQualifiedName::Constant(parts, constant) => {
                assert_eq!(parts.len(), 4);
                assert_eq!(parts[0].to_string(), "A");
                assert_eq!(parts[1].to_string(), "B");
                assert_eq!(parts[2].to_string(), "C");
                assert_eq!(parts[3].to_string(), "D");
                assert_eq!(constant.to_string(), "CONST");
            }
            _ => panic!("Expected Constant FQN, got {:?}", identifier),
        }
    }

    #[test]
    fn test_nested_call_node() {
        // Test case: a.b.c with cursor at b
        let mut visitor = IdentifierVisitor::new("a.b.c".to_string(), Position::new(0, 2));
        let parse_result = ruby_prism::parse("a.b.c".as_bytes());
        visitor.visit(&parse_result.node());

        // Ensure we found an identifier
        let identifier = visitor.identifier.expect("Expected to find an identifier");

        match identifier {
            FullyQualifiedName::InstanceMethod(_, method) => {
                assert_eq!(method.to_string(), "b");
            }
            _ => panic!("Expected InstanceMethod FQN, got {:?}", identifier),
        }
    }

    #[test]
    fn test_deeply_nested_call_node() {
        // Test case: a.b.c.d.e with cursor at d
        let mut visitor = IdentifierVisitor::new("a.b.c.d.e".to_string(), Position::new(0, 6));
        let parse_result = ruby_prism::parse("a.b.c.d.e".as_bytes());
        visitor.visit(&parse_result.node());

        // Ensure we found an identifier
        let identifier = visitor.identifier.expect("Expected to find an identifier");

        match identifier {
            FullyQualifiedName::InstanceMethod(_, method) => {
                assert_eq!(method.to_string(), "d");
            }
            _ => panic!("Expected InstanceMethod FQN, got {:?}", identifier),
        }
    }

    #[test]
    fn test_constant_in_error_raising() {
        // Test case: raise Error::Type.new(Error::Messages::CONSTANT, Error::Codes::CODE)
        // with cursor at CONSTANT
        let code = "raise Error::Type.new(Error::Messages::CONSTANT, Error::Codes::CODE)";
        let mut visitor = IdentifierVisitor::new(code.to_string(), Position::new(0, 40));
        let parse_result = ruby_prism::parse(code.as_bytes());
        visitor.visit(&parse_result.node());

        // Ensure we found an identifier
        let identifier = visitor.identifier.expect("Expected to find an identifier");

        match identifier {
            FullyQualifiedName::Constant(parts, constant) => {
                assert_eq!(parts.len(), 2);
                assert_eq!(parts[0].to_string(), "Error");
                assert_eq!(parts[1].to_string(), "Messages");
                assert_eq!(constant.to_string(), "CONSTANT");
            }
            _ => panic!("Expected Constant FQN, got {:?}", identifier),
        }
    }

    #[test]
    fn test_complex_error_raising() {
        // Test case with complex nested constant paths in error raising:
        // raise RubyLSP::Core::Errors::ValidationError.new(
        //       RubyLSP::Core::Constants::ErrorMessages::INVALID_SYNTAX,
        //       RubyLSP::Core::Constants::ErrorCodes::PARSE_ERROR
        //     )
        let code = String::from("raise RubyLSP::Core::Errors::ValidationError.new(\n")
            + "          RubyLSP::Core::Constants::ErrorMessages::INVALID_SYNTAX,\n"
            + "          RubyLSP::Core::Constants::ErrorCodes::PARSE_ERROR\n"
            + "        )";

        // Test with cursor on INVALID_SYNTAX
        let mut visitor = IdentifierVisitor::new(code.to_string(), Position::new(1, 60));
        let parse_result = ruby_prism::parse(code.as_bytes());
        visitor.visit(&parse_result.node());

        // Ensure we found an identifier
        let identifier = visitor.identifier.expect("Expected to find an identifier");

        match identifier {
            FullyQualifiedName::Constant(parts, constant) => {
                assert_eq!(parts.len(), 4);
                assert_eq!(parts[0].to_string(), "RubyLSP");
                assert_eq!(parts[1].to_string(), "Core");
                assert_eq!(parts[2].to_string(), "Constants");
                assert_eq!(parts[3].to_string(), "ErrorMessages");
                assert_eq!(constant.to_string(), "INVALID_SYNTAX");
            }
            _ => panic!("Expected Constant FQN, got {:?}", identifier),
        }

        // Test with cursor on PARSE_ERROR
        let mut visitor = IdentifierVisitor::new(code.to_string(), Position::new(2, 55));
        let parse_result = ruby_prism::parse(code.as_bytes());
        visitor.visit(&parse_result.node());

        // Ensure we found an identifier
        let identifier = visitor.identifier.expect("Expected to find an identifier");

        match identifier {
            FullyQualifiedName::Constant(parts, constant) => {
                assert_eq!(parts.len(), 4);
                assert_eq!(parts[0].to_string(), "RubyLSP");
                assert_eq!(parts[1].to_string(), "Core");
                assert_eq!(parts[2].to_string(), "Constants");
                assert_eq!(parts[3].to_string(), "ErrorCodes");
                assert_eq!(constant.to_string(), "PARSE_ERROR");
            }
            _ => panic!("Expected Constant FQN, got {:?}", identifier),
        }
    }

    #[test]
    fn test_constant_in_block() {
        // Test case with constant paths in a block:
        // items.each do |item|
        //   raise Error::InvalidItem.new(
        //     Error::Messages::INVALID_ITEM,
        //     Error::Codes::ITEM_ERROR
        //   )
        // end
        let code = String::from("items.each do |item|\n")
            + "  raise Error::InvalidItem.new(\n"
            + "    Error::Messages::INVALID_ITEM,\n"
            + "    Error::Codes::ITEM_ERROR\n"
            + "  )\n"
            + "end";

        // Test with cursor on INVALID_ITEM
        let mut visitor = IdentifierVisitor::new(code.to_string(), Position::new(2, 25));
        let parse_result = ruby_prism::parse(code.as_bytes());
        visitor.visit(&parse_result.node());

        // Ensure we found an identifier
        let identifier = visitor.identifier.expect("Expected to find an identifier");

        match identifier {
            FullyQualifiedName::Constant(parts, constant) => {
                assert_eq!(parts.len(), 2);
                assert_eq!(parts[0].to_string(), "Error");
                assert_eq!(parts[1].to_string(), "Messages");
                assert_eq!(constant.to_string(), "INVALID_ITEM");
            }
            _ => panic!("Expected Constant FQN, got {:?}", identifier),
        }

        // Test with cursor on ITEM_ERROR
        let mut visitor = IdentifierVisitor::new(code.to_string(), Position::new(3, 20));
        let parse_result = ruby_prism::parse(code.as_bytes());
        visitor.visit(&parse_result.node());

        // Ensure we found an identifier
        let identifier = visitor.identifier.expect("Expected to find an identifier");

        match identifier {
            FullyQualifiedName::Constant(parts, constant) => {
                assert_eq!(parts.len(), 2);
                assert_eq!(parts[0].to_string(), "Error");
                assert_eq!(parts[1].to_string(), "Codes");
                assert_eq!(constant.to_string(), "ITEM_ERROR");
            }
            _ => panic!("Expected Constant FQN, got {:?}", identifier),
        }
    }
}
