use crate::core::{FullyQualifiedName, SymbolKind, TypeFact, TypeProvenance, TypeSubject};
use log::error;
use ruby_prism::{
    ClassVariableAndWriteNode, ClassVariableOperatorWriteNode, ClassVariableOrWriteNode,
    ClassVariableTargetNode, ClassVariableWriteNode, Node,
};

use crate::inference::RubyType;

use super::FactCollector;

impl FactCollector {
    fn parsed_class_variable_name(name: &[u8]) -> Option<String> {
        let variable_name = String::from_utf8_lossy(name).to_string();

        if !variable_name.starts_with("@@") {
            error!("Class variable name must start with @@: {}", variable_name);
            return None;
        }

        if variable_name.len() < 3 {
            error!("Class variable name too short: {}", variable_name);
            return None;
        }

        Some(variable_name)
    }

    fn class_variable_subject(&self, variable_name: String) -> TypeSubject {
        let owner = FullyQualifiedName::namespace_with_kind(
            self.scope_tracker.get_ns_stack(),
            self.scope_tracker.current_method_context(),
        );
        TypeSubject::ClassVariable {
            owner,
            name: variable_name,
        }
    }

    fn declare_class_variable_write(&mut self, name: &[u8], name_loc: ruby_prism::Location) {
        let Some(variable_name) = Self::parsed_class_variable_name(name) else {
            return;
        };
        if let Ok(fqn) = FullyQualifiedName::class_variable(variable_name.clone()) {
            self.direct_push_variable_symbol(fqn, SymbolKind::ClassVariable, &name_loc);
        }
        let subject = self.class_variable_subject(variable_name);
        let range = self.document.prism_location_to_text_range(&name_loc);
        self.begin_nonlocal_write(subject, range);
    }

    fn bind_class_variable_write(
        &mut self,
        name: &[u8],
        name_loc: ruby_prism::Location,
        value_node: Option<&Node>,
    ) {
        let Some(variable_name) = Self::parsed_class_variable_name(name) else {
            return;
        };
        let inferred_type = if let Some(value) = value_node {
            self.infer_assignment_type_from_value(value)
        } else {
            RubyType::Unknown
        };
        let subject = self.class_variable_subject(variable_name);
        let range = self.document.prism_location_to_text_range(&name_loc);
        self.direct_push_assignment_type(subject.clone(), inferred_type.clone(), &name_loc);

        self.type_store.add(TypeFact::new(
            subject,
            inferred_type,
            range,
            TypeProvenance::Assignment,
        ));
    }

    // ClassVariableWriteNode
    pub fn process_class_variable_write_node_entry(&mut self, node: &ClassVariableWriteNode) {
        self.declare_class_variable_write(node.name().as_slice(), node.name_loc());
    }

    pub fn process_class_variable_write_node_exit(&mut self, node: &ClassVariableWriteNode) {
        self.bind_class_variable_write(
            node.name().as_slice(),
            node.name_loc(),
            Some(&node.value()),
        );
        self.finish_nonlocal_write();
    }

    // ClassVariableTargetNode
    pub fn process_class_variable_target_node_entry(&mut self, node: &ClassVariableTargetNode) {
        self.declare_class_variable_write(node.name().as_slice(), node.location());
        self.bind_class_variable_write(node.name().as_slice(), node.location(), None);
    }

    pub fn process_class_variable_target_node_exit(&mut self, _node: &ClassVariableTargetNode) {
        self.finish_nonlocal_write();
    }

    // ClassVariableOrWriteNode
    pub fn process_class_variable_or_write_node_entry(&mut self, node: &ClassVariableOrWriteNode) {
        self.declare_class_variable_write(node.name().as_slice(), node.name_loc());
    }

    pub fn process_class_variable_or_write_node_exit(&mut self, node: &ClassVariableOrWriteNode) {
        self.bind_class_variable_write(
            node.name().as_slice(),
            node.name_loc(),
            Some(&node.value()),
        );
        self.finish_nonlocal_write();
    }

    // ClassVariableAndWriteNode
    pub fn process_class_variable_and_write_node_entry(
        &mut self,
        node: &ClassVariableAndWriteNode,
    ) {
        self.declare_class_variable_write(node.name().as_slice(), node.name_loc());
    }

    pub fn process_class_variable_and_write_node_exit(&mut self, node: &ClassVariableAndWriteNode) {
        self.bind_class_variable_write(
            node.name().as_slice(),
            node.name_loc(),
            Some(&node.value()),
        );
        self.finish_nonlocal_write();
    }

    // ClassVariableOperatorWriteNode
    pub fn process_class_variable_operator_write_node_entry(
        &mut self,
        node: &ClassVariableOperatorWriteNode,
    ) {
        self.declare_class_variable_write(node.name().as_slice(), node.name_loc());
    }

    pub fn process_class_variable_operator_write_node_exit(
        &mut self,
        node: &ClassVariableOperatorWriteNode,
    ) {
        self.bind_class_variable_write(
            node.name().as_slice(),
            node.name_loc(),
            Some(&node.value()),
        );
        self.finish_nonlocal_write();
    }
}
