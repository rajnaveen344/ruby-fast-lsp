//! Semantic graph reads, constant path resolution, ancestry edges, method
//! lookup chain caches, and retry of unresolved graph edges.

use crate::core::storage::graph_store::StoredGraphEdgeFact;
use crate::core::storage::graph_store::StoredSuperclassResolution;
use crate::core::storage::graph_store::StoredUnresolvedGraphEdgeFact;
use crate::core::{
    FullyQualifiedName, GraphEdgeFact, GraphEdgeKind, GraphNodeFact, GraphNodeKind, NamespaceKind,
    RubyConstant, SourceFileId, TextRange, UnresolvedGraphEdgeFact,
};
use crate::invariant::ExpectInvariant;

use super::AnalysisEngine;

impl AnalysisEngine {
    pub(crate) fn query_cache_identity(&self) -> (u64, u64) {
        (self.instance_id, self.semantic_revision)
    }

    pub(in crate::engine) fn cached_top_level_method_lookup_chain(
        &self,
    ) -> Option<Vec<FullyQualifiedName>> {
        self.top_level_method_lookup_chain_cache.lock().clone()
    }

    pub(in crate::engine) fn cache_top_level_method_lookup_chain(
        &self,
        chain: Vec<FullyQualifiedName>,
    ) {
        *self.top_level_method_lookup_chain_cache.lock() = Some(chain);
    }

    pub(in crate::engine) fn cached_universal_object_method_lookup_chain(
        &self,
    ) -> Option<Vec<FullyQualifiedName>> {
        self.universal_object_method_lookup_chain_cache
            .lock()
            .clone()
    }

    pub(in crate::engine) fn cache_universal_object_method_lookup_chain(
        &self,
        chain: Vec<FullyQualifiedName>,
    ) {
        *self.universal_object_method_lookup_chain_cache.lock() = Some(chain);
    }

    #[cfg(test)]
    pub(super) fn valid_method_lookup_chain_cache_len_for_test(&self) -> usize {
        usize::from(self.top_level_method_lookup_chain_cache.lock().is_some())
            + usize::from(
                self.universal_object_method_lookup_chain_cache
                    .lock()
                    .is_some(),
            )
    }
}

impl AnalysisEngine {
    pub fn graph_nodes_for(&self, fqn: &FullyQualifiedName) -> Vec<GraphNodeFact> {
        let Some(fqn_id) = self.names.fqn_id(fqn) else {
            return Vec::new();
        };
        self.graph
            .nodes_for(fqn_id)
            .into_iter()
            .map(|fact| self.expand_graph_node_fact(fact))
            .collect()
    }

    pub fn has_graph_node(&self, fqn: &FullyQualifiedName) -> bool {
        let Some(fqn_id) = self.names.fqn_id(fqn) else {
            return false;
        };
        self.graph.has_node_definition(fqn_id)
    }

    pub fn first_graph_node_kind(&self, fqn: &FullyQualifiedName) -> Option<GraphNodeKind> {
        let fqn_id = self.names.fqn_id(fqn)?;
        self.graph.first_node_kind(fqn_id)
    }

    pub fn latest_graph_node_kind(&self, fqn: &FullyQualifiedName) -> Option<GraphNodeKind> {
        let fqn_id = self.names.fqn_id(fqn)?;
        self.graph.latest_node_kind(fqn_id)
    }

    pub fn graph_node_has_kind(&self, fqn: &FullyQualifiedName, kind: GraphNodeKind) -> bool {
        let Some(fqn_id) = self.names.fqn_id(fqn) else {
            return false;
        };
        self.graph.has_node_kind(fqn_id, kind)
    }

    pub fn first_graph_node_definition(
        &self,
        fqn: &FullyQualifiedName,
    ) -> Option<(GraphNodeKind, TextRange)> {
        let fqn_id = self.names.fqn_id(fqn)?;
        self.graph.first_node_definition(fqn_id)
    }
}

impl AnalysisEngine {
    pub(in crate::engine) fn resolve_constant_reference(
        &self,
        parts: &[RubyConstant],
        current_namespace: &[RubyConstant],
    ) -> Option<FullyQualifiedName> {
        self.resolve_constant_path(parts, current_namespace, true, true)
    }

    pub(in crate::engine) fn resolve_constant_path(
        &self,
        parts: &[RubyConstant],
        current_namespace: &[RubyConstant],
        namespace_symbols: bool,
        value_constants: bool,
    ) -> Option<FullyQualifiedName> {
        let mut search = current_namespace.to_vec();
        loop {
            let mut probe = search.clone();
            probe.extend(parts.iter().cloned());
            let namespace_fqn = FullyQualifiedName::namespace(probe.clone());
            if self.has_graph_node(&namespace_fqn)
                || (namespace_symbols && self.has_symbol_facts(&namespace_fqn))
            {
                return Some(namespace_fqn);
            }
            if value_constants {
                let constant_fqn = FullyQualifiedName::constant(probe);
                if self.has_symbol_facts(&constant_fqn) {
                    return Some(constant_fqn);
                }
            }
            if search.is_empty() {
                break;
            }
            search.pop();
        }
        None
    }

    pub fn graph_edges_from(&self, source: &FullyQualifiedName) -> Vec<GraphEdgeFact> {
        self.graph_stored_edges_from(source)
            .into_iter()
            .map(|fact| self.expand_graph_edge_fact(fact))
            .collect()
    }

    pub(in crate::engine) fn graph_stored_edges_from_kind(
        &self,
        source: &FullyQualifiedName,
        kind: GraphEdgeKind,
    ) -> Vec<StoredGraphEdgeFact> {
        let Some(source_id) = self.names.fqn_id(source) else {
            return Vec::new();
        };
        self.graph.edges_from_kind(source_id, kind)
    }

    pub(in crate::engine) fn graph_ancestry_edges_from(
        &self,
        source: &FullyQualifiedName,
    ) -> Vec<StoredGraphEdgeFact> {
        let Some(source_id) = self.names.fqn_id(source) else {
            return Vec::new();
        };
        self.graph.ancestry_edges_from(source_id)
    }

    fn graph_stored_edges_from(&self, source: &FullyQualifiedName) -> Vec<StoredGraphEdgeFact> {
        let Some(source_id) = self.names.fqn_id(source) else {
            return Vec::new();
        };
        self.graph.edges_from(source_id)
    }
}

impl AnalysisEngine {
    /// Returns the one superclass that is statically proven for `source`.
    /// Explicit declarations outrank per-declaration implicit `Object` facts,
    /// but two distinct explicit targets or any unresolved explicit target
    /// make the superclass unknown. Duplicate declarations of the same target
    /// remain one semantic proof while retaining every file-owned fact.
    pub fn proven_superclass_edge(&self, source: &FullyQualifiedName) -> Option<GraphEdgeFact> {
        self.proven_superclass_stored_edge(source)
            .map(|edge| self.expand_graph_edge_fact(edge))
    }

    pub(in crate::engine) fn proven_superclass_stored_edge(
        &self,
        source: &FullyQualifiedName,
    ) -> Option<StoredGraphEdgeFact> {
        if self.superclass_source_has_unresolved_explicit_edge(source) {
            return None;
        }
        let source_id = self.names.fqn_id(source)?;
        match self.graph.superclass_resolution(source_id) {
            StoredSuperclassResolution::Unique(edge) => Some(edge),
            StoredSuperclassResolution::Missing | StoredSuperclassResolution::Ambiguous => None,
        }
    }

    pub fn superclass_is_ambiguous(&self, source: &FullyQualifiedName) -> bool {
        self.names.fqn_id(source).is_some_and(|source_id| {
            self.graph.superclass_resolution(source_id) == StoredSuperclassResolution::Ambiguous
        })
    }

    fn superclass_source_has_unresolved_explicit_edge(&self, source: &FullyQualifiedName) -> bool {
        let instance_source = match source.namespace_kind() {
            Some(NamespaceKind::Singleton) => source.to_instance_namespace().expect_invariant(
                "singleton superclass source cannot produce an instance namespace",
                "graph superclass sources are Namespace FQNs",
                "preserve Namespace identity for class graph nodes",
            ),
            Some(NamespaceKind::Instance) => source.clone(),
            None => return false,
        };
        let Some(source_id) = self.names.fqn_id(&instance_source) else {
            return false;
        };
        self.graph.has_unresolved_explicit_superclass(source_id)
    }

    pub fn graph_edges_to(&self, target: &FullyQualifiedName) -> Vec<GraphEdgeFact> {
        let Some(target_id) = self.names.fqn_id(target) else {
            return Vec::new();
        };
        self.graph
            .edges_to(target_id)
            .into_iter()
            .map(|fact| self.expand_graph_edge_fact(fact))
            .collect()
    }

    pub fn graph_nodes_in_file(&self, file_id: SourceFileId) -> Vec<GraphNodeFact> {
        self.graph
            .nodes_in_file(file_id)
            .into_iter()
            .map(|fact| self.expand_graph_node_fact(fact))
            .collect()
    }

    pub fn graph_edges_in_file(&self, file_id: SourceFileId) -> Vec<GraphEdgeFact> {
        self.graph
            .edges_in_file(file_id)
            .into_iter()
            .map(|fact| self.expand_graph_edge_fact(fact))
            .collect()
    }

    pub fn all_graph_nodes(&self) -> Vec<GraphNodeFact> {
        self.graph
            .all_nodes()
            .into_iter()
            .map(|fact| self.expand_graph_node_fact(fact))
            .collect()
    }

    pub fn all_graph_edges(&self) -> Vec<GraphEdgeFact> {
        self.graph
            .all_edges()
            .into_iter()
            .map(|fact| self.expand_graph_edge_fact(fact))
            .collect()
    }
}

impl AnalysisEngine {
    pub fn unresolved_graph_edges(&self) -> Vec<UnresolvedGraphEdgeFact> {
        self.graph
            .unresolved_edges()
            .into_iter()
            .map(|edge| self.expand_unresolved_graph_edge_fact(edge))
            .collect()
    }
}

impl AnalysisEngine {
    pub(super) fn retry_unresolved_graph_edges(&mut self) {
        if self.graph.unresolved_edges().is_empty() {
            return;
        }

        let pending = self.graph.take_unresolved_edges();
        for unresolved in pending {
            if let Some(target) = self.resolve_unresolved_graph_target(&unresolved) {
                let singleton_superclass =
                    self.resolved_singleton_superclass_companion(&unresolved, &target);
                let target = self.names.intern_fqn(target);
                self.graph.add_edge(
                    StoredGraphEdgeFact::new(
                        unresolved.source,
                        target,
                        unresolved.kind,
                        unresolved.range,
                    )
                    .with_provenance(unresolved.provenance),
                );
                if let Some((source, target)) = singleton_superclass {
                    let source = self.names.intern_fqn(source);
                    let target = self.names.intern_fqn(target);
                    self.graph.add_edge(
                        StoredGraphEdgeFact::new(
                            source,
                            target,
                            GraphEdgeKind::Superclass,
                            unresolved.range,
                        )
                        .with_provenance(unresolved.provenance),
                    );
                }
            } else {
                self.graph.add_unresolved_edge(unresolved);
            }
        }
    }

    /// Ruby class-method inheritance follows the singleton classes of the
    /// ordinary superclass chain. The collector can emit both edges when the
    /// target is already known, but a cross-file superclass starts as one
    /// unresolved instance edge. Materialize its exact singleton companion at
    /// the same resolution boundary so indexing order cannot change class
    /// method lookup.
    fn resolved_singleton_superclass_companion(
        &self,
        unresolved: &StoredUnresolvedGraphEdgeFact,
        target: &FullyQualifiedName,
    ) -> Option<(FullyQualifiedName, FullyQualifiedName)> {
        if unresolved.kind != GraphEdgeKind::Superclass
            || target.namespace_kind() != Some(NamespaceKind::Instance)
        {
            return None;
        }
        let source = self.names.fqn(unresolved.source).expect_invariant(
            "unresolved superclass edge points to a missing source FQN",
            "graph edges retain interned sources for their full lifetime",
            "retain source FQNs until unresolved edges are removed",
        );
        if source.namespace_kind() != Some(NamespaceKind::Instance)
            || !self.graph_node_has_kind(source, GraphNodeKind::Class)
            || !self.graph_node_has_kind(target, GraphNodeKind::Class)
        {
            return None;
        }
        Some((
            source.to_singleton_namespace().expect_invariant(
                "a class instance namespace cannot produce its singleton namespace",
                "class declarations always use Namespace FQNs",
                "keep class graph nodes namespace-owned",
            ),
            target.to_singleton_namespace().expect_invariant(
                "a class superclass cannot produce its singleton namespace",
                "resolved superclass targets are class Namespace FQNs",
                "validate graph node kinds before materializing class inheritance",
            ),
        ))
    }

    fn resolve_unresolved_graph_target(
        &self,
        unresolved: &StoredUnresolvedGraphEdgeFact,
    ) -> Option<FullyQualifiedName> {
        let lookup = self.names.const_lookup(unresolved.target).expect_invariant(
            "unresolved graph edge points to missing constant lookup id",
            "unresolved graph edges must only store interned constant lookup ids",
            "intern unresolved graph edge targets before inserting facts",
        );
        let path = lookup.path.clone();
        let current_namespace = if lookup.absolute {
            Vec::new()
        } else {
            self.names
                .fqn(lookup.context)
                .expect_invariant(
                    "unresolved graph edge lookup points to missing context FQN id",
                    "constant lookups must only store interned context FQN ids",
                    "intern constant lookup contexts before inserting facts",
                )
                .namespace_parts()
        };
        self.resolve_constant_path(&path, &current_namespace, false, false)
    }
}
