use log::debug;
use ruby_prism::{CallNode, Node};

use crate::{
    analyzer_prism::utils,
    indexer::{entry::MethodKind, index::UnresolvedEntry},
    types::{
        fully_qualified_name::FullyQualifiedName, ruby_method::RubyMethod,
        ruby_namespace::RubyConstant,
    },
};

use super::ReferenceVisitor;

/// Information about the receiver of a method call
#[derive(Debug, Clone)]
enum ReceiverInfo {
    /// No receiver (e.g., `method_name`)
    NoReceiver,
    /// Self receiver (e.g., `self.method_name`)
    SelfReceiver,
    /// Constant receiver (e.g., `Foo.method` or `Foo::Bar.method`)
    ConstantReceiver(String),
    /// Expression receiver (e.g., `variable.method`)
    ExpressionReceiver,
    /// Invalid constant path (contains non-constant nodes)
    InvalidConstantPath,
}

impl ReferenceVisitor {
    pub fn process_call_node_entry(&mut self, node: &CallNode) {
        // Skip if this is a mixin call (include, extend, prepend) as those are handled by index_visitor
        let method_name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        if matches!(method_name.as_str(), "include" | "extend" | "prepend") {
            return;
        }

        // Skip if this is an attribute accessor call
        if matches!(
            method_name.as_str(),
            "attr_reader" | "attr_writer" | "attr_accessor"
        ) {
            return;
        }

        // Skip method names that don't follow Ruby method naming conventions
        if !RubyMethod::is_valid_ruby_method_name(&method_name) {
            debug!("Skipping method call with invalid name: {}", method_name);
            return;
        }

        // Get the full call location for references
        let call_location = self
            .document
            .prism_location_to_lsp_location(&node.location());

        // Get the message (method name) location for diagnostics - only underline the method name
        let message_location = node
            .message_loc()
            .map(|loc| self.document.prism_location_to_lsp_location(&loc))
            .unwrap_or_else(|| call_location.clone());

        let current_namespace = self.scope_tracker.get_ns_stack();

        // Determine the target namespace, method kind, and receiver info based on receiver
        let (target_namespace, method_kind, receiver_info) = match node.receiver() {
            Some(receiver_node) => {
                let result =
                    self.handle_receiver_node_with_info(&receiver_node, &current_namespace);
                (result.0, result.1, result.2)
            }
            None => {
                let (ns, kind) = self.handle_no_receiver(&current_namespace);
                (ns, kind, ReceiverInfo::NoReceiver)
            }
        };

        // Create the method, handling potential validation errors gracefully
        let method = match RubyMethod::new(&method_name, method_kind) {
            Ok(method) => method,
            Err(err) => {
                debug!("Failed to create RubyMethod for '{}': {}", method_name, err);
                return;
            }
        };

        let method_fqn = FullyQualifiedName::method(target_namespace.clone(), method);

        debug!(
            "Adding method call reference: {} at {:?}",
            method_fqn.to_string(),
            call_location
        );

        // Add the reference to the index (use full call location for references)
        let mut index = self.index.lock();
        index.add_reference(method_fqn.clone(), call_location);

        // Track unresolved method calls if enabled (use message location for diagnostics)
        if self.track_unresolved {
            // Only track methods without receiver or with constant receiver
            match &receiver_info {
                ReceiverInfo::NoReceiver => {
                    // Method without receiver - check if it exists in the index
                    if !self.method_exists_in_index(
                        &index,
                        &method_name,
                        &target_namespace,
                        method_kind,
                    ) {
                        debug!("Adding unresolved method call: {}", method_name);
                        index.add_unresolved_entry(
                            self.document.uri.clone(),
                            UnresolvedEntry::method(method_name.clone(), None, message_location),
                        );
                    }
                }
                ReceiverInfo::ConstantReceiver(receiver_name) => {
                    // Method with constant receiver - check if it exists
                    if !self.method_exists_in_index(
                        &index,
                        &method_name,
                        &target_namespace,
                        method_kind,
                    ) {
                        debug!(
                            "Adding unresolved method call: {}.{}",
                            receiver_name, method_name
                        );
                        index.add_unresolved_entry(
                            self.document.uri.clone(),
                            UnresolvedEntry::method(
                                method_name.clone(),
                                Some(receiver_name.clone()),
                                message_location,
                            ),
                        );
                    }
                }
                ReceiverInfo::ExpressionReceiver
                | ReceiverInfo::SelfReceiver
                | ReceiverInfo::InvalidConstantPath => {
                    // Skip tracking for expression receivers, self receivers, and invalid constant paths
                    // as we can't determine the type statically
                }
            }
        }
    }

    /// Check if a method exists in the index
    fn method_exists_in_index(
        &self,
        index: &crate::indexer::index::RubyIndex,
        method_name: &str,
        target_namespace: &[RubyConstant],
        method_kind: MethodKind,
    ) -> bool {
        // Create a method to check
        let method = match RubyMethod::new(method_name, method_kind) {
            Ok(m) => m,
            Err(_) => return true, // If we can't create the method, assume it exists
        };

        // Check if the method exists by name (loose check)
        if index.methods_by_name.contains_key(&method) {
            return true;
        }

        // Also check with Unknown kind for flexibility
        if method_kind != MethodKind::Unknown {
            if let Ok(unknown_method) = RubyMethod::new(method_name, MethodKind::Unknown) {
                if index.methods_by_name.contains_key(&unknown_method) {
                    return true;
                }
            }
        }

        // Check the specific FQN
        let method_fqn = FullyQualifiedName::method(target_namespace.to_vec(), method);
        if index.definitions.contains_key(&method_fqn) {
            return true;
        }

        // For methods without receiver, also check if it might be inherited
        // by checking the method name in any namespace
        if target_namespace.is_empty() {
            // Top-level method call - just check by name
            return false;
        }

        // Check parent namespaces for inherited methods
        let mut ancestors = target_namespace.to_vec();
        while !ancestors.is_empty() {
            if let Ok(m) = RubyMethod::new(method_name, method_kind) {
                let fqn = FullyQualifiedName::method(ancestors.clone(), m);
                if index.definitions.contains_key(&fqn) {
                    return true;
                }
            }
            ancestors.pop();
        }

        false
    }

    /// Handle method calls without a receiver (e.g., `method_name`)
    fn handle_no_receiver(
        &self,
        current_namespace: &Vec<RubyConstant>,
    ) -> (Vec<RubyConstant>, MethodKind) {
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

    /// Handle method calls with a receiver node (returns receiver info for diagnostics)
    fn handle_receiver_node_with_info(
        &self,
        receiver_node: &Node,
        current_namespace: &Vec<RubyConstant>,
    ) -> (Vec<RubyConstant>, MethodKind, ReceiverInfo) {
        if receiver_node.as_self_node().is_some() {
            let (ns, kind) = self.handle_self_receiver(current_namespace);
            (ns, kind, ReceiverInfo::SelfReceiver)
        } else if let Some(constant_read) = receiver_node.as_constant_read_node() {
            let name = String::from_utf8_lossy(constant_read.name().as_slice()).to_string();
            let (ns, kind) = self.handle_constant_read_receiver(&constant_read, current_namespace);
            (ns, kind, ReceiverInfo::ConstantReceiver(name))
        } else if let Some(constant_path) = receiver_node.as_constant_path_node() {
            // Check if the constant path is valid (all nodes are constant paths or constant reads)
            if self.is_valid_constant_path_receiver(receiver_node) {
                let receiver_name = self.build_constant_path_name(receiver_node);
                let (ns, kind) = self.handle_constant_path_receiver(
                    &constant_path,
                    receiver_node,
                    current_namespace,
                );
                (ns, kind, ReceiverInfo::ConstantReceiver(receiver_name))
            } else {
                // Invalid constant path - contains non-constant nodes
                let (ns, kind) = self.handle_expression_receiver(current_namespace);
                (ns, kind, ReceiverInfo::InvalidConstantPath)
            }
        } else {
            let (ns, kind) = self.handle_expression_receiver(current_namespace);
            (ns, kind, ReceiverInfo::ExpressionReceiver)
        }
    }

    /// Check if a constant path receiver is valid (only contains constant paths and constant reads)
    fn is_valid_constant_path_receiver(&self, node: &Node) -> bool {
        if node.as_constant_read_node().is_some() {
            return true;
        }

        if let Some(constant_path) = node.as_constant_path_node() {
            // Check if the parent is valid (if present)
            if let Some(parent) = constant_path.parent() {
                return self.is_valid_constant_path_receiver(&parent);
            }
            // No parent means it's a root constant path (::Foo), which is valid
            return true;
        }

        // Any other node type is invalid
        false
    }

    /// Build the full constant path name as a string (e.g., "Foo::Bar::Baz")
    fn build_constant_path_name(&self, node: &Node) -> String {
        let mut parts = Vec::new();
        self.collect_constant_path_parts_for_name(node, &mut parts);
        parts.join("::")
    }

    /// Recursively collect constant path parts for building the name
    fn collect_constant_path_parts_for_name(&self, node: &Node, parts: &mut Vec<String>) {
        if let Some(constant_path) = node.as_constant_path_node() {
            // Process parent first (left side)
            if let Some(parent) = constant_path.parent() {
                self.collect_constant_path_parts_for_name(&parent, parts);
            }
            // Then add the name (right side)
            if let Some(name_bytes) = constant_path.name() {
                let name = String::from_utf8_lossy(name_bytes.as_slice()).to_string();
                parts.push(name);
            }
        } else if let Some(constant_read) = node.as_constant_read_node() {
            let name = String::from_utf8_lossy(constant_read.name().as_slice()).to_string();
            parts.push(name);
        }
    }

    /// Handle method calls with self receiver (e.g., `self.method_name`)
    fn handle_self_receiver(
        &self,
        current_namespace: &Vec<RubyConstant>,
    ) -> (Vec<RubyConstant>, MethodKind) {
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
        if let Some(resolved_fqn) =
            utils::resolve_constant_fqn(&*index_guard, receiver_node, &current_fqn)
        {
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
    fn handle_expression_receiver(
        &self,
        current_namespace: &Vec<RubyConstant>,
    ) -> (Vec<RubyConstant>, MethodKind) {
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
                if test_namespace
                    .iter()
                    .any(|c| c.to_string() == first_part.to_string())
                {
                    // Found the constant in the namespace hierarchy
                    // Find the position where this constant appears
                    if let Some(pos) = test_namespace
                        .iter()
                        .position(|c| c.to_string() == first_part.to_string())
                    {
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
