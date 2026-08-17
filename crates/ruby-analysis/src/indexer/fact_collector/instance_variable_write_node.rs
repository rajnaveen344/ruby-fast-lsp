use crate::core::{FullyQualifiedName, SymbolKind, TypeFact, TypeProvenance, TypeSubject};
use log::{error, trace};
use ruby_prism::{
    InstanceVariableAndWriteNode, InstanceVariableOperatorWriteNode, InstanceVariableOrWriteNode,
    InstanceVariableTargetNode, InstanceVariableWriteNode, Node,
};

use crate::inference::RubyType;

use super::FactCollector;

impl FactCollector {
    fn parsed_instance_variable_name(name: &[u8]) -> Option<String> {
        let variable_name = String::from_utf8_lossy(name).to_string();
        trace!("Processing instance variable: {}", variable_name);

        if !variable_name.starts_with('@') {
            error!(
                "Instance variable name must start with @: {}",
                variable_name
            );
            return None;
        }

        if variable_name.len() < 2 {
            error!("Instance variable name too short: {}", variable_name);
            return None;
        }

        Some(variable_name)
    }

    fn instance_variable_subject(&self, variable_name: String) -> TypeSubject {
        let owner = FullyQualifiedName::namespace_with_kind(
            self.scope_tracker.get_ns_stack(),
            self.scope_tracker.current_method_context(),
        );
        TypeSubject::InstanceVariable {
            owner,
            name: variable_name,
        }
    }

    fn declare_instance_variable_write(&mut self, name: &[u8], name_loc: ruby_prism::Location) {
        let Some(variable_name) = Self::parsed_instance_variable_name(name) else {
            return;
        };
        if let Ok(fqn) = FullyQualifiedName::instance_variable(variable_name.clone()) {
            self.direct_push_variable_symbol(fqn, SymbolKind::InstanceVariable, &name_loc);
        }
        let subject = self.instance_variable_subject(variable_name);
        let range = self.document.prism_location_to_text_range(&name_loc);
        self.begin_nonlocal_write(subject, range);
    }

    fn bind_instance_variable_write(
        &mut self,
        name: &[u8],
        name_loc: ruby_prism::Location,
        value_node: Option<&Node>,
    ) {
        let Some(variable_name) = Self::parsed_instance_variable_name(name) else {
            return;
        };
        let inferred_type = if let Some(value) = value_node {
            self.infer_assignment_type_from_value(value)
        } else {
            RubyType::Unknown
        };
        let subject = self.instance_variable_subject(variable_name);
        let range = self.document.prism_location_to_text_range(&name_loc);
        self.direct_push_assignment_type(subject.clone(), inferred_type.clone(), &name_loc);

        self.type_store.add(TypeFact::new(
            subject,
            inferred_type,
            range,
            TypeProvenance::Assignment,
        ));
    }

    // InstanceVariableWriteNode
    pub fn process_instance_variable_write_node_entry(&mut self, node: &InstanceVariableWriteNode) {
        self.declare_instance_variable_write(node.name().as_slice(), node.name_loc());
    }

    pub fn process_instance_variable_write_node_exit(&mut self, node: &InstanceVariableWriteNode) {
        self.bind_instance_variable_write(
            node.name().as_slice(),
            node.name_loc(),
            Some(&node.value()),
        );
        self.finish_nonlocal_write();
    }

    // InstanceVariableTargetNode
    pub fn process_instance_variable_target_node_entry(
        &mut self,
        node: &InstanceVariableTargetNode,
    ) {
        self.declare_instance_variable_write(node.name().as_slice(), node.location());
        self.bind_instance_variable_write(node.name().as_slice(), node.location(), None);
    }

    pub fn process_instance_variable_target_node_exit(
        &mut self,
        _node: &InstanceVariableTargetNode,
    ) {
        self.finish_nonlocal_write();
    }

    // InstanceVariableOrWriteNode
    pub fn process_instance_variable_or_write_node_entry(
        &mut self,
        node: &InstanceVariableOrWriteNode,
    ) {
        self.declare_instance_variable_write(node.name().as_slice(), node.name_loc());
    }

    pub fn process_instance_variable_or_write_node_exit(
        &mut self,
        node: &InstanceVariableOrWriteNode,
    ) {
        self.bind_instance_variable_write(
            node.name().as_slice(),
            node.name_loc(),
            Some(&node.value()),
        );
        self.finish_nonlocal_write();
    }

    // InstanceVariableAndWriteNode
    pub fn process_instance_variable_and_write_node_entry(
        &mut self,
        node: &InstanceVariableAndWriteNode,
    ) {
        self.declare_instance_variable_write(node.name().as_slice(), node.name_loc());
    }

    pub fn process_instance_variable_and_write_node_exit(
        &mut self,
        node: &InstanceVariableAndWriteNode,
    ) {
        self.bind_instance_variable_write(
            node.name().as_slice(),
            node.name_loc(),
            Some(&node.value()),
        );
        self.finish_nonlocal_write();
    }

    // InstanceVariableOperatorWriteNode
    pub fn process_instance_variable_operator_write_node_entry(
        &mut self,
        node: &InstanceVariableOperatorWriteNode,
    ) {
        self.declare_instance_variable_write(node.name().as_slice(), node.name_loc());
    }

    pub fn process_instance_variable_operator_write_node_exit(
        &mut self,
        node: &InstanceVariableOperatorWriteNode,
    ) {
        self.bind_instance_variable_write(
            node.name().as_slice(),
            node.name_loc(),
            Some(&node.value()),
        );
        self.finish_nonlocal_write();
    }
}
