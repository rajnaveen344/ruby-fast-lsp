use crate::collect_namespaces;
use crate::core::{
    FullyQualifiedName, RubyConstant, SymbolFact, SymbolKind, TypeFact, TypeSubject,
};
use log::error;
use ruby_prism::{
    ConstantPathAndWriteNode, ConstantPathNode, ConstantPathOperatorWriteNode,
    ConstantPathOrWriteNode, ConstantPathWriteNode, Location, Node,
};

use super::FactCollector;

impl FactCollector {
    fn constant_path_write_fqn(
        &self,
        constant_path: &ConstantPathNode<'_>,
    ) -> Option<(String, FullyQualifiedName)> {
        let constant_name = match constant_path.name() {
            Some(name) => String::from_utf8_lossy(name.as_slice()).to_string(),
            None => {
                error!("Could not extract constant name from constant path write target");
                return None;
            }
        };

        let constant = match RubyConstant::new(&constant_name) {
            Ok(constant) => constant,
            Err(e) => {
                error!("Error creating constant: {}", e);
                return None;
            }
        };

        let mut namespace_parts = Vec::new();
        collect_namespaces(constant_path, &mut namespace_parts);

        let mut fqn_parts = self.scope_tracker.get_ns_stack();
        fqn_parts.extend(namespace_parts);
        assert!(
            fqn_parts.last() == Some(&constant),
            "INVARIANT VIOLATED: constant path write target `{}` did not end with its name. \
             This is a bug because Prism target path collection must preserve the written constant. \
             Fix: inspect collect_namespaces for ConstantPathWriteNode targets.",
            constant_name
        );

        Some((constant_name, FullyQualifiedName::constant(fqn_parts)))
    }

    fn record_constant_path_symbol(
        &mut self,
        fqn: FullyQualifiedName,
        constant_path: &ConstantPathNode<'_>,
        constant_name: &str,
        full_location: &Location<'_>,
    ) {
        self.direct_facts.symbols.push(
            SymbolFact::new(fqn, SymbolKind::Constant, self.direct_range(full_location))
                .with_name_range(self.direct_terminal_name_range(
                    &constant_path.location(),
                    constant_name.as_bytes(),
                )),
        );
    }

    fn record_constant_path_value_type(
        &mut self,
        fqn: FullyQualifiedName,
        value: &Node<'_>,
        constant_path: &ConstantPathNode<'_>,
        full_location: &Location<'_>,
    ) {
        let (inferred_type, provenance) = self.assignment_type_and_provenance(value);
        self.direct_push_type(
            TypeSubject::Constant(fqn.clone()),
            inferred_type.clone(),
            &constant_path.location(),
            provenance,
        );
        self.type_store.add(TypeFact::new(
            TypeSubject::Constant(fqn),
            inferred_type,
            self.document.prism_location_to_text_range(full_location),
            provenance,
        ));
    }

    pub fn process_constant_path_write_node_entry(&mut self, node: &ConstantPathWriteNode) {
        let constant_path = node.target();
        let Some((constant_name, fqn)) = self.constant_path_write_fqn(&constant_path) else {
            return;
        };
        self.record_constant_path_symbol(fqn, &constant_path, &constant_name, &node.location());
    }

    pub fn process_constant_path_write_node_exit(&mut self, node: &ConstantPathWriteNode) {
        let constant_path = node.target();
        let Some((_constant_name, fqn)) = self.constant_path_write_fqn(&constant_path) else {
            return;
        };
        self.record_constant_path_value_type(fqn, &node.value(), &constant_path, &node.location());
    }

    pub fn process_constant_path_or_write_node_entry(&mut self, node: &ConstantPathOrWriteNode) {
        let constant_path = node.target();
        let Some((constant_name, fqn)) = self.constant_path_write_fqn(&constant_path) else {
            return;
        };
        self.record_constant_path_symbol(fqn, &constant_path, &constant_name, &node.location());
    }

    pub fn process_constant_path_or_write_node_exit(&mut self, node: &ConstantPathOrWriteNode) {
        let constant_path = node.target();
        let Some((_constant_name, fqn)) = self.constant_path_write_fqn(&constant_path) else {
            return;
        };
        self.record_constant_path_value_type(fqn, &node.value(), &constant_path, &node.location());
    }

    pub fn process_constant_path_and_write_node_entry(&mut self, node: &ConstantPathAndWriteNode) {
        let constant_path = node.target();
        let Some((constant_name, fqn)) = self.constant_path_write_fqn(&constant_path) else {
            return;
        };
        self.record_constant_path_symbol(fqn, &constant_path, &constant_name, &node.location());
    }

    pub fn process_constant_path_and_write_node_exit(&mut self, node: &ConstantPathAndWriteNode) {
        let constant_path = node.target();
        let Some((_constant_name, fqn)) = self.constant_path_write_fqn(&constant_path) else {
            return;
        };
        self.record_constant_path_value_type(fqn, &node.value(), &constant_path, &node.location());
    }

    pub fn process_constant_path_operator_write_node_entry(
        &mut self,
        node: &ConstantPathOperatorWriteNode,
    ) {
        let constant_path = node.target();
        let Some((constant_name, fqn)) = self.constant_path_write_fqn(&constant_path) else {
            return;
        };
        self.record_constant_path_symbol(fqn, &constant_path, &constant_name, &node.location());
    }

    pub fn process_constant_path_operator_write_node_exit(
        &mut self,
        node: &ConstantPathOperatorWriteNode,
    ) {
        let constant_path = node.target();
        let Some((_constant_name, fqn)) = self.constant_path_write_fqn(&constant_path) else {
            return;
        };
        self.record_constant_path_value_type(fqn, &node.value(), &constant_path, &node.location());
    }
}
