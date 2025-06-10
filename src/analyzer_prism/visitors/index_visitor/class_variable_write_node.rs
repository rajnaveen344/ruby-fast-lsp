use log::{debug, error};
use ruby_prism::{
    ClassVariableAndWriteNode, ClassVariableOperatorWriteNode, ClassVariableOrWriteNode,
    ClassVariableTargetNode, ClassVariableWriteNode,
};

use crate::indexer::entry::{entry_builder::EntryBuilder, entry_kind::EntryKind};
use crate::types::{
    fully_qualified_name::FullyQualifiedName,
    ruby_variable::{RubyVariable, RubyVariableType},
};

use super::IndexVisitor;

impl IndexVisitor {
    fn process_class_variable_write(&mut self, name: &[u8], name_loc: ruby_prism::Location) {
        let variable_name = String::from_utf8_lossy(name).to_string();
        debug!("Processing class variable: {}", variable_name);

        let var = RubyVariable::new(&variable_name, RubyVariableType::Class);

        match var {
            Ok(variable) => {
                // Class variables are associated with the class/module, not with methods
                let fqn = FullyQualifiedName::variable(
                    self.namespace_stack.clone(),
                    None, // No method context for class variables
                    variable.clone(),
                );

                debug!("Adding class variable entry: {:?}", fqn);

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
                    debug!("Added class variable entry: {:?}", variable);
                } else {
                    error!("Error creating entry for class variable: {}", variable_name);
                }
            }
            Err(err) => {
                error!("Invalid class variable name '{}': {}", variable_name, err);
            }
        }
    }

    // ClassVariableWriteNode
    pub fn process_class_variable_write_node_entry(&mut self, node: &ClassVariableWriteNode) {
        self.process_class_variable_write(node.name().as_slice(), node.name_loc());
    }

    pub fn process_class_variable_write_node_exit(&mut self, _node: &ClassVariableWriteNode) {
        // No-op for now
    }

    // ClassVariableTargetNode
    pub fn process_class_variable_target_node_entry(&mut self, node: &ClassVariableTargetNode) {
        self.process_class_variable_write(node.name().as_slice(), node.location());
    }

    pub fn process_class_variable_target_node_exit(&mut self, _node: &ClassVariableTargetNode) {
        // No-op for now
    }

    // ClassVariableOrWriteNode
    pub fn process_class_variable_or_write_node_entry(&mut self, node: &ClassVariableOrWriteNode) {
        self.process_class_variable_write(node.name().as_slice(), node.name_loc());
    }

    pub fn process_class_variable_or_write_node_exit(&mut self, _node: &ClassVariableOrWriteNode) {
        // No-op for now
    }

    // ClassVariableAndWriteNode
    pub fn process_class_variable_and_write_node_entry(
        &mut self,
        node: &ClassVariableAndWriteNode,
    ) {
        self.process_class_variable_write(node.name().as_slice(), node.name_loc());
    }

    pub fn process_class_variable_and_write_node_exit(
        &mut self,
        _node: &ClassVariableAndWriteNode,
    ) {
        // No-op for now
    }

    // ClassVariableOperatorWriteNode
    pub fn process_class_variable_operator_write_node_entry(
        &mut self,
        node: &ClassVariableOperatorWriteNode,
    ) {
        self.process_class_variable_write(node.name().as_slice(), node.name_loc());
    }

    pub fn process_class_variable_operator_write_node_exit(
        &mut self,
        _node: &ClassVariableOperatorWriteNode,
    ) {
        // No-op for now
    }
}
