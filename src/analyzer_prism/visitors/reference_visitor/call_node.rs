use log::debug;
use ruby_prism::CallNode;

use crate::indexer::entry::MethodKind;
use crate::types::{
    fully_qualified_name::FullyQualifiedName,
    ruby_method::RubyMethod,
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
        // (method names should start with lowercase letter or underscore)
        if !method_name.chars().next().map_or(false, |c| c.is_lowercase() || c == '_') {
            debug!("Skipping method call with invalid name: {}", method_name);
            return;
        }

        let location = self.document.prism_location_to_lsp_location(&node.location());
        let current_namespace = self.scope_tracker.get_ns_stack();

        // Determine method kind based on receiver
        let method_kind = if node.receiver().is_some() {
            // Has a receiver - could be instance or class method depending on the receiver
            MethodKind::Unknown // Let the reference finder determine the correct kind
        } else {
            // No receiver - instance method call
            MethodKind::Instance
        };

        // Create the method, handling potential validation errors gracefully
        let method = match RubyMethod::new(&method_name, method_kind) {
            Ok(method) => method,
            Err(err) => {
                debug!("Failed to create RubyMethod for '{}': {}", method_name, err);
                return;
            }
        };

        let method_fqn = FullyQualifiedName::method(current_namespace.clone(), method);

        debug!(
            "Adding method call reference: {} at {:?}",
            method_fqn.to_string(),
            location
        );

        // Add the reference to the index
        let mut index = self.index.lock();
        index.add_reference(method_fqn, location);
    }

    pub fn process_call_node_exit(&mut self, _node: &CallNode) {
        // Nothing to do on exit
    }
}