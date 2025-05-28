use log::debug;
use ruby_prism::CallNode;

use crate::indexer::types::{fully_qualified_name::FullyQualifiedName, ruby_method::RubyMethod};

use super::IndexVisitor;

impl IndexVisitor {
    pub fn process_call_node_entry(&mut self, node: &CallNode) {
        // Check if it's a simple method call (receiverless or self)
        // TODO: Handle calls with explicit receivers (e.g., obj.method)
        if node.receiver().is_none() {
            // Assuming name() returns the ConstantId or similar for the method name
            let method_name_id = node.name();
            let method_name_bytes = method_name_id.as_slice();
            let method_name_str = String::from_utf8_lossy(method_name_bytes);

            if let Ok(method_name) = RubyMethod::try_from(method_name_str.as_ref()) {
                // Use the location of the message (method name) as the reference location
                let message_location = node.message_loc(); // Assuming this returns Option<Location>
                if let Some(loc) = message_location {
                    let location = self.prism_loc_to_lsp_loc(loc);
                    let current_namespace_path = self.namespace_stack.clone();

                    // Construct the FQN assuming it's an instance method in the current scope
                    // TODO: This is a simplification. Need context analysis to determine
                    // if it's a local method, inherited, from a mixin, or a top-level function.
                    let fqn = FullyQualifiedName::instance_method(
                        current_namespace_path.clone(),
                        method_name,
                    );

                    debug!(
                        "Found potential method reference: {} at {:?}",
                        fqn, location.range.start
                    );

                    // Add to references map
                    let mut index = self.index.lock().unwrap();
                    index.add_reference(fqn, location);
                } else {
                    // Fallback or log if message location is not available
                    debug!(
                        "Could not get message location for call: {}",
                        method_name_str
                    );
                }
            } else {
                // Might be a non-standard method name or something else
                debug!("Skipping call with non-method name: {}", method_name_str);
            }
        }
        // TODO: Handle attribute assignments (e.g., obj.attr = value) which are also CallNodes
    }

    pub fn process_call_node_exit(&mut self, _node: &CallNode) {
        // No specific action needed when exiting a call node for basic reference tracking
    }
}
