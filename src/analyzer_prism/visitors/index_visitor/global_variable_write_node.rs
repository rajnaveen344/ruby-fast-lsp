use log::{debug, error};
use ruby_prism::GlobalVariableWriteNode;

use crate::indexer::entry::{entry_builder::EntryBuilder, entry_kind::EntryKind};
use crate::types::{
    fully_qualified_name::FullyQualifiedName,
    ruby_variable::{RubyVariable, RubyVariableType},
};

use super::IndexVisitor;

impl IndexVisitor {
    pub fn process_global_variable_write_node_entry(&mut self, node: &GlobalVariableWriteNode) {
        // Extract the variable name from the node
        let variable_name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        debug!("Visiting global variable write node: {}", variable_name);

        let var = RubyVariable::new(&variable_name, RubyVariableType::Global);

        match var {
            Ok(variable) => {
                // Create a fully qualified name for the global variable
                // Global variables are not associated with any namespace or method
                let fqn = FullyQualifiedName::variable(
                    self.uri.clone(),
                    vec![], // No namespace for globals
                    None,   // No method context for globals
                    variable.clone(),
                );

                debug!("Adding global variable entry: {:?}", fqn);

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
                    debug!("Added global variable entry: {:?}", variable);
                } else {
                    error!("Error creating entry for global variable: {}", variable_name);
                }
            }
            Err(err) => {
                error!("Invalid global variable name '{}': {}", variable_name, err);
            }
        }
    }


    pub fn process_global_variable_write_node_exit(&mut self, _node: &GlobalVariableWriteNode) {
        // No-op for now
    }
}
