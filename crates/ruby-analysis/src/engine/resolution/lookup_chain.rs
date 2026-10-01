//! Method lookup chain construction (MRO), its caches, and universal fallbacks.

use crate::invariant::ExpectInvariant;
#[cfg(test)]
use std::cell::Cell;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};

use super::{is_module_instance_namespace, MethodLookupChainCache};
use crate::core::names::fqn_id::FqnId;
use crate::core::storage::graph_store::StoredGraphEdgeFact;
use crate::core::{
    FullyQualifiedName, GraphEdgeKind, GraphNodeKind, RubyConstant, RubyMethod, SourceKind,
};

fn is_universal_open_root(owner: &FullyQualifiedName) -> bool {
    match owner.namespace_parts_slice() {
        [] => true,
        [name] => matches!(
            name.as_str(),
            "BasicObject" | "Object" | "Kernel" | "Module" | "Class"
        ),
        [_, _, ..] => false,
    }
}

const UNIVERSAL_OPEN_ROOT_NAMES: [&str; 5] = ["BasicObject", "Object", "Kernel", "Module", "Class"];

pub(super) fn interned_universal_open_root_ids(
    engine: &crate::engine::AnalysisEngine,
    cache: &mut MethodLookupChainCache,
) -> Vec<FqnId> {
    if cache.universal_open_root_ids.is_none() {
        cache.universal_open_root_ids = Some(collect_interned_universal_open_root_ids(engine));
    }
    cache
        .universal_open_root_ids
        .as_ref()
        .expect_invariant(
            "universal open-root ids disappeared after insertion",
            "the resolve-local chain cache retains that list for one pass",
            "populate interned universal roots once before method lookup",
        )
        .clone()
}

fn collect_interned_universal_open_root_ids(engine: &crate::engine::AnalysisEngine) -> Vec<FqnId> {
    let mut ids = Vec::with_capacity(12);
    for kind in [
        crate::core::NamespaceKind::Instance,
        crate::core::NamespaceKind::Singleton,
    ] {
        let empty = FullyQualifiedName::namespace_with_kind(Vec::new(), kind);
        if let Some(id) = engine.names.fqn_id(&empty) {
            ids.push(id);
        }
        for name in UNIVERSAL_OPEN_ROOT_NAMES {
            let constant = RubyConstant::new(name).unwrap_or_else(|error| {
                unreachable_invariant!(
                    what = "Ruby universal root name `{name}` is invalid: {error}",
                    why = "BasicObject, Object, Kernel, Module, and Class are language-defined constants",
                    fix = "preserve RubyConstant support for those names",
                    name = name,
                    error = error,
                )
            });
            let fqn = FullyQualifiedName::namespace_with_kind(vec![constant], kind);
            if let Some(id) = engine.names.fqn_id(&fqn) {
                ids.push(id);
            }
        }
    }
    ids
}

#[cfg(test)]
thread_local! {
    static METHOD_LOOKUP_CHAIN_UNCACHED_CONSTRUCTIONS: Cell<u64> = const { Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn method_lookup_chain_uncached_construction_count() -> u64 {
    METHOD_LOOKUP_CHAIN_UNCACHED_CONSTRUCTIONS.with(Cell::get)
}

fn record_method_lookup_chain_uncached_construction() {
    #[cfg(test)]
    METHOD_LOOKUP_CHAIN_UNCACHED_CONSTRUCTIONS.with(|count| {
        count.set(count.get().saturating_add(1));
    });
}

const MAX_THREAD_METHOD_LOOKUP_CHAIN_CACHE_ENTRIES: usize = 8192;

/// Thread-local memo of constructed method lookup chains for one engine identity.
///
/// Fact collection calls `method_lookup_chain` on every receiver/method return
/// probe. Rebuilding MRO plus scanning every unresolved graph edge on each
/// probe dominated the project visitor. Hits are O(1) and do not touch FIFO
/// order. Identity changes drop the entries. Do not share one map across a
/// parallel file batch; keep this on the worker thread like method-return
/// memoization. Do not raise the cap without an RSS measurement.
struct ThreadMethodLookupChainCache {
    identity: Option<(u64, u64)>,
    chains: HashMap<FullyQualifiedName, Vec<FullyQualifiedName>>,
    order: VecDeque<FullyQualifiedName>,
}

impl Default for ThreadMethodLookupChainCache {
    fn default() -> Self {
        Self {
            identity: None,
            chains: HashMap::new(),
            order: VecDeque::new(),
        }
    }
}

impl ThreadMethodLookupChainCache {
    fn bind(&mut self, identity: (u64, u64)) {
        if self.identity != Some(identity) {
            self.identity = Some(identity);
            self.chains.clear();
            self.order.clear();
        }
    }

    fn get(&self, owner: &FullyQualifiedName) -> Option<Vec<FullyQualifiedName>> {
        self.chains.get(owner).cloned()
    }

    fn insert(&mut self, owner: FullyQualifiedName, chain: Vec<FullyQualifiedName>) {
        if self.chains.contains_key(&owner) {
            self.chains.insert(owner, chain);
            return;
        }
        while self.chains.len() >= MAX_THREAD_METHOD_LOOKUP_CHAIN_CACHE_ENTRIES {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            self.chains.remove(&oldest);
        }
        self.order.push_back(owner.clone());
        self.chains.insert(owner, chain);
    }
}

thread_local! {
    static THREAD_METHOD_LOOKUP_CHAIN_CACHE: RefCell<ThreadMethodLookupChainCache> =
        RefCell::new(ThreadMethodLookupChainCache::default());
}

fn thread_method_lookup_chain_get(
    identity: (u64, u64),
    owner: &FullyQualifiedName,
) -> Option<Vec<FullyQualifiedName>> {
    THREAD_METHOD_LOOKUP_CHAIN_CACHE.with(|cell| {
        let mut cache = cell.borrow_mut();
        cache.bind(identity);
        cache.get(owner)
    })
}

fn thread_method_lookup_chain_insert(
    identity: (u64, u64),
    owner: FullyQualifiedName,
    chain: Vec<FullyQualifiedName>,
) {
    THREAD_METHOD_LOOKUP_CHAIN_CACHE.with(|cell| {
        let mut cache = cell.borrow_mut();
        cache.bind(identity);
        cache.insert(owner, chain);
    })
}

pub(in crate::engine) fn method_lookup_chain(
    engine: &crate::engine::AnalysisEngine,
    fqn: &FullyQualifiedName,
) -> Vec<FullyQualifiedName> {
    let identity = engine.query_cache_identity();
    if let Some(cached) = thread_method_lookup_chain_get(identity, fqn) {
        return cached;
    }
    record_method_lookup_chain_uncached_construction();
    let chain = method_lookup_chain_uncached(engine, fqn);
    thread_method_lookup_chain_insert(identity, fqn.clone(), chain.clone());
    chain
}

fn method_lookup_chain_uncached(
    engine: &crate::engine::AnalysisEngine,
    fqn: &FullyQualifiedName,
) -> Vec<FullyQualifiedName> {
    let mut chain = method_lookup_chain_without_metaclass(engine, fqn);
    let Some(metaclass) = metaclass_namespace_for_object(engine, fqn) else {
        return chain;
    };
    let mut visited = chain
        .iter()
        .cloned()
        .collect::<std::collections::HashSet<_>>();
    build_mro(engine, &metaclass, &mut chain, &mut visited, false);
    chain
}

pub(super) fn method_lookup_chain_without_metaclass(
    engine: &crate::engine::AnalysisEngine,
    fqn: &FullyQualifiedName,
) -> Vec<FullyQualifiedName> {
    let allow_top_level_fallback =
        !method_lookup_chain_has_unresolved_dependency_from_graph(engine, fqn);
    method_lookup_chain_without_metaclass_with_fallback(engine, fqn, allow_top_level_fallback)
}

fn method_lookup_chain_without_metaclass_with_fallback(
    engine: &crate::engine::AnalysisEngine,
    fqn: &FullyQualifiedName,
    allow_top_level_fallback: bool,
) -> Vec<FullyQualifiedName> {
    invariant!(
        matches!(fqn, FullyQualifiedName::Namespace(_, _)),
        what = "analysis method lookup requested for non-namespace FQN: {fqn}",
        why = "only namespaces have method lookup chains",
        fix = "resolve receivers to Namespace FQNs before method lookup",
        fqn = fqn,
    );

    if !engine.has_graph_node(fqn) {
        if fqn.namespace_parts().is_empty() {
            let mut chain = Vec::new();
            let mut visited = std::collections::HashSet::new();
            append_top_level_instance_fallback(engine, &mut chain, &mut visited);
            if chain.is_empty() {
                chain.push(fqn.clone());
            }
            chain
        } else {
            // A constant or inferred receiver without an indexed namespace
            // has no proven ancestry. Retain its exact receiver identity for
            // reference candidates, but never guess that it falls through to
            // top-level Object/Kernel methods.
            vec![fqn.clone()]
        }
    } else {
        let mut chain = Vec::new();
        let mut visited = std::collections::HashSet::new();
        build_mro(
            engine,
            fqn,
            &mut chain,
            &mut visited,
            is_universal_open_root(fqn),
        );

        let root = FullyQualifiedName::namespace_with_kind(
            Vec::new(),
            crate::core::NamespaceKind::Instance,
        );
        if !chain.contains(&root)
            && !fqn.namespace_parts().is_empty()
            && (is_module_instance_namespace(engine, fqn)
                || fqn.namespace_kind() == Some(crate::core::NamespaceKind::Singleton))
            && allow_top_level_fallback
        {
            append_universal_object_fallback(engine, &mut chain, &mut visited);
        }

        chain
    }
}

pub(super) fn method_lookup_chain_has_unresolved_dependency_cached(
    engine: &crate::engine::AnalysisEngine,
    owner: &FullyQualifiedName,
    cache: &mut MethodLookupChainCache,
) -> bool {
    if let Some(incomplete) = cache.unresolved_dependencies.get(owner) {
        return *incomplete;
    }
    let incomplete = method_lookup_chain_has_unresolved_dependency_from_graph(engine, owner);
    invariant!(
        cache
            .unresolved_dependencies
            .insert(owner.clone(), incomplete)
            .is_none(),
        what = "unresolved method-dependency cache replaced an entry after a confirmed miss",
        why = "the engine graph is immutable during one resolve pass",
        fix = "keep lookup and insertion in one candidate step",
    );
    incomplete
}

pub(super) fn method_lookup_chain_has_unresolved_dependency_from_graph(
    engine: &crate::engine::AnalysisEngine,
    owner: &FullyQualifiedName,
) -> bool {
    let mut pending = vec![owner.clone()];
    let mut visited = HashSet::new();

    while let Some(current) = pending.pop() {
        if !visited.insert(current.clone()) {
            continue;
        }
        if engine.superclass_is_ambiguous(&current) {
            return true;
        }

        let unresolved_source = match current.namespace_kind() {
            Some(crate::core::NamespaceKind::Singleton) => {
                current.to_instance_namespace().expect_invariant(
                    "singleton lookup owner cannot produce an instance namespace",
                    "unresolved superclass facts are keyed by class/module instance identity",
                    "preserve Namespace identity while checking method proof barriers",
                )
            }
            Some(crate::core::NamespaceKind::Instance) => current.clone(),
            None => unreachable_invariant!(
                what = "method lookup proof barrier received non-namespace FQN `{current}`",
                why = "only namespace receivers have ancestry",
                fix = "convert receiver types before method resolution",
                current = current,
            ),
        };
        if let Some(source_id) = engine.names.fqn_id(&unresolved_source) {
            if engine.graph.has_explicit_unresolved_edge_from(source_id) {
                return true;
            }
        }

        pending.extend(
            engine
                .graph_ancestry_edges_from(&current)
                .into_iter()
                .map(|edge| engine.expand_interned_fqn(edge.target)),
        );
    }

    false
}

pub(super) fn metaclass_namespace_for_object(
    engine: &crate::engine::AnalysisEngine,
    fqn: &FullyQualifiedName,
) -> Option<FullyQualifiedName> {
    if fqn.namespace_kind() != Some(crate::core::NamespaceKind::Singleton) {
        return None;
    }
    let metaclass_name = match node_kind(engine, fqn) {
        Some(GraphNodeKind::Class) => "Class",
        Some(GraphNodeKind::Module) => "Module",
        None => return None,
    };
    let metaclass = FullyQualifiedName::namespace_with_kind(
        vec![RubyConstant::new(metaclass_name).unwrap_or_else(|error| {
            unreachable_invariant!(
                what = "Ruby metaclass name `{metaclass_name}` is invalid: {error}",
                why = "class and Module are universal Ruby constants",
                fix = "preserve RubyConstant support for language-defined class names",
                metaclass_name = metaclass_name,
                error = error,
            )
        })],
        crate::core::NamespaceKind::Instance,
    );
    engine.has_graph_node(&metaclass).then_some(metaclass)
}

pub(in crate::engine) fn method_lookup_chain_for_reference_cached<'cache>(
    engine: &crate::engine::AnalysisEngine,
    fqn: &FullyQualifiedName,
    chain_cache: &'cache mut MethodLookupChainCache,
) -> &'cache [FqnId] {
    if !chain_cache.chains.contains_key(fqn) {
        let allow_top_level_fallback =
            !method_lookup_chain_has_unresolved_dependency_cached(engine, fqn, chain_cache);
        let chain = method_lookup_chain_without_metaclass_with_fallback(
            engine,
            fqn,
            allow_top_level_fallback,
        )
        .into_iter()
        // An uninterned fallback namespace cannot own graph or method
        // facts because both stores are keyed exclusively by FqnId.
        // Omitting it is therefore the exact ID-domain equivalent of
        // the previous owner lookup returning Missing.
        .filter_map(|fqn| engine.names.fqn_id(&fqn))
        .collect();
        invariant!(
            chain_cache.chains.insert(fqn.clone(), chain).is_none(),
            what = "method lookup-chain cache replaced an entry after a confirmed miss",
            why = "the engine graph is immutable during one resolve pass",
            fix = "keep chain construction and insertion in one candidate step",
        );
    }
    chain_cache
        .chains
        .get(fqn)
        .expect_invariant(
            "method lookup-chain cache lost an entry immediately after insertion",
            "the cache is not mutated between insertion and lookup",
            "keep cached chain access in one resolution step",
        )
        .as_slice()
}

fn append_top_level_instance_fallback(
    engine: &crate::engine::AnalysisEngine,
    chain: &mut Vec<FullyQualifiedName>,
    visited: &mut std::collections::HashSet<FullyQualifiedName>,
) {
    let fallback = engine
        .cached_top_level_method_lookup_chain()
        .unwrap_or_else(|| {
            let mut fallback = Vec::new();
            let mut fallback_visited = std::collections::HashSet::new();
            compute_top_level_instance_fallback(engine, &mut fallback, &mut fallback_visited);
            engine.cache_top_level_method_lookup_chain(fallback.clone());
            fallback
        });
    for fqn in fallback {
        if visited.insert(fqn.clone()) {
            chain.push(fqn);
        }
    }
}

fn append_universal_object_fallback(
    engine: &crate::engine::AnalysisEngine,
    chain: &mut Vec<FullyQualifiedName>,
    visited: &mut std::collections::HashSet<FullyQualifiedName>,
) {
    for fqn in compute_universal_object_fallback(engine) {
        if visited.insert(fqn.clone()) {
            chain.push(fqn);
        }
    }
}

fn compute_universal_object_fallback(
    engine: &crate::engine::AnalysisEngine,
) -> Vec<FullyQualifiedName> {
    if let Some(cached) = engine.cached_universal_object_method_lookup_chain() {
        return cached;
    }
    let mut fallback = Vec::new();
    let mut visited = std::collections::HashSet::new();
    let object = top_level_object_instance_fqn();
    if engine.has_graph_node(&object) {
        build_mro(engine, &object, &mut fallback, &mut visited, false);
    }
    engine.cache_universal_object_method_lookup_chain(fallback.clone());
    fallback
}

pub(super) fn unproven_universal_method_exists(
    engine: &crate::engine::AnalysisEngine,
    universal_roots: &[FqnId],
    method: &RubyMethod,
    cache: &mut MethodLookupChainCache,
) -> bool {
    for root_id in universal_roots {
        if !cache.unproven_universal_methods.contains_key(root_id) {
            let root = engine.names.fqn(*root_id).expect_invariant(
                "universal lookup root ID is absent from the name registry",
                "roots are selected from a cached chain owned by the same immutable registry",
                "invalidate resolution-local caches whenever names change",
            );
            let mut broad = Vec::new();
            let mut broad_visited = HashSet::new();
            build_mro(engine, root, &mut broad, &mut broad_visited, true);
            let mut proven = Vec::new();
            let mut proven_visited = HashSet::new();
            build_mro(engine, root, &mut proven, &mut proven_visited, false);
            let proven = proven.into_iter().collect::<HashSet<_>>();
            let mut methods = HashSet::new();
            for owner in broad.into_iter().filter(|owner| !proven.contains(owner)) {
                let Some(owner_id) = engine.names.fqn_id(&owner) else {
                    continue;
                };
                methods.extend(engine.ruby_method_names_for_owner_id(owner_id));
            }
            invariant!(
                cache
                    .unproven_universal_methods
                    .insert(*root_id, methods)
                    .is_none(),
                what = "unproven universal-method cache replaced one root after a confirmed miss",
                why = "the engine graph is immutable during one resolve pass",
                fix = "keep root lookup and insertion atomic",
            );
        }
        if cache
            .unproven_universal_methods
            .get(root_id)
            .expect_invariant(
                "unproven universal-method cache lost one root immediately after insertion",
                "the resolution-local cache is not cleared during one pass",
                "retain root entries for the cache lifetime",
            )
            .contains(method)
        {
            return true;
        }
    }
    false
}

fn compute_top_level_instance_fallback(
    engine: &crate::engine::AnalysisEngine,
    chain: &mut Vec<FullyQualifiedName>,
    visited: &mut std::collections::HashSet<FullyQualifiedName>,
) {
    let root =
        FullyQualifiedName::namespace_with_kind(Vec::new(), crate::core::NamespaceKind::Instance);
    build_mro(engine, &root, chain, visited, true);

    let object_fqn = top_level_object_instance_fqn();
    if engine.has_graph_node(&object_fqn) {
        build_mro(engine, &object_fqn, chain, visited, true);
    }
}

fn top_level_object_instance_fqn() -> FullyQualifiedName {
    FullyQualifiedName::namespace_with_kind(
        vec![RubyConstant::new("Object").expect_invariant(
            "`Object` is not a valid Ruby constant",
            "ruby core class names must be valid constants",
            "inspect RubyConstant validation",
        )],
        crate::core::NamespaceKind::Instance,
    )
}

fn build_mro(
    engine: &crate::engine::AnalysisEngine,
    fqn: &FullyQualifiedName,
    chain: &mut Vec<FullyQualifiedName>,
    visited: &mut std::collections::HashSet<FullyQualifiedName>,
    allow_non_language_universal_edges: bool,
) {
    if !visited.insert(fqn.clone()) {
        return;
    }

    let language_owned_only = is_universal_open_root(fqn) && !allow_non_language_universal_edges;
    let edge_is_allowed = |edge: &StoredGraphEdgeFact| {
        !language_owned_only || method_lookup_edge_is_language_owned(engine, edge)
    };

    let prepends = engine
        .graph_stored_edges_from_kind(fqn, GraphEdgeKind::Prepend)
        .into_iter()
        .filter(&edge_is_allowed)
        .collect::<Vec<_>>();
    for edge in prepends.iter().rev() {
        let target = engine.expand_interned_fqn(edge.target);
        build_mro(
            engine,
            &target,
            chain,
            visited,
            allow_non_language_universal_edges,
        );
    }

    chain.push(fqn.clone());

    let includes = engine
        .graph_stored_edges_from_kind(fqn, GraphEdgeKind::Include)
        .into_iter()
        .filter(&edge_is_allowed)
        .collect::<Vec<_>>();
    for edge in includes.iter().rev() {
        let target = engine.expand_interned_fqn(edge.target);
        build_mro(
            engine,
            &target,
            chain,
            visited,
            allow_non_language_universal_edges,
        );
    }

    let included_hook_extends = included_hook_extend_edges(engine, fqn, language_owned_only)
        .into_iter()
        .filter(&edge_is_allowed)
        .collect::<Vec<_>>();
    for edge in included_hook_extends.iter().rev() {
        let target = engine.expand_interned_fqn(edge.target);
        build_mro(
            engine,
            &target,
            chain,
            visited,
            allow_non_language_universal_edges,
        );
    }

    if let Some(superclass) = engine
        .proven_superclass_stored_edge(fqn)
        .filter(&edge_is_allowed)
    {
        let target = engine.expand_interned_fqn(superclass.target);
        build_mro(
            engine,
            &target,
            chain,
            visited,
            allow_non_language_universal_edges,
        );
    }
}

fn method_lookup_edge_is_language_owned(
    engine: &crate::engine::AnalysisEngine,
    edge: &StoredGraphEdgeFact,
) -> bool {
    matches!(
        engine
            .file(edge.range.file_id)
            .unwrap_or_else(|| {
                unreachable_invariant!(
                    what = "method lookup graph edge references missing source file {}",
                    why = "every graph edge must remain owned by a registered source",
                    fix = "remove graph edges before unregistering their file",
                    edge.range.file_id.0,
                )
            })
            .kind,
        SourceKind::Stub | SourceKind::Signature
    )
}

fn included_hook_extend_edges(
    engine: &crate::engine::AnalysisEngine,
    fqn: &FullyQualifiedName,
    language_owned_only: bool,
) -> Vec<StoredGraphEdgeFact> {
    if fqn.namespace_kind() != Some(crate::core::NamespaceKind::Singleton) {
        return Vec::new();
    }
    let Some(instance_fqn) = fqn.to_instance_namespace() else {
        return Vec::new();
    };

    let mut hook_edges = Vec::new();
    for edge in engine
        .graph_stored_edges_from_kind(&instance_fqn, GraphEdgeKind::Include)
        .into_iter()
        .chain(engine.graph_stored_edges_from_kind(&instance_fqn, GraphEdgeKind::Prepend))
        .filter(|edge| !language_owned_only || method_lookup_edge_is_language_owned(engine, edge))
    {
        let mixin = engine.expand_interned_fqn(edge.target);
        hook_edges.extend(
            engine
                .graph_stored_edges_from_kind(&mixin, GraphEdgeKind::Extend)
                .into_iter()
                .filter(|hook| {
                    !language_owned_only || method_lookup_edge_is_language_owned(engine, hook)
                }),
        );
    }
    hook_edges
}

pub(in crate::engine) fn node_kind(
    engine: &crate::engine::AnalysisEngine,
    fqn: &FullyQualifiedName,
) -> Option<GraphNodeKind> {
    engine.first_graph_node_kind(fqn)
}
