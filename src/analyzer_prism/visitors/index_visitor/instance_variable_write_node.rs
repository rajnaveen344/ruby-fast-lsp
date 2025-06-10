use log::{debug, error};
use ruby_prism::InstanceVariableWriteNode;

use crate::indexer::entry::{entry_builder::EntryBuilder, entry_kind::EntryKind};
use crate::types::{
    fully_qualified_name::FullyQualifiedName,
    ruby_variable::{RubyVariable, RubyVariableType},
};

use super::IndexVisitor;

impl IndexVisitor {
    pub fn process_instance_variable_write_node_entry(&mut self, node: &InstanceVariableWriteNode) {
        // Extract the variable name from the node
        let variable_name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        debug!("Visiting instance variable write node: {}", variable_name);

        let var = RubyVariable::new(&variable_name, RubyVariableType::Instance);

        match var {
            Ok(variable) => {
                // Create a fully qualified name for the variable
                // Instance variables are associated with the class/module, not with methods
                let fqn = FullyQualifiedName::variable(
                    self.namespace_stack.clone(),
                    None, // No method context for instance variables
                    variable.clone(),
                );

                debug!("Adding instance variable entry: {:?}", fqn);

                let entry = EntryBuilder::new()
                    .fqn(fqn)
                    .location(self.prism_loc_to_lsp_loc(node.name_loc()))
                    .kind(EntryKind::Variable {
                        name: variable.clone(),
                    })
                    .build();

                if let Ok(entry) = entry {
                    let mut index = self.index.lock().unwrap();
                    index.add_entry(entry);
                    debug!("Added instance variable entry: {:?}", variable);
                } else {
                    error!(
                        "Error creating entry for instance variable: {}",
                        variable_name
                    );
                }
            }
            Err(err) => {
                error!(
                    "Invalid instance variable name '{}': {}",
                    variable_name, err
                );
            }
        }
    }

    pub fn process_instance_variable_write_node_exit(&mut self, _node: &InstanceVariableWriteNode) {
        // No-op for now
    }
}
