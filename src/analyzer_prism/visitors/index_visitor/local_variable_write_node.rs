use log::{debug, error};
use ruby_prism::{
    LocalVariableAndWriteNode, LocalVariableOperatorWriteNode, LocalVariableOrWriteNode,
    LocalVariableTargetNode, LocalVariableWriteNode, Location,
};

use crate::indexer::entry::{entry_builder::EntryBuilder, entry_kind::EntryKind};
use crate::types::{
    fully_qualified_name::FullyQualifiedName,
    ruby_variable::{RubyVariable, RubyVariableType},
};

use super::IndexVisitor;

impl IndexVisitor {
    fn process_local_variable_write(&mut self, name: &[u8], name_loc: Location) {
        let variable_name = String::from_utf8_lossy(name).to_string();
        debug!("Processing local variable: {}", variable_name);

        let var = RubyVariable::new(
            &variable_name,
            RubyVariableType::Local(self.scope_stack.clone()),
        );

        match var {
            Ok(variable) => {
                let fqn = FullyQualifiedName::variable(
                    self.current_namespace(),
                    self.current_method.clone(),
                    variable.clone(),
                );

                debug!("Adding local variable entry: {:?}", fqn);

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

    // LocalVariableWriteNode
    pub fn process_local_variable_write_node_entry(&mut self, node: &LocalVariableWriteNode) {
        self.process_local_variable_write(node.name().as_slice(), node.name_loc());
    }

    pub fn process_local_variable_write_node_exit(&mut self, _node: &LocalVariableWriteNode) {
        // No-op for now
    }

    // LocalVariableTargetNode
    pub fn process_local_variable_target_node_entry(&mut self, node: &LocalVariableTargetNode) {
        self.process_local_variable_write(node.name().as_slice(), node.location());
    }

    pub fn process_local_variable_target_node_exit(&mut self, _node: &LocalVariableTargetNode) {
        // No-op for now
    }

    // LocalVariableOrWriteNode
    pub fn process_local_variable_or_write_node_entry(&mut self, node: &LocalVariableOrWriteNode) {
        self.process_local_variable_write(node.name().as_slice(), node.name_loc());
    }

    pub fn process_local_variable_or_write_node_exit(&mut self, _node: &LocalVariableOrWriteNode) {
        // No-op for now
    }

    // LocalVariableAndWriteNode
    pub fn process_local_variable_and_write_node_entry(
        &mut self,
        node: &LocalVariableAndWriteNode,
    ) {
        self.process_local_variable_write(node.name().as_slice(), node.name_loc());
    }

    pub fn process_local_variable_and_write_node_exit(
        &mut self,
        _node: &LocalVariableAndWriteNode,
    ) {
        // No-op for now
    }

    // LocalVariableOperatorWriteNode
    pub fn process_local_variable_operator_write_node_entry(
        &mut self,
        node: &LocalVariableOperatorWriteNode,
    ) {
        self.process_local_variable_write(node.name().as_slice(), node.name_loc());
    }

    pub fn process_local_variable_operator_write_node_exit(
        &mut self,
        _node: &LocalVariableOperatorWriteNode,
    ) {
        // No-op for now
    }
}
