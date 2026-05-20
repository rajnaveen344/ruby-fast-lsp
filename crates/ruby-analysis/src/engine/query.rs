use std::collections::HashSet;
use std::path::Path;

use crate::core::{
    DiagnosticFact, FullyQualifiedName, GraphEdgeFact, GraphEdgeKind, GraphNodeFact, GraphNodeKind,
    MethodCalleeResolution, MethodFact, NamespaceKind, ReferenceFact, ResolvedMethodCallee,
    RubyConstant, RubyMethod, RubyType, SourceFileId, SymbolFact, SymbolKind, TextRange, TypeFact,
    TypeResolution, TypeSubject,
};

use crate::engine::query_types::{
    ConstantCompletionCandidate, ConstantCompletionRequest, MethodCompletionCandidate, MixinUsage,
    MixinUsageKind, VariableTypeKind,
};
use crate::{AnalysisEngine, SourceFile};

pub struct AnalysisQuery<'a> {
    pub(crate) engine: &'a AnalysisEngine,
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

    pub fn has_symbols(&self) -> bool {
        !self.engine.all_symbol_facts().is_empty()
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

    pub fn diagnostic_facts_in_file(&self, file_id: SourceFileId) -> Vec<DiagnosticFact> {
        self.engine.diagnostic_facts_in_file(file_id)
    }

    pub fn all_diagnostic_facts(&self) -> Vec<DiagnosticFact> {
        self.engine.all_diagnostic_facts()
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

    pub fn constant_completion_candidates(
        &self,
        request: &ConstantCompletionRequest,
    ) -> Vec<ConstantCompletionCandidate> {
        let mut seen = HashSet::new();
        let mut candidates = self
            .engine
            .all_symbol_facts()
            .into_iter()
            .filter(|fact| {
                matches!(
                    fact.kind,
                    SymbolKind::Class | SymbolKind::Module | SymbolKind::Constant
                )
            })
            .filter(|fact| seen.insert(fact.fqn.namespace_parts()))
            .filter(|fact| constant_completion_matches(&fact.fqn, request))
            .map(|fact| ConstantCompletionCandidate {
                fqn: fact.fqn,
                kind: fact.kind,
            })
            .collect::<Vec<_>>();

        candidates.sort_by(|left, right| left.fqn.name().cmp(&right.fqn.name()));
        candidates.truncate(request.limit);
        candidates
    }

    pub fn method_completion_candidates(
        &self,
        receiver_type: &RubyType,
        partial_method: &str,
        kind: NamespaceKind,
    ) -> Vec<MethodCompletionCandidate> {
        let mut candidates = Vec::new();
        for namespace_fqn in receiver_type_to_namespaces(receiver_type, kind) {
            for fact in self.method_completion_facts(&namespace_fqn, partial_method) {
                candidates.push(self.method_completion_candidate(&fact));
            }
        }
        candidates
    }

    pub fn top_level_method_completion_candidates(
        &self,
        partial_method: &str,
    ) -> Vec<MethodCompletionCandidate> {
        self.top_level_method_completion_facts(partial_method)
            .into_iter()
            .map(|fact| self.method_completion_candidate(&fact))
            .collect()
    }

    pub fn module_mixin_usages(&self, module_fqn: &FullyQualifiedName) -> Vec<MixinUsage> {
        let mut usages = Vec::new();
        for edge in self.engine.all_graph_edges() {
            if edge.target.namespace_parts() != module_fqn.namespace_parts() {
                continue;
            }
            if matches!(edge.kind, GraphEdgeKind::Include | GraphEdgeKind::Prepend)
                && edge.source.namespace_kind() != Some(NamespaceKind::Instance)
            {
                continue;
            }
            let Some(kind) = mixin_usage_kind_for_graph_edge(edge.kind) else {
                continue;
            };
            usages.push(MixinUsage {
                kind,
                range: edge.range,
            });
        }
        usages.sort_by_key(|usage| (usage.kind, usage.range.file_id, usage.range.start_byte));
        usages
    }

    pub fn module_including_class_definition_ranges(
        &self,
        module_fqn: &FullyQualifiedName,
    ) -> Vec<TextRange> {
        let mut result = Vec::new();
        let mut queue = vec![module_fqn.clone()];
        let mut visited = Vec::new();

        while let Some(target) = queue.pop() {
            if visited.contains(&target) {
                continue;
            }
            visited.push(target.clone());

            for edge in self.engine.all_graph_edges() {
                if !matches!(
                    edge.kind,
                    GraphEdgeKind::Include | GraphEdgeKind::Prepend | GraphEdgeKind::Extend
                ) {
                    continue;
                }
                if matches!(edge.kind, GraphEdgeKind::Include | GraphEdgeKind::Prepend)
                    && edge.source.namespace_kind() != Some(NamespaceKind::Instance)
                {
                    continue;
                }
                if edge.target.namespace_parts() != target.namespace_parts() {
                    continue;
                }

                let nodes = self.engine.graph_nodes_for(&edge.source);
                if nodes.iter().any(|node| node.kind == GraphNodeKind::Class) {
                    result.extend(
                        nodes
                            .into_iter()
                            .filter(|node| node.kind == GraphNodeKind::Class)
                            .map(|node| node.range),
                    );
                } else if nodes.iter().any(|node| node.kind == GraphNodeKind::Module) {
                    queue.push(edge.source.clone());
                }
            }
        }

        result.sort_by_key(|range| (range.file_id, range.start_byte, range.end_byte));
        result.dedup();
        result
    }

    pub fn method_return_type_at(
        &self,
        name: &str,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> Option<RubyType> {
        let method_fact = self
            .engine
            .method_store()
            .facts_in_file(file_id)
            .into_iter()
            .find(|fact| {
                let FullyQualifiedName::Method(_, method) = &fact.fqn else {
                    return false;
                };
                method.as_str() == name
                    && fact.range.start_byte <= byte_offset
                    && byte_offset <= fact.range.end_byte
            })?;

        self.engine
            .type_store()
            .facts_in_file(file_id)
            .into_iter()
            .filter_map(|fact| match &fact.subject {
                TypeSubject::MethodReturn(method) if method == &method_fact.fqn => Some(fact),
                TypeSubject::Constant(_)
                | TypeSubject::Local { .. }
                | TypeSubject::InstanceVariable { .. }
                | TypeSubject::ClassVariable { .. }
                | TypeSubject::GlobalVariable(_)
                | TypeSubject::MethodReturn(_)
                | TypeSubject::Parameter { .. }
                | TypeSubject::Expression(_) => None,
            })
            .max_by_key(|fact| fact.range.start_byte)
            .map(|fact| fact.ruby_type)
    }

    pub fn parameter_type_at(
        &self,
        method_name: &str,
        param_name: &str,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> Option<RubyType> {
        let method_fact = self
            .engine
            .method_store()
            .facts_in_file(file_id)
            .into_iter()
            .find(|fact| {
                let FullyQualifiedName::Method(_, method) = &fact.fqn else {
                    return false;
                };
                method.as_str() == method_name
                    && fact.range.start_byte <= byte_offset
                    && byte_offset <= fact.range.end_byte
            })?;

        self.engine
            .type_store()
            .facts_in_file(file_id)
            .into_iter()
            .filter_map(|fact| match &fact.subject {
                TypeSubject::Parameter { method, name }
                    if method == &method_fact.fqn
                        && name == param_name
                        && fact.ruby_type != RubyType::Unknown =>
                {
                    Some(fact)
                }
                TypeSubject::Constant(_)
                | TypeSubject::Local { .. }
                | TypeSubject::InstanceVariable { .. }
                | TypeSubject::ClassVariable { .. }
                | TypeSubject::GlobalVariable(_)
                | TypeSubject::MethodReturn(_)
                | TypeSubject::Parameter { .. }
                | TypeSubject::Expression(_) => None,
            })
            .max_by_key(|fact| fact.range.start_byte)
            .map(|fact| fact.ruby_type)
    }

    pub fn variable_type_before(
        &self,
        kind: VariableTypeKind,
        name: &str,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> Option<RubyType> {
        self.engine
            .type_store()
            .facts_in_file(file_id)
            .into_iter()
            .filter(|fact| fact.range.start_byte <= byte_offset)
            .filter_map(|fact| match (&fact.subject, kind) {
                (
                    TypeSubject::Local {
                        scope_id: _,
                        name: fact_name,
                    },
                    VariableTypeKind::Local,
                ) if fact_name == name && fact.ruby_type != RubyType::Unknown => Some(fact),
                (
                    TypeSubject::InstanceVariable {
                        name: fact_name, ..
                    },
                    VariableTypeKind::Instance,
                ) if fact_name == name && fact.ruby_type != RubyType::Unknown => Some(fact),
                (
                    TypeSubject::ClassVariable {
                        name: fact_name, ..
                    },
                    VariableTypeKind::Class,
                ) if fact_name == name && fact.ruby_type != RubyType::Unknown => Some(fact),
                (TypeSubject::GlobalVariable(fact_name), VariableTypeKind::Global)
                    if fact_name == name && fact.ruby_type != RubyType::Unknown =>
                {
                    Some(fact)
                }
                (
                    TypeSubject::Constant(_)
                    | TypeSubject::Local { .. }
                    | TypeSubject::InstanceVariable { .. }
                    | TypeSubject::ClassVariable { .. }
                    | TypeSubject::GlobalVariable(_)
                    | TypeSubject::MethodReturn(_)
                    | TypeSubject::Parameter { .. }
                    | TypeSubject::Expression(_),
                    VariableTypeKind::Local
                    | VariableTypeKind::Instance
                    | VariableTypeKind::Class
                    | VariableTypeKind::Global
                    | VariableTypeKind::Constant,
                ) => None,
            })
            .max_by_key(|fact| fact.range.start_byte)
            .map(|fact| fact.ruby_type)
    }

    pub fn variable_type_any_before(
        &self,
        name: &str,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> Option<RubyType> {
        self.engine
            .type_store()
            .facts_in_file(file_id)
            .into_iter()
            .filter(|fact| fact.range.start_byte <= byte_offset)
            .filter_map(|fact| match &fact.subject {
                TypeSubject::Local {
                    scope_id: _,
                    name: fact_name,
                }
                | TypeSubject::InstanceVariable {
                    name: fact_name, ..
                }
                | TypeSubject::ClassVariable {
                    name: fact_name, ..
                } if fact_name == name && fact.ruby_type != RubyType::Unknown => Some(fact),
                TypeSubject::GlobalVariable(fact_name)
                    if fact_name == name && fact.ruby_type != RubyType::Unknown =>
                {
                    Some(fact)
                }
                TypeSubject::Constant(_)
                | TypeSubject::Local { .. }
                | TypeSubject::InstanceVariable { .. }
                | TypeSubject::ClassVariable { .. }
                | TypeSubject::GlobalVariable(_)
                | TypeSubject::MethodReturn(_)
                | TypeSubject::Parameter { .. }
                | TypeSubject::Expression(_) => None,
            })
            .max_by_key(|fact| fact.range.start_byte)
            .map(|fact| fact.ruby_type)
    }

    pub fn local_variable_type_at(
        &self,
        name: &str,
        scope_id: u32,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> Option<RubyType> {
        match self.engine.type_store().type_at(
            &TypeSubject::Local {
                scope_id,
                name: name.to_string(),
            },
            file_id,
            byte_offset,
        ) {
            TypeResolution::Resolved(fact) => return Some(fact.ruby_type),
            TypeResolution::Ambiguous(_) => return None,
            TypeResolution::Unresolved => {}
        }

        self.engine
            .type_store()
            .facts_in_file(file_id)
            .into_iter()
            .filter(|fact| fact.range.start_byte <= byte_offset)
            .filter_map(|fact| match &fact.subject {
                TypeSubject::Parameter {
                    method: _,
                    name: fact_name,
                } if fact_name == name && fact.ruby_type != RubyType::Unknown => Some(fact),
                TypeSubject::Constant(_)
                | TypeSubject::Local { .. }
                | TypeSubject::InstanceVariable { .. }
                | TypeSubject::ClassVariable { .. }
                | TypeSubject::GlobalVariable(_)
                | TypeSubject::MethodReturn(_)
                | TypeSubject::Parameter { .. }
                | TypeSubject::Expression(_) => None,
            })
            .max_by_key(|fact| fact.range.start_byte)
            .map(|fact| fact.ruby_type)
    }

    pub fn variable_type_in_file(
        &self,
        kind: VariableTypeKind,
        name: &str,
        file_id: SourceFileId,
    ) -> Option<RubyType> {
        self.engine
            .type_store()
            .facts_in_file(file_id)
            .into_iter()
            .filter_map(|fact| variable_type_fact_match(fact, kind, name))
            .max_by_key(|fact| fact.range.start_byte)
            .map(|fact| fact.ruby_type)
    }

    pub fn namespace_node_kind(&self, namespace_fqn: &FullyQualifiedName) -> Option<GraphNodeKind> {
        self.engine
            .graph_nodes_for(namespace_fqn)
            .iter()
            .max_by_key(|fact| {
                (
                    fact.range.file_id,
                    fact.range.start_byte,
                    fact.range.end_byte,
                )
            })
            .map(|fact| fact.kind)
    }

    pub fn namespace_type(&self, namespace_fqn: &FullyQualifiedName) -> Option<RubyType> {
        match self.namespace_node_kind(namespace_fqn)? {
            GraphNodeKind::Class => Some(RubyType::Class(namespace_fqn.clone())),
            GraphNodeKind::Module => Some(RubyType::Module(namespace_fqn.clone())),
        }
    }

    pub fn constant_reference_type(&self, path: &[RubyConstant]) -> Option<RubyType> {
        let namespace_fqn = FullyQualifiedName::namespace(path.to_vec());
        let constant_fqn = FullyQualifiedName::Constant(path.to_vec());
        match self.namespace_node_kind(&namespace_fqn)? {
            GraphNodeKind::Class => Some(RubyType::ClassReference(constant_fqn)),
            GraphNodeKind::Module => Some(RubyType::ModuleReference(constant_fqn)),
        }
    }

    pub fn constant_value_type(&self, constant_fqn: &FullyQualifiedName) -> Option<RubyType> {
        self.engine
            .type_store()
            .facts_for(&TypeSubject::Constant(constant_fqn.clone()))
            .iter()
            .filter(|fact| fact.ruby_type != RubyType::Unknown)
            .max_by_key(|fact| {
                (
                    fact.range.file_id,
                    fact.range.start_byte,
                    fact.range.end_byte,
                )
            })
            .map(|fact| fact.ruby_type.clone())
    }

    pub fn known_namespace_fqns(&self) -> HashSet<FullyQualifiedName> {
        self.engine
            .all_symbol_facts()
            .into_iter()
            .filter(|fact| matches!(fact.kind, SymbolKind::Class | SymbolKind::Module))
            .filter_map(|fact| fact.fqn.to_instance_namespace())
            .collect()
    }

    pub fn method_fact_for_receiver(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Option<MethodFact> {
        if !namespace_target_exists(self.engine, namespace_fqn) {
            return None;
        }

        for ancestor in method_lookup_chain(self.engine, namespace_fqn) {
            let method_fqn = FullyQualifiedName::method(ancestor.namespace_parts(), *method);
            let mut facts = self
                .engine
                .method_facts_for(&method_fqn)
                .iter()
                .filter(|fact| {
                    fact.owner.namespace_parts() == ancestor.namespace_parts()
                        && fact.owner.namespace_kind() == ancestor.namespace_kind()
                })
                .cloned()
                .collect::<Vec<_>>();

            facts.sort_by_key(|fact| {
                (
                    fact.range.file_id,
                    fact.range.start_byte,
                    fact.range.end_byte,
                    fact.fqn.to_string(),
                )
            });
            facts.dedup();

            match facts.len() {
                0 => continue,
                1 => return facts.pop(),
                _ => return None,
            }
        }

        None
    }

    pub fn method_return_type(&self, fact: &MethodFact) -> Option<crate::core::RubyType> {
        match self.engine.type_at(
            &TypeSubject::MethodReturn(fact.fqn.clone()),
            fact.range.file_id,
            fact.range.end_byte,
        ) {
            TypeResolution::Resolved(type_fact) => Some(type_fact.ruby_type),
            TypeResolution::Ambiguous(_) | TypeResolution::Unresolved => None,
        }
    }

    pub fn method_return_type_for_receiver(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Option<crate::core::RubyType> {
        if !namespace_target_exists(self.engine, namespace_fqn) {
            return None;
        }

        for ancestor in method_lookup_chain(self.engine, namespace_fqn) {
            let method_fqn = FullyQualifiedName::method(ancestor.namespace_parts(), *method);
            let facts = self
                .engine
                .method_facts_for(&method_fqn)
                .iter()
                .filter(|fact| {
                    fact.owner.namespace_parts() == ancestor.namespace_parts()
                        && fact.owner.namespace_kind() == ancestor.namespace_kind()
                })
                .collect::<Vec<_>>();

            if facts.is_empty() {
                continue;
            }

            let mut return_types = facts
                .into_iter()
                .filter_map(|fact| self.method_return_type(fact))
                .collect::<Vec<_>>();

            if return_types.is_empty() {
                return None;
            }

            return_types.sort_by_key(|ruby_type| ruby_type.to_string());
            return_types.dedup();
            return match return_types.len() {
                1 => return_types.pop(),
                _ => Some(crate::core::RubyType::union(return_types)),
            };
        }

        None
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

    fn method_completion_candidate(&self, fact: &MethodFact) -> MethodCompletionCandidate {
        let FullyQualifiedName::Method(_, method) = &fact.fqn else {
            panic!(
                "INVARIANT VIOLATED: analysis method completion fact has non-method FQN: {}. \
                 This is a bug because MethodStore must only contain method facts. \
                 Fix: reject non-method FQNs in MethodFact construction.",
                fact.fqn
            );
        };

        MethodCompletionCandidate {
            name: method.get_name(),
            params: fact
                .params
                .iter()
                .filter(|param| !param.is_empty())
                .cloned()
                .collect(),
            return_type: self.method_return_type(fact),
        }
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

fn namespace_target_exists(engine: &crate::AnalysisEngine, fqn: &FullyQualifiedName) -> bool {
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

fn constant_completion_matches(
    fqn: &FullyQualifiedName,
    request: &ConstantCompletionRequest,
) -> bool {
    if request.is_qualified {
        if let Some(namespace_prefix) = &request.namespace_prefix {
            let fqn_parts = fqn.namespace_parts();
            let namespace_parts = namespace_prefix.namespace_parts();
            if fqn_parts.len() != namespace_parts.len() + 1 {
                return false;
            }
            if !fqn_parts.starts_with(&namespace_parts) {
                return false;
            }
        } else if fqn.namespace_parts().len() > 1 {
            return false;
        }
    }

    fqn.name()
        .to_lowercase()
        .starts_with(&request.partial_name.to_lowercase())
}

fn receiver_type_to_namespaces(
    ruby_type: &RubyType,
    kind: NamespaceKind,
) -> Vec<FullyQualifiedName> {
    match ruby_type {
        RubyType::Class(fqn)
        | RubyType::ClassReference(fqn)
        | RubyType::Module(fqn)
        | RubyType::ModuleReference(fqn) => {
            vec![FullyQualifiedName::namespace_with_kind(
                fqn.namespace_parts(),
                kind,
            )]
        }
        RubyType::Array(_) => namespace_for_builtin("Array", kind),
        RubyType::Hash(_, _) => namespace_for_builtin("Hash", kind),
        RubyType::Union(types) => types
            .iter()
            .flat_map(|ty| receiver_type_to_namespaces(ty, kind))
            .collect(),
        RubyType::Unknown => Vec::new(),
    }
}

fn namespace_for_builtin(name: &str, kind: NamespaceKind) -> Vec<FullyQualifiedName> {
    let Ok(constant) = RubyConstant::new(name) else {
        return Vec::new();
    };
    vec![FullyQualifiedName::namespace_with_kind(
        vec![constant],
        kind,
    )]
}

fn mixin_usage_kind_for_graph_edge(kind: GraphEdgeKind) -> Option<MixinUsageKind> {
    match kind {
        GraphEdgeKind::Include => Some(MixinUsageKind::Include),
        GraphEdgeKind::Prepend => Some(MixinUsageKind::Prepend),
        GraphEdgeKind::Extend => Some(MixinUsageKind::Extend),
        GraphEdgeKind::Superclass => None,
    }
}

fn variable_type_fact_match(
    fact: TypeFact,
    kind: VariableTypeKind,
    name: &str,
) -> Option<TypeFact> {
    match (&fact.subject, kind) {
        (
            TypeSubject::Local {
                scope_id: _,
                name: fact_name,
            },
            VariableTypeKind::Local,
        ) if fact_name == name && fact.ruby_type != RubyType::Unknown => Some(fact),
        (
            TypeSubject::InstanceVariable {
                name: fact_name, ..
            },
            VariableTypeKind::Instance,
        ) if fact_name == name && fact.ruby_type != RubyType::Unknown => Some(fact),
        (
            TypeSubject::ClassVariable {
                name: fact_name, ..
            },
            VariableTypeKind::Class,
        ) if fact_name == name && fact.ruby_type != RubyType::Unknown => Some(fact),
        (TypeSubject::GlobalVariable(fact_name), VariableTypeKind::Global)
            if fact_name == name && fact.ruby_type != RubyType::Unknown =>
        {
            Some(fact)
        }
        (
            TypeSubject::Constant(_)
            | TypeSubject::Local { .. }
            | TypeSubject::InstanceVariable { .. }
            | TypeSubject::ClassVariable { .. }
            | TypeSubject::GlobalVariable(_)
            | TypeSubject::MethodReturn(_)
            | TypeSubject::Parameter { .. }
            | TypeSubject::Expression(_),
            VariableTypeKind::Local
            | VariableTypeKind::Instance
            | VariableTypeKind::Class
            | VariableTypeKind::Global
            | VariableTypeKind::Constant,
        ) => None,
    }
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
