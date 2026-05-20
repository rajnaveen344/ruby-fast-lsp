use std::collections::HashSet;

use crate::core::{
    FullyQualifiedName, GraphEdgeFact, GraphEdgeKind, GraphNodeKind, RubyConstant, RubyMethod,
    SourceFileId, TextRange,
};
use crate::engine::query::AnalysisQuery;
use crate::engine::query_types::{
    CallHierarchyMethod, IncomingCall, OutgoingCall, TypeHierarchyEntry, TypeHierarchyRelation,
};

impl<'a> AnalysisQuery<'a> {
    pub fn parse_method_fqn(&self, fqn: &str) -> Option<FullyQualifiedName> {
        parse_method_fqn_string(fqn)
    }

    pub fn call_hierarchy_method(
        &self,
        method_fqn: &FullyQualifiedName,
    ) -> Option<CallHierarchyMethod> {
        let fact = self.engine.method_facts_for(method_fqn).first()?;
        Some(CallHierarchyMethod {
            fqn: method_fqn.clone(),
            range: fact.range,
        })
    }

    pub fn incoming_calls(&self, method_fqn: &FullyQualifiedName) -> Vec<IncomingCall> {
        let mut grouped = Vec::<(FullyQualifiedName, Vec<TextRange>)>::new();
        for fact in self.engine.reference_facts_for(method_fqn) {
            let Some(caller) = &fact.caller else {
                continue;
            };
            push_grouped_text_range(&mut grouped, caller.clone(), fact.range);
        }

        grouped.sort_by(|(left, _), (right, _)| left.to_string().cmp(&right.to_string()));
        grouped
            .into_iter()
            .filter_map(|(caller_fqn, ranges)| {
                Some(IncomingCall {
                    from: self.call_hierarchy_method(&caller_fqn)?,
                    from_ranges: ranges,
                })
            })
            .collect()
    }

    pub fn outgoing_calls(&self, method_fqn: &FullyQualifiedName) -> Vec<OutgoingCall> {
        let mut grouped = Vec::<(FullyQualifiedName, Vec<TextRange>)>::new();
        for fact in self.engine.reference_store().all_facts() {
            if fact.caller.as_ref() != Some(method_fqn) {
                continue;
            }
            push_grouped_text_range(&mut grouped, fact.target, fact.range);
        }

        grouped.sort_by(|(left, _), (right, _)| left.to_string().cmp(&right.to_string()));
        grouped
            .into_iter()
            .filter_map(|(callee_fqn, ranges)| {
                Some(OutgoingCall {
                    to: self.call_hierarchy_method(&callee_fqn)?,
                    from_ranges: ranges,
                })
            })
            .collect()
    }

    pub fn parse_namespace_fqn(&self, fqn: &str) -> Option<FullyQualifiedName> {
        parse_namespace_fqn_string(fqn)
    }

    pub fn type_hierarchy_node(
        &self,
        fqn: &FullyQualifiedName,
    ) -> Option<(GraphNodeKind, TextRange)> {
        let node = self.engine.graph_nodes_for(fqn).first()?;
        Some((node.kind, node.range))
    }

    pub fn supertypes(&self, fqn: &FullyQualifiedName) -> Vec<TypeHierarchyEntry> {
        let primary_file_id = match self.engine.graph_nodes_for(fqn).first() {
            Some(node) => node.range.file_id,
            None => return Vec::new(),
        };

        let edges = self.engine.graph_edges_from(fqn);
        let mut supertypes = Vec::new();
        push_supertype_entries(
            self.engine,
            edges,
            GraphEdgeKind::Prepend,
            TypeHierarchyRelation::Prepend,
            &mut supertypes,
        );
        push_supertype_entries(
            self.engine,
            edges,
            GraphEdgeKind::Include,
            TypeHierarchyRelation::Include,
            &mut supertypes,
        );
        push_supertype_entries(
            self.engine,
            edges,
            GraphEdgeKind::Superclass,
            TypeHierarchyRelation::Superclass,
            &mut supertypes,
        );
        push_supertype_entries(
            self.engine,
            edges,
            GraphEdgeKind::Extend,
            TypeHierarchyRelation::Extend,
            &mut supertypes,
        );
        push_unresolved_supertype_entries(self.engine, fqn, &mut supertypes);

        for entry in &mut supertypes {
            if entry.edge_file_id == Some(primary_file_id) {
                entry.edge_file_id = None;
            }
        }

        supertypes
    }

    pub fn subtypes(&self, fqn: &FullyQualifiedName) -> Vec<TypeHierarchyEntry> {
        if self.engine.graph_nodes_for(fqn).is_empty() {
            return Vec::new();
        }

        let mut subclass_edges = Vec::new();
        let mut included_by_edges = Vec::new();
        let mut prepended_by_edges = Vec::new();
        let mut extended_by_edges = Vec::new();

        for edge in self.engine.all_graph_edges() {
            if &edge.target != fqn {
                continue;
            }
            match edge.kind {
                GraphEdgeKind::Superclass => subclass_edges.push(edge),
                GraphEdgeKind::Include
                    if edge.source.namespace_kind()
                        == Some(crate::core::NamespaceKind::Instance) =>
                {
                    included_by_edges.push(edge)
                }
                GraphEdgeKind::Prepend
                    if edge.source.namespace_kind()
                        == Some(crate::core::NamespaceKind::Instance) =>
                {
                    prepended_by_edges.push(edge)
                }
                GraphEdgeKind::Include | GraphEdgeKind::Prepend => {}
                GraphEdgeKind::Extend => extended_by_edges.push(edge),
            }
        }

        let mut subtypes = Vec::new();
        push_subtype_entries(
            self.engine,
            &mut subclass_edges,
            TypeHierarchyRelation::Subclass,
            &mut subtypes,
        );
        push_subtype_entries(
            self.engine,
            &mut included_by_edges,
            TypeHierarchyRelation::IncludedBy,
            &mut subtypes,
        );
        push_subtype_entries(
            self.engine,
            &mut prepended_by_edges,
            TypeHierarchyRelation::PrependedBy,
            &mut subtypes,
        );
        push_subtype_entries(
            self.engine,
            &mut extended_by_edges,
            TypeHierarchyRelation::ExtendedBy,
            &mut subtypes,
        );
        subtypes
    }

    pub fn implementor_namespaces(
        &self,
        origin_fqn: &FullyQualifiedName,
    ) -> Vec<FullyQualifiedName> {
        collect_all_implementors(self.engine, origin_fqn)
    }

    pub fn method_implementation_ranges(
        &self,
        owner_fqn: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Vec<TextRange> {
        let namespaces_to_check = collect_all_implementors(self.engine, owner_fqn);
        let mut ranges = Vec::new();

        for ns_fqn in &namespaces_to_check {
            let method_fqn = FullyQualifiedName::method(ns_fqn.namespace_parts(), *method);
            for fact in self.engine.method_facts_for(&method_fqn) {
                if fact.owner.namespace_parts() == ns_fqn.namespace_parts()
                    && fact.owner.namespace_kind() == ns_fqn.namespace_kind()
                {
                    ranges.push(fact.range);
                }
            }
        }

        ranges
    }

    pub fn namespace_implementation_ranges(&self, fqn: &FullyQualifiedName) -> Vec<TextRange> {
        collect_all_implementors(self.engine, fqn)
            .iter()
            .filter_map(|impl_fqn| self.engine.graph_nodes_for(impl_fqn).first())
            .map(|fact| fact.range)
            .collect()
    }
}

fn parse_method_fqn_string(fqn_str: &str) -> Option<FullyQualifiedName> {
    let (namespace_str, method_str) = fqn_str.rsplit_once('#')?;
    let method = RubyMethod::new(method_str).ok()?;
    let namespace = if namespace_str.is_empty() {
        Vec::new()
    } else {
        namespace_str
            .split("::")
            .map(RubyConstant::new)
            .collect::<Result<Vec<_>, _>>()
            .ok()?
    };
    Some(FullyQualifiedName::method(namespace, method))
}

fn parse_namespace_fqn_string(fqn: &str) -> Option<FullyQualifiedName> {
    let namespace = fqn
        .split("::")
        .map(RubyConstant::new)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    if namespace.is_empty() {
        return None;
    }
    Some(FullyQualifiedName::namespace(namespace))
}

fn push_grouped_text_range(
    grouped: &mut Vec<(FullyQualifiedName, Vec<TextRange>)>,
    fqn: FullyQualifiedName,
    range: TextRange,
) {
    if let Some((_, ranges)) = grouped.iter_mut().find(|(existing, _)| *existing == fqn) {
        ranges.push(range);
        return;
    }
    grouped.push((fqn, vec![range]));
}

fn push_subtype_entries(
    engine: &crate::AnalysisEngine,
    edges: &mut [GraphEdgeFact],
    relation: TypeHierarchyRelation,
    entries: &mut Vec<TypeHierarchyEntry>,
) {
    edges.sort_by(|left, right| left.source.to_string().cmp(&right.source.to_string()));
    for edge in edges {
        if let Some(entry) = hierarchy_entry_for_node(engine, &edge.source, relation, None, false) {
            entries.push(entry);
        }
    }
}

fn push_supertype_entries(
    engine: &crate::AnalysisEngine,
    edges: &[GraphEdgeFact],
    kind: GraphEdgeKind,
    relation: TypeHierarchyRelation,
    entries: &mut Vec<TypeHierarchyEntry>,
) {
    edges
        .iter()
        .filter(|edge| edge.kind == kind)
        .rev()
        .filter_map(|edge| {
            hierarchy_entry_for_node(
                engine,
                &edge.target,
                relation,
                Some(edge.range.file_id),
                false,
            )
        })
        .for_each(|entry| entries.push(entry));
}

fn push_unresolved_supertype_entries(
    engine: &crate::AnalysisEngine,
    fqn: &FullyQualifiedName,
    entries: &mut Vec<TypeHierarchyEntry>,
) {
    for edge in engine.unresolved_graph_edges() {
        if edge.source != *fqn {
            continue;
        }
        let relation = match edge.kind {
            GraphEdgeKind::Superclass => TypeHierarchyRelation::Superclass,
            GraphEdgeKind::Include => TypeHierarchyRelation::Include,
            GraphEdgeKind::Prepend => TypeHierarchyRelation::Prepend,
            GraphEdgeKind::Extend => TypeHierarchyRelation::Extend,
        };
        entries.push(TypeHierarchyEntry {
            fqn: FullyQualifiedName::constant(edge.target_parts.clone()),
            node_kind: None,
            relation,
            range: edge.range,
            edge_file_id: Some(edge.range.file_id),
            unresolved: true,
        });
    }
}

fn hierarchy_entry_for_node(
    engine: &crate::AnalysisEngine,
    fqn: &FullyQualifiedName,
    relation: TypeHierarchyRelation,
    edge_file_id: Option<SourceFileId>,
    unresolved: bool,
) -> Option<TypeHierarchyEntry> {
    let node = engine.graph_nodes_for(fqn).first()?;
    Some(TypeHierarchyEntry {
        fqn: fqn.clone(),
        node_kind: Some(node.kind),
        relation,
        range: node.range,
        edge_file_id,
        unresolved,
    })
}

fn collect_all_implementors(
    engine: &crate::AnalysisEngine,
    origin_fqn: &FullyQualifiedName,
) -> Vec<FullyQualifiedName> {
    let mut result = Vec::new();
    let mut visited = HashSet::new();
    let mut queue = vec![origin_fqn.clone()];

    visited.insert(origin_fqn.clone());

    while let Some(current) = queue.pop() {
        for descendant in descendants(engine, &current) {
            if visited.insert(descendant.clone()) {
                result.push(descendant);
            }
        }

        for mixer in mixers(engine, &current) {
            if visited.insert(mixer.clone()) {
                result.push(mixer.clone());
                queue.push(mixer);
            }
        }
    }

    result.sort_by_key(|fqn| fqn.to_string());
    result
}

fn mixers(
    engine: &crate::AnalysisEngine,
    origin_fqn: &FullyQualifiedName,
) -> Vec<FullyQualifiedName> {
    let mut mixers = engine
        .all_graph_edges()
        .into_iter()
        .filter(|edge| {
            edge.target == *origin_fqn
                && matches!(
                    edge.kind,
                    GraphEdgeKind::Include | GraphEdgeKind::Prepend | GraphEdgeKind::Extend
                )
                && (matches!(edge.kind, GraphEdgeKind::Extend)
                    || edge.source.namespace_kind() == Some(crate::core::NamespaceKind::Instance))
        })
        .map(|edge| edge.source)
        .collect::<Vec<_>>();
    mixers.sort_by_key(|fqn| fqn.to_string());
    mixers.dedup();
    mixers
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

#[cfg(test)]
mod tests {
    use crate::core::{
        FullyQualifiedName, RubyConstant, RubyMethod, SourceFileId, SymbolFact, SymbolKind,
        TextRange,
    };
    use crate::engine::AnalysisQuery;
    use crate::AnalysisEngine;

    fn query_with_symbols() -> (AnalysisEngine, SourceFileId) {
        let source = "class User\n  def name\n  end\nend";
        let mut engine = AnalysisEngine::new();
        let file_id = engine.open_or_update_file("/tmp/user.rb", source);
        let user = RubyConstant::new("User").expect("test constant must be valid");
        engine.add_symbol_fact(SymbolFact::new(
            FullyQualifiedName::namespace(vec![user.clone()]),
            SymbolKind::Class,
            TextRange::new(file_id, 6, 10),
        ));
        engine.add_symbol_fact(SymbolFact::new(
            FullyQualifiedName::method(
                vec![user],
                RubyMethod::new("name").expect("test method must be valid"),
            ),
            SymbolKind::Method,
            TextRange::new(file_id, 17, 21),
        ));
        (engine, file_id)
    }

    #[test]
    fn parse_method_fqn_strings() {
        let (engine, _) = query_with_symbols();
        let query = AnalysisQuery::new(&engine);

        assert_eq!(
            query.parse_method_fqn("Foo#bar").unwrap().to_string(),
            "Foo#bar"
        );
        assert_eq!(
            query.parse_method_fqn("Foo::Bar#baz").unwrap().to_string(),
            "Foo::Bar#baz"
        );
        assert_eq!(query.parse_method_fqn("#foo").unwrap().to_string(), "#foo");
        assert!(query.parse_method_fqn("Foo::bar").is_none());
    }

    #[test]
    fn parse_namespace_fqn_strings() {
        let (engine, _) = query_with_symbols();
        let query = AnalysisQuery::new(&engine);

        assert_eq!(
            query
                .parse_namespace_fqn("Foo::Bar::Baz")
                .unwrap()
                .to_string(),
            "Foo::Bar::Baz"
        );
        assert_eq!(
            query.parse_namespace_fqn("User").unwrap().to_string(),
            "User"
        );
        assert!(query.parse_namespace_fqn("").is_none());
    }
}
