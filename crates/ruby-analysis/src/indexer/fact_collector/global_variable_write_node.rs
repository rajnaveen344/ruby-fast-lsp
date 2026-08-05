use crate::core::{FullyQualifiedName, SymbolKind, TypeFact, TypeProvenance, TypeSubject};
use log::{error, trace};
use ruby_prism::{
    GlobalVariableAndWriteNode, GlobalVariableOperatorWriteNode, GlobalVariableOrWriteNode,
    GlobalVariableTargetNode, GlobalVariableWriteNode, Node,
};

use crate::inference::RubyType;

use super::FactCollector;

impl FactCollector {
    fn process_global_variable_write(
        &mut self,
        name: &[u8],
        name_loc: ruby_prism::Location,
        value_node: Option<&Node>,
    ) {
        let variable_name = String::from_utf8_lossy(name).to_string();
        trace!("Processing global variable: {}", variable_name);

        // Validate global variable name
        if !variable_name.starts_with('$') {
            error!("Global variable name must start with $: {}", variable_name);
            return;
        }

        if variable_name.len() < 2 {
            error!("Global variable name too short: {}", variable_name);
            return;
        }
        if let Ok(fqn) = FullyQualifiedName::global_variable(variable_name.clone()) {
            self.direct_push_variable_symbol(fqn, SymbolKind::GlobalVariable, &name_loc);
        }

        // Infer type from value if available
        let inferred_type = if let Some(value) = value_node {
            self.infer_assignment_type_from_value(value)
        } else {
            RubyType::Unknown
        };
        let subject = TypeSubject::GlobalVariable(variable_name.clone());
        let range = self.document.prism_location_to_text_range(&name_loc);
        self.direct_push_assignment_type(subject.clone(), inferred_type.clone(), &name_loc);

        self.type_store.add(TypeFact::new(
            subject.clone(),
            inferred_type,
            range,
            TypeProvenance::Assignment,
        ));
        self.begin_nonlocal_write(subject, range);
    }

    // GlobalVariableWriteNode
    pub fn process_global_variable_write_node_entry(&mut self, node: &GlobalVariableWriteNode) {
        self.process_global_variable_write(
            node.name().as_slice(),
            node.name_loc(),
            Some(&node.value()),
        );
    }

    pub fn process_global_variable_write_node_exit(&mut self, _node: &GlobalVariableWriteNode) {
        self.finish_nonlocal_write();
    }

    // GlobalVariableTargetNode
    pub fn process_global_variable_target_node_entry(&mut self, node: &GlobalVariableTargetNode) {
        self.process_global_variable_write(node.name().as_slice(), node.location(), None);
    }

    pub fn process_global_variable_target_node_exit(&mut self, _node: &GlobalVariableTargetNode) {
        self.finish_nonlocal_write();
    }

    // GlobalVariableOrWriteNode
    pub fn process_global_variable_or_write_node_entry(
        &mut self,
        node: &GlobalVariableOrWriteNode,
    ) {
        self.process_global_variable_write(
            node.name().as_slice(),
            node.name_loc(),
            Some(&node.value()),
        );
    }

    pub fn process_global_variable_or_write_node_exit(
        &mut self,
        _node: &GlobalVariableOrWriteNode,
    ) {
        self.finish_nonlocal_write();
    }

    // GlobalVariableAndWriteNode
    pub fn process_global_variable_and_write_node_entry(
        &mut self,
        node: &GlobalVariableAndWriteNode,
    ) {
        self.process_global_variable_write(
            node.name().as_slice(),
            node.name_loc(),
            Some(&node.value()),
        );
    }

    pub fn process_global_variable_and_write_node_exit(
        &mut self,
        _node: &GlobalVariableAndWriteNode,
    ) {
        self.finish_nonlocal_write();
    }

    // GlobalVariableOperatorWriteNode
    pub fn process_global_variable_operator_write_node_entry(
        &mut self,
        node: &GlobalVariableOperatorWriteNode,
    ) {
        self.process_global_variable_write(
            node.name().as_slice(),
            node.name_loc(),
            Some(&node.value()),
        );
    }

    pub fn process_global_variable_operator_write_node_exit(
        &mut self,
        _node: &GlobalVariableOperatorWriteNode,
    ) {
        self.finish_nonlocal_write();
    }
}
