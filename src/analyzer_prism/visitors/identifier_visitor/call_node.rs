use ruby_prism::{CallNode, Node};

use crate::{
    analyzer_prism::{utils, Identifier, MethodReceiver},
    indexer::entry::NamespaceKind,
    inferrer::r#type::ruby::RubyType,
    types::{ruby_method::RubyMethod, ruby_namespace::RubyConstant},
};

use super::{IdentifierType, IdentifierVisitor};

/// Extract the receiver from a CallNode recursively
fn extract_receiver_from_call_node(call_node: &CallNode) -> MethodReceiver {
    if let Some(receiver_node) = call_node.receiver() {
        extract_receiver_from_node(&receiver_node)
    } else {
        MethodReceiver::None
    }
}

/// Extract the receiver type from any Node
fn extract_receiver_from_node(node: &Node) -> MethodReceiver {
    if node.as_self_node().is_some() {
        MethodReceiver::SelfReceiver
    } else if let Some(constant_read) = node.as_constant_read_node() {
        let name = String::from_utf8_lossy(constant_read.name().as_slice()).to_string();
        if let Ok(constant) = RubyConstant::new(&name) {
            MethodReceiver::Constant(vec![constant])
        } else {
            MethodReceiver::Expression
        }
    } else if let Some(constant_path) = node.as_constant_path_node() {
        let mut namespaces = Vec::new();
        utils::collect_namespaces(&constant_path, &mut namespaces);
        MethodReceiver::Constant(namespaces)
    } else if let Some(local_var) = node.as_local_variable_read_node() {
        let var_name = String::from_utf8_lossy(local_var.name().as_slice()).to_string();
        MethodReceiver::LocalVariable(var_name)
    } else if let Some(instance_var) = node.as_instance_variable_read_node() {
        let var_name = String::from_utf8_lossy(instance_var.name().as_slice()).to_string();
        MethodReceiver::InstanceVariable(var_name)
    } else if let Some(class_var) = node.as_class_variable_read_node() {
        let var_name = String::from_utf8_lossy(class_var.name().as_slice()).to_string();
        MethodReceiver::ClassVariable(var_name)
    } else if let Some(global_var) = node.as_global_variable_read_node() {
        let var_name = String::from_utf8_lossy(global_var.name().as_slice()).to_string();
        MethodReceiver::GlobalVariable(var_name)
    } else if let Some(call_node) = node.as_call_node() {
        // Nested method call
        let inner_method_name = String::from_utf8_lossy(call_node.name().as_slice()).to_string();
        let inner_receiver = extract_receiver_from_call_node(&call_node);
        MethodReceiver::MethodCall {
            inner_receiver: Box::new(inner_receiver),
            method_name: inner_method_name,
        }
    } else if let Some(literal_type) = infer_literal_receiver_type(node) {
        MethodReceiver::Literal(literal_type)
    } else {
        MethodReceiver::Expression
    }
}

/// Infer the RubyType from a literal AST node used as a receiver
fn infer_literal_receiver_type(node: &Node) -> Option<RubyType> {
    use crate::inferrer::r#type::literal::LiteralAnalyzer;

    if node.as_string_node().is_some() || node.as_interpolated_string_node().is_some() {
        return Some(RubyType::string());
    }
    if node.as_integer_node().is_some() {
        return Some(RubyType::integer());
    }
    if node.as_float_node().is_some() {
        return Some(RubyType::float());
    }
    if node.as_symbol_node().is_some() {
        return Some(RubyType::symbol());
    }
    if node.as_array_node().is_some() || node.as_hash_node().is_some() {
        let analyzer = LiteralAnalyzer::new();
        return analyzer.analyze_literal(node);
    }
    if node.as_true_node().is_some() {
        return Some(RubyType::true_class());
    }
    if node.as_false_node().is_some() {
        return Some(RubyType::false_class());
    }
    if node.as_nil_node().is_some() {
        return Some(RubyType::nil_class());
    }
    None
}

impl IdentifierVisitor {
    pub fn process_call_node_entry(&mut self, node: &CallNode) {
        if self.is_result_set() {
            return;
        }

        if !self.is_position_in_location(&node.location()) {
            return;
        }

        // Check if cursor is in the arguments - if so, skip matching the method call
        // and let the argument visitors (like constant_read_node) handle it
        if let Some(arguments) = node.arguments() {
            if self.is_position_in_location(&arguments.location()) {
                // Cursor is in arguments, don't match the method call
                return;
            }
        }

        // Check if position is on the method name
        if let Some(message_loc) = node.message_loc() {
            if self.is_position_in_location(&message_loc) {
                let method_name = String::from_utf8_lossy(node.name().as_slice()).to_string();

                // Skip method names that don't follow Ruby method naming conventions
                if !RubyMethod::is_valid_ruby_method_name(&method_name) {
                    return;
                }

                // Determine receiver and namespace kind
                let (receiver, namespace_kind) = if let Some(receiver_node) = node.receiver() {
                    if receiver_node.as_self_node().is_some() {
                        // Use context from scope tracker for self receiver
                        let kind = self.infer_namespace_kind_from_context();
                        (MethodReceiver::SelfReceiver, kind)
                    } else if let Some(constant_read) = receiver_node.as_constant_read_node() {
                        let name =
                            String::from_utf8_lossy(constant_read.name().as_slice()).to_string();
                        let constant = RubyConstant::new(&name).unwrap();
                        (
                            MethodReceiver::Constant(vec![constant]),
                            NamespaceKind::Singleton,
                        )
                    } else if let Some(constant_path) = receiver_node.as_constant_path_node() {
                        let mut namespaces = Vec::new();
                        utils::collect_namespaces(&constant_path, &mut namespaces);
                        (
                            MethodReceiver::Constant(namespaces),
                            NamespaceKind::Singleton,
                        )
                    } else if let Some(local_var) = receiver_node.as_local_variable_read_node() {
                        let var_name =
                            String::from_utf8_lossy(local_var.name().as_slice()).to_string();
                        (
                            MethodReceiver::LocalVariable(var_name),
                            NamespaceKind::Instance,
                        )
                    } else if let Some(instance_var) =
                        receiver_node.as_instance_variable_read_node()
                    {
                        let var_name =
                            String::from_utf8_lossy(instance_var.name().as_slice()).to_string();
                        (
                            MethodReceiver::InstanceVariable(var_name),
                            NamespaceKind::Instance,
                        )
                    } else if let Some(class_var) = receiver_node.as_class_variable_read_node() {
                        let var_name =
                            String::from_utf8_lossy(class_var.name().as_slice()).to_string();
                        (
                            MethodReceiver::ClassVariable(var_name),
                            NamespaceKind::Instance,
                        )
                    } else if let Some(global_var) = receiver_node.as_global_variable_read_node() {
                        let var_name =
                            String::from_utf8_lossy(global_var.name().as_slice()).to_string();
                        (
                            MethodReceiver::GlobalVariable(var_name),
                            NamespaceKind::Instance,
                        )
                    } else if let Some(call_node) = receiver_node.as_call_node() {
                        // Method call receiver, e.g., `user.name` in `user.name.upcase`
                        let inner_method_name =
                            String::from_utf8_lossy(call_node.name().as_slice()).to_string();
                        let inner_receiver = extract_receiver_from_call_node(&call_node);
                        (
                            MethodReceiver::MethodCall {
                                inner_receiver: Box::new(inner_receiver),
                                method_name: inner_method_name,
                            },
                            NamespaceKind::Instance,
                        )
                    } else {
                        (MethodReceiver::Expression, NamespaceKind::Instance)
                    }
                } else {
                    // No receiver - use context from scope tracker
                    let kind = self.infer_namespace_kind_from_context();
                    (MethodReceiver::None, kind)
                };

                // Note: namespace_kind is used by the query layer to determine which ancestor chain to use
                let _ = namespace_kind; // Mark as intentionally unused for now
                let method = RubyMethod::new(&method_name).unwrap();

                self.set_result(
                    Some(Identifier::RubyMethod {
                        namespace: self.scope_tracker.get_ns_stack(),
                        receiver,
                        iden: method,
                    }),
                    Some(IdentifierType::MethodCall),
                    self.scope_tracker.get_ns_stack(),
                    Some(0),
                );
            }
        }
    }

    /// Determines the NamespaceKind for a call with no explicit class receiver.
    /// Uses scope context to distinguish between:
    /// - Instance namespace (in instance methods or at top-level)
    /// - Singleton namespace (in class body)
    fn infer_namespace_kind_from_context(&self) -> NamespaceKind {
        self.scope_tracker.current_method_context()
    }

    pub fn process_call_node_exit(&mut self, _node: &CallNode) {
        // No cleanup needed for call nodes
    }
}
