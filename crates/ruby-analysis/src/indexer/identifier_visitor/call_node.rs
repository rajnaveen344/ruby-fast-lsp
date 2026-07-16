use ruby_prism::{CallNode, Node};

use crate::core::{NamespaceKind, RubyConstant, RubyMethod};
use crate::inference::RubyType;
use crate::{analyzer_utils as utils, Identifier, LVScopeKind, MethodReceiver};

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
        if let Some(receiver) = extract_const_get_receiver_from_call_node(&call_node) {
            return receiver;
        }
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

fn extract_const_get_receiver_from_call_node(call_node: &CallNode) -> Option<MethodReceiver> {
    if call_node.name().as_slice() != b"const_get" {
        return None;
    }
    let receiver_node = call_node.receiver()?;
    let MethodReceiver::Constant(mut namespace) = extract_receiver_from_node(&receiver_node) else {
        return None;
    };
    let (constant_name, _) = call_arg_name_and_location(call_node, 0)?;
    let Ok(constant) = RubyConstant::new(&constant_name) else {
        return None;
    };
    namespace.push(constant);
    Some(MethodReceiver::Constant(namespace))
}

/// Infer the RubyType from a literal AST node used as a receiver
fn infer_literal_receiver_type(node: &Node) -> Option<RubyType> {
    use crate::inference::r#type::literal::LiteralAnalyzer;

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

fn call_arg_name_and_location<'a>(
    node: &CallNode<'a>,
    index: usize,
) -> Option<(String, ruby_prism::Location<'a>)> {
    let arguments = node.arguments()?;
    let arg = arguments.arguments().iter().nth(index)?;
    call_arg_name_and_location_from_node(&arg)
}

fn call_arg_name_and_location_from_node<'a>(
    arg: &ruby_prism::Node<'a>,
) -> Option<(String, ruby_prism::Location<'a>)> {
    if let Some(symbol) = arg.as_symbol_node() {
        return Some((
            String::from_utf8_lossy(symbol.unescaped()).to_string(),
            symbol.location(),
        ));
    }
    if let Some(string) = arg.as_string_node() {
        return Some((
            String::from_utf8_lossy(string.unescaped()).to_string(),
            string.content_loc(),
        ));
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

        if node.receiver().is_none() && node.name().as_slice() == b"delegate" {
            if self.process_delegate_method_symbol(node) {
                return;
            }
        }
        if node.name().as_slice() == b"class_attribute" {
            if self.process_class_attribute_symbol(node) {
                return;
            }
        }
        if node.receiver().is_none()
            && matches!(node.name().as_slice(), b"def_delegator" | b"def_delegators")
        {
            if self.process_forwardable_delegate_method_symbol(node) {
                return;
            }
        }
        if node.receiver().is_none() && node.name().as_slice() == b"alias_method" {
            if self.process_alias_method_symbol(node) {
                return;
            }
        }
        if self.process_const_lookup_constant_symbol(node) {
            return;
        }
        if self.process_define_method_symbol(node) {
            return;
        }
        if self.process_static_send_method_symbol(node) {
            return;
        }
        if self.process_reflected_method_symbol(node) {
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

                // Empty or invalid method name means cursor is at the dot position
                // (prism produces a zero-width message_loc right after the dot).
                // Extract the receiver so completion can provide method suggestions.
                if method_name.is_empty() || !RubyMethod::is_valid_ruby_method_name(&method_name) {
                    let receiver = if let Some(receiver_node) = node.receiver() {
                        extract_receiver_from_node(&receiver_node)
                    } else {
                        return;
                    };
                    self.set_result(
                        Some(Identifier::RubyMethod {
                            namespace: self.scope_tracker.get_ns_stack(),
                            receiver,
                            iden: RubyMethod::empty(),
                        }),
                        Some(IdentifierType::MethodCall),
                        self.scope_tracker.get_ns_stack(),
                        Some(0),
                    );
                    return;
                }

                let receiver = if let Some(receiver_node) = node.receiver() {
                    extract_receiver_from_node(&receiver_node)
                } else {
                    MethodReceiver::None
                };

                let namespace = if node.receiver().is_none() {
                    self.scope_tracker.implicit_receiver_context().0
                } else {
                    self.scope_tracker.get_ns_stack()
                };

                let method = RubyMethod::new(&method_name).unwrap();
                self.set_result(
                    Some(Identifier::RubyMethod {
                        namespace: namespace.clone(),
                        receiver,
                        iden: method,
                    }),
                    Some(IdentifierType::MethodCall),
                    namespace,
                    Some(0),
                );
            }
        }
    }

    pub fn process_call_node_exit(&mut self, _node: &CallNode) {
        // No cleanup needed for call nodes
    }

    fn process_define_method_symbol(&mut self, node: &CallNode) -> bool {
        let (name_index, namespace, owner_kind) = if node.receiver().is_none() {
            match node.name().as_slice() {
                b"define_method" => (
                    0,
                    {
                        let (namespace, receiver_kind) =
                            self.scope_tracker.implicit_receiver_context();
                        if receiver_kind != NamespaceKind::Singleton || namespace.is_empty() {
                            return false;
                        }
                        namespace
                    },
                    if !self.scope_tracker.execution_context_active()
                        && self.scope_tracker.in_singleton()
                    {
                        NamespaceKind::Singleton
                    } else {
                        NamespaceKind::Instance
                    },
                ),
                b"define_singleton_method" => {
                    let (namespace, receiver_kind) = self.scope_tracker.implicit_receiver_context();
                    if receiver_kind != NamespaceKind::Singleton || namespace.is_empty() {
                        return false;
                    }
                    (0, namespace, NamespaceKind::Singleton)
                }
                _ => return false,
            }
        } else {
            let Some(receiver) = node.receiver() else {
                return false;
            };
            let MethodReceiver::Constant(namespace) = extract_receiver_from_node(&receiver) else {
                return false;
            };
            match node.name().as_slice() {
                b"define_singleton_method" => (0, namespace, NamespaceKind::Singleton),
                b"send" | b"public_send" | b"__send__" => {
                    let Some((selector, _)) = call_arg_name_and_location(node, 0) else {
                        return false;
                    };
                    let owner_kind = match selector.as_str() {
                        "define_method" if node.name().as_slice() != b"public_send" => {
                            NamespaceKind::Instance
                        }
                        "define_singleton_method" => NamespaceKind::Singleton,
                        _ => return false,
                    };
                    (1, namespace, owner_kind)
                }
                _ => return false,
            }
        };

        let Some((method_name, name_loc)) = call_arg_name_and_location(node, name_index) else {
            return false;
        };
        if !self.is_position_in_location(&name_loc) {
            return false;
        }
        let Ok(method) = RubyMethod::new(&method_name) else {
            return false;
        };
        self.set_result(
            Some(Identifier::RubyMethod {
                namespace: namespace.clone(),
                receiver: MethodReceiver::None,
                iden: method,
            }),
            Some(IdentifierType::MethodDef),
            namespace,
            Some(0),
        );
        self.namespace_kind_at_pos = Some(owner_kind);
        true
    }

    fn process_const_lookup_constant_symbol(&mut self, node: &CallNode) -> bool {
        let Some((parts, name_loc)) = self.const_lookup_constant_parts_and_location(node) else {
            return false;
        };
        if !self.is_position_in_location(&name_loc) {
            return false;
        }
        self.set_result(
            Some(Identifier::RubyConstant {
                namespace: self.scope_tracker.get_ns_stack(),
                iden: parts,
            }),
            Some(IdentifierType::ConstantDef),
            self.scope_tracker.get_ns_stack(),
            Some(0),
        );
        true
    }

    fn const_lookup_constant_parts_and_location<'a>(
        &self,
        node: &CallNode<'a>,
    ) -> Option<(Vec<RubyConstant>, ruby_prism::Location<'a>)> {
        if !matches!(node.name().as_slice(), b"const_get" | b"const_defined?") {
            return None;
        }
        let (constant_name, location) = call_arg_name_and_location(node, 0)?;
        let constant = RubyConstant::new(&constant_name).ok()?;
        let mut parts = match node.receiver() {
            Some(receiver) if receiver.as_self_node().is_some() => {
                self.scope_tracker.get_ns_stack()
            }
            Some(receiver) => self.const_lookup_base_parts(&receiver)?,
            None => self.scope_tracker.get_ns_stack(),
        };
        parts.push(constant);
        Some((parts, location))
    }

    fn const_lookup_base_parts(&self, receiver: &Node<'_>) -> Option<Vec<RubyConstant>> {
        if let Some(call) = receiver.as_call_node() {
            if let Some((parts, _location)) = self.const_lookup_constant_parts_and_location(&call) {
                return Some(parts);
            }
        }
        let MethodReceiver::Constant(parts) = extract_receiver_from_node(receiver) else {
            return None;
        };
        if parts.len() == 1 {
            let mut qualified = self.scope_tracker.get_ns_stack();
            qualified.extend(parts);
            return Some(qualified);
        }
        Some(parts)
    }

    fn process_static_send_method_symbol(&mut self, node: &CallNode) -> bool {
        match node.name().as_slice() {
            b"send" | b"public_send" | b"__send__" => {}
            _ => return false,
        }

        let Some((method_name, name_loc)) = call_arg_name_and_location(node, 0) else {
            return false;
        };
        if method_name == "define_method" || !self.is_position_in_location(&name_loc) {
            return false;
        }
        let Ok(method) = RubyMethod::new(&method_name) else {
            return false;
        };
        let receiver = if let Some(receiver_node) = node.receiver() {
            extract_receiver_from_node(&receiver_node)
        } else {
            MethodReceiver::None
        };
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
        true
    }

    fn process_delegate_method_symbol(&mut self, node: &CallNode) -> bool {
        let Some(arguments) = node.arguments() else {
            return false;
        };

        for arg in arguments.arguments().iter() {
            let Some(symbol) = arg.as_symbol_node() else {
                continue;
            };
            if !self.is_position_in_location(&symbol.location()) {
                continue;
            }
            let name = String::from_utf8_lossy(symbol.unescaped()).to_string();
            let Ok(method) = RubyMethod::new(&name) else {
                return false;
            };
            let namespace_kind = self.scope_tracker.current_macro_definition_context();
            let receiver = match namespace_kind {
                NamespaceKind::Instance => MethodReceiver::None,
                NamespaceKind::Singleton => MethodReceiver::SelfReceiver,
            };
            let scope_kind = match namespace_kind {
                NamespaceKind::Instance => LVScopeKind::InstanceMethod,
                NamespaceKind::Singleton => LVScopeKind::ClassMethod,
            };
            self.scope_tracker.push_scope_kind(scope_kind);
            self.set_result(
                Some(Identifier::RubyMethod {
                    namespace: self.scope_tracker.get_ns_stack(),
                    receiver,
                    iden: method,
                }),
                Some(IdentifierType::MethodDef),
                self.scope_tracker.get_ns_stack(),
                Some(0),
            );
            return true;
        }

        false
    }

    fn process_class_attribute_symbol(&mut self, node: &CallNode) -> bool {
        let Some(arguments) = node.arguments() else {
            return false;
        };

        for arg in arguments.arguments().iter() {
            let Some((name, location)) = call_arg_name_and_location_from_node(&arg) else {
                continue;
            };
            if !self.is_position_in_location(&location) {
                continue;
            }
            let Ok(method) = RubyMethod::new(&name) else {
                return false;
            };
            self.scope_tracker
                .push_scope_kind(LVScopeKind::InstanceMethod);
            self.set_result(
                Some(Identifier::RubyMethod {
                    namespace: self.scope_tracker.get_ns_stack(),
                    receiver: MethodReceiver::None,
                    iden: method,
                }),
                Some(IdentifierType::MethodDef),
                self.scope_tracker.get_ns_stack(),
                Some(0),
            );
            return true;
        }

        false
    }

    fn process_forwardable_delegate_method_symbol(&mut self, node: &CallNode) -> bool {
        let method_name = match node.name().as_slice() {
            b"def_delegators" => {
                let Some(arguments) = node.arguments() else {
                    return false;
                };
                let mut found = None;
                for (index, arg) in arguments.arguments().iter().enumerate() {
                    if index == 0 {
                        continue;
                    }
                    let Some(symbol) = arg.as_symbol_node() else {
                        continue;
                    };
                    if self.is_position_in_location(&symbol.location()) {
                        found = Some(String::from_utf8_lossy(symbol.unescaped()).to_string());
                        break;
                    }
                }
                let Some(found) = found else {
                    return false;
                };
                found
            }
            b"def_delegator" => {
                if let Some((alias_name, alias_loc)) = call_arg_name_and_location(node, 2) {
                    if self.is_position_in_location(&alias_loc) {
                        alias_name
                    } else {
                        return false;
                    }
                } else {
                    let Some((target_name, target_loc)) = call_arg_name_and_location(node, 1)
                    else {
                        return false;
                    };
                    if !self.is_position_in_location(&target_loc) {
                        return false;
                    }
                    target_name
                }
            }
            _ => return false,
        };

        let Ok(method) = RubyMethod::new(&method_name) else {
            return false;
        };
        let namespace_kind = self.scope_tracker.current_macro_definition_context();
        let receiver = match namespace_kind {
            NamespaceKind::Instance => MethodReceiver::None,
            NamespaceKind::Singleton => MethodReceiver::SelfReceiver,
        };
        let scope_kind = match namespace_kind {
            NamespaceKind::Instance => LVScopeKind::InstanceMethod,
            NamespaceKind::Singleton => LVScopeKind::ClassMethod,
        };
        self.scope_tracker.push_scope_kind(scope_kind);
        self.set_result(
            Some(Identifier::RubyMethod {
                namespace: self.scope_tracker.get_ns_stack(),
                receiver,
                iden: method,
            }),
            Some(IdentifierType::MethodDef),
            self.scope_tracker.get_ns_stack(),
            Some(0),
        );
        true
    }

    fn process_reflected_method_symbol(&mut self, node: &CallNode) -> bool {
        let Some((method_name, method_loc)) = call_arg_name_and_location(node, 0) else {
            return false;
        };
        if !matches!(node.name().as_slice(), b"method" | b"instance_method") {
            return false;
        }
        if !self.is_position_in_location(&method_loc) {
            return false;
        }
        let Ok(method) = RubyMethod::new(&method_name) else {
            return false;
        };

        match node.name().as_slice() {
            b"method" => {
                let receiver = node
                    .receiver()
                    .map(|receiver| extract_receiver_from_node(&receiver))
                    .unwrap_or(MethodReceiver::None);
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
                true
            }
            b"instance_method" => {
                let namespace = node
                    .receiver()
                    .and_then(|receiver| match extract_receiver_from_node(&receiver) {
                        MethodReceiver::Constant(path) => Some(path),
                        MethodReceiver::None
                        | MethodReceiver::SelfReceiver
                        | MethodReceiver::Super
                        | MethodReceiver::LocalVariable(_)
                        | MethodReceiver::InstanceVariable(_)
                        | MethodReceiver::ClassVariable(_)
                        | MethodReceiver::GlobalVariable(_)
                        | MethodReceiver::MethodCall { .. }
                        | MethodReceiver::Literal(_)
                        | MethodReceiver::Expression => None,
                    })
                    .unwrap_or_else(|| self.scope_tracker.get_ns_stack());
                self.scope_tracker
                    .push_scope_kind(LVScopeKind::InstanceMethod);
                self.set_result(
                    Some(Identifier::RubyMethod {
                        namespace: namespace.clone(),
                        receiver: MethodReceiver::None,
                        iden: method,
                    }),
                    Some(IdentifierType::MethodCall),
                    namespace,
                    Some(0),
                );
                true
            }
            b"delegate" | b"def_delegator" | b"def_delegators" | b"class_attribute"
            | b"attr_reader" | b"attr_writer" | b"attr_accessor" | b"module_function"
            | b"alias_method" | b"define_method" | b"include" | b"prepend" | b"extend"
            | b"send" | b"public_send" | b"__send__" => false,
            _ => false,
        }
    }

    fn process_alias_method_symbol(&mut self, node: &CallNode) -> bool {
        let Some(arguments) = node.arguments() else {
            return false;
        };
        let Some(first_arg) = arguments.arguments().iter().next() else {
            return false;
        };
        let Some(symbol) = first_arg.as_symbol_node() else {
            return false;
        };
        if !self.is_position_in_location(&symbol.location()) {
            return false;
        }

        let name = String::from_utf8_lossy(symbol.unescaped()).to_string();
        let Ok(method) = RubyMethod::new(&name) else {
            return false;
        };
        let namespace_kind = self.scope_tracker.current_macro_definition_context();
        let receiver = match namespace_kind {
            NamespaceKind::Instance => MethodReceiver::None,
            NamespaceKind::Singleton => MethodReceiver::SelfReceiver,
        };
        let scope_kind = match namespace_kind {
            NamespaceKind::Instance => LVScopeKind::InstanceMethod,
            NamespaceKind::Singleton => LVScopeKind::ClassMethod,
        };
        self.scope_tracker.push_scope_kind(scope_kind);
        self.set_result(
            Some(Identifier::RubyMethod {
                namespace: self.scope_tracker.get_ns_stack(),
                receiver,
                iden: method,
            }),
            Some(IdentifierType::MethodDef),
            self.scope_tracker.get_ns_stack(),
            Some(0),
        );
        true
    }
}
