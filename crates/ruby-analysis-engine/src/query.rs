use std::path::Path;

use ruby_analysis_core::{
    FullyQualifiedName, GraphEdgeFact, GraphEdgeKind, GraphNodeFact, GraphNodeKind,
    MethodCalleeResolution, MethodFact, ReferenceFact, ResolvedMethodCallee, RubyConstant,
    RubyMethod, SourceFileId, SymbolFact, TypeFact, TypeResolution, TypeSubject,
};

use crate::{AnalysisEngine, SourceFile};

pub struct AnalysisQuery<'a> {
    engine: &'a AnalysisEngine,
}

impl<'a> AnalysisQuery<'a> {
    pub fn new(engine: &'a AnalysisEngine) -> Self {
        Self { engine }
    }

    pub fn file_id(&self, path: impl AsRef<Path>) -> Option<SourceFileId> {
        self.engine.file_id(path)
    }

    pub fn file(&self, file_id: SourceFileId) -> Option<&'a SourceFile> {
        self.engine.file(file_id)
    }

    pub fn type_at(
        &self,
        subject: &TypeSubject,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> TypeResolution {
        self.engine.type_at(subject, file_id, byte_offset)
    }

    pub fn type_facts_in_file(&self, file_id: SourceFileId) -> Vec<TypeFact> {
        self.engine.type_store().facts_in_file(file_id)
    }

    pub fn symbol_facts_in_file(&self, file_id: SourceFileId) -> Vec<SymbolFact> {
        self.engine.symbol_store().facts_in_file(file_id)
    }

    pub fn all_symbol_facts(&self) -> Vec<SymbolFact> {
        self.engine.all_symbol_facts()
    }

    pub fn symbols_for_fqn(&self, fqn: &FullyQualifiedName) -> &'a [SymbolFact] {
        self.engine.symbol_facts_for(fqn)
    }

    pub fn references_for_fqn(&self, fqn: &FullyQualifiedName) -> &'a [ReferenceFact] {
        self.engine.reference_facts_for(fqn)
    }

    pub fn methods_for_fqn(&self, fqn: &FullyQualifiedName) -> &'a [MethodFact] {
        self.engine.method_facts_for(fqn)
    }

    pub fn method_facts_in_file(&self, file_id: SourceFileId) -> Vec<MethodFact> {
        self.engine.method_store().facts_in_file(file_id)
    }

    pub fn all_method_facts(&self) -> Vec<MethodFact> {
        self.engine.all_method_facts()
    }

    pub fn references_in_file(&self, file_id: SourceFileId) -> Vec<ReferenceFact> {
        self.engine.reference_store().facts_in_file(file_id)
    }

    pub fn graph_nodes_for(&self, fqn: &FullyQualifiedName) -> &'a [GraphNodeFact] {
        self.engine.graph_nodes_for(fqn)
    }

    pub fn graph_edges_from(&self, fqn: &FullyQualifiedName) -> &'a [GraphEdgeFact] {
        self.engine.graph_edges_from(fqn)
    }

    pub fn all_graph_edges(&self) -> Vec<GraphEdgeFact> {
        self.engine.all_graph_edges()
    }

    pub fn graph_nodes_in_file(&self, file_id: SourceFileId) -> Vec<GraphNodeFact> {
        self.engine.graph_store().nodes_in_file(file_id)
    }

    pub fn graph_edges_in_file(&self, file_id: SourceFileId) -> Vec<GraphEdgeFact> {
        self.engine.graph_store().edges_in_file(file_id)
    }

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

    pub fn method_completion_facts(
        &self,
        namespace_fqn: &FullyQualifiedName,
        partial: &str,
    ) -> Vec<MethodFact> {
        if !namespace_target_exists(self.engine, namespace_fqn) {
            return Vec::new();
        }

        let mut facts = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for ancestor in method_lookup_chain(self.engine, namespace_fqn) {
            for fact in self.engine.all_method_facts() {
                if fact.owner.namespace_parts() != ancestor.namespace_parts()
                    || fact.owner.namespace_kind() != ancestor.namespace_kind()
                {
                    continue;
                }
                let FullyQualifiedName::Method(_, method) = &fact.fqn else {
                    continue;
                };
                let method_name = method.get_name();
                if !method_name.starts_with(partial) {
                    continue;
                }
                if seen.insert(method_name) {
                    facts.push(fact);
                }
            }
        }
        facts.sort_by_key(|fact| fact.fqn.to_string());
        facts
    }

    pub fn method_return_type(&self, fact: &MethodFact) -> Option<ruby_analysis_core::RubyType> {
        match self.engine.type_at(
            &TypeSubject::MethodReturn(fact.fqn.clone()),
            fact.range.file_id,
            fact.range.end_byte,
        ) {
            TypeResolution::Resolved(type_fact) => Some(type_fact.ruby_type),
            TypeResolution::Ambiguous(_) | TypeResolution::Unresolved => None,
        }
    }

    pub fn top_level_method_completion_facts(&self, partial: &str) -> Vec<MethodFact> {
        let mut facts = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for fact in self.engine.all_method_facts() {
            if !fact.owner.namespace_parts().is_empty() {
                continue;
            }

            let FullyQualifiedName::Method(_, method) = &fact.fqn else {
                continue;
            };
            let method_name = method.get_name();
            if !method_name.starts_with(partial) {
                continue;
            }

            if seen.insert(method_name) {
                facts.push(fact);
            }
        }

        facts.sort_by_key(|fact| fact.fqn.to_string());
        facts
    }

    pub fn resolve_constant_receiver(
        &self,
        path: &[RubyConstant],
        current_namespace: &[RubyConstant],
    ) -> FullyQualifiedName {
        let current_fqn = FullyQualifiedName::namespace_with_kind(
            current_namespace.to_vec(),
            ruby_analysis_core::NamespaceKind::Instance,
        );
        let resolved = resolve_constant_fqn(self.engine, path, false, &current_fqn)
            .unwrap_or_else(|| FullyQualifiedName::constant(path.to_vec()));

        FullyQualifiedName::namespace_with_kind(
            resolved.namespace_parts(),
            ruby_analysis_core::NamespaceKind::Singleton,
        )
    }
}

fn namespace_target_exists(engine: &crate::AnalysisEngine, fqn: &FullyQualifiedName) -> bool {
    let parts = fqn.namespace_parts();
    if parts.is_empty() {
        return true;
    }
    let instance_fqn = FullyQualifiedName::namespace_with_kind(
        parts.clone(),
        ruby_analysis_core::NamespaceKind::Instance,
    );
    let singleton_fqn = FullyQualifiedName::namespace_with_kind(
        parts.clone(),
        ruby_analysis_core::NamespaceKind::Singleton,
    );
    let constant_fqn = FullyQualifiedName::constant(parts);

    !engine.graph_nodes_for(&instance_fqn).is_empty()
        || !engine.graph_nodes_for(&singleton_fqn).is_empty()
        || !engine.symbol_facts_for(&constant_fqn).is_empty()
}

fn is_module_instance_namespace(engine: &crate::AnalysisEngine, fqn: &FullyQualifiedName) -> bool {
    if fqn.namespace_kind() != Some(ruby_analysis_core::NamespaceKind::Instance) {
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

fn method_lookup_chain(
    engine: &crate::AnalysisEngine,
    fqn: &FullyQualifiedName,
) -> Vec<FullyQualifiedName> {
    assert!(
        matches!(fqn, FullyQualifiedName::Namespace(_, _)),
        "INVARIANT VIOLATED: analysis method lookup requested for non-namespace FQN: {fqn}. \
         This is a bug because only namespaces have method lookup chains. \
         Fix: resolve receivers to Namespace FQNs before method lookup."
    );

    if fqn.namespace_parts().is_empty() {
        return vec![fqn.clone()];
    }

    if engine.graph_nodes_for(fqn).is_empty() {
        return vec![
            fqn.clone(),
            FullyQualifiedName::namespace_with_kind(
                Vec::new(),
                ruby_analysis_core::NamespaceKind::Instance,
            ),
        ];
    }

    let mut chain = Vec::new();
    let mut visited = std::collections::HashSet::new();
    build_mro(engine, fqn, &mut chain, &mut visited);

    let root = FullyQualifiedName::namespace_with_kind(
        Vec::new(),
        ruby_analysis_core::NamespaceKind::Instance,
    );
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
            ruby_analysis_core::NamespaceKind::Instance,
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

fn node_kind(engine: &crate::AnalysisEngine, fqn: &FullyQualifiedName) -> Option<GraphNodeKind> {
    engine.graph_nodes_for(fqn).first().map(|fact| fact.kind)
}
