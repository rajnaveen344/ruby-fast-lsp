use crate::core::{FullyQualifiedName, SymbolFact, SymbolKind, TypeFact, TypeSubject};
use crate::indexer::documents::scope_rules::{
    constant_path_write_target, implicit_singleton_namespace,
};
use ruby_prism::{
    ConstantPathAndWriteNode, ConstantPathNode, ConstantPathOperatorWriteNode,
    ConstantPathOrWriteNode, ConstantPathTargetNode, ConstantPathWriteNode, Location, Node,
};

use crate::indexer::fact_collector::FactCollector;

impl FactCollector {
    /// The constant a `Parent::NAME` write names: `Parent` resolves lexically
    /// from the current scope, `::NAME` is top level, and `self::NAME` is the
    /// current namespace.
    fn constant_path_write_fqn(
        &self,
        constant_path: &ConstantPathNode<'_>,
    ) -> Option<(String, FullyQualifiedName)> {
        let name = constant_path.name()?;
        let fqn = constant_path_write_target(
            constant_path.parent().as_ref(),
            name.as_slice(),
            implicit_singleton_namespace(&self.scope_tracker).as_deref(),
            &self.scope_tracker.get_ns_stack(),
            &|fqn| self.namespace_is_known(fqn),
        )?;
        Some((String::from_utf8_lossy(name.as_slice()).to_string(), fqn))
    }

    fn record_constant_path_symbol(
        &mut self,
        fqn: FullyQualifiedName,
        constant_path: &ConstantPathNode<'_>,
        constant_name: &str,
        full_location: &Location<'_>,
    ) {
        self.facts.analysis.symbols.push(
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
        self.facts.flow_types.add(TypeFact::new(
            TypeSubject::Constant(fqn),
            inferred_type,
            self.document.prism_location_to_text_range(full_location),
            provenance,
        ));
    }

    pub(in crate::indexer::fact_collector) fn process_constant_path_write_node_entry(
        &mut self,
        node: &ConstantPathWriteNode,
    ) {
        let constant_path = node.target();
        let Some((constant_name, fqn)) = self.constant_path_write_fqn(&constant_path) else {
            return;
        };
        self.record_constant_path_symbol(fqn, &constant_path, &constant_name, &node.location());
    }

    pub(in crate::indexer::fact_collector) fn process_constant_path_write_node_exit(
        &mut self,
        node: &ConstantPathWriteNode,
    ) {
        let constant_path = node.target();
        let Some((_constant_name, fqn)) = self.constant_path_write_fqn(&constant_path) else {
            return;
        };
        self.record_constant_path_value_type(
            fqn.clone(),
            &node.value(),
            &constant_path,
            &node.location(),
        );
        if let Ok(summary) = crate::inference::callable_body::lower_callable_literal(&node.value())
        {
            if summary.is_capture_free() {
                self.constants.callable_bodies.push(
                    crate::core::callables::callable_body::ConstantCallableBodyFact {
                        constant: fqn,
                        summary,
                        range: self.direct_range(&node.location()),
                    },
                );
            }
        }
    }

    pub(in crate::indexer::fact_collector) fn process_constant_path_or_write_node_entry(
        &mut self,
        node: &ConstantPathOrWriteNode,
    ) {
        let constant_path = node.target();
        let Some((constant_name, fqn)) = self.constant_path_write_fqn(&constant_path) else {
            return;
        };
        self.record_constant_path_symbol(fqn, &constant_path, &constant_name, &node.location());
    }

    pub(in crate::indexer::fact_collector) fn process_constant_path_or_write_node_exit(
        &mut self,
        node: &ConstantPathOrWriteNode,
    ) {
        let constant_path = node.target();
        let Some((_constant_name, fqn)) = self.constant_path_write_fqn(&constant_path) else {
            return;
        };
        self.record_constant_path_value_type(fqn, &node.value(), &constant_path, &node.location());
    }

    pub(in crate::indexer::fact_collector) fn process_constant_path_and_write_node_entry(
        &mut self,
        node: &ConstantPathAndWriteNode,
    ) {
        let constant_path = node.target();
        let Some((constant_name, fqn)) = self.constant_path_write_fqn(&constant_path) else {
            return;
        };
        self.record_constant_path_symbol(fqn, &constant_path, &constant_name, &node.location());
    }

    pub(in crate::indexer::fact_collector) fn process_constant_path_and_write_node_exit(
        &mut self,
        node: &ConstantPathAndWriteNode,
    ) {
        let constant_path = node.target();
        let Some((_constant_name, fqn)) = self.constant_path_write_fqn(&constant_path) else {
            return;
        };
        self.record_constant_path_value_type(fqn, &node.value(), &constant_path, &node.location());
    }

    pub(in crate::indexer::fact_collector) fn process_constant_path_operator_write_node_entry(
        &mut self,
        node: &ConstantPathOperatorWriteNode,
    ) {
        let constant_path = node.target();
        let Some((constant_name, fqn)) = self.constant_path_write_fqn(&constant_path) else {
            return;
        };
        self.record_constant_path_symbol(fqn, &constant_path, &constant_name, &node.location());
    }

    pub(in crate::indexer::fact_collector) fn process_constant_path_operator_write_node_exit(
        &mut self,
        node: &ConstantPathOperatorWriteNode,
    ) {
        let constant_path = node.target();
        let Some((_constant_name, fqn)) = self.constant_path_write_fqn(&constant_path) else {
            return;
        };
        self.record_constant_path_value_type(fqn, &node.value(), &constant_path, &node.location());
    }

    pub(in crate::indexer::fact_collector) fn process_constant_path_target_node_entry(
        &mut self,
        node: &ConstantPathTargetNode,
    ) {
        let Some(name) = node.name() else {
            return;
        };
        let Some(fqn) = constant_path_write_target(
            node.parent().as_ref(),
            name.as_slice(),
            implicit_singleton_namespace(&self.scope_tracker).as_deref(),
            &self.scope_tracker.get_ns_stack(),
            &|fqn| self.namespace_is_known(fqn),
        ) else {
            return;
        };
        let location = node.location();
        self.facts.analysis.symbols.push(
            SymbolFact::new(
                fqn.clone(),
                SymbolKind::Constant,
                self.direct_range(&location),
            )
            .with_name_range(self.direct_terminal_name_range(&location, name.as_slice())),
        );
        let inferred_type = self.current_assignment_target_type();
        self.record_constant_value_type_explicit(fqn, inferred_type, &location, &location);
    }
}
