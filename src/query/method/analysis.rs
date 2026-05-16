use ruby_analysis_core::{GraphEdgeFact, GraphEdgeKind, GraphNodeKind};
use tower_lsp::lsp_types::Location;

use crate::indexer::entry::NamespaceKind;
use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::types::ruby_method::RubyMethod;
use crate::types::ruby_namespace::RubyConstant;

use super::{MethodCalleeResolution, ResolvedMethodCallee};
use crate::query::analysis_location::location_for_range;
use crate::query::IndexQuery;

pub(super) fn resolve_method_callees(
    query: &IndexQuery,
    namespace_fqn: &FullyQualifiedName,
    method: &RubyMethod,
) -> Option<Vec<ResolvedMethodCallee>> {
    let engine = query.analysis_engine()?;
    let engine = engine.lock();
    if !namespace_target_exists(&engine, namespace_fqn) {
        return None;
    }

    let fqns_to_search = if is_module_instance_namespace(&engine, namespace_fqn) {
        let includers = module_includers(&engine, namespace_fqn);
        if includers.is_empty() {
            vec![namespace_fqn.clone()]
        } else {
            includers
        }
    } else {
        vec![namespace_fqn.clone()]
    };

    let mut callees = Vec::new();
    for fqn in &fqns_to_search {
        let ancestor_chain = method_lookup_chain(&engine, fqn)?;
        if let Some(callee) = method_callee_in_chain(
            &engine,
            &ancestor_chain,
            method,
            MethodCalleeResolution::Exact,
        ) {
            callees.push(callee);
        }
    }

    if callees.is_empty() {
        return None;
    }

    Some(callees)
}

pub(super) fn resolve_constant_receiver(
    query: &IndexQuery,
    path: &[RubyConstant],
    current_namespace: &[RubyConstant],
) -> Option<FullyQualifiedName> {
    let engine = query.analysis_engine()?;
    let engine = engine.lock();
    let current_fqn = FullyQualifiedName::namespace_with_kind(
        current_namespace.to_vec(),
        NamespaceKind::Instance,
    );
    let resolved = resolve_constant_fqn(&engine, path, false, &current_fqn)
        .unwrap_or_else(|| FullyQualifiedName::Constant(path.to_vec()));

    Some(FullyQualifiedName::namespace_with_kind(
        resolved.namespace_parts(),
        NamespaceKind::Singleton,
    ))
}

pub(super) fn method_locations(
    query: &IndexQuery,
    method_fqn: &FullyQualifiedName,
    ancestor_chain: &[FullyQualifiedName],
) -> Option<Vec<Location>> {
    let engine = query.analysis_engine()?;
    let engine = engine.lock();
    let locations = engine
        .method_facts_for(method_fqn)
        .iter()
        .filter(|fact| {
            ancestor_chain.iter().any(|ancestor| {
                ancestor.namespace_parts() == fact.owner.namespace_parts()
                    && ancestor.namespace_kind() == fact.owner.namespace_kind()
            })
        })
        .filter_map(|fact| location_for_range(&engine, fact.range))
        .collect::<Vec<_>>();

    if locations.is_empty() {
        None
    } else {
        Some(locations)
    }
}

fn namespace_target_exists(
    engine: &ruby_analysis_engine::AnalysisEngine,
    fqn: &FullyQualifiedName,
) -> bool {
    let parts = fqn.namespace_parts();
    let instance_fqn =
        FullyQualifiedName::namespace_with_kind(parts.clone(), NamespaceKind::Instance);
    let singleton_fqn =
        FullyQualifiedName::namespace_with_kind(parts.clone(), NamespaceKind::Singleton);
    let constant_fqn = FullyQualifiedName::constant(parts);

    !engine.graph_nodes_for(&instance_fqn).is_empty()
        || !engine.graph_nodes_for(&singleton_fqn).is_empty()
        || !engine.symbol_facts_for(&constant_fqn).is_empty()
}

fn is_module_instance_namespace(
    engine: &ruby_analysis_engine::AnalysisEngine,
    fqn: &FullyQualifiedName,
) -> bool {
    if fqn.namespace_kind() != Some(NamespaceKind::Instance) {
        return false;
    }
    engine
        .graph_nodes_for(fqn)
        .iter()
        .any(|fact| fact.kind == GraphNodeKind::Module)
}

fn module_includers(
    engine: &ruby_analysis_engine::AnalysisEngine,
    module_fqn: &FullyQualifiedName,
) -> Vec<FullyQualifiedName> {
    let mut result = Vec::new();
    let mut visited = std::collections::HashSet::new();
    let mut queue = std::collections::VecDeque::new();

    for edge in engine.all_graph_edges() {
        if &edge.target == module_fqn
            && matches!(edge.kind, GraphEdgeKind::Include | GraphEdgeKind::Prepend)
            && edge.source.namespace_kind() == Some(NamespaceKind::Instance)
            && visited.insert(edge.source.clone())
        {
            queue.push_back(edge.source);
        }
    }

    while let Some(current) = queue.pop_front() {
        if node_kind(engine, &current) == Some(GraphNodeKind::Class) {
            result.push(current);
            continue;
        }

        if node_kind(engine, &current) == Some(GraphNodeKind::Module) {
            for edge in engine.all_graph_edges() {
                if edge.target == current
                    && matches!(edge.kind, GraphEdgeKind::Include | GraphEdgeKind::Prepend)
                    && edge.source.namespace_kind() == Some(NamespaceKind::Instance)
                    && visited.insert(edge.source.clone())
                {
                    queue.push_back(edge.source);
                }
            }
        }
    }

    result.sort_by_key(|fqn| fqn.to_string());
    result
}

fn method_lookup_chain(
    engine: &ruby_analysis_engine::AnalysisEngine,
    fqn: &FullyQualifiedName,
) -> Option<Vec<FullyQualifiedName>> {
    assert!(
        matches!(fqn, FullyQualifiedName::Namespace(_, _)),
        "INVARIANT VIOLATED: analysis method lookup requested for non-namespace FQN: {fqn}. \
         This is a bug because only namespaces have method lookup chains. \
         Fix: resolve receivers to Namespace FQNs before method lookup."
    );

    if fqn.namespace_parts().is_empty() {
        return Some(vec![fqn.clone()]);
    }

    if engine.graph_nodes_for(fqn).is_empty() {
        return Some(vec![
            fqn.clone(),
            FullyQualifiedName::namespace_with_kind(Vec::new(), NamespaceKind::Instance),
        ]);
    }

    let mut chain = Vec::new();
    let mut visited = std::collections::HashSet::new();
    build_mro(engine, fqn, &mut chain, &mut visited);

    let root = FullyQualifiedName::namespace_with_kind(Vec::new(), NamespaceKind::Instance);
    if !chain.contains(&root) && !fqn.namespace_parts().is_empty() {
        chain.push(root);
    }

    Some(chain)
}

fn build_mro(
    engine: &ruby_analysis_engine::AnalysisEngine,
    fqn: &FullyQualifiedName,
    chain: &mut Vec<FullyQualifiedName>,
    visited: &mut std::collections::HashSet<FullyQualifiedName>,
) {
    if !visited.insert(fqn.clone()) {
        return;
    }

    let mut prepends = edges_from(engine, fqn, GraphEdgeKind::Prepend);
    for edge in prepends.iter_mut().rev() {
        build_mro(engine, &edge.target, chain, visited);
    }

    chain.push(fqn.clone());

    let mut includes = edges_from(engine, fqn, GraphEdgeKind::Include);
    for edge in includes.iter_mut().rev() {
        build_mro(engine, &edge.target, chain, visited);
    }

    if let Some(superclass) = edges_from(engine, fqn, GraphEdgeKind::Superclass).first() {
        build_mro(engine, &superclass.target, chain, visited);
    }
}

fn edges_from(
    engine: &ruby_analysis_engine::AnalysisEngine,
    fqn: &FullyQualifiedName,
    kind: GraphEdgeKind,
) -> Vec<GraphEdgeFact> {
    engine
        .graph_edges_from(fqn)
        .iter()
        .filter(|edge| edge.kind == kind)
        .cloned()
        .collect()
}

fn method_callee_in_chain(
    engine: &ruby_analysis_engine::AnalysisEngine,
    ancestor_chain: &[FullyQualifiedName],
    method: &RubyMethod,
    resolution: MethodCalleeResolution,
) -> Option<ResolvedMethodCallee> {
    for ancestor in ancestor_chain {
        let method_fqn = FullyQualifiedName::method(ancestor.namespace_parts(), *method);
        let locations = engine
            .method_facts_for(&method_fqn)
            .iter()
            .filter(|fact| {
                ancestor_chain.iter().any(|chain_fqn| {
                    chain_fqn.namespace_parts() == fact.owner.namespace_parts()
                        && chain_fqn.namespace_kind() == fact.owner.namespace_kind()
                })
            })
            .filter_map(|fact| location_for_range(engine, fact.range))
            .collect::<Vec<_>>();

        if !locations.is_empty() {
            return Some(ResolvedMethodCallee {
                owner: ancestor.clone(),
                method: *method,
                resolution,
                definition_locations: locations,
            });
        }
    }

    None
}

fn resolve_constant_fqn(
    engine: &ruby_analysis_engine::AnalysisEngine,
    parts: &[RubyConstant],
    absolute: bool,
    context_fqn: &FullyQualifiedName,
) -> Option<FullyQualifiedName> {
    let mut search_namespaces = if absolute {
        Vec::new()
    } else {
        context_fqn.namespace_parts()
    };

    loop {
        let mut probe = search_namespaces.clone();
        probe.extend(parts.iter().cloned());

        let namespace_fqn =
            FullyQualifiedName::namespace_with_kind(probe.clone(), NamespaceKind::Instance);
        if !engine.graph_nodes_for(&namespace_fqn).is_empty() {
            return Some(namespace_fqn);
        }

        let constant_fqn = FullyQualifiedName::constant(probe);
        if !engine.symbol_facts_for(&constant_fqn).is_empty() {
            return Some(constant_fqn);
        }

        if absolute || search_namespaces.is_empty() {
            break;
        }
        search_namespaces.pop();
    }

    None
}

fn node_kind(
    engine: &ruby_analysis_engine::AnalysisEngine,
    fqn: &FullyQualifiedName,
) -> Option<GraphNodeKind> {
    engine.graph_nodes_for(fqn).first().map(|fact| fact.kind)
}
