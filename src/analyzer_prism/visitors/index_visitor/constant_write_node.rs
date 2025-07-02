use log::{debug, error};
use ruby_prism::ConstantWriteNode;

use crate::indexer::entry::{entry_builder::EntryBuilder, entry_kind::EntryKind};
use crate::types::{fully_qualified_name::FullyQualifiedName, ruby_namespace::RubyConstant};

use super::IndexVisitor;

impl IndexVisitor {
    pub fn process_constant_write_node_entry(&mut self, node: &ConstantWriteNode) {
        let constant_name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        debug!("Visiting constant write node: {}", constant_name);

        // Create a RubyConstant from the name
        let constant = match RubyConstant::new(&constant_name) {
            Ok(constant) => constant,
            Err(e) => {
                error!("Error creating constant: {}", e);
                return;
            }
        };

        // Create a FullyQualifiedName using the current namespace stack and the constant
        // First get the current flattened namespace, then add the new constant
        let mut namespace = self.scope_tracker.get_ns_stack();
        namespace.push(constant);
        let fqn = FullyQualifiedName::namespace(namespace);

        // Create an Entry with EntryKind::Constant
        let entry = EntryBuilder::new()
            .fqn(fqn)
            .location(
                self.document
                    .prism_location_to_lsp_location(&node.location()),
            )
            .kind(EntryKind::Constant {
                value: None,      // We could extract the value here if needed
                visibility: None, // Default to public
            })
            .build();

        // Add the entry to the index
        if let Ok(entry) = entry {
            let mut index = self.index.lock();
            index.add_entry(entry);
        } else {
            error!("Error creating entry for constant: {}", constant_name);
        }
    }

    pub fn process_constant_write_node_exit(&mut self, _node: &ConstantWriteNode) {
        // No-op for now
    }
}
