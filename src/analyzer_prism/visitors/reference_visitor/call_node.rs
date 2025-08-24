use log::debug;
use ruby_prism::{CallNode, Node};

use crate::{
    analyzer_prism::utils,
    indexer::entry::MethodKind,
    types::{
        fully_qualified_name::FullyQualifiedName,
        ruby_method::RubyMethod,
        ruby_namespace::RubyConstant,
    },
};

use super::ReferenceVisitor;

impl ReferenceVisitor {
    pub fn process_call_node_entry(&mut self, node: &CallNode) {
        // Skip if this is a mixin call (include, extend, prepend) as those are handled by index_visitor
        let method_name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        if matches!(method_name.as_str(), "include" | "extend" | "prepend") {
            return;
        }

        // Skip if this is an attribute accessor call
        if matches!(method_name.as_str(), "attr_reader" | "attr_writer" | "attr_accessor") {
            return;
        }

        // Skip method names that don't follow Ruby method naming conventions
        if !RubyMethod::is_valid_ruby_method_name(&method_name) {
            debug!("Skipping method call with invalid name: {}", method_name);
            return;
        }

        let location = self.document.prism_location_to_lsp_location(&node.location());
        let current_namespace = self.scope_tracker.get_ns_stack();

        // Determine the target namespace and method kind based on receiver
        let (target_namespace, method_kind) = match node.receiver() {
            Some(receiver_node) => self.handle_receiver_node(&receiver_node, &current_namespace),
            None => self.handle_no_receiver(&current_namespace),
        };

        // Create the method, handling potential validation errors gracefully
        let method = match RubyMethod::new(&method_name, method_kind) {
            Ok(method) => method,
            Err(err) => {
                debug!("Failed to create RubyMethod for '{}': {}", method_name, err);
                return;
            }
        };

        let method_fqn = FullyQualifiedName::method(target_namespace, method);

        debug!(
            "Adding method call reference: {} at {:?}",
            method_fqn.to_string(),
            location
        );

        // Add the reference to the index
        let mut index = self.index.lock();
        index.add_reference(method_fqn, location);
    }

    /// Handle method calls without a receiver (e.g., `method_name`)
    fn handle_no_receiver(&self, current_namespace: &Vec<RubyConstant>) -> (Vec<RubyConstant>, MethodKind) {
        // Determine method kind based on current method context
        let method_kind = match self.scope_tracker.current_method_context() {
            Some(context_kind) => {
                // We're inside a method definition, use the same kind for bare calls
                context_kind
            }
            None => {
                // We're not inside a method definition (e.g., class body, top-level)
                // Check if we're in a singleton context
                if self.scope_tracker.in_singleton() {
                    MethodKind::Class
                } else {
                    // Default to instance method for most cases
                    // This covers class body and top-level calls
                    MethodKind::Instance
                }
            }
        };

        (current_namespace.clone(), method_kind)
    }

    /// Handle method calls with a receiver node
    fn handle_receiver_node(&self, receiver_node: &Node, current_namespace: &Vec<RubyConstant>) -> (Vec<RubyConstant>, MethodKind) {
        if let Some(_) = receiver_node.as_self_node() {
            self.handle_self_receiver(current_namespace)
        } else if let Some(constant_read) = receiver_node.as_constant_read_node() {
            self.handle_constant_read_receiver(&constant_read, current_namespace)
        } else if let Some(constant_path) = receiver_node.as_constant_path_node() {
            self.handle_constant_path_receiver(&constant_path, receiver_node, current_namespace)
        } else {
            self.handle_expression_receiver(current_namespace)
        }
    }

    /// Handle method calls with self receiver (e.g., `self.method_name`)
    fn handle_self_receiver(&self, current_namespace: &Vec<RubyConstant>) -> (Vec<RubyConstant>, MethodKind) {
        // Self receiver - method is in current namespace
        (current_namespace.clone(), MethodKind::Instance)
    }

    /// Handle method calls with constant read receiver (e.g., `Class.method`)
    fn handle_constant_read_receiver(
        &self,
        constant_read: &ruby_prism::ConstantReadNode,
        current_namespace: &Vec<RubyConstant>,
    ) -> (Vec<RubyConstant>, MethodKind) {
        let name = String::from_utf8_lossy(constant_read.name().as_slice()).to_string();
        if let Ok(constant) = RubyConstant::new(&name) {
            let mut receiver_namespace = current_namespace.clone();
            receiver_namespace.push(constant);
            (receiver_namespace, MethodKind::Class)
        } else {
            (current_namespace.clone(), MethodKind::Unknown)
        }
    }

    /// Handle method calls with constant path receiver (e.g., `Module::Class.method`)
    fn handle_constant_path_receiver(
        &self,
        _constant_path: &ruby_prism::ConstantPathNode,
        receiver_node: &Node,
        current_namespace: &Vec<RubyConstant>,
    ) -> (Vec<RubyConstant>, MethodKind) {
        // Use the centralized constant resolution utility
        let current_fqn = FullyQualifiedName::Constant(current_namespace.clone());
        let index_guard = self.index.lock();
        if let Some(resolved_fqn) = utils::resolve_constant_fqn(&*index_guard, receiver_node, &current_fqn) {
            if let FullyQualifiedName::Constant(parts) = resolved_fqn {
                return (parts, MethodKind::Class);
            }
        }
        
        // Fallback to mixin_ref approach if resolution fails
        if let Some(mixin_ref) = utils::mixin_ref_from_node(receiver_node) {
            let final_namespace = if mixin_ref.absolute {
                mixin_ref.parts
            } else {
                self.resolve_relative_constant_path(&mixin_ref.parts, current_namespace)
            };
            (final_namespace, MethodKind::Class)
        } else {
            (current_namespace.clone(), MethodKind::Unknown)
        }
    }

    /// Handle method calls with expression receiver (e.g., `variable.method`)
    fn handle_expression_receiver(&self, current_namespace: &Vec<RubyConstant>) -> (Vec<RubyConstant>, MethodKind) {
        // Expression receiver - use current namespace
        (current_namespace.clone(), MethodKind::Instance)
    }

    /// Resolve relative constant paths by checking namespace hierarchy
    fn resolve_relative_constant_path(
        &self,
        parts: &Vec<RubyConstant>,
        current_namespace: &Vec<RubyConstant>,
    ) -> Vec<RubyConstant> {
        // For relative paths, resolve by checking namespace hierarchy
        // Ruby constant resolution: look for the first part in current namespace and ancestors
        if let Some(first_part) = parts.first() {
            let mut resolved = None;
            
            // Check if the first constant exists in current namespace or ancestors
            for i in (0..=current_namespace.len()).rev() {
                let test_namespace = &current_namespace[..i];
                
                // Check if first_part exists at this level
                // Look for the constant in the namespace parts
                if test_namespace.iter().any(|c| c.to_string() == first_part.to_string()) {
                    // Found the constant in the namespace hierarchy
                    // Find the position where this constant appears
                    if let Some(pos) = test_namespace.iter().position(|c| c.to_string() == first_part.to_string()) {
                        let mut result = test_namespace[..=pos].to_vec();
                        // Skip the first part since it's already in the namespace
                        result.extend(parts.iter().skip(1).cloned());
                        
                        resolved = Some(result);
                        break;
                    }
                }
            }
            
            // If not found in hierarchy, try from root
            resolved.unwrap_or_else(|| {
                // Check if it should be resolved from a parent namespace
                // For Platform::PlatformServices in GoshPosh::Platform::SpecHelpers,
                // Platform should resolve to GoshPosh::Platform
                if current_namespace.len() >= 2 {
                    let parent_ns = &current_namespace[..current_namespace.len() - 1];
                    if parent_ns.last().map(|c| c.to_string()) == Some(first_part.to_string()) {
                        let mut result = parent_ns.to_vec();
                        result.extend(parts.iter().cloned());
                        return result;
                    }
                }
                
                // Default: append to current namespace
                let mut ns = current_namespace.clone();
                ns.extend(parts.iter().cloned());
                ns
            })
        } else {
            current_namespace.clone()
        }
    }

    pub fn process_call_node_exit(&mut self, _node: &CallNode) {
        // Nothing to do on exit
    }
}