use log::{debug, error};
use ruby_prism::LocalVariableWriteNode;

use crate::indexer::entry::{entry_builder::EntryBuilder, entry_kind::EntryKind};
use crate::types::{
    fully_qualified_name::FullyQualifiedName,
    ruby_variable::{RubyVariable, RubyVariableType},
};

use super::IndexVisitor;

impl IndexVisitor {
    pub fn process_local_variable_write_node_entry(&mut self, node: &LocalVariableWriteNode) {
        // Extract the variable name from the node
        let variable_name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        debug!("Visiting local variable write node: {}", variable_name);

        let var = RubyVariable::new(
            &variable_name,
            RubyVariableType::Local(self.uri.clone(), self.scope_stack.clone()),
        );

        debug!("Adding local variable entry: {:?}", var.clone().unwrap());
        match var {
            Ok(variable) => {
                // Create a fully qualified name for the variable
                let fqn = FullyQualifiedName::variable(
                    self.namespace_stack.clone(),
                    self.current_method.clone(),
                    variable.clone(),
                );

                // Create an entry with EntryKind::Variable
                let entry = EntryBuilder::new()
                    .fqn(fqn)
                    .location(self.prism_loc_to_lsp_loc(node.name_loc()))
                    .kind(EntryKind::Variable {
                        name: variable.clone(),
                    })
                    .build();

                // Add the entry to the index
                if let Ok(entry) = entry {
                    let mut index = self.index.lock().unwrap();
                    index.add_entry(entry);
                    debug!("Added local variable entry: {:?}", variable);
                } else {
                    error!("Error creating entry for local variable: {}", variable_name);
                }
            }
            Err(err) => {
                error!("Invalid local variable name '{}': {}", variable_name, err);
            }
        }
    }

    pub fn process_local_variable_write_node_exit(&mut self, _node: &LocalVariableWriteNode) {
        // No-op for now
    }
}
