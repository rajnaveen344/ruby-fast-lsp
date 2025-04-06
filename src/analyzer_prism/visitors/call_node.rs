use crate::analyzer_prism::visitors::identifier_visitor::IdentifierVisitor;
use crate::indexer::types::fully_qualified_name::FullyQualifiedName;
use crate::indexer::types::ruby_constant::RubyConstant;
use crate::indexer::types::ruby_method::RubyMethod;
use crate::indexer::types::ruby_namespace::RubyNamespace;
use ruby_prism::{visit_arguments_node, CallNode, ConstantPathNode, ConstantReadNode};

/// Handle the receiver part of a call node
pub fn handle_receiver(visitor: &mut IdentifierVisitor, node: &CallNode) -> bool {
    if let Some(receiver) = node.receiver() {
        // Check if the receiver is a constant read node
        if let Some(constant_node) = receiver.as_constant_read_node() {
            if visitor.is_position_in_location(&constant_node.location())
                && visitor.identifier.is_none()
            {
                handle_constant_read_receiver(visitor, &constant_node);
                return true;
            }
        }
        // Check if the receiver is a constant path node
        else if let Some(constant_path) = receiver.as_constant_path_node() {
            if visitor.is_position_in_location(&constant_path.location())
                && visitor.identifier.is_none()
            {
                handle_constant_path_receiver(visitor, &constant_path);
                return true;
            }
        }
        // Check if the receiver is a call node (nested method calls like a.b.c)
        else if let Some(call_node) = receiver.as_call_node() {
            // Check if the cursor is on the method name of the receiver call node
            if let Some(message_loc) = call_node.message_loc() {
                if visitor.is_position_in_location(&message_loc) && visitor.identifier.is_none() {
                    // Extract the method name
                    let method_name_id = call_node.name();
                    let method_name_bytes = method_name_id.as_slice();
                    let method_name_str = String::from_utf8_lossy(method_name_bytes).to_string();

                    // Try to create a RubyMethod from the name
                    if let Ok(method_name) = RubyMethod::try_from(method_name_str.as_ref()) {
                        // For nested call nodes, we'll use instance_method as the default
                        visitor.identifier = Some(FullyQualifiedName::instance_method(
                            visitor.namespace_stack.clone(),
                            method_name,
                        ));
                        visitor.ancestors = visitor.namespace_stack.clone();
                        return true;
                    }
                }
            }

            // If the cursor is not on the method name, check if it's on the receiver of the nested call
            if visitor.is_position_in_location(&call_node.location())
                && visitor.identifier.is_none()
            {
                // Recursively handle the nested call node
                return handle_receiver(visitor, &call_node);
            }
        }
    }
    false
}

/// Handle a constant read node receiver
fn handle_constant_read_receiver(
    visitor: &mut IdentifierVisitor,
    constant_node: &ConstantReadNode,
) {
    // Cursor is on the constant part of a method call (e.g., Foo.bar)
    let constant_name = String::from_utf8_lossy(constant_node.name().as_slice()).to_string();

    // In Ruby, constants start with an uppercase letter
    // If the name starts with an uppercase letter and is all uppercase, it's a constant (like MAX_VALUE)
    // If the name starts with an uppercase letter but has mixed case, it could be either a constant or a namespace
    // For method calls like Foo.bar, we need to treat Foo as a namespace
    if constant_name.chars().next().unwrap().is_uppercase() {
        // For method calls, we want to treat the receiver as a namespace
        // This is because in Ruby, method calls on constants are typically class/module method calls
        let namespace = RubyNamespace::new(constant_name.as_str()).unwrap();
        visitor.identifier = Some(FullyQualifiedName::namespace(vec![namespace]));
    } else {
        // If it doesn't start with uppercase, it can't be a constant or namespace
        // This shouldn't happen for valid Ruby code
        // Just use the name as is, even if it's not a valid namespace
        // This is a fallback for invalid names
        let namespace = RubyNamespace::new(constant_name.as_str())
            .unwrap_or_else(|_| panic!("Invalid namespace name: {}", constant_name));
        visitor.identifier = Some(FullyQualifiedName::namespace(vec![namespace]));
    }

    visitor.ancestors = visitor.namespace_stack.clone();
}

/// Handle a constant path node receiver
pub fn handle_constant_path_receiver(
    visitor: &mut IdentifierVisitor,
    constant_path: &ConstantPathNode,
) {
    // Cursor is on the constant path part of a method call (e.g., Foo::Bar.baz)
    let (mut namespaces, is_root_constant) = visitor.determine_const_path_target(constant_path);

    // If namespaces is empty, the cursor is on a scope resolution operator (::)
    // In that case, we don't want to set an identifier
    if !namespaces.is_empty() {
        if let Some(last_part) = namespaces.last() {
            let last_part_str = last_part.to_string();

            match RubyConstant::new(&last_part_str) {
                Ok(constant) => {
                    namespaces.pop(); // Remove the last part (constant name)
                    visitor.identifier = Some(FullyQualifiedName::constant(namespaces, constant));
                }
                Err(_) => {
                    visitor.identifier = Some(FullyQualifiedName::namespace(namespaces));
                }
            }

            if is_root_constant {
                visitor.ancestors = vec![];
            } else {
                visitor.ancestors = visitor.namespace_stack.clone();
            }
        }
    }
}

/// Handle the arguments part of a call node
pub fn handle_arguments(visitor: &mut IdentifierVisitor, node: &CallNode) -> bool {
    if let Some(arguments) = node.arguments() {
        if visitor.is_position_in_location(&arguments.location()) && visitor.identifier.is_none() {
            // Visit the arguments node to check if the cursor is on a constant within the arguments
            visit_arguments_node(visitor, &arguments);

            // If we found an identifier in the arguments, return true
            if visitor.identifier.is_some() {
                return true;
            }
        }
    }
    false
}

/// Handle a constant path in an argument
pub fn handle_constant_path_in_argument(
    visitor: &mut IdentifierVisitor,
    node: &ConstantPathNode,
) -> bool {
    if visitor.is_position_in_location(&node.location()) && visitor.identifier.is_none() {
        // Get all namespace parts
        let (mut namespaces, is_root_constant) = visitor.determine_const_path_target(node);

        // Handle the case when cursor is on scope resolution operator
        if namespaces.is_empty() {
            visitor.identifier = None;
            visitor.ancestors = vec![];
            return false;
        }

        if let Some(last_part) = namespaces.last() {
            let last_part_str = last_part.to_string();

            match RubyConstant::new(&last_part_str) {
                Ok(constant) => {
                    namespaces.pop(); // Remove the last part (constant name)
                    visitor.identifier = Some(FullyQualifiedName::constant(namespaces, constant));
                }
                Err(_) => {
                    visitor.identifier = Some(FullyQualifiedName::namespace(namespaces));
                }
            }

            if is_root_constant {
                visitor.ancestors = vec![];
            } else {
                visitor.ancestors = visitor.namespace_stack.clone();
            }
            return true;
        }
    }
    false
}

/// Handle the method name part of a call node
pub fn handle_method_name(visitor: &mut IdentifierVisitor, node: &CallNode) -> bool {
    if let Some(message_loc) = node.message_loc() {
        if visitor.is_position_in_location(&message_loc) && visitor.identifier.is_none() {
            // Extract the method name
            let method_name_id = node.name();
            let method_name_bytes = method_name_id.as_slice();
            let method_name_str = String::from_utf8_lossy(method_name_bytes).to_string();

            // Try to create a RubyMethod from the name
            if let Ok(method_name) = RubyMethod::try_from(method_name_str.as_ref()) {
                // Determine the method type based on the receiver and context
                if let Some(receiver) = node.receiver() {
                    if let Some(constant_node) = receiver.as_constant_read_node() {
                        handle_method_with_constant_receiver(visitor, &constant_node, method_name);
                    } else if let Some(constant_path) = receiver.as_constant_path_node() {
                        handle_method_with_constant_path_receiver(
                            visitor,
                            &constant_path,
                            method_name,
                        );
                    } else {
                        handle_method_with_other_receiver(visitor, method_name);
                    }
                } else {
                    handle_local_method_call(visitor, method_name);
                }
                return true;
            }
        }
    }
    false
}

/// Handle a method call on a constant receiver
fn handle_method_with_constant_receiver(
    visitor: &mut IdentifierVisitor,
    constant_node: &ConstantReadNode,
    method_name: RubyMethod,
) {
    // It's a class method call on a constant
    let constant_name = String::from_utf8_lossy(constant_node.name().as_slice()).to_string();
    let namespace = RubyNamespace::new(&constant_name).unwrap();

    // Check if this might be a module function
    // For now, we'll use class_method as the default for constant receivers
    // The actual determination of ModuleFunc will happen in the indexer
    visitor.identifier = Some(FullyQualifiedName::module_method(
        vec![namespace.clone()],
        method_name.clone(),
    ));

    visitor.ancestors = vec![]; // Class/module methods are absolute references
}

/// Handle a method call on a constant path receiver
fn handle_method_with_constant_path_receiver(
    visitor: &mut IdentifierVisitor,
    constant_path: &ConstantPathNode,
    method_name: RubyMethod,
) {
    // It's a class method call on a namespaced constant
    let (namespaces, is_root_constant) = visitor.determine_const_path_target(constant_path);

    // Check if this might be a module function
    // For now, we'll use module_method as the default for constant path receivers
    // The actual determination of ModuleFunc will happen in the indexer
    visitor.identifier = Some(FullyQualifiedName::module_method(
        namespaces.clone(),
        method_name.clone(),
    ));

    if is_root_constant {
        visitor.ancestors = vec![];
    } else {
        visitor.ancestors = visitor.namespace_stack.clone();
    }
}

/// Handle a method call on some other expression
fn handle_method_with_other_receiver(visitor: &mut IdentifierVisitor, method_name: RubyMethod) {
    // It's an instance method call on some other expression
    // For now, we'll use the current namespace
    visitor.identifier = Some(FullyQualifiedName::instance_method(
        visitor.namespace_stack.clone(),
        method_name,
    ));
    visitor.ancestors = visitor.namespace_stack.clone();
}

/// Handle a local method call (no receiver)
fn handle_local_method_call(visitor: &mut IdentifierVisitor, method_name: RubyMethod) {
    // No receiver, it's a local method call in the current context
    // This could be either an instance method or a module function
    // For now, we'll use instance_method as the default
    visitor.identifier = Some(FullyQualifiedName::instance_method(
        visitor.namespace_stack.clone(),
        method_name.clone(),
    ));

    // Also check if it might be a module function in the current namespace
    if !visitor.namespace_stack.is_empty() {
        visitor.identifier = Some(FullyQualifiedName::module_method(
            visitor.namespace_stack.clone(),
            method_name,
        ));
    }

    visitor.ancestors = visitor.namespace_stack.clone();
}
