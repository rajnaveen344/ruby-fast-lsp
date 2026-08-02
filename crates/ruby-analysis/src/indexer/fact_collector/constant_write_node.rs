use crate::core::{
    FullyQualifiedName, RubyConstant, SymbolFact, SymbolKind, TypeFact, TypeProvenance,
    TypeSubject,
};
use log::{error, trace};
use ruby_prism::{
    ConstantAndWriteNode, ConstantOperatorWriteNode, ConstantOrWriteNode, ConstantTargetNode,
    ConstantWriteNode, Location, Node,
};

use super::FactCollector;
use crate::inference::RubyType;

impl FactCollector {
    fn constant_fqn_from_name(&self, constant_name: &str) -> Option<FullyQualifiedName> {
        let constant = match RubyConstant::new(constant_name) {
            Ok(constant) => constant,
            Err(e) => {
                error!("Error creating constant: {}", e);
                return None;
            }
        };
        let mut namespace = self.scope_tracker.get_ns_stack();
        namespace.push(constant);
        Some(FullyQualifiedName::constant(namespace))
    }

    fn record_constant_symbol(
        &mut self,
        fqn: FullyQualifiedName,
        full_location: &Location<'_>,
        name_location: &Location<'_>,
    ) {
        self.direct_facts.symbols.push(
            SymbolFact::new(
                fqn,
                SymbolKind::Constant,
                self.direct_range(full_location),
            )
            .with_name_range(self.direct_range(name_location)),
        );
    }

    fn record_constant_value_type(
        &mut self,
        fqn: FullyQualifiedName,
        value: &Node<'_>,
        name_location: &Location<'_>,
        full_location: &Location<'_>,
    ) {
        let (inferred_type, provenance) = self.assignment_type_and_provenance(value);
        self.direct_push_type(
            TypeSubject::Constant(fqn.clone()),
            inferred_type.clone(),
            name_location,
            provenance,
        );
        self.type_store.add(TypeFact::new(
            TypeSubject::Constant(fqn),
            inferred_type,
            self.document.prism_location_to_text_range(full_location),
            provenance,
        ));
    }

    fn record_constant_value_type_explicit(
        &mut self,
        fqn: FullyQualifiedName,
        inferred_type: RubyType,
        name_location: &Location<'_>,
        full_location: &Location<'_>,
    ) {
        let provenance = TypeProvenance::Assignment;
        self.direct_push_type(
            TypeSubject::Constant(fqn.clone()),
            inferred_type.clone(),
            name_location,
            provenance,
        );
        self.type_store.add(TypeFact::new(
            TypeSubject::Constant(fqn),
            inferred_type,
            self.document.prism_location_to_text_range(full_location),
            provenance,
        ));
    }

    pub fn process_constant_write_node_entry(&mut self, node: &ConstantWriteNode) {
        let constant_name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        trace!("Visiting constant write node: {}", constant_name);
        let Some(fqn) = self.constant_fqn_from_name(&constant_name) else {
            return;
        };
        self.record_constant_symbol(fqn, &node.location(), &node.name_loc());
    }

    pub fn process_constant_write_node_exit(&mut self, node: &ConstantWriteNode) {
        let constant_name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        let Some(fqn) = self.constant_fqn_from_name(&constant_name) else {
            return;
        };
        self.record_constant_value_type(fqn, &node.value(), &node.name_loc(), &node.location());
    }

    pub fn process_constant_or_write_node_entry(&mut self, node: &ConstantOrWriteNode) {
        let constant_name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        let Some(fqn) = self.constant_fqn_from_name(&constant_name) else {
            return;
        };
        self.record_constant_symbol(fqn, &node.location(), &node.name_loc());
    }

    pub fn process_constant_or_write_node_exit(&mut self, node: &ConstantOrWriteNode) {
        let constant_name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        let Some(fqn) = self.constant_fqn_from_name(&constant_name) else {
            return;
        };
        self.record_constant_value_type(fqn, &node.value(), &node.name_loc(), &node.location());
    }

    pub fn process_constant_and_write_node_entry(&mut self, node: &ConstantAndWriteNode) {
        let constant_name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        let Some(fqn) = self.constant_fqn_from_name(&constant_name) else {
            return;
        };
        self.record_constant_symbol(fqn, &node.location(), &node.name_loc());
    }

    pub fn process_constant_and_write_node_exit(&mut self, node: &ConstantAndWriteNode) {
        let constant_name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        let Some(fqn) = self.constant_fqn_from_name(&constant_name) else {
            return;
        };
        self.record_constant_value_type(fqn, &node.value(), &node.name_loc(), &node.location());
    }

    pub fn process_constant_operator_write_node_entry(
        &mut self,
        node: &ConstantOperatorWriteNode,
    ) {
        let constant_name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        let Some(fqn) = self.constant_fqn_from_name(&constant_name) else {
            return;
        };
        self.record_constant_symbol(fqn, &node.location(), &node.name_loc());
    }

    pub fn process_constant_operator_write_node_exit(
        &mut self,
        node: &ConstantOperatorWriteNode,
    ) {
        let constant_name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        let Some(fqn) = self.constant_fqn_from_name(&constant_name) else {
            return;
        };
        self.record_constant_value_type(fqn, &node.value(), &node.name_loc(), &node.location());
    }

    pub fn process_constant_target_node_entry(&mut self, node: &ConstantTargetNode) {
        let constant_name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        let Some(fqn) = self.constant_fqn_from_name(&constant_name) else {
            return;
        };
        self.record_constant_symbol(fqn.clone(), &node.location(), &node.location());

        let inferred_type = self
            .multi_write_lhs_types
            .last_mut()
            .and_then(|types| {
                if types.is_empty() {
                    None
                } else {
                    Some(types.remove(0))
                }
            })
            .unwrap_or(RubyType::Unknown);
        self.record_constant_value_type_explicit(
            fqn,
            inferred_type,
            &node.location(),
            &node.location(),
        );
    }

    pub fn process_constant_target_node_exit(&mut self, _node: &ConstantTargetNode) {}
}
