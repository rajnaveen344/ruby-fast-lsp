use log::{debug, error};
use ruby_prism::{
    GlobalVariableAndWriteNode, GlobalVariableOperatorWriteNode, GlobalVariableOrWriteNode,
    GlobalVariableTargetNode, GlobalVariableWriteNode,
};

use crate::indexer::entry::{entry_builder::EntryBuilder, entry_kind::EntryKind};
use crate::types::{
    fully_qualified_name::FullyQualifiedName,
    ruby_variable::{RubyVariable, RubyVariableType},
};

use super::IndexVisitor;

impl IndexVisitor {
    fn process_global_variable_write(&mut self, name: &[u8], name_loc: ruby_prism::Location) {
        let variable_name = String::from_utf8_lossy(name).to_string();
        debug!("Processing global variable: {}", variable_name);

        let var = RubyVariable::new(&variable_name, RubyVariableType::Global);

        match var {
            Ok(variable) => {
                // Global variables are not associated with any namespace or method
                let fqn = FullyQualifiedName::variable(
                    vec![], // No namespace for globals
                    None,   // No method context for globals
                    variable.clone(),
                );

                debug!("Adding global variable entry: {:?}", fqn);

                let entry = EntryBuilder::new()
                    .fqn(fqn)
                    .location(self.prism_loc_to_lsp_loc(name_loc))
                    .kind(EntryKind::Variable {
                        name: variable.clone(),
                    })
                    .build();

                if let Ok(entry) = entry {
                    let mut index = self.index.lock().unwrap();
                    index.add_entry(entry);
                    debug!("Added global variable entry: {:?}", variable);
                } else {
                    error!(
                        "Error creating entry for global variable: {}",
                        variable_name
                    );
                }
            }
            Err(err) => {
                error!("Invalid global variable name '{}': {}", variable_name, err);
            }
        }
    }

    // GlobalVariableWriteNode
    pub fn process_global_variable_write_node_entry(&mut self, node: &GlobalVariableWriteNode) {
        self.process_global_variable_write(node.name().as_slice(), node.name_loc());
    }

    pub fn process_global_variable_write_node_exit(&mut self, _node: &GlobalVariableWriteNode) {
        // No-op for now
    }

    // GlobalVariableTargetNode
    pub fn process_global_variable_target_node_entry(&mut self, node: &GlobalVariableTargetNode) {
        self.process_global_variable_write(node.name().as_slice(), node.location());
    }

    pub fn process_global_variable_target_node_exit(&mut self, _node: &GlobalVariableTargetNode) {
        // No-op for now
    }

    // GlobalVariableOrWriteNode
    pub fn process_global_variable_or_write_node_entry(&mut self, node: &GlobalVariableOrWriteNode) {
        self.process_global_variable_write(node.name().as_slice(), node.name_loc());
    }

    pub fn process_global_variable_or_write_node_exit(&mut self, _node: &GlobalVariableOrWriteNode) {
        // No-op for now
    }

    // GlobalVariableAndWriteNode
    pub fn process_global_variable_and_write_node_entry(
        &mut self,
        node: &GlobalVariableAndWriteNode,
    ) {
        self.process_global_variable_write(node.name().as_slice(), node.name_loc());
    }

    pub fn process_global_variable_and_write_node_exit(
        &mut self,
        _node: &GlobalVariableAndWriteNode,
    ) {
        // No-op for now
    }

    // GlobalVariableOperatorWriteNode
    pub fn process_global_variable_operator_write_node_entry(
        &mut self,
        node: &GlobalVariableOperatorWriteNode,
    ) {
        self.process_global_variable_write(node.name().as_slice(), node.name_loc());
    }

    pub fn process_global_variable_operator_write_node_exit(
        &mut self,
        _node: &GlobalVariableOperatorWriteNode,
    ) {
        // No-op for now
    }
}
