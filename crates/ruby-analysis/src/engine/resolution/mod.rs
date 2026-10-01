//! Ruby lookup policy: method lookup chains and MRO, visibility, callee and
//! reference resolution, constant resolution, and rename safety.

mod callees;
mod chain_methods;
mod definitions;
mod lookup_chain;
mod method_references;
mod reference_ranges;
mod rename;
mod signatures;

pub(in crate::engine) use chain_methods::{
    chain_has_custom_method_missing, effective_method_visibility_for_chain,
    execution_context_application_targets, method_facts_in_chain, method_missing_method,
    protected_method_visible_from,
};
#[cfg(test)]
pub(in crate::engine) use lookup_chain::method_lookup_chain_for_reference_cached;
#[cfg(test)]
pub(crate) use lookup_chain::method_lookup_chain_uncached_construction_count;
pub(in crate::engine) use lookup_chain::{method_lookup_chain, node_kind};

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::core::names::fqn_id::FqnId;
use crate::core::{
    FullyQualifiedName, GraphEdgeKind, GraphNodeKind, MethodFact, RubyConstant, RubyMethod,
    RubyType, TextRange,
};

#[derive(Default)]
pub(crate) struct MethodLookupChainCache {
    chains: HashMap<FullyQualifiedName, Vec<FqnId>>,
    module_receivers: HashMap<FullyQualifiedName, Arc<[FullyQualifiedName]>>,
    metaclass_methods: HashMap<(FqnId, RubyMethod), MethodLookupResult>,
    unresolved_dependencies: HashMap<FullyQualifiedName, bool>,
    unproven_universal_methods: HashMap<FqnId, HashSet<RubyMethod>>,
    /// Interned Object/Kernel/Class/Module/BasicObject identities, both
    /// instance and singleton. Language-closed; not a resolve HashMap.
    universal_open_root_ids: Option<Vec<FqnId>>,
}

impl MethodLookupChainCache {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn len(&self) -> usize {
        self.chains.len()
    }

    pub(crate) fn get(&self, owner: &FullyQualifiedName) -> Option<&Vec<FqnId>> {
        self.chains.get(owner)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstantRenameTarget {
    pub fqn: FullyQualifiedName,
    pub current_name: RubyConstant,
    pub ranges: Vec<TextRange>,
}

#[derive(Clone)]
pub enum MethodLookupResult {
    Unique(Arc<MethodFact>),
    Ambiguous {
        owner: FullyQualifiedName,
        method: RubyMethod,
    },
    Missing,
}

impl MethodLookupResult {
    pub fn reference_parts(
        &self,
    ) -> Option<(&FullyQualifiedName, RubyMethod, Option<&MethodFact>)> {
        match self {
            MethodLookupResult::Unique(fact) => {
                Some((&fact.owner, method_name_from_fact(fact), Some(fact)))
            }
            MethodLookupResult::Ambiguous { owner, method } => Some((owner, *method, None)),
            MethodLookupResult::Missing => None,
        }
    }

    pub fn is_missing(&self) -> bool {
        matches!(self, MethodLookupResult::Missing)
    }
}

fn receiver_type_members(receiver_type: &RubyType) -> &[RubyType] {
    match receiver_type {
        RubyType::Union(members) => {
            assert!(
                members.len() >= 2,
                "INVARIANT VIOLATED: method resolution received a RubyType::Union with fewer than two members. This is a bug because canonical union construction must collapse empty and singleton inputs. Fix: construct receiver unions only through RubyType::union helpers."
            );
            members
        }
        RubyType::Class(_)
        | RubyType::Module(_)
        | RubyType::ClassReference(_)
        | RubyType::ModuleReference(_)
        | RubyType::Literal(_)
        | RubyType::Array(_)
        | RubyType::Hash(_, _)
        | RubyType::Shape(_)
        | RubyType::Unknown => std::slice::from_ref(receiver_type),
    }
}

pub(in crate::engine) fn method_name_from_fact(fact: &MethodFact) -> RubyMethod {
    let FullyQualifiedName::Method(_, method) = &fact.fqn else {
        panic!(
            "INVARIANT VIOLATED: method fact has non-method FQN `{}`. \
             This is a bug because method facts must be keyed by method FQNs. \
             Fix: only insert MethodFact values built from FullyQualifiedName::Method.",
            fact.fqn
        );
    };
    *method
}

pub(in crate::engine) fn namespace_target_exists(
    engine: &crate::engine::AnalysisEngine,
    fqn: &FullyQualifiedName,
) -> bool {
    let parts = fqn.namespace_parts_slice();
    if parts.is_empty() {
        return true;
    }
    if matches!(fqn, FullyQualifiedName::Namespace(_, _)) && engine.has_graph_node(fqn) {
        return true;
    }
    if let Some(kind) = fqn.namespace_kind() {
        let other_kind = match kind {
            crate::core::NamespaceKind::Instance => crate::core::NamespaceKind::Singleton,
            crate::core::NamespaceKind::Singleton => crate::core::NamespaceKind::Instance,
        };
        let other = FullyQualifiedName::namespace_with_kind(parts.to_vec(), other_kind);
        if engine.has_graph_node(&other) {
            return true;
        }
    } else {
        let instance = FullyQualifiedName::namespace_with_kind(
            parts.to_vec(),
            crate::core::NamespaceKind::Instance,
        );
        let singleton = FullyQualifiedName::namespace_with_kind(
            parts.to_vec(),
            crate::core::NamespaceKind::Singleton,
        );
        if engine.has_graph_node(&instance) || engine.has_graph_node(&singleton) {
            return true;
        }
    }
    engine.has_symbol_facts(&FullyQualifiedName::constant(parts.to_vec()))
}

fn is_module_instance_namespace(
    engine: &crate::engine::AnalysisEngine,
    fqn: &FullyQualifiedName,
) -> bool {
    if fqn.namespace_kind() != Some(crate::core::NamespaceKind::Instance) {
        return false;
    }
    engine.graph_node_has_kind(fqn, GraphNodeKind::Module)
}

/// Concrete receiver roots reachable through a module's reverse mixin edges.
/// Share this selection across navigation, references, and return inference;
/// selecting a method from the module first would hide receiver overrides.
pub(in crate::engine) fn module_instance_receivers(
    engine: &crate::engine::AnalysisEngine,
    module_fqn: &FullyQualifiedName,
) -> Vec<FullyQualifiedName> {
    if !is_module_instance_namespace(engine, module_fqn) {
        return Vec::new();
    }

    let mut result = Vec::new();
    let mut visited = std::collections::HashSet::new();
    let mut queue = std::collections::VecDeque::new();

    for edge in engine.graph_edges_to(module_fqn) {
        if matches!(edge.kind, GraphEdgeKind::Include | GraphEdgeKind::Prepend)
            && visited.insert(edge.source.clone())
        {
            queue.push_back(edge.source.clone());
        }
    }

    while let Some(current) = queue.pop_front() {
        if node_kind(engine, &current) == Some(GraphNodeKind::Class) {
            result.push(current);
            continue;
        }

        if node_kind(engine, &current) == Some(GraphNodeKind::Module) {
            for edge in engine.graph_edges_to(&current) {
                if matches!(edge.kind, GraphEdgeKind::Include | GraphEdgeKind::Prepend)
                    && visited.insert(edge.source.clone())
                {
                    queue.push_back(edge.source.clone());
                }
            }
        }
    }

    result.sort_by_key(|fqn| fqn.to_string());
    result
}
