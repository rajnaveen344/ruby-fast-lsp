use log::{debug, error};
use ruby_prism::{ConstantPathNode, ConstantPathWriteNode};

use crate::indexer::entry::{entry_builder::EntryBuilder, entry_kind::EntryKind};
use crate::types::{
    fully_qualified_name::FullyQualifiedName, ruby_namespace::RubyConstant,
};

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
        let mut namespace_parts = self.extract_namespace_parts(&constant_path);
        
        // With the combined RubyConstant type, we add the constant to the namespace parts
        namespace_parts.push(constant);

        // Create a FullyQualifiedName using the namespace parts
        let fqn = FullyQualifiedName::namespace(namespace_parts);

        // Create an Entry with EntryKind::Constant
        let entry = EntryBuilder::new()
            .fqn(fqn)
            .location(self.prism_loc_to_lsp_loc(node.location()))
            .kind(EntryKind::Constant {
                value: None,      // We could extract the value here if needed
                visibility: None, // Default to public
            })
            .build();

        // Add the entry to the index
        if let Ok(entry) = entry {
            let mut index = self.index.lock().unwrap();
            index.add_entry(entry);
        } else {
            error!("Error creating entry for constant path: {}", constant_name);
        }
    }

    pub fn process_constant_path_write_node_exit(&mut self, _node: &ConstantPathWriteNode) {
        // No-op for now
    }

    // Helper method to extract namespace parts from a ConstantPathNode
    pub fn extract_namespace_parts(&self, node: &ConstantPathNode) -> Vec<RubyConstant> {
        let mut namespace_parts = Vec::new();

        // Start with the parent node if it exists
        if let Some(parent) = node.parent() {
            // Recursively extract namespace parts from the parent
            match parent.as_constant_path_node() {
                Some(parent_path) => {
                    // Add parts from the parent path first (left-to-right order)
                    namespace_parts.extend(self.extract_namespace_parts(&parent_path));

                    // Add the parent's name
                    if let Some(name) = parent_path.name() {
                        let name_str = String::from_utf8_lossy(name.as_slice()).to_string();
                        if let Ok(namespace) = RubyConstant::new(&name_str) {
                            namespace_parts.push(namespace);
                        }
                    }
                }
                None => {
                    // Handle the case where the parent is a ConstantReadNode
                    if let Some(constant_read) = parent.as_constant_read_node() {
                        let name_str =
                            String::from_utf8_lossy(constant_read.name().as_slice()).to_string();
                        if let Ok(namespace) = RubyConstant::new(&name_str) {
                            namespace_parts.push(namespace);
                        }
                    }
                }
            }
        }

        namespace_parts
    }
}
