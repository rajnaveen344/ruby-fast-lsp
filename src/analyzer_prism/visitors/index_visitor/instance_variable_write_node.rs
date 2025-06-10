use log::{debug, error};
use ruby_prism::{
    InstanceVariableAndWriteNode, InstanceVariableOperatorWriteNode, InstanceVariableOrWriteNode,
    InstanceVariableTargetNode, InstanceVariableWriteNode,
};

use crate::indexer::entry::{entry_builder::EntryBuilder, entry_kind::EntryKind};
use crate::types::{
    fully_qualified_name::FullyQualifiedName,
    ruby_variable::{RubyVariable, RubyVariableType},
};

use super::IndexVisitor;

impl IndexVisitor {
    fn process_instance_variable_write(&mut self, name: &[u8], name_loc: ruby_prism::Location) {
        let variable_name = String::from_utf8_lossy(name).to_string();
        debug!("Processing instance variable: {}", variable_name);

        let var = RubyVariable::new(&variable_name, RubyVariableType::Instance);

        match var {
            Ok(variable) => {
                // Instance variables are associated with the class/module, not with methods
                let fqn = FullyQualifiedName::variable(
                    self.namespace_stack.clone(),
                    None, // No method context for instance variables
                    variable.clone(),
                );

                debug!("Adding instance variable entry: {:?}", fqn);

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

    // InstanceVariableWriteNode
    pub fn process_instance_variable_write_node_entry(&mut self, node: &InstanceVariableWriteNode) {
        self.process_instance_variable_write(node.name().as_slice(), node.name_loc());
    }

    pub fn process_instance_variable_write_node_exit(&mut self, _node: &InstanceVariableWriteNode) {
        // No-op for now
    }

    // InstanceVariableTargetNode
    pub fn process_instance_variable_target_node_entry(
        &mut self,
        node: &InstanceVariableTargetNode,
    ) {
        self.process_instance_variable_write(node.name().as_slice(), node.location());
    }

    pub fn process_instance_variable_target_node_exit(
        &mut self,
        _node: &InstanceVariableTargetNode,
    ) {
        // No-op for now
    }

    // InstanceVariableOrWriteNode
    pub fn process_instance_variable_or_write_node_entry(
        &mut self,
        node: &InstanceVariableOrWriteNode,
    ) {
        self.process_instance_variable_write(node.name().as_slice(), node.name_loc());
    }

    pub fn process_instance_variable_or_write_node_exit(
        &mut self,
        _node: &InstanceVariableOrWriteNode,
    ) {
        // No-op for now
    }

    // InstanceVariableAndWriteNode
    pub fn process_instance_variable_and_write_node_entry(
        &mut self,
        node: &InstanceVariableAndWriteNode,
    ) {
        self.process_instance_variable_write(node.name().as_slice(), node.name_loc());
    }

    pub fn process_instance_variable_and_write_node_exit(
        &mut self,
        _node: &InstanceVariableAndWriteNode,
    ) {
        // No-op for now
    }

    // InstanceVariableOperatorWriteNode
    pub fn process_instance_variable_operator_write_node_entry(
        &mut self,
        node: &InstanceVariableOperatorWriteNode,
    ) {
        self.process_instance_variable_write(node.name().as_slice(), node.name_loc());
    }

    pub fn process_instance_variable_operator_write_node_exit(
        &mut self,
        _node: &InstanceVariableOperatorWriteNode,
    ) {
        // No-op for now
    }
}
