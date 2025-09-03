use log::{debug, error};
use ruby_prism::{
    GlobalVariableAndWriteNode, GlobalVariableOperatorWriteNode, GlobalVariableOrWriteNode,
    GlobalVariableTargetNode, GlobalVariableWriteNode,
};

use crate::indexer::entry::{entry_builder::EntryBuilder, entry_kind::EntryKind};
use crate::type_inference::ruby_type::RubyType;
use crate::types::fully_qualified_name::FullyQualifiedName;

use super::IndexVisitor;

impl IndexVisitor {
    fn process_global_variable_write(&mut self, name: &[u8], name_loc: ruby_prism::Location) {
        let variable_name = String::from_utf8_lossy(name).to_string();
        debug!("Processing global variable: {}", variable_name);

        // Validate global variable name
        if !variable_name.starts_with('$') {
            error!("Global variable name must start with $: {}", variable_name);
            return;
        }

        if variable_name.len() < 2 {
            error!("Global variable name too short: {}", variable_name);
            return;
        }

        // Global variables are not associated with any namespace or method
        let fqn = FullyQualifiedName::global_variable(variable_name.clone()).unwrap();

        debug!("Adding global variable entry: {:?}", fqn);

        let entry = EntryBuilder::new()
            .fqn(fqn)
            .location(self.document.prism_location_to_lsp_location(&name_loc))
            .kind(EntryKind::new_global_variable(
                variable_name.clone(),
                RubyType::Unknown,
            ))
            .build();

        if let Ok(entry) = entry {
            let mut index = self.index.lock();
            index.add_entry(entry);
            debug!("Added global variable entry: {}", variable_name);
        } else {
            error!(
                "Error creating entry for global variable: {}",
                variable_name
            );
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
    pub fn process_global_variable_or_write_node_entry(
        &mut self,
        node: &GlobalVariableOrWriteNode,
    ) {
        self.process_global_variable_write(node.name().as_slice(), node.name_loc());
    }

    pub fn process_global_variable_or_write_node_exit(
        &mut self,
        _node: &GlobalVariableOrWriteNode,
    ) {
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
