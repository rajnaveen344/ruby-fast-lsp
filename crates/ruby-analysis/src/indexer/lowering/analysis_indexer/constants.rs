//! Value-constant declarations from constant writes and assignment targets.

use crate::core::{
    FullyQualifiedName, GraphEdgeKind, GraphEdgeProvenance, GraphNodeKind, RubyConstant,
    SymbolFact, SymbolKind, TypeSubject, UnresolvedGraphEdgeFact,
};
use crate::indexer::documents::scope_rules::{
    constant_path_write_target, implicit_singleton_namespace,
};
use crate::indexer::documents::struct_definition::{struct_class_execution, struct_definition};
use ruby_prism::{ConstantWriteNode, Location, Node};

use super::syntax::terminal_name_range;
use super::AnalysisIndexer;

impl AnalysisIndexer<'_> {
    /// Declare the class `Name = Struct.new(:member, ...)` builds: the
    /// class, its `Struct` parent, and a reader and a writer per member.
    /// Returns false when the write is any other constant write.
    pub(super) fn push_struct_class(&mut self, node: &ConstantWriteNode<'_>) -> bool {
        let lexical_context = self.scope.get_ns_stack();
        let Some(definition) = struct_definition(
            node.name().as_slice(),
            &node.value(),
            &lexical_context,
            |fqn| self.is_namespace(fqn),
        ) else {
            return false;
        };
        let Ok(constant) = RubyConstant::new(&String::from_utf8_lossy(node.name().as_slice()))
        else {
            return false;
        };
        let mut parts = lexical_context.clone();
        parts.push(constant);
        let fqn = FullyQualifiedName::namespace(parts.clone());
        let range = self.range(&node.location());
        self.push_namespace_facts(
            fqn.clone(),
            GraphNodeKind::Class,
            range,
            self.range(&node.name_loc()),
        );
        let parent_range = self.range(&definition.call.location());
        match self.resolve_namespace_from(
            &definition.parent,
            definition.parent_is_absolute,
            &lexical_context,
        ) {
            Some(target) => {
                let singletons = fqn
                    .to_singleton_namespace()
                    .zip(target.to_singleton_namespace());
                for (source, target) in std::iter::once((fqn, target)).chain(singletons) {
                    self.push_resolved_edge_with_provenance(
                        source,
                        target,
                        GraphEdgeKind::Superclass,
                        GraphEdgeProvenance::StructDefinition,
                        parent_range,
                    );
                }
            }
            None => self.facts.unresolved_graph_edges.push(
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
        struct_class_execution(parts.clone()).enter(&mut self.scope);
        self.push_attr_method_facts(&definition.call, true, true);
        self.scope.pop_execution_context();
        self.scope
            .expect_struct_definition_block(definition.call.location().start_offset(), parts);
        true
    }

    /// Declare `NAME` in the current lexical namespace.
    pub(super) fn push_constant_declaration(
        &mut self,
        name: &[u8],
        name_location: Location<'_>,
        full_location: Location<'_>,
        value: Option<&Node<'_>>,
    ) {
        let Ok(constant) = RubyConstant::new(&String::from_utf8_lossy(name)) else {
            return;
        };
        let mut parts = self.scope.get_ns_stack();
        parts.push(constant);
        let fqn = FullyQualifiedName::constant(parts);
        self.facts.symbols.push(
            SymbolFact::new(
                fqn.clone(),
                SymbolKind::Constant,
                self.range(&full_location),
            )
            .with_name_range(self.range(&name_location)),
        );
        let value_type = value.and_then(|value| self.assignment_type(value));
        self.push_type_fact(TypeSubject::Constant(fqn), value_type, name_location);
    }

    /// Declare the constant a `Parent::NAME` write or target names.
    pub(super) fn push_constant_path_declaration(
        &mut self,
        parent: Option<&Node<'_>>,
        name: &[u8],
        path_location: Location<'_>,
        full_location: Location<'_>,
        value: Option<&Node<'_>>,
    ) {
        let Some(fqn) = constant_path_write_target(
            parent,
            name,
            implicit_singleton_namespace(&self.scope).as_deref(),
            &self.scope.get_ns_stack(),
            &|fqn| self.is_namespace(fqn),
        ) else {
            return;
        };
        self.facts.symbols.push(
            SymbolFact::new(
                fqn.clone(),
                SymbolKind::Constant,
                self.range(&full_location),
            )
            .with_name_range(terminal_name_range(self.file_id, &path_location, name)),
        );
        let value_type = value.and_then(|value| self.assignment_type(value));
        self.push_type_fact(TypeSubject::Constant(fqn), value_type, path_location);
    }

    /// Declare an assignment target that is a constant, with the value Ruby
    /// assigns to it when syntax decides one. Returns false for other targets.
    pub(super) fn push_constant_target_declaration(
        &mut self,
        target: &Node<'_>,
        value: Option<&Node<'_>>,
    ) -> bool {
        if let Some(target) = target.as_constant_target_node() {
            self.push_constant_declaration(
                target.name().as_slice(),
                target.location(),
                target.location(),
                value,
            );
            return true;
        }
        if let Some(target) = target.as_constant_path_target_node() {
            let Some(name) = target.name() else {
                return true;
            };
            self.push_constant_path_declaration(
                target.parent().as_ref(),
                name.as_slice(),
                target.location(),
                target.location(),
                value,
            );
            return true;
        }
        false
    }
}
