//! `Hierarchy`: the engine's semantic graph of namespace nodes and ancestry
//! edges, constant path resolution, retry of unresolved graph edges, and the
//! method lookup chain caches that graph replacement invalidates.

use std::collections::{HashMap, HashSet};

use parking_lot::Mutex;

use crate::core::names::fqn_id::FqnId;
use crate::core::storage::graph_store::{
    SemanticGraph, StoredGraphEdgeFact, StoredGraphNodeFact, StoredSuperclassResolution,
    StoredUnresolvedGraphEdgeFact,
};
use crate::core::storage::memory_estimate::{
    fqn_heap_bytes, map_table_bytes, set_table_bytes, vec_payload_bytes,
};
use crate::core::storage::reference_store::ConstLookup;
use crate::core::{
    ConstantPath, FullyQualifiedName, GraphEdgeFact, GraphEdgeKind, GraphNodeFact, GraphNodeKind,
    NamespaceKind, RubyConstant, SourceFileId, TextRange, UnresolvedGraphEdgeFact,
};
use crate::invariant::ExpectInvariant;

use super::decls::DeclIndex;
use super::names::Names;
use super::Project;
use crate::engine::View;

#[derive(Debug, Default)]
pub(in crate::engine) struct Hierarchy {
    graph: SemanticGraph,
    retried: RetriedEdges,
    top_level_method_lookup_chain: Mutex<Option<Vec<FullyQualifiedName>>>,
    universal_object_method_lookup_chain: Mutex<Option<Vec<FullyQualifiedName>>>,
}

/// A clone shares no query cache with its source engine.
impl Clone for Hierarchy {
    fn clone(&self) -> Self {
        Self {
            graph: self.graph.clone(),
            retried: self.retried.clone(),
            top_level_method_lookup_chain: Mutex::new(None),
            universal_object_method_lookup_chain: Mutex::new(None),
        }
    }
}

/// An unresolved edge that a retry resolved against another file's node,
/// with the resolved edges it produced. When the target loses its last
/// definition, the produced edges are removed and the origin becomes
/// unresolved again, so the next retry sees current declarations.
#[derive(Debug, Clone)]
struct RetriedEdge {
    origin: StoredUnresolvedGraphEdgeFact,
    target: FqnId,
    edges: Vec<StoredGraphEdgeFact>,
}

#[derive(Debug, Clone, Default)]
struct RetriedEdges {
    /// Records keyed by the file that owns the origin edge.
    by_file: HashMap<SourceFileId, Vec<RetriedEdge>>,
    /// Origin files of the records resolved to each target.
    files_by_target: HashMap<FqnId, HashSet<SourceFileId>>,
}

impl RetriedEdges {
    fn record(&mut self, retried: RetriedEdge) {
        let file_id = retried.origin.range.file_id;
        self.files_by_target
            .entry(retried.target)
            .or_default()
            .insert(file_id);
        self.by_file.entry(file_id).or_default().push(retried);
    }

    /// Forget the records owned by `file_id`; the graph already dropped the
    /// file's edges.
    fn forget_file(&mut self, file_id: SourceFileId) {
        let Some(records) = self.by_file.remove(&file_id) else {
            return;
        };
        for record in records {
            if let Some(files) = self.files_by_target.get_mut(&record.target) {
                files.remove(&file_id);
                if files.is_empty() {
                    self.files_by_target.remove(&record.target);
                }
            }
        }
    }

    /// Take the records whose target satisfies `lost`.
    fn take_lost(&mut self, mut lost: impl FnMut(FqnId) -> bool) -> Vec<RetriedEdge> {
        let lost_targets: Vec<FqnId> = self
            .files_by_target
            .keys()
            .copied()
            .filter(|target| lost(*target))
            .collect();
        let mut taken = Vec::new();
        for target in lost_targets {
            let files = self.files_by_target.remove(&target).unwrap_or_default();
            for file_id in files {
                let Some(records) = self.by_file.get_mut(&file_id) else {
                    continue;
                };
                let (lost, kept): (Vec<_>, Vec<_>) = std::mem::take(records)
                    .into_iter()
                    .partition(|record| record.target == target);
                taken.extend(lost);
                if kept.is_empty() {
                    self.by_file.remove(&file_id);
                } else {
                    *records = kept;
                }
            }
        }
        taken
    }

    fn heap_bytes(&self) -> usize {
        map_table_bytes(&self.by_file)
            + self
                .by_file
                .values()
                .map(|records| {
                    vec_payload_bytes(records)
                        + records
                            .iter()
                            .map(|record| vec_payload_bytes(&record.edges))
                            .sum::<usize>()
                })
                .sum::<usize>()
            + map_table_bytes(&self.files_by_target)
            + self
                .files_by_target
                .values()
                .map(set_table_bytes)
                .sum::<usize>()
    }

    fn shrink_to_fit(&mut self) {
        self.by_file.shrink_to_fit();
        self.files_by_target.shrink_to_fit();
        for records in self.by_file.values_mut() {
            records.shrink_to_fit();
        }
        for files in self.files_by_target.values_mut() {
            files.shrink_to_fit();
        }
    }
}

impl Hierarchy {
    /// Intern one file's graph facts and replace the file's previous nodes,
    /// edges, and unresolved edges.
    pub(in crate::engine) fn replace_file(
        &mut self,
        names: &mut Names,
        file_id: SourceFileId,
        nodes: Vec<GraphNodeFact>,
        edges: Vec<GraphEdgeFact>,
        unresolved: Vec<UnresolvedGraphEdgeFact>,
    ) {
        let nodes = intern_node_facts(names, nodes);
        let edges = intern_edge_facts(names, edges);
        let unresolved = intern_unresolved_edge_facts(names, unresolved);
        let defined_nodes = self.graph.defines_nodes_in(file_id);
        self.graph.replace_file(file_id, nodes, edges, unresolved);
        self.retried.forget_file(file_id);
        if defined_nodes {
            self.unresolve_edges_to_lost_targets();
        }
    }

    /// Drop one file's nodes, edges, and unresolved edges. Edges from other
    /// files that resolved to nodes only this file defined become unresolved.
    pub(in crate::engine) fn remove_file(&mut self, file_id: SourceFileId) {
        let defined_nodes = self.graph.defines_nodes_in(file_id);
        self.graph.remove_file(file_id);
        self.retried.forget_file(file_id);
        if defined_nodes {
            self.unresolve_edges_to_lost_targets();
        }
    }

    /// Return retried edges to unresolved when their target no longer has
    /// any node definition. Retry resolves only against graph nodes, so a
    /// target without definitions can no longer justify the edge.
    fn unresolve_edges_to_lost_targets(&mut self) {
        let graph = &self.graph;
        let lost = self
            .retried
            .take_lost(|target| !graph.has_node_definition(target));
        for record in lost {
            for edge in &record.edges {
                invariant!(
                    self.graph.remove_edge_fact(edge),
                    what = "retried graph edge record outlived its resolved edge",
                    why = "records are forgotten whenever their origin file is replaced",
                    fix = "forget retried records together with their origin file's edges",
                );
            }
            self.graph.add_unresolved_edge(record.origin);
        }
    }

    pub(in crate::engine) fn invalidate_method_lookup_chains(&mut self) {
        *self.top_level_method_lookup_chain.get_mut() = None;
        *self.universal_object_method_lookup_chain.get_mut() = None;
    }

    pub(in crate::engine) fn has_node(&self, names: &Names, fqn: &FullyQualifiedName) -> bool {
        let Some(fqn_id) = names.fqn_id(fqn) else {
            return false;
        };
        self.graph.has_node_definition(fqn_id)
    }

    pub(in crate::engine) fn node_has_kind(
        &self,
        names: &Names,
        fqn: &FullyQualifiedName,
        kind: GraphNodeKind,
    ) -> bool {
        let Some(fqn_id) = names.fqn_id(fqn) else {
            return false;
        };
        self.graph.has_node_kind(fqn_id, kind)
    }

    pub(in crate::engine) fn has_explicit_unresolved_edge_from(
        &self,
        names: &Names,
        source: &FullyQualifiedName,
    ) -> bool {
        names
            .fqn_id(source)
            .is_some_and(|source_id| self.graph.has_explicit_unresolved_edge_from(source_id))
    }

    /// Namespaces whose ancestry has an unresolved edge, other than the
    /// implicit `::Object` superclass. Their method lookup chains are
    /// incomplete, so they cannot prove a method absent.
    pub(in crate::engine) fn unresolved_lookup_edge_sources(
        &self,
        names: &Names,
    ) -> HashSet<Vec<RubyConstant>> {
        self.graph
            .unresolved_edges()
            .into_iter()
            .filter(|edge| {
                let lookup = names.const_lookup(edge.target).expect_invariant(
                    "unresolved graph edge points to a missing constant lookup",
                    "graph edges must retain valid interned targets",
                    "intern and retain every unresolved graph target for the edge lifetime",
                );
                !(edge.kind == GraphEdgeKind::Superclass
                    && lookup.absolute
                    && lookup.path.len() == 1
                    && lookup.path[0].as_str() == "Object")
            })
            .filter_map(|edge| {
                names
                    .fqn(edge.source)
                    .map(FullyQualifiedName::namespace_parts)
            })
            .collect()
    }

    pub(in crate::engine) fn resolve_constant_path(
        &self,
        names: &Names,
        decls: &DeclIndex,
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
            if self.has_node(names, &namespace_fqn)
                || (namespace_symbols && decls.has_symbol_facts(names, &namespace_fqn))
            {
                return Some(namespace_fqn);
            }
            if value_constants {
                let constant_fqn = FullyQualifiedName::constant(probe);
                if decls.has_symbol_facts(names, &constant_fqn) {
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

    pub(in crate::engine) fn retry_unresolved_edges(
        &mut self,
        names: &mut Names,
        decls: &DeclIndex,
    ) {
        if !self.graph.has_unresolved_edges() {
            return;
        }

        let pending = self.graph.take_unresolved_edges();
        for unresolved in pending {
            if let Some(target) = self.resolve_unresolved_target(names, decls, &unresolved) {
                let singleton_superclass =
                    self.resolved_singleton_superclass_companion(names, &unresolved, &target);
                let target = names.intern_fqn(target);
                let mut edges = vec![StoredGraphEdgeFact::new(
                    unresolved.source,
                    target,
                    unresolved.kind,
                    unresolved.range,
                )
                .with_provenance(unresolved.provenance)];
                if let Some((source, singleton_target)) = singleton_superclass {
                    let source = names.intern_fqn(source);
                    let singleton_target = names.intern_fqn(singleton_target);
                    edges.push(
                        StoredGraphEdgeFact::new(
                            source,
                            singleton_target,
                            GraphEdgeKind::Superclass,
                            unresolved.range,
                        )
                        .with_provenance(unresolved.provenance),
                    );
                }
                for edge in &edges {
                    self.graph.add_edge(*edge);
                }
                self.retried.record(RetriedEdge {
                    origin: unresolved,
                    target,
                    edges,
                });
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
        names: &Names,
        unresolved: &StoredUnresolvedGraphEdgeFact,
        target: &FullyQualifiedName,
    ) -> Option<(FullyQualifiedName, FullyQualifiedName)> {
        if unresolved.kind != GraphEdgeKind::Superclass
            || target.namespace_kind() != Some(NamespaceKind::Instance)
        {
            return None;
        }
        let source = names.fqn(unresolved.source).expect_invariant(
            "unresolved superclass edge points to a missing source FQN",
            "graph edges retain interned sources for their full lifetime",
            "retain source FQNs until unresolved edges are removed",
        );
        if source.namespace_kind() != Some(NamespaceKind::Instance)
            || !self.node_has_kind(names, source, GraphNodeKind::Class)
            || !self.node_has_kind(names, target, GraphNodeKind::Class)
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

    fn resolve_unresolved_target(
        &self,
        names: &Names,
        decls: &DeclIndex,
        unresolved: &StoredUnresolvedGraphEdgeFact,
    ) -> Option<FullyQualifiedName> {
        let lookup = names.const_lookup(unresolved.target).expect_invariant(
            "unresolved graph edge points to missing constant lookup id",
            "unresolved graph edges must only store interned constant lookup ids",
            "intern unresolved graph edge targets before inserting facts",
        );
        let path = lookup.path.clone();
        let current_namespace = if lookup.absolute {
            Vec::new()
        } else {
            names
                .fqn(lookup.context)
                .expect_invariant(
                    "unresolved graph edge lookup points to missing context FQN id",
                    "constant lookups must only store interned context FQN ids",
                    "intern constant lookup contexts before inserting facts",
                )
                .namespace_parts()
        };
        self.resolve_constant_path(names, decls, &path, &current_namespace, false, false)
    }

    pub(in crate::engine) fn cached_top_level_method_lookup_chain(
        &self,
    ) -> Option<Vec<FullyQualifiedName>> {
        self.top_level_method_lookup_chain.lock().clone()
    }

    pub(in crate::engine) fn cache_top_level_method_lookup_chain(
        &self,
        chain: Vec<FullyQualifiedName>,
    ) {
        *self.top_level_method_lookup_chain.lock() = Some(chain);
    }

    pub(in crate::engine) fn cached_universal_object_method_lookup_chain(
        &self,
    ) -> Option<Vec<FullyQualifiedName>> {
        self.universal_object_method_lookup_chain.lock().clone()
    }

    pub(in crate::engine) fn cache_universal_object_method_lookup_chain(
        &self,
        chain: Vec<FullyQualifiedName>,
    ) {
        *self.universal_object_method_lookup_chain.lock() = Some(chain);
    }

    #[cfg(test)]
    pub(in crate::engine) fn cached_method_lookup_chain_count(&self) -> usize {
        usize::from(self.top_level_method_lookup_chain.lock().is_some())
            + usize::from(self.universal_object_method_lookup_chain.lock().is_some())
    }

    pub(in crate::engine) fn method_lookup_chain_heap_bytes(&self) -> usize {
        let chain_bytes = |chain: &Vec<FullyQualifiedName>| {
            vec_payload_bytes(chain) + chain.iter().map(fqn_heap_bytes).sum::<usize>()
        };
        self.top_level_method_lookup_chain
            .lock()
            .as_ref()
            .map(chain_bytes)
            .unwrap_or(0)
            + self
                .universal_object_method_lookup_chain
                .lock()
                .as_ref()
                .map(chain_bytes)
                .unwrap_or(0)
    }

    pub(in crate::engine) fn node_count(&self) -> usize {
        self.graph.node_count()
    }

    pub(in crate::engine) fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }

    pub(in crate::engine) fn unresolved_edge_count(&self) -> usize {
        self.graph.unresolved_edge_count()
    }

    pub(in crate::engine) fn graph_heap_bytes(&self) -> usize {
        self.graph.estimated_heap_bytes()
    }

    pub(in crate::engine) fn unresolved_heap_bytes(&self) -> usize {
        self.graph.estimated_unresolved_heap_bytes() + self.retried.heap_bytes()
    }

    pub(in crate::engine) fn shrink_to_fit(&mut self) {
        self.graph.shrink_to_fit();
        self.retried.shrink_to_fit();
    }
}

impl Project {
    pub(in crate::engine) fn cached_top_level_method_lookup_chain(
        &self,
    ) -> Option<Vec<FullyQualifiedName>> {
        self.hierarchy.cached_top_level_method_lookup_chain()
    }

    pub(in crate::engine) fn cache_top_level_method_lookup_chain(
        &self,
        chain: Vec<FullyQualifiedName>,
    ) {
        self.hierarchy.cache_top_level_method_lookup_chain(chain);
    }

    pub(in crate::engine) fn cached_universal_object_method_lookup_chain(
        &self,
    ) -> Option<Vec<FullyQualifiedName>> {
        self.hierarchy.cached_universal_object_method_lookup_chain()
    }

    pub(in crate::engine) fn cache_universal_object_method_lookup_chain(
        &self,
        chain: Vec<FullyQualifiedName>,
    ) {
        self.hierarchy
            .cache_universal_object_method_lookup_chain(chain);
    }

    #[cfg(test)]
    pub(super) fn valid_method_lookup_chain_cache_len_for_test(&self) -> usize {
        self.hierarchy.cached_method_lookup_chain_count()
    }
}

impl Project {
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
        self.hierarchy.resolve_constant_path(
            &self.names,
            &self.decls,
            parts,
            current_namespace,
            namespace_symbols,
            value_constants,
        )
    }

    pub(in crate::engine) fn has_explicit_unresolved_graph_edge_from(
        &self,
        source: &FullyQualifiedName,
    ) -> bool {
        self.hierarchy
            .has_explicit_unresolved_edge_from(&self.names, source)
    }

    pub(in crate::engine) fn unresolved_lookup_edge_sources(&self) -> HashSet<Vec<RubyConstant>> {
        self.hierarchy.unresolved_lookup_edge_sources(&self.names)
    }

    pub(in crate::engine) fn graph_stored_edges_from_kind(
        &self,
        source: &FullyQualifiedName,
        kind: GraphEdgeKind,
    ) -> Vec<StoredGraphEdgeFact> {
        let Some(source_id) = self.names.fqn_id(source) else {
            return Vec::new();
        };
        self.hierarchy.graph.edges_from_kind(source_id, kind)
    }

    pub(in crate::engine) fn graph_ancestry_edges_from(
        &self,
        source: &FullyQualifiedName,
    ) -> Vec<StoredGraphEdgeFact> {
        let Some(source_id) = self.names.fqn_id(source) else {
            return Vec::new();
        };
        self.hierarchy.graph.ancestry_edges_from(source_id)
    }
}

impl<'a> View<'a> {
    pub fn graph_nodes_for(&self, fqn: &FullyQualifiedName) -> Vec<GraphNodeFact> {
        let Some(fqn_id) = self.engine.names.fqn_id(fqn) else {
            return Vec::new();
        };
        self.engine
            .hierarchy
            .graph
            .nodes_for(fqn_id)
            .into_iter()
            .map(|fact| expand_node_fact(&self.engine.names, fact))
            .collect()
    }

    pub fn has_graph_node(&self, fqn: &FullyQualifiedName) -> bool {
        self.engine.hierarchy.has_node(&self.engine.names, fqn)
    }

    pub fn first_graph_node_kind(&self, fqn: &FullyQualifiedName) -> Option<GraphNodeKind> {
        let fqn_id = self.engine.names.fqn_id(fqn)?;
        self.engine.hierarchy.graph.first_node_kind(fqn_id)
    }

    pub fn latest_graph_node_kind(&self, fqn: &FullyQualifiedName) -> Option<GraphNodeKind> {
        let fqn_id = self.engine.names.fqn_id(fqn)?;
        self.engine.hierarchy.graph.latest_node_kind(fqn_id)
    }

    pub fn graph_node_has_kind(&self, fqn: &FullyQualifiedName, kind: GraphNodeKind) -> bool {
        self.engine
            .hierarchy
            .node_has_kind(&self.engine.names, fqn, kind)
    }

    pub fn first_graph_node_definition(
        &self,
        fqn: &FullyQualifiedName,
    ) -> Option<(GraphNodeKind, TextRange)> {
        let fqn_id = self.engine.names.fqn_id(fqn)?;
        self.engine.hierarchy.graph.first_node_definition(fqn_id)
    }

    pub fn graph_edges_from(&self, source: &FullyQualifiedName) -> Vec<GraphEdgeFact> {
        let Some(source_id) = self.engine.names.fqn_id(source) else {
            return Vec::new();
        };
        self.engine
            .hierarchy
            .graph
            .edges_from(source_id)
            .into_iter()
            .map(|fact| expand_edge_fact(&self.engine.names, fact))
            .collect()
    }

    /// Returns the one superclass that is statically proven for `source`.
    /// Explicit declarations outrank per-declaration implicit `Object` facts,
    /// but two distinct explicit targets or any unresolved explicit target
    /// make the superclass unknown. Duplicate declarations of the same target
    /// remain one semantic proof while retaining every file-owned fact.
    pub fn proven_superclass_edge(&self, source: &FullyQualifiedName) -> Option<GraphEdgeFact> {
        self.engine
            .proven_superclass_stored_edge(source)
            .map(|edge| expand_edge_fact(&self.engine.names, edge))
    }

    pub fn superclass_is_ambiguous(&self, source: &FullyQualifiedName) -> bool {
        self.engine.names.fqn_id(source).is_some_and(|source_id| {
            self.engine.hierarchy.graph.superclass_resolution(source_id)
                == StoredSuperclassResolution::Ambiguous
        })
    }

    pub fn graph_edges_to(&self, target: &FullyQualifiedName) -> Vec<GraphEdgeFact> {
        let Some(target_id) = self.engine.names.fqn_id(target) else {
            return Vec::new();
        };
        self.engine
            .hierarchy
            .graph
            .edges_to(target_id)
            .into_iter()
            .map(|fact| expand_edge_fact(&self.engine.names, fact))
            .collect()
    }

    pub fn graph_nodes_in_file(&self, file_id: SourceFileId) -> Vec<GraphNodeFact> {
        self.engine
            .hierarchy
            .graph
            .nodes_in_file(file_id)
            .into_iter()
            .map(|fact| expand_node_fact(&self.engine.names, fact))
            .collect()
    }

    pub fn graph_edges_in_file(&self, file_id: SourceFileId) -> Vec<GraphEdgeFact> {
        self.engine
            .hierarchy
            .graph
            .edges_in_file(file_id)
            .into_iter()
            .map(|fact| expand_edge_fact(&self.engine.names, fact))
            .collect()
    }

    pub fn all_graph_nodes(&self) -> Vec<GraphNodeFact> {
        self.engine
            .hierarchy
            .graph
            .all_nodes()
            .into_iter()
            .map(|fact| expand_node_fact(&self.engine.names, fact))
            .collect()
    }

    pub fn all_graph_edges(&self) -> Vec<GraphEdgeFact> {
        self.engine
            .hierarchy
            .graph
            .all_edges()
            .into_iter()
            .map(|fact| expand_edge_fact(&self.engine.names, fact))
            .collect()
    }

    pub fn unresolved_graph_edges(&self) -> Vec<UnresolvedGraphEdgeFact> {
        self.engine
            .hierarchy
            .graph
            .unresolved_edges()
            .into_iter()
            .map(|edge| expand_unresolved_edge_fact(&self.engine.names, edge))
            .collect()
    }
}

impl Project {
    pub(in crate::engine) fn proven_superclass_stored_edge(
        &self,
        source: &FullyQualifiedName,
    ) -> Option<StoredGraphEdgeFact> {
        if self.superclass_source_has_unresolved_explicit_edge(source) {
            return None;
        }
        let source_id = self.names.fqn_id(source)?;
        match self.hierarchy.graph.superclass_resolution(source_id) {
            StoredSuperclassResolution::Unique(edge) => Some(edge),
            StoredSuperclassResolution::Missing | StoredSuperclassResolution::Ambiguous => None,
        }
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
        self.hierarchy
            .graph
            .has_unresolved_explicit_superclass(source_id)
    }

    pub(super) fn retry_unresolved_graph_edges(&mut self) {
        self.hierarchy
            .retry_unresolved_edges(&mut self.names, &self.decls);
    }
}

fn intern_node_facts(names: &mut Names, facts: Vec<GraphNodeFact>) -> Vec<StoredGraphNodeFact> {
    facts
        .into_iter()
        .map(|fact| {
            let fqn = names.intern_fqn(fact.fqn);
            StoredGraphNodeFact::new(fqn, fact.kind, fact.range)
        })
        .collect()
}

fn intern_edge_facts(names: &mut Names, facts: Vec<GraphEdgeFact>) -> Vec<StoredGraphEdgeFact> {
    facts
        .into_iter()
        .map(|fact| {
            let source = names.intern_fqn(fact.source);
            let target = names.intern_fqn(fact.target);
            StoredGraphEdgeFact::new(source, target, fact.kind, fact.range)
                .with_provenance(fact.provenance)
        })
        .collect()
}

fn intern_unresolved_edge_facts(
    names: &mut Names,
    facts: Vec<UnresolvedGraphEdgeFact>,
) -> Vec<StoredUnresolvedGraphEdgeFact> {
    facts
        .into_iter()
        .map(|fact| {
            let source = names.intern_fqn(fact.source);
            let context = names.intern_fqn(fact.context);
            let target = names.intern_const_lookup(ConstLookup::new(
                ConstantPath::from_vec(fact.target_parts),
                fact.absolute,
                context,
            ));
            StoredUnresolvedGraphEdgeFact::new(source, target, fact.kind, fact.range)
                .with_provenance(fact.provenance)
        })
        .collect()
}

fn expand_node_fact(names: &Names, fact: StoredGraphNodeFact) -> GraphNodeFact {
    let fqn = names
        .fqn(fact.fqn)
        .expect_invariant(
            "graph node points to missing FQN id",
            "graph nodes must only store interned FQN ids",
            "intern graph node FQNs before inserting facts",
        )
        .clone();
    GraphNodeFact::new(fqn, fact.kind, fact.range)
}

fn expand_edge_fact(names: &Names, fact: StoredGraphEdgeFact) -> GraphEdgeFact {
    GraphEdgeFact::new(
        names.expand_interned_fqn(fact.source),
        names.expand_interned_fqn(fact.target),
        fact.kind,
        fact.range,
    )
    .with_provenance(fact.provenance)
}

fn expand_unresolved_edge_fact(
    names: &Names,
    fact: StoredUnresolvedGraphEdgeFact,
) -> UnresolvedGraphEdgeFact {
    let source = names
        .fqn(fact.source)
        .expect_invariant(
            "unresolved graph edge points to missing source FQN id",
            "unresolved graph edges must only store interned source FQN ids",
            "intern unresolved graph edge sources before inserting facts",
        )
        .clone();
    let lookup = names.const_lookup(fact.target).expect_invariant(
        "unresolved graph edge points to missing constant lookup id",
        "unresolved graph edges must only store interned constant lookup ids",
        "intern unresolved graph edge targets before inserting facts",
    );
    let context = names
        .fqn(lookup.context)
        .expect_invariant(
            "unresolved graph edge lookup points to missing context FQN id",
            "constant lookups must only store interned context FQN ids",
            "intern constant lookup contexts before inserting facts",
        )
        .clone();
    UnresolvedGraphEdgeFact::new(
        source,
        lookup.path.to_vec(),
        lookup.absolute,
        context,
        fact.kind,
        fact.range,
    )
    .with_provenance(fact.provenance)
}
