//! The class a `Name = Struct.new(:member, ...)` write declares.

use crate::core::{
    FullyQualifiedName, GraphEdgeKind, GraphEdgeProvenance, GraphNodeKind, RubyConstant,
    UnresolvedGraphEdgeFact,
};
use crate::indexer::documents::struct_definition::{
    struct_class_execution, struct_definition, StructDefinition,
};
use crate::indexer::fact_collector::FactCollector;
use ruby_prism::ConstantWriteNode;

impl FactCollector {
    pub(super) fn struct_definition<'pr>(
        &self,
        node: &ConstantWriteNode<'pr>,
    ) -> Option<StructDefinition<'pr>> {
        struct_definition(
            node.name().as_slice(),
            &node.value(),
            &self.scope_tracker.get_ns_stack(),
            |fqn| self.namespace_is_known(fqn),
        )
    }

    /// Declare the class, its `Struct` parent, and a reader and a writer
    /// per member. Returns false when the write is any other constant write.
    pub(super) fn record_struct_class(&mut self, node: &ConstantWriteNode<'_>) -> bool {
        let Some(definition) = self.struct_definition(node) else {
            return false;
        };
        let Ok(constant) = RubyConstant::new(&String::from_utf8_lossy(node.name().as_slice()))
        else {
            return false;
        };
        let lexical_context = self.scope_tracker.get_ns_stack();
        let mut parts = lexical_context.clone();
        parts.push(constant);
        let fqn = FullyQualifiedName::namespace(parts.clone());
        self.direct_push_namespace_facts(
            fqn.clone(),
            GraphNodeKind::Class,
            self.direct_range(&node.location()),
            self.direct_range(&node.name_loc()),
        );
        let parent_range = self.direct_range(&definition.call.location());
        match self.declared_resolve_namespace_from(
            &definition.parent,
            definition.parent_is_absolute,
            &lexical_context,
        ) {
            Some(target) => {
                let singletons = fqn
                    .to_singleton_namespace()
                    .zip(target.to_singleton_namespace());
                for (source, target) in std::iter::once((fqn, target)).chain(singletons) {
                    self.direct_push_resolved_edge_with_provenance(
                        source,
                        target,
                        GraphEdgeKind::Superclass,
                        GraphEdgeProvenance::StructDefinition,
                        parent_range,
                    );
                }
            }
            None => self.facts.analysis.unresolved_graph_edges.push(
                UnresolvedGraphEdgeFact::new(
                    fqn,
                    definition.parent.clone(),
                    definition.parent_is_absolute,
                    FullyQualifiedName::namespace(lexical_context),
                    GraphEdgeKind::Superclass,
                    parent_range,
                )
                .with_provenance(GraphEdgeProvenance::StructDefinition),
            ),
        }
        struct_class_execution(parts.clone()).enter(&mut self.scope_tracker);
        self.push_direct_attr_method_facts(&definition.call, true, true);
        self.scope_tracker.pop_execution_context();
        self.scope_tracker
            .expect_struct_definition_block(definition.call.location().start_offset(), parts);
        true
    }
}
