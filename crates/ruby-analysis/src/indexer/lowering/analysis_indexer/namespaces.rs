//! Namespace graph facts, mixin edges, and lexical namespace resolution.

use crate::core::{
    FullyQualifiedName, GraphEdgeFact, GraphEdgeKind, GraphEdgeProvenance, GraphNodeFact,
    GraphNodeKind, RubyConstant, RubyType, SymbolFact, SymbolKind, TextRange, TypeFact,
    TypeProvenance, TypeSubject, UnresolvedGraphEdgeFact,
};
use crate::invariant::ExpectInvariant;
use ruby_prism::{CallNode, Node};

use super::syntax::{constant_parts_and_absolute, included_hook_mixin_call_kind};
use super::AnalysisIndexer;
use crate::indexer::documents::scope_rules::{
    class_methods_block, edge_admission, implicit_singleton_namespace, resolve_lexical_namespace,
    resolve_receiver_namespace, BlockExecution, EdgeAdmission,
};

impl AnalysisIndexer<'_> {
    pub(super) fn push_namespace_facts(
        &mut self,
        fqn: FullyQualifiedName,
        kind: GraphNodeKind,
        range: TextRange,
        name_range: TextRange,
    ) {
        self.declared_namespaces.insert(fqn.clone());
        self.facts.symbols.push(
            SymbolFact::new(
                fqn.clone(),
                match kind {
                    GraphNodeKind::Class => SymbolKind::Class,
                    GraphNodeKind::Module => SymbolKind::Module,
                },
                range,
            )
            .with_name_range(name_range),
        );
        self.facts
            .graph_nodes
            .push(GraphNodeFact::new(fqn.clone(), kind, range));
        let constant_fqn = FullyQualifiedName::constant(fqn.namespace_parts());
        self.facts.types.push(TypeFact::new(
            TypeSubject::Constant(constant_fqn.clone()),
            match kind {
                GraphNodeKind::Class => RubyType::ClassReference(constant_fqn.clone()),
                GraphNodeKind::Module => RubyType::ModuleReference(constant_fqn),
            },
            range,
            TypeProvenance::Inferred,
        ));

        let singleton_fqn = fqn.to_singleton_namespace().expect_invariant(
            "namespace fact could not convert to singleton namespace",
            "class/module graph nodes must be namespace FQNs",
            "only call push_namespace_facts with Namespace facts",
        );
        self.declared_namespaces.insert(singleton_fqn.clone());
        self.facts
            .graph_nodes
            .push(GraphNodeFact::new(singleton_fqn, kind, range));
    }

    /// Whether this walk or another file declares `fqn` as a namespace.
    pub(super) fn is_namespace(&self, fqn: &FullyQualifiedName) -> bool {
        self.declared_namespaces.contains(fqn) || self.known.is_known_namespace(fqn)
    }

    fn resolve_namespace(
        &self,
        parts: &[RubyConstant],
        absolute: bool,
    ) -> Option<FullyQualifiedName> {
        self.resolve_namespace_from(parts, absolute, &self.scope.get_ns_stack())
    }

    pub(super) fn resolve_namespace_from(
        &self,
        parts: &[RubyConstant],
        absolute: bool,
        lexical_context: &[RubyConstant],
    ) -> Option<FullyQualifiedName> {
        resolve_lexical_namespace(parts, absolute, lexical_context, |fqn| {
            self.is_namespace(fqn)
        })
    }

    pub(super) fn push_edge(
        &mut self,
        source: FullyQualifiedName,
        parts: &[RubyConstant],
        absolute: bool,
        kind: GraphEdgeKind,
        range: TextRange,
    ) {
        self.push_edge_with_provenance(
            source,
            parts,
            absolute,
            kind,
            GraphEdgeProvenance::Explicit,
            range,
        );
    }

    pub(super) fn push_edge_with_provenance(
        &mut self,
        source: FullyQualifiedName,
        parts: &[RubyConstant],
        absolute: bool,
        kind: GraphEdgeKind,
        provenance: GraphEdgeProvenance,
        range: TextRange,
    ) {
        let Some(target) = self.resolve_namespace(parts, absolute) else {
            self.facts.unresolved_graph_edges.push(
                UnresolvedGraphEdgeFact::new(
                    source,
                    parts.to_vec(),
                    absolute,
                    FullyQualifiedName::namespace(self.scope.get_ns_stack()),
                    kind,
                    range,
                )
                .with_provenance(provenance),
            );
            return;
        };
        self.push_resolved_edge_with_provenance(source, target, kind, provenance, range);
    }

    /// Record a resolved edge the file's ancestry admits. Duplicate,
    /// conflicting-superclass, and cyclic edges are left to the collector,
    /// which reports them.
    pub(super) fn push_resolved_edge_with_provenance(
        &mut self,
        source: FullyQualifiedName,
        target: FullyQualifiedName,
        kind: GraphEdgeKind,
        provenance: GraphEdgeProvenance,
        range: TextRange,
    ) {
        if edge_admission(&self.facts.graph_edges, &source, &target, kind) != EdgeAdmission::Admit {
            return;
        }
        self.facts
            .graph_edges
            .push(GraphEdgeFact::new(source, target, kind, range).with_provenance(provenance));
    }

    pub(super) fn push_included_hook_mixin_edges(&mut self, node: &CallNode<'_>) {
        if !self.inside_singleton_included_method() {
            return;
        }
        if node.receiver().is_none() {
            return;
        }

        let Some((kind, first_mixin_index)) = included_hook_mixin_call_kind(node, self.file_id)
        else {
            return;
        };
        let Some(arguments) = node.arguments() else {
            return;
        };

        let source = FullyQualifiedName::namespace(self.owner_namespace());
        let range = self.range(&node.location());
        for arg in arguments.arguments().iter().skip(first_mixin_index) {
            let Some((parts, absolute)) = constant_parts_and_absolute(&arg) else {
                continue;
            };
            self.push_edge(source.clone(), &parts, absolute, kind, range);
        }
    }

    pub(super) fn resolve_constant_receiver_namespace(
        &self,
        receiver: &Node<'_>,
    ) -> Option<Vec<RubyConstant>> {
        resolve_receiver_namespace(
            receiver,
            implicit_singleton_namespace(&self.scope).as_deref(),
            &self.scope.get_ns_stack(),
            &|fqn| self.is_namespace(fqn),
        )
    }

    /// A Concern `class_methods` block: declares the `ClassMethods` module,
    /// extends its owner with it, and returns where its methods land.
    pub(super) fn push_concern_class_methods_block(
        &mut self,
        node: &CallNode<'_>,
    ) -> Option<BlockExecution> {
        let execution = class_methods_block(node, implicit_singleton_namespace(&self.scope))?;
        let target = execution.definition_namespace.clone();
        let owner = FullyQualifiedName::namespace(target[..target.len() - 1].to_vec());
        let range = self.range(&node.location());
        self.push_namespace_facts(
            FullyQualifiedName::namespace(target.clone()),
            GraphNodeKind::Module,
            range,
            range,
        );
        self.push_edge(owner, &target, true, GraphEdgeKind::Extend, range);
        Some(execution)
    }
}
