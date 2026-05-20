use log::error;
use ruby_analysis_core::{TypeFact, TypeProvenance, TypeSubject};
use ruby_prism::{
    ClassVariableAndWriteNode, ClassVariableOperatorWriteNode, ClassVariableOrWriteNode,
    ClassVariableTargetNode, ClassVariableWriteNode, Node,
};

use crate::types::fully_qualified_name::FullyQualifiedName;
use ruby_analysis_inference::RubyType;

use super::FactCollector;

impl FactCollector {
    fn process_class_variable_write(
        &mut self,
        name: &[u8],
        name_loc: ruby_prism::Location,
        value_node: Option<&Node>,
    ) {
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

        // Infer type from value if available
        let inferred_type = if let Some(value) = value_node {
            self.infer_type_from_value(value)
        } else {
            RubyType::Unknown
        };

        self.type_store.add(TypeFact::new(
            TypeSubject::ClassVariable {
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

    // ClassVariableWriteNode
    pub fn process_class_variable_write_node_entry(&mut self, node: &ClassVariableWriteNode) {
        self.process_class_variable_write(
            node.name().as_slice(),
            node.name_loc(),
            Some(&node.value()),
        );
    }

    pub fn process_class_variable_write_node_exit(&mut self, _node: &ClassVariableWriteNode) {
        // No-op for now
    }

    // ClassVariableTargetNode
    pub fn process_class_variable_target_node_entry(&mut self, node: &ClassVariableTargetNode) {
        self.process_class_variable_write(node.name().as_slice(), node.location(), None);
    }

    pub fn process_class_variable_target_node_exit(&mut self, _node: &ClassVariableTargetNode) {
        // No-op for now
    }

    // ClassVariableOrWriteNode
    pub fn process_class_variable_or_write_node_entry(&mut self, node: &ClassVariableOrWriteNode) {
        self.process_class_variable_write(
            node.name().as_slice(),
            node.name_loc(),
            Some(&node.value()),
        );
    }

    pub fn process_class_variable_or_write_node_exit(&mut self, _node: &ClassVariableOrWriteNode) {
        // No-op for now
    }

    // ClassVariableAndWriteNode
    pub fn process_class_variable_and_write_node_entry(
        &mut self,
        node: &ClassVariableAndWriteNode,
    ) {
        self.process_class_variable_write(
            node.name().as_slice(),
            node.name_loc(),
            Some(&node.value()),
        );
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
        self.process_class_variable_write(
            node.name().as_slice(),
            node.name_loc(),
            Some(&node.value()),
        );
    }

    pub fn process_class_variable_operator_write_node_exit(
        &mut self,
        _node: &ClassVariableOperatorWriteNode,
    ) {
        // No-op for now
    }
}
