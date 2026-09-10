use crate::core::{FullyQualifiedName, TypeFact, TypeProvenance, TypeSubject};
use crate::engine::VariableTypeKind;
use ruby_prism::{ClassVariableReadNode, GlobalVariableReadNode, InstanceVariableReadNode};

use super::FactCollector;

impl FactCollector {
    fn record_nonlocal_variable_read(
        &mut self,
        kind: VariableTypeKind,
        name: &[u8],
        location: ruby_prism::Location,
    ) {
        let name = crate::indexer::utf8_str(name);
        let range = self.document.prism_location_to_text_range(&location);
        let owner = FullyQualifiedName::namespace_with_kind(
            self.scope_tracker.get_ns_stack(),
            self.scope_tracker.current_method_context(),
        );
        let outcome =
            self.collected_nonlocal_variable_outcome_before(kind, name, &owner, range.start_byte);
        if let Some(reason) = outcome.unknown_reason() {
            self.expressions.unknown_reasons.insert(range, reason);
        }
        let ruby_type = outcome.into_ruby_type();

        self.facts.types.add(TypeFact::new(
            TypeSubject::Expression(range),
            ruby_type,
            range,
            TypeProvenance::Flow,
        ));
    }

    pub(super) fn process_instance_variable_read_node_entry(
        &mut self,
        node: &InstanceVariableReadNode,
    ) {
        self.record_nonlocal_variable_read(
            VariableTypeKind::Instance,
            node.name().as_slice(),
            node.location(),
        );
    }

    pub(super) fn process_class_variable_read_node_entry(&mut self, node: &ClassVariableReadNode) {
        self.record_nonlocal_variable_read(
            VariableTypeKind::Class,
            node.name().as_slice(),
            node.location(),
        );
    }

    pub(super) fn process_global_variable_read_node_entry(
        &mut self,
        node: &GlobalVariableReadNode,
    ) {
        self.record_nonlocal_variable_read(
            VariableTypeKind::Global,
            node.name().as_slice(),
            node.location(),
        );
    }
}
