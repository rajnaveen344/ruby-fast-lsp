use log::{debug, error};
use ruby_prism::ConstantPathWriteNode;

use crate::analyzer_prism::utils;
use crate::indexer::entry::{entry_builder::EntryBuilder, entry_kind::EntryKind};
use crate::types::{fully_qualified_name::FullyQualifiedName, ruby_namespace::RubyConstant};

use super::IndexVisitor;

impl IndexVisitor {
    pub fn process_constant_path_write_node_entry(&mut self, node: &ConstantPathWriteNode) {
        // Extract the constant path
        let constant_path = node.target();

        // Extract the constant name (the rightmost part of the path)
        let constant_name = match constant_path.name() {
            Some(name) => String::from_utf8_lossy(name.as_slice()).to_string(),
            None => {
                error!("Could not extract constant name from ConstantPathWriteNode");
                return;
            }
        };

        debug!("Visiting constant path write node: {}", constant_name);

        // Create a RubyConstant from the name
        let constant = match RubyConstant::new(&constant_name) {
            Ok(constant) => constant,
            Err(e) => {
                error!("Error creating constant: {}", e);
                return;
            }
        };

        // Extract the namespace path
        let mut namespace_parts = Vec::new();
        utils::collect_namespaces(&constant_path, &mut namespace_parts);

        // Get the current namespace and add the collected parts
        let mut fqn_parts = self.scope_tracker.get_ns_stack();
        fqn_parts.extend(namespace_parts);
        fqn_parts.push(constant);

        // Create a FullyQualifiedName using the combined namespace parts
        let fqn = FullyQualifiedName::namespace(fqn_parts);

        // Create an Entry with EntryKind::Constant
        let entry = EntryBuilder::new()
            .fqn(fqn)
            .location(
                self.document
                    .prism_location_to_lsp_location(&node.location()),
            )
            .kind(EntryKind::new_constant(None, None))
            .build();

        // Add the entry to the index
        if let Ok(entry) = entry {
            self.add_entry(entry);
            // NOTE: `inferred_type` is not defined in this context.
            // The instruction provided a snippet that included it, but to maintain
            // syntactical correctness and avoid unrelated edits, it's omitted here.
            // debug!("Added constant path entry: {} -> {:?}", fqn, inferred_type);
        } else {
            error!("Error creating entry for constant path: {}", constant_name);
        }
    }

    pub fn process_constant_path_write_node_exit(&mut self, _node: &ConstantPathWriteNode) {
        // No-op for now
    }
}
