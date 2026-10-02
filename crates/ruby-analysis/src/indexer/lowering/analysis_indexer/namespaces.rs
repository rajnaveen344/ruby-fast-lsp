//! Namespace graph facts, mixin edges, and lexical namespace resolution.

use crate::core::{
    FullyQualifiedName, GraphEdgeFact, GraphEdgeKind, GraphEdgeProvenance, GraphNodeFact,
    GraphNodeKind, RubyConstant, RubyType, SymbolFact, SymbolKind, TextRange, TypeFact,
    TypeProvenance, TypeSubject, UnresolvedGraphEdgeFact,
};
use crate::invariant::ExpectInvariant;
use ruby_prism::{CallNode, Node};

use super::syntax::{constant_parts_and_absolute, included_hook_mixin_call_kind};
use super::{AnalysisIndexer, ScopeKind};
use crate::indexer::documents::scope_rules::{
    class_methods_block, resolve_lexical_namespace, resolve_receiver_namespace,
};

impl AnalysisIndexer {
    pub(super) fn push_namespace_facts(
        &mut self,
        fqn: FullyQualifiedName,
        kind: GraphNodeKind,
        range: TextRange,
        name_range: TextRange,
    ) {
        self.known_namespaces.insert(fqn.clone());
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
        self.known_namespaces.insert(singleton_fqn.clone());
        self.facts
            .graph_nodes
            .push(GraphNodeFact::new(singleton_fqn, kind, range));
    }

    fn resolve_namespace(
        &self,
        parts: &[RubyConstant],
        absolute: bool,
    ) -> Option<FullyQualifiedName> {
        self.resolve_namespace_from(parts, absolute, &self.lexical_stack)
    }

    pub(super) fn resolve_namespace_from(
        &self,
        parts: &[RubyConstant],
        absolute: bool,
        lexical_context: &[RubyConstant],
    ) -> Option<FullyQualifiedName> {
        resolve_lexical_namespace(parts, absolute, lexical_context, |fqn| {
            self.known_namespaces.contains(fqn)
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
                    FullyQualifiedName::namespace(self.lexical_stack.clone()),
                    kind,
                    range,
                )
                .with_provenance(provenance),
            );
            return;
        };
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

        let source = FullyQualifiedName::namespace(self.owner_stack.clone());
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
            self.namespace_body_owner(),
            &self.lexical_stack,
            &|fqn| self.known_namespaces.contains(fqn),
        )
    }

    /// The class or module `self` names at a namespace body, including an
    /// eval block body, where `self` is the evaluated receiver.
    fn namespace_body_owner(&self) -> Option<&[RubyConstant]> {
        (!self.owner_stack.is_empty() && self.method_context_stack.is_empty())
            .then_some(self.owner_stack.as_slice())
    }

    /// Whether `receiver` is the bare name of the enclosing class or module,
    /// as in `def Registry.lookup` inside `class Registry`.
    pub(super) fn names_enclosing_namespace(&self, receiver: &Node<'_>) -> bool {
        let Some(read) = receiver.as_constant_read_node() else {
            return false;
        };
        self.owner_stack == self.lexical_stack
            && self
                .lexical_stack
                .last()
                .is_some_and(|last| last.as_str().as_bytes() == read.name().as_slice())
    }

    pub(super) fn static_eval_block_context(
        &self,
        node: &CallNode<'_>,
    ) -> Option<(Vec<RubyConstant>, ScopeKind)> {
        let definition_scope = match node.name().as_slice() {
            b"class_eval" | b"module_eval" | b"class_exec" | b"module_exec" => ScopeKind::Instance,
            b"instance_eval" | b"instance_exec" => ScopeKind::Singleton,
            _ => return None,
        };
        node.block()?;
        let namespace = match node.receiver() {
            Some(receiver) => self.resolve_constant_receiver_namespace(&receiver)?,
            // An implicit receiver evaluates in the current owner.
            None => self.namespace_body_owner()?.to_vec(),
        };
        Some((namespace, definition_scope))
    }

    /// A Concern `class_methods` block: declares the `ClassMethods` module,
    /// extends its owner with it, and returns where its methods land.
    pub(super) fn push_concern_class_methods_block(
        &mut self,
        node: &CallNode<'_>,
    ) -> Option<Vec<RubyConstant>> {
        let target = class_methods_block(node, self.namespace_body_owner().map(<[_]>::to_vec))?
            .definition_namespace;
        let owner = FullyQualifiedName::namespace(target[..target.len() - 1].to_vec());
        let range = self.range(&node.location());
        self.push_namespace_facts(
            FullyQualifiedName::namespace(target.clone()),
            GraphNodeKind::Module,
            range,
            range,
        );
        self.push_edge(owner, &target, true, GraphEdgeKind::Extend, range);
        Some(target)
    }
}
