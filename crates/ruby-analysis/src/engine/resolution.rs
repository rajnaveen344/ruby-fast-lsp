use crate::core::{
    FullyQualifiedName, GraphEdgeFact, GraphEdgeKind, GraphNodeKind, MethodCalleeResolution,
    ResolvedMethodCallee, RubyConstant, RubyMethod, SymbolKind, TextRange,
};
use crate::engine::query::AnalysisQuery;

impl<'a> AnalysisQuery<'a> {
    pub fn resolve_method_callees(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Option<Vec<ResolvedMethodCallee>> {
        if !namespace_target_exists(self.engine, namespace_fqn) {
            return None;
        }

        let fqns_to_search = if is_module_instance_namespace(self.engine, namespace_fqn) {
            let includers = module_includers(self.engine, namespace_fqn);
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
            let ancestor_chain = method_lookup_chain(self.engine, fqn);
            if let Some(callee) = method_callee_in_chain(
                self.engine,
                &ancestor_chain,
                method,
                MethodCalleeResolution::Exact,
            ) {
                callees.push(callee);
            }
        }

        if callees.is_empty() {
            return Some(
                fqns_to_search
                    .into_iter()
                    .map(|fqn| ResolvedMethodCallee {
                        owner: fqn,
                        method: *method,
                        resolution: MethodCalleeResolution::ReceiverOnly,
                        definition_ranges: Vec::new(),
                    })
                    .collect(),
            );
        }

        Some(callees)
    }

    pub fn method_reference_targets(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Vec<FullyQualifiedName> {
        if !namespace_target_exists(self.engine, namespace_fqn) {
            return Vec::new();
        }

        let mut targets = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for namespace in related_namespaces_for_method_references(self.engine, namespace_fqn) {
            for ancestor in method_lookup_chain(self.engine, &namespace) {
                let method_fqn = FullyQualifiedName::method(ancestor.namespace_parts(), *method);
                if seen.insert(method_fqn.clone()) {
                    targets.push(method_fqn);
                }
            }
        }
        targets
    }

    pub fn resolve_constant_receiver(
        &self,
        path: &[RubyConstant],
        current_namespace: &[RubyConstant],
    ) -> FullyQualifiedName {
        let current_fqn = FullyQualifiedName::namespace_with_kind(
            current_namespace.to_vec(),
            crate::core::NamespaceKind::Instance,
        );
        let resolved = resolve_constant_fqn(self.engine, path, false, &current_fqn)
            .unwrap_or_else(|| FullyQualifiedName::constant(path.to_vec()));

        FullyQualifiedName::namespace_with_kind(
            resolved.namespace_parts(),
            crate::core::NamespaceKind::Singleton,
        )
    }

    pub fn resolve_constant_in_context(
        &self,
        parts: &[RubyConstant],
        context: &[RubyConstant],
    ) -> Option<FullyQualifiedName> {
        let context_fqn = FullyQualifiedName::namespace(context.to_vec());
        resolve_constant_fqn(self.engine, parts, false, &context_fqn)
    }

    pub fn reference_ranges_for_fqn(&self, fqn: &FullyQualifiedName) -> Vec<TextRange> {
        self.engine
            .reference_facts_for(fqn)
            .iter()
            .map(|fact| fact.range)
            .collect()
    }

    pub fn method_reference_ranges(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Vec<TextRange> {
        let mut ranges = Vec::new();
        for target in self.method_reference_targets(namespace_fqn, method) {
            ranges.extend(
                self.engine
                    .reference_facts_for(&target)
                    .iter()
                    .map(|fact| fact.range),
            );
        }
        ranges
    }

    pub fn symbol_definition_ranges(
        &self,
        fqn: &FullyQualifiedName,
        allowed_kinds: &[SymbolKind],
    ) -> Vec<TextRange> {
        self.engine
            .symbol_facts_for(fqn)
            .iter()
            .filter(|fact| allowed_kinds.contains(&fact.kind))
            .map(|fact| fact.range)
            .collect()
    }
}

pub(super) fn namespace_target_exists(
    engine: &crate::AnalysisEngine,
    fqn: &FullyQualifiedName,
) -> bool {
    let parts = fqn.namespace_parts();
    if parts.is_empty() {
        return true;
    }
    let instance_fqn = FullyQualifiedName::namespace_with_kind(
        parts.clone(),
        crate::core::NamespaceKind::Instance,
    );
    let singleton_fqn = FullyQualifiedName::namespace_with_kind(
        parts.clone(),
        crate::core::NamespaceKind::Singleton,
    );
    let constant_fqn = FullyQualifiedName::constant(parts);

    !engine.graph_nodes_for(&instance_fqn).is_empty()
        || !engine.graph_nodes_for(&singleton_fqn).is_empty()
        || !engine.symbol_facts_for(&constant_fqn).is_empty()
}

fn is_module_instance_namespace(engine: &crate::AnalysisEngine, fqn: &FullyQualifiedName) -> bool {
    if fqn.namespace_kind() != Some(crate::core::NamespaceKind::Instance) {
        return false;
    }
    engine
        .graph_nodes_for(fqn)
        .iter()
        .any(|fact| fact.kind == GraphNodeKind::Module)
}

fn module_includers(
    engine: &crate::AnalysisEngine,
    module_fqn: &FullyQualifiedName,
) -> Vec<FullyQualifiedName> {
    let mut result = Vec::new();
    let mut visited = std::collections::HashSet::new();
    let mut queue = std::collections::VecDeque::new();

    for edge in engine.all_graph_edges() {
        if &edge.target == module_fqn
            && matches!(edge.kind, GraphEdgeKind::Include | GraphEdgeKind::Prepend)
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

fn related_namespaces_for_method_references(
    engine: &crate::AnalysisEngine,
    origin_fqn: &FullyQualifiedName,
) -> Vec<FullyQualifiedName> {
    let mut result = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut queue = std::collections::VecDeque::new();

    if seen.insert(origin_fqn.clone()) {
        queue.push_back(origin_fqn.clone());
    }

    while let Some(current) = queue.pop_front() {
        result.push(current.clone());

        for descendant in descendants(engine, &current) {
            if seen.insert(descendant.clone()) {
                queue.push_back(descendant);
            }
        }

        for includer in module_includers(engine, &current) {
            if seen.insert(includer.clone()) {
                queue.push_back(includer);
            }
        }
    }

    result.sort_by_key(|fqn| fqn.to_string());
    result
}

fn descendants(
    engine: &crate::AnalysisEngine,
    origin_fqn: &FullyQualifiedName,
) -> Vec<FullyQualifiedName> {
    let mut result = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut queue = std::collections::VecDeque::new();
    queue.push_back(origin_fqn.clone());
    seen.insert(origin_fqn.clone());

    while let Some(current) = queue.pop_front() {
        for edge in engine.all_graph_edges() {
            if edge.kind == GraphEdgeKind::Superclass
                && edge.target == current
                && seen.insert(edge.source.clone())
            {
                result.push(edge.source.clone());
                queue.push_back(edge.source);
            }
        }
    }

    result
}

pub(super) fn method_lookup_chain(
    engine: &crate::AnalysisEngine,
    fqn: &FullyQualifiedName,
) -> Vec<FullyQualifiedName> {
    assert!(
        matches!(fqn, FullyQualifiedName::Namespace(_, _)),
        "INVARIANT VIOLATED: analysis method lookup requested for non-namespace FQN: {fqn}. \
         This is a bug because only namespaces have method lookup chains. \
         Fix: resolve receivers to Namespace FQNs before method lookup."
    );

    if engine.graph_nodes_for(fqn).is_empty() {
        if fqn.namespace_parts().is_empty() {
            let mut chain = Vec::new();
            let mut visited = std::collections::HashSet::new();
            build_mro(engine, fqn, &mut chain, &mut visited);
            if chain.is_empty() {
                chain.push(fqn.clone());
            }
            return chain;
        }

        return vec![
            fqn.clone(),
            FullyQualifiedName::namespace_with_kind(
                Vec::new(),
                crate::core::NamespaceKind::Instance,
            ),
        ];
    }

    let mut chain = Vec::new();
    let mut visited = std::collections::HashSet::new();
    build_mro(engine, fqn, &mut chain, &mut visited);

    let root =
        FullyQualifiedName::namespace_with_kind(Vec::new(), crate::core::NamespaceKind::Instance);
    if !chain.contains(&root) && !fqn.namespace_parts().is_empty() {
        chain.push(root);
    }

    chain
}

fn build_mro(
    engine: &crate::AnalysisEngine,
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
    engine: &crate::AnalysisEngine,
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
    engine: &crate::AnalysisEngine,
    ancestor_chain: &[FullyQualifiedName],
    method: &RubyMethod,
    resolution: MethodCalleeResolution,
) -> Option<ResolvedMethodCallee> {
    for ancestor in ancestor_chain {
        let method_fqn = FullyQualifiedName::method(ancestor.namespace_parts(), *method);
        let definition_ranges = engine
            .method_facts_for(&method_fqn)
            .iter()
            .filter(|fact| {
                ancestor_chain.iter().any(|chain_fqn| {
                    chain_fqn.namespace_parts() == fact.owner.namespace_parts()
                        && chain_fqn.namespace_kind() == fact.owner.namespace_kind()
                })
            })
            .map(|fact| fact.range)
            .collect::<Vec<_>>();

        if !definition_ranges.is_empty() {
            return Some(ResolvedMethodCallee {
                owner: ancestor.clone(),
                method: *method,
                resolution,
                definition_ranges,
            });
        }
    }

    None
}

fn resolve_constant_fqn(
    engine: &crate::AnalysisEngine,
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

        let namespace_fqn = FullyQualifiedName::namespace_with_kind(
            probe.clone(),
            crate::core::NamespaceKind::Instance,
        );
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

pub(super) fn node_kind(
    engine: &crate::AnalysisEngine,
    fqn: &FullyQualifiedName,
) -> Option<GraphNodeKind> {
    engine.graph_nodes_for(fqn).first().map(|fact| fact.kind)
}
