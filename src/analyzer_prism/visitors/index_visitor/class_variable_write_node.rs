use log::error;
use ruby_prism::{
    ClassVariableAndWriteNode, ClassVariableOperatorWriteNode, ClassVariableOrWriteNode,
    ClassVariableTargetNode, ClassVariableWriteNode,
};

use crate::indexer::entry::{entry_builder::EntryBuilder, entry_kind::EntryKind};
use crate::type_inference::ruby_type::RubyType;
use crate::types::fully_qualified_name::FullyQualifiedName;

use super::IndexVisitor;

impl IndexVisitor {
    fn process_class_variable_write(&mut self, name: &[u8], name_loc: ruby_prism::Location) {
        let variable_name = String::from_utf8_lossy(name).to_string();

        // Validate class variable name
        if !variable_name.starts_with("@@") {
            error!("Class variable name must start with @@: {}", variable_name);
            return;
        }

        if variable_name.len() < 3 {
            error!("Class variable name too short: {}", variable_name);
            return;
        }

        // Class variables are associated with the class/module, not with methods
        let fqn = FullyQualifiedName::class_variable(variable_name.clone()).unwrap();

        let entry = EntryBuilder::new()
            .fqn(fqn)
            .location(self.document.prism_location_to_lsp_location(&name_loc))
            .kind(EntryKind::new_class_variable(
                variable_name.clone(),
                RubyType::Unknown,
            ))
            .build();

        if let Ok(entry) = entry {
            let mut index = self.index.lock();
            index.add_entry(entry);
        } else {
            error!("Error creating entry for class variable: {}", variable_name);
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
