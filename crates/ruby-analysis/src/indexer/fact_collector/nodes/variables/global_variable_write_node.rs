use crate::core::{FullyQualifiedName, SymbolKind, TypeFact, TypeProvenance, TypeSubject};
use log::{error, trace};
use ruby_prism::{
    GlobalVariableAndWriteNode, GlobalVariableOperatorWriteNode, GlobalVariableOrWriteNode,
    GlobalVariableTargetNode, GlobalVariableWriteNode, Node,
};

use crate::core::RubyType;

use crate::indexer::fact_collector::FactCollector;

impl FactCollector {
    fn parsed_global_variable_name(name: &[u8]) -> Option<String> {
        let variable_name = String::from_utf8_lossy(name).to_string();
        trace!("Processing global variable: {}", variable_name);

        if !variable_name.starts_with('$') {
            error!("Global variable name must start with $: {}", variable_name);
            return None;
        }

        if variable_name.len() < 2 {
            error!("Global variable name too short: {}", variable_name);
            return None;
        }

        Some(variable_name)
    }

    fn declare_global_variable_write(&mut self, name: &[u8], name_loc: ruby_prism::Location) {
        let Some(variable_name) = Self::parsed_global_variable_name(name) else {
            return;
        };
        if let Ok(fqn) = FullyQualifiedName::global_variable(variable_name.clone()) {
            self.direct_push_variable_symbol(fqn, SymbolKind::GlobalVariable, &name_loc);
        }
        let subject = TypeSubject::GlobalVariable(variable_name);
        let range = self.document.prism_location_to_text_range(&name_loc);
        self.begin_nonlocal_write(subject, range);
    }

    fn bind_global_variable_write(
        &mut self,
        name: &[u8],
        name_loc: ruby_prism::Location,
        value_node: Option<&Node>,
    ) {
        let Some(variable_name) = Self::parsed_global_variable_name(name) else {
            return;
        };
        let inferred_type = if let Some(value) = value_node {
            self.infer_assignment_type_from_value(value)
        } else {
            RubyType::Unknown
        };
        let subject = TypeSubject::GlobalVariable(variable_name);
        let range = self.document.prism_location_to_text_range(&name_loc);
        self.direct_push_assignment_type(subject.clone(), inferred_type.clone(), &name_loc);

        self.facts.types.add(TypeFact::new(
            subject,
            inferred_type,
            range,
            TypeProvenance::Assignment,
        ));
    }

    // GlobalVariableWriteNode
    pub(in crate::indexer::fact_collector) fn process_global_variable_write_node_entry(
        &mut self,
        node: &GlobalVariableWriteNode,
    ) {
        self.declare_global_variable_write(node.name().as_slice(), node.name_loc());
    }

    pub(in crate::indexer::fact_collector) fn process_global_variable_write_node_exit(
        &mut self,
        node: &GlobalVariableWriteNode,
    ) {
        self.bind_global_variable_write(
            node.name().as_slice(),
            node.name_loc(),
            Some(&node.value()),
        );
        self.finish_nonlocal_write();
    }

    // GlobalVariableTargetNode
    pub(in crate::indexer::fact_collector) fn process_global_variable_target_node_entry(
        &mut self,
        node: &GlobalVariableTargetNode,
    ) {
        self.declare_global_variable_write(node.name().as_slice(), node.location());
        self.bind_global_variable_write(node.name().as_slice(), node.location(), None);
    }

    pub(in crate::indexer::fact_collector) fn process_global_variable_target_node_exit(
        &mut self,
        _node: &GlobalVariableTargetNode,
    ) {
        self.finish_nonlocal_write();
    }

    // GlobalVariableOrWriteNode
    pub(in crate::indexer::fact_collector) fn process_global_variable_or_write_node_entry(
        &mut self,
        node: &GlobalVariableOrWriteNode,
    ) {
        self.declare_global_variable_write(node.name().as_slice(), node.name_loc());
    }

    pub(in crate::indexer::fact_collector) fn process_global_variable_or_write_node_exit(
        &mut self,
        node: &GlobalVariableOrWriteNode,
    ) {
        self.bind_global_variable_write(
            node.name().as_slice(),
            node.name_loc(),
            Some(&node.value()),
        );
        self.finish_nonlocal_write();
    }

    // GlobalVariableAndWriteNode
    pub(in crate::indexer::fact_collector) fn process_global_variable_and_write_node_entry(
        &mut self,
        node: &GlobalVariableAndWriteNode,
    ) {
        self.declare_global_variable_write(node.name().as_slice(), node.name_loc());
    }

    pub(in crate::indexer::fact_collector) fn process_global_variable_and_write_node_exit(
        &mut self,
        node: &GlobalVariableAndWriteNode,
    ) {
        self.bind_global_variable_write(
            node.name().as_slice(),
            node.name_loc(),
            Some(&node.value()),
        );
        self.finish_nonlocal_write();
    }

    // GlobalVariableOperatorWriteNode
    pub(in crate::indexer::fact_collector) fn process_global_variable_operator_write_node_entry(
        &mut self,
        node: &GlobalVariableOperatorWriteNode,
    ) {
        self.declare_global_variable_write(node.name().as_slice(), node.name_loc());
    }

    pub(in crate::indexer::fact_collector) fn process_global_variable_operator_write_node_exit(
        &mut self,
        node: &GlobalVariableOperatorWriteNode,
    ) {
        self.bind_global_variable_write(
            node.name().as_slice(),
            node.name_loc(),
            Some(&node.value()),
        );
        self.finish_nonlocal_write();
    }
}
