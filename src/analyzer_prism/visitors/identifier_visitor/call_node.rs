use ruby_prism::CallNode;

use crate::{
    analyzer_prism::{utils, Identifier, ReceiverKind},
    indexer::entry::MethodKind,
    types::{ruby_method::RubyMethod, ruby_namespace::RubyConstant},
};

use super::{IdentifierType, IdentifierVisitor};

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

                // Determine receiver kind and receiver information
                let (receiver_kind, receiver, method_kind) =
                    if let Some(receiver_node) = node.receiver() {
                        if receiver_node.as_self_node().is_some() {
                            (ReceiverKind::SelfReceiver, None, MethodKind::Instance)
                        } else if let Some(constant_read) = receiver_node.as_constant_read_node() {
                            let name = String::from_utf8_lossy(constant_read.name().as_slice())
                                .to_string();
                            let constant = RubyConstant::new(&name).unwrap();
                            (
                                ReceiverKind::Constant,
                                Some(vec![constant]),
                                MethodKind::Class,
                            )
                        } else if let Some(constant_path) = receiver_node.as_constant_path_node() {
                            let mut namespaces = Vec::new();
                            utils::collect_namespaces(&constant_path, &mut namespaces);
                            (ReceiverKind::Constant, Some(namespaces), MethodKind::Class)
                        } else {
                            (ReceiverKind::Expr, None, MethodKind::Instance)
                        }
                    } else {
                        (ReceiverKind::None, None, MethodKind::Instance)
                    };

                let method = RubyMethod::new(&method_name, method_kind).unwrap();

                self.set_result(
                    Some(Identifier::RubyMethod {
                        namespace: self.scope_tracker.get_ns_stack(),
                        receiver_kind,
                        receiver,
                        iden: method,
                    }),
                    Some(IdentifierType::MethodCall),
                    self.scope_tracker.get_ns_stack(),
                    self.scope_tracker.get_lv_stack(),
                );
            }
        }
    }

    pub fn process_call_node_exit(&mut self, _node: &CallNode) {
        // No cleanup needed for call nodes
    }
}
