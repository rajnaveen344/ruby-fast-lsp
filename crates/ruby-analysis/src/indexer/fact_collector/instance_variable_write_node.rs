use log::{error, trace};
use crate::core::{FullyQualifiedName, TypeFact, TypeProvenance, TypeSubject};
use ruby_prism::{
    InstanceVariableAndWriteNode, InstanceVariableOperatorWriteNode, InstanceVariableOrWriteNode,
    InstanceVariableTargetNode, InstanceVariableWriteNode, Node,
};

use crate::inference::RubyType;

use super::FactCollector;

impl FactCollector {
    fn process_instance_variable_write(
        &mut self,
        name: &[u8],
        name_loc: ruby_prism::Location,
        value_node: Option<&Node>,
    ) {
        let variable_name = String::from_utf8_lossy(name).to_string();
        trace!("Processing instance variable: {}", variable_name);

        // Validate instance variable name
        if !variable_name.starts_with('@') {
            error!(
                "Instance variable name must start with @: {}",
                variable_name
            );
            return;
        }

        if variable_name.len() < 2 {
            error!("Instance variable name too short: {}", variable_name);
            return;
        }

        // Infer type from value if available
        let inferred_type = if let Some(value) = value_node {
            self.infer_type_from_value(value)
        } else {
            RubyType::Unknown
        };

        self.type_store.add(TypeFact::new(
            TypeSubject::InstanceVariable {
                owner: FullyQualifiedName::namespace_with_kind(
                    self.scope_tracker.get_ns_stack(),
                    self.scope_tracker.current_method_context(),
                ),
                name: variable_name.clone(),
            },
            inferred_type.clone(),
            self.document.prism_location_to_text_range(&name_loc),
            TypeProvenance::Assignment,
        ));
    }

    // InstanceVariableWriteNode
    pub fn process_instance_variable_write_node_entry(&mut self, node: &InstanceVariableWriteNode) {
        self.process_instance_variable_write(
            node.name().as_slice(),
            node.name_loc(),
            Some(&node.value()),
        );
    }

    pub fn process_instance_variable_write_node_exit(&mut self, _node: &InstanceVariableWriteNode) {
        // No-op for now
    }

    // InstanceVariableTargetNode
    pub fn process_instance_variable_target_node_entry(
        &mut self,
        node: &InstanceVariableTargetNode,
    ) {
        self.process_instance_variable_write(node.name().as_slice(), node.location(), None);
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
        self.process_instance_variable_write(
            node.name().as_slice(),
            node.name_loc(),
            Some(&node.value()),
        );
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
        self.process_instance_variable_write(
            node.name().as_slice(),
            node.name_loc(),
            Some(&node.value()),
        );
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
        self.process_instance_variable_write(
            node.name().as_slice(),
            node.name_loc(),
            Some(&node.value()),
        );
    }

    pub fn process_instance_variable_operator_write_node_exit(
        &mut self,
        _node: &InstanceVariableOperatorWriteNode,
    ) {
        // No-op for now
    }
}
