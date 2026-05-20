use std::collections::HashSet;

use crate::core::{
    FullyQualifiedName, GraphEdgeKind, GraphNodeKind, MethodFact, NamespaceKind, RubyConstant,
    RubyType, SymbolKind, TextRange,
};
use crate::engine::query::{method_lookup_chain, namespace_target_exists, AnalysisQuery};
use crate::engine::query_types::{
    ConstantCompletionCandidate, ConstantCompletionRequest, MethodCompletionCandidate, MixinUsage,
    MixinUsageKind,
};

impl<'a> AnalysisQuery<'a> {
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
            .filter(|fact| Self::constant_completion_matches(&fact.fqn, request))
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
        for namespace_fqn in Self::receiver_type_to_namespaces(receiver_type, kind) {
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
            let Some(kind) = Self::mixin_usage_kind_for_graph_edge(edge.kind) else {
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
            RubyType::Array(_) => Self::namespace_for_builtin("Array", kind),
            RubyType::Hash(_, _) => Self::namespace_for_builtin("Hash", kind),
            RubyType::Union(types) => types
                .iter()
                .flat_map(|ty| Self::receiver_type_to_namespaces(ty, kind))
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
}
