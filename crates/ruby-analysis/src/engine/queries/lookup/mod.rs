pub(in crate::engine) mod types;

use std::collections::HashSet;

use crate::core::{
    FullyQualifiedName, GraphEdgeKind, GraphNodeKind, MethodFact, NamespaceKind, RubyConstant,
    RubyType, SymbolKind, TextRange,
};
use crate::engine::queries::lookup::types::{
    ConstantLookupRequest, ConstantMatch, MethodMatch, MixinUsage, MixinUsageKind,
};
use crate::engine::queries::View;
use crate::engine::resolution::{
    execution_context_application_targets, method_lookup_chain, namespace_target_exists,
};

impl<'a> View<'a> {
    pub fn method_facts_matching(
        &self,
        namespace_fqn: &FullyQualifiedName,
        partial: &str,
    ) -> Vec<MethodFact> {
        if !namespace_target_exists(self.engine, namespace_fqn) {
            return Vec::new();
        }

        let mut facts = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let mut lookup_roots = vec![namespace_fqn.clone()];
        lookup_roots.extend(execution_context_application_targets(
            self.engine,
            namespace_fqn,
        ));
        for root in lookup_roots {
            for ancestor in method_lookup_chain(self.engine, &root) {
                for fact in self.method_facts_matching_owner(&ancestor, partial) {
                    let FullyQualifiedName::Method(_, method) = &fact.fqn else {
                        continue;
                    };
                    let method_name = method.get_name();
                    if seen.insert(method_name) {
                        facts.push(fact);
                    }
                }
            }
        }
        facts.sort_by_key(|fact| fact.fqn.to_string());
        facts
    }

    pub fn constant_matches(&self, request: &ConstantLookupRequest) -> Vec<ConstantMatch> {
        let mut seen = HashSet::new();
        let mut candidates = self
            .symbol_facts()
            .filter(|fact| {
                matches!(
                    fact.kind,
                    SymbolKind::Class | SymbolKind::Module | SymbolKind::Constant
                ) && !fact.fqn.has_generated_owner()
            })
            .filter(|fact| seen.insert(fact.fqn.namespace_parts()))
            .filter(|fact| Self::constant_matches_request(&fact.fqn, request))
            .map(|fact| ConstantMatch {
                fqn: fact.fqn,
                kind: fact.kind,
            })
            .collect::<Vec<_>>();

        candidates.sort_by(|left, right| left.fqn.name().cmp(&right.fqn.name()));
        candidates.truncate(request.limit);
        candidates
    }

    pub fn method_matches_for_type(
        &self,
        receiver_type: &RubyType,
        partial_method: &str,
        kind: NamespaceKind,
    ) -> Vec<MethodMatch> {
        if let RubyType::Union(members) = receiver_type {
            let Some((first, rest)) = members.split_first() else {
                unreachable_invariant!(
                    what = "completion received an empty RubyType::Union",
                    why = "RubyType::union collapses empty inputs to Unknown",
                    fix = "construct receiver unions only through the canonical RubyType helpers",
                );
            };
            let mut common = self
                .method_matches_for_type(first, partial_method, kind)
                .into_iter()
                .map(|candidate| (candidate.name.clone(), candidate))
                .collect::<std::collections::BTreeMap<_, _>>();

            for member in rest {
                let member_matches = self
                    .method_matches_for_type(member, partial_method, kind)
                    .into_iter()
                    .map(|candidate| (candidate.name.clone(), candidate))
                    .collect::<std::collections::BTreeMap<_, _>>();
                common.retain(|name, candidate| {
                    let Some(member_candidate) = member_matches.get(name) else {
                        return false;
                    };
                    if candidate.params != member_candidate.params {
                        return false;
                    }
                    candidate.return_type = match (
                        candidate.return_type.take(),
                        member_candidate.return_type.clone(),
                    ) {
                        (Some(left), Some(right)) => {
                            RubyType::union_from_proven([left, right], Some)
                        }
                        (Some(_), None) | (None, Some(_)) | (None, None) => None,
                    };
                    true
                });
            }

            return common.into_values().collect();
        }

        let mut candidates = Vec::new();
        for namespace_fqn in Self::receiver_type_to_namespaces(receiver_type, kind) {
            for fact in self.method_facts_matching(&namespace_fqn, partial_method) {
                candidates.push(self.method_match(&fact));
            }
        }
        candidates
    }

    pub fn top_level_method_matches(&self, partial_method: &str) -> Vec<MethodMatch> {
        self.top_level_method_facts_matching(partial_method)
            .into_iter()
            .map(|fact| self.method_match(&fact))
            .collect()
    }

    pub fn module_mixin_usages(&self, module_fqn: &FullyQualifiedName) -> Vec<MixinUsage> {
        let mut usages = Vec::new();
        for edge in self.graph_edges_targeting(module_fqn.namespace_parts_slice()) {
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
        usages.sort_by_key(|usage| {
            (
                usage.kind,
                usage.range.file_id,
                usage.range.start_byte,
                usage.range.end_byte,
            )
        });
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

            for edge in self.graph_edges_targeting(target.namespace_parts_slice()) {
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

                let nodes = self.graph_nodes_for(&edge.source);
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

    pub fn top_level_method_facts_matching(&self, partial: &str) -> Vec<MethodFact> {
        let mut facts = Vec::new();
        let mut seen = std::collections::HashSet::new();

        let top_level = self.method_facts_where(|fact, names| {
            fact.method
                .is_some_and(|method| method.get_name().starts_with(partial))
                && names
                    .fqn(fact.owner)
                    .is_none_or(|owner| owner.namespace_parts_slice().is_empty())
        });
        for fact in top_level {
            let FullyQualifiedName::Method(_, method) = &fact.fqn else {
                continue;
            };
            if seen.insert(method.get_name()) {
                facts.push(fact);
            }
        }

        facts.sort_by_key(|fact| fact.fqn.to_string());
        facts
    }

    fn method_match(&self, fact: &MethodFact) -> MethodMatch {
        let FullyQualifiedName::Method(_, method) = &fact.fqn else {
            unreachable_invariant!(
                what = "analysis method match fact has non-method FQN: {}",
                why = "MethodStore must only contain method facts",
                fix = "reject non-method FQNs in MethodFact construction",
                fact.fqn,
            );
        };

        MethodMatch {
            name: method.get_name(),
            params: fact.param_names().map(str::to_string).collect(),
            return_type: self.method_return_type(fact),
        }
    }

    fn constant_matches_request(fqn: &FullyQualifiedName, request: &ConstantLookupRequest) -> bool {
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

    pub fn receiver_type_to_namespaces(
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
            RubyType::Hash(_, _) | RubyType::Shape(_) => Self::namespace_for_builtin("Hash", kind),
            RubyType::Literal(value) => {
                Self::receiver_type_to_namespaces(&value.widened_type(), kind)
            }
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
            GraphEdgeKind::ExecutionContextApplication => None,
        }
    }
}
