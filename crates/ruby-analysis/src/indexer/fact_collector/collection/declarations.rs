use crate::core::storage::method_store::{MethodVisibility, MethodVisibilityOverrideFact};
use crate::core::{
    FullyQualifiedName, GraphEdgeFact, GraphEdgeKind, GraphEdgeProvenance, GraphNodeFact,
    GraphNodeKind, MethodAvailability, MethodFact, MethodParamFact, NamespaceKind, RubyConstant,
    RubyMethod, RubyType, SymbolFact, SymbolKind, TextRange, TypeFact, TypeProvenance, TypeSubject,
    UnresolvedGraphEdgeFact,
};
use crate::indexer::fact_collector::context::source::u32_offset;
use crate::indexer::fact_collector::FactCollector;
use std::collections::HashSet;
use std::sync::Arc;

impl FactCollector {
    pub fn direct_range(&self, location: &ruby_prism::Location<'_>) -> TextRange {
        TextRange::new(
            self.document.analysis_file_id(),
            u32_offset(location.start_offset()),
            u32_offset(location.end_offset()),
        )
    }

    pub fn direct_terminal_name_range(
        &self,
        path: &ruby_prism::Location<'_>,
        name: &[u8],
    ) -> TextRange {
        let end = path.end_offset();
        let start = end.checked_sub(name.len()).expect(
            "INVARIANT VIOLATED: constant name is longer than its Prism path location. \
             This is a bug because the terminal name must be contained in the constant path. \
             Fix: inspect Prism constant path locations before deriving declaration ranges.",
        );
        TextRange::new(
            self.document.analysis_file_id(),
            u32_offset(start),
            u32_offset(end),
        )
    }

    pub fn direct_push_namespace_facts(
        &mut self,
        fqn: FullyQualifiedName,
        kind: GraphNodeKind,
        range: TextRange,
        name_range: TextRange,
    ) {
        self.semantics.known_namespaces.insert(fqn.clone());
        self.facts.direct.symbols.push(
            SymbolFact::new(
                fqn.clone(),
                match kind {
                    GraphNodeKind::Class => SymbolKind::Class,
                    GraphNodeKind::Module => SymbolKind::Module,
                },
                range,
            )
            .with_name_range(name_range),
        );
        self.facts
            .direct
            .graph_nodes
            .push(GraphNodeFact::new(fqn.clone(), kind, range));
        let constant_fqn = FullyQualifiedName::constant(fqn.namespace_parts());
        self.facts.direct.types.push(TypeFact::new(
            TypeSubject::Constant(constant_fqn.clone()),
            match kind {
                GraphNodeKind::Class => RubyType::ClassReference(constant_fqn.clone()),
                GraphNodeKind::Module => RubyType::ModuleReference(constant_fqn),
            },
            range,
            TypeProvenance::Inferred,
        ));

        let singleton_fqn = fqn.to_singleton_namespace().expect(
            "INVARIANT VIOLATED: namespace fact could not convert to singleton namespace. \
             This is a bug because class/module graph nodes must be namespace FQNs. \
             Fix: only call direct_push_namespace_facts with Namespace facts.",
        );
        self.semantics
            .known_namespaces
            .insert(singleton_fqn.clone());
        self.facts
            .direct
            .graph_nodes
            .push(GraphNodeFact::new(singleton_fqn, kind, range));
    }

    pub fn direct_resolve_namespace(
        &self,
        parts: &[RubyConstant],
        absolute: bool,
    ) -> Option<FullyQualifiedName> {
        self.direct_resolve_namespace_from(parts, absolute, &self.scope_tracker.get_ns_stack())
    }

    pub fn direct_resolve_namespace_from(
        &self,
        parts: &[RubyConstant],
        absolute: bool,
        lexical_context: &[RubyConstant],
    ) -> Option<FullyQualifiedName> {
        let mut search = if absolute {
            Vec::new()
        } else {
            lexical_context.to_vec()
        };

        loop {
            let mut probe = search.clone();
            probe.extend(parts.iter().cloned());
            let fqn = FullyQualifiedName::namespace(probe);
            if self.direct_namespace_is_known(&fqn) {
                return Some(fqn);
            }
            if absolute || search.is_empty() {
                break;
            }
            search.pop();
        }

        let fqn = FullyQualifiedName::namespace(parts.to_vec());
        self.direct_namespace_is_known(&fqn).then_some(fqn)
    }

    pub fn namespace_is_known(&self, fqn: &FullyQualifiedName) -> bool {
        if self.direct_namespace_is_known(fqn) {
            return true;
        }
        let engine = self.semantics.engine.read();
        crate::engine::AnalysisQuery::new(&engine).has_graph_node(fqn)
    }

    pub fn resolve_constant_value_type_from(
        &self,
        parts: &[RubyConstant],
        absolute: bool,
        lexical_context: &[RubyConstant],
    ) -> Option<(FullyQualifiedName, RubyType)> {
        let mut search = if absolute {
            Vec::new()
        } else {
            lexical_context.to_vec()
        };
        let engine = self.semantics.engine.read();
        let query = crate::engine::AnalysisQuery::new(&engine);

        loop {
            let mut probe = search.clone();
            probe.extend(parts.iter().cloned());
            let constant = FullyQualifiedName::constant(probe);
            if let Some(ruby_type) = self
                .direct_constant_value_type(&constant)
                .or_else(|| query.constant_value_type(&constant))
            {
                return Some((constant, ruby_type));
            }
            if absolute || search.is_empty() {
                break;
            }
            search.pop();
        }

        None
    }

    pub fn resolve_declaration_constant_value_type_from(
        &self,
        parts: &[RubyConstant],
        absolute: bool,
        lexical_context: &[RubyConstant],
    ) -> Option<(FullyQualifiedName, RubyType)> {
        let mut candidates = Vec::new();
        let mut exact = if absolute {
            Vec::new()
        } else {
            lexical_context.to_vec()
        };
        exact.extend(parts.iter().cloned());
        candidates.push(exact);
        if !absolute && parts.len() > 1 && !lexical_context.is_empty() {
            candidates.push(parts.to_vec());
        }

        let engine = self.semantics.engine.read();
        let query = crate::engine::AnalysisQuery::new(&engine);
        candidates.into_iter().find_map(|candidate| {
            let constant = FullyQualifiedName::constant(candidate);
            self.direct_constant_value_type(&constant)
                .or_else(|| query.constant_value_type(&constant))
                .map(|ruby_type| (constant, ruby_type))
        })
    }

    pub fn direct_push_edge(
        &mut self,
        source: FullyQualifiedName,
        parts: &[RubyConstant],
        absolute: bool,
        kind: GraphEdgeKind,
        range: TextRange,
    ) {
        self.direct_push_edge_with_provenance(
            source,
            parts,
            absolute,
            kind,
            GraphEdgeProvenance::Explicit,
            range,
        );
    }

    pub fn direct_push_edge_with_provenance(
        &mut self,
        source: FullyQualifiedName,
        parts: &[RubyConstant],
        absolute: bool,
        kind: GraphEdgeKind,
        provenance: GraphEdgeProvenance,
        range: TextRange,
    ) {
        let Some(target) = self.direct_resolve_namespace(parts, absolute) else {
            self.facts.direct.unresolved_graph_edges.push(
                UnresolvedGraphEdgeFact::new(
                    source,
                    parts.to_vec(),
                    absolute,
                    FullyQualifiedName::namespace(self.scope_tracker.get_ns_stack()),
                    kind,
                    range,
                )
                .with_provenance(provenance),
            );
            return;
        };
        self.direct_push_resolved_edge_with_provenance(source, target, kind, provenance, range);
    }

    pub fn direct_push_resolved_edge(
        &mut self,
        source: FullyQualifiedName,
        target: FullyQualifiedName,
        kind: GraphEdgeKind,
        range: TextRange,
    ) -> bool {
        self.direct_push_resolved_edge_with_provenance(
            source,
            target,
            kind,
            GraphEdgeProvenance::Explicit,
            range,
        )
    }

    pub(in crate::indexer::fact_collector) fn direct_push_resolved_edge_with_provenance(
        &mut self,
        source: FullyQualifiedName,
        target: FullyQualifiedName,
        kind: GraphEdgeKind,
        provenance: GraphEdgeProvenance,
        range: TextRange,
    ) -> bool {
        if self
            .facts
            .direct
            .graph_edges
            .iter()
            .any(|edge| edge.source == source && edge.target == target && edge.kind == kind)
        {
            return true;
        }

        if kind == GraphEdgeKind::Superclass {
            if let Some(existing) = self
                .facts
                .direct
                .graph_edges
                .iter()
                .find(|edge| edge.source == source && edge.kind == GraphEdgeKind::Superclass)
            {
                self.push_error_diagnostic(
                    range,
                    "conflicting-superclass",
                    format!(
                        "Class `{source}` already inherits `{}` and cannot also inherit `{target}`",
                        existing.target
                    ),
                );
                return false;
            }
        }

        if ancestry_edge_kind(kind)
            && (source == target || self.direct_ancestry_path_exists(&target, &source))
        {
            self.push_error_diagnostic(
                range,
                "cyclic-inheritance",
                format!("Inheritance edge `{source}` -> `{target}` creates a cycle"),
            );
            return false;
        }

        self.facts
            .direct
            .graph_edges
            .push(GraphEdgeFact::new(source, target, kind, range).with_provenance(provenance));
        true
    }

    pub(in crate::indexer::fact_collector) fn direct_ancestry_path_exists(
        &self,
        start: &FullyQualifiedName,
        destination: &FullyQualifiedName,
    ) -> bool {
        let mut pending = vec![start.clone()];
        let mut visited = HashSet::new();
        while let Some(current) = pending.pop() {
            if &current == destination {
                return true;
            }
            if !visited.insert(current.clone()) {
                continue;
            }
            pending.extend(
                self.facts
                    .direct
                    .graph_edges
                    .iter()
                    .filter(|edge| edge.source == current && ancestry_edge_kind(edge.kind))
                    .map(|edge| edge.target.clone()),
            );
        }
        false
    }

    pub fn direct_push_method_fact(
        &mut self,
        namespace: Vec<RubyConstant>,
        owner_kind: NamespaceKind,
        method: RubyMethod,
        range: TextRange,
        params: Vec<MethodParamFact>,
    ) {
        self.direct_push_method_fact_with_signature(
            namespace, owner_kind, method, range, params, None, None,
        );
    }

    pub fn direct_push_method_fact_with_signature(
        &mut self,
        namespace: Vec<RubyConstant>,
        owner_kind: NamespaceKind,
        method: RubyMethod,
        range: TextRange,
        params: Vec<MethodParamFact>,
        documentation: Option<String>,
        return_type_label: Option<String>,
    ) {
        self.direct_push_method_fact_with_signature_and_name_range(
            namespace,
            owner_kind,
            method,
            range,
            range,
            params,
            documentation,
            return_type_label,
        );
    }

    pub fn direct_push_method_fact_with_signature_and_name_range(
        &mut self,
        namespace: Vec<RubyConstant>,
        owner_kind: NamespaceKind,
        method: RubyMethod,
        range: TextRange,
        name_range: TextRange,
        params: Vec<MethodParamFact>,
        documentation: Option<String>,
        return_type_label: Option<String>,
    ) {
        self.direct_push_method_fact_with_signature_name_range_and_availability(
            namespace,
            owner_kind,
            method,
            range,
            name_range,
            params,
            documentation,
            return_type_label,
            crate::core::MethodAvailability::Available,
        );
    }

    pub fn direct_push_method_fact_with_signature_name_range_and_availability(
        &mut self,
        namespace: Vec<RubyConstant>,
        owner_kind: NamespaceKind,
        method: RubyMethod,
        range: TextRange,
        name_range: TextRange,
        params: Vec<MethodParamFact>,
        documentation: Option<String>,
        return_type_label: Option<String>,
        availability: crate::core::MethodAvailability,
    ) {
        let fqn = FullyQualifiedName::method(namespace.clone(), method);
        let owner = FullyQualifiedName::namespace_with_kind(namespace, owner_kind);
        self.facts.direct.symbols.push(
            SymbolFact::new(fqn.clone(), SymbolKind::Method, range).with_name_range(name_range),
        );
        self.push_direct_method_fact(
            MethodFact::with_param_facts(fqn, owner, range, params)
                .with_name_range(name_range)
                .with_signature_metadata(documentation, return_type_label)
                .with_availability(availability)
                .with_visibility(self.scope_tracker.current_visibility()),
        );
    }

    pub fn direct_push_method_fact_with_visibility(
        &mut self,
        namespace: Vec<RubyConstant>,
        owner_kind: NamespaceKind,
        method: RubyMethod,
        range: TextRange,
        visibility: MethodVisibility,
    ) {
        let fqn = FullyQualifiedName::method(namespace.clone(), method);
        let owner = FullyQualifiedName::namespace_with_kind(namespace, owner_kind);
        self.facts
            .direct
            .symbols
            .push(SymbolFact::new(fqn.clone(), SymbolKind::Method, range));
        self.push_direct_method_fact(
            MethodFact::new(fqn, owner, range).with_visibility(visibility),
        );
    }

    pub fn direct_set_visibility(&mut self, visibility: MethodVisibility) {
        self.scope_tracker.set_current_visibility(visibility);
    }

    pub fn direct_set_method_visibility(
        &mut self,
        method: RubyMethod,
        visibility: MethodVisibility,
        range: TextRange,
    ) {
        let owner = FullyQualifiedName::namespace_with_kind(
            self.scope_tracker.get_ns_stack(),
            self.scope_tracker.current_macro_definition_context(),
        );
        self.facts
            .direct
            .method_visibility_overrides
            .push(MethodVisibilityOverrideFact::new(
                owner.clone(),
                method,
                visibility,
                range,
            ));
        let mut changed_direct_fact = false;
        for fact in &mut self.facts.direct.methods {
            let FullyQualifiedName::Method(_, fact_method) = &fact.fqn else {
                continue;
            };
            if *fact_method == method && fact.owner == owner {
                fact.visibility = visibility;
                changed_direct_fact = true;
            }
        }
        if changed_direct_fact {
            let fqn = FullyQualifiedName::method(owner.namespace_parts(), method);
            self.refresh_local_public_method_candidate(&fqn);
        }
    }

    pub(in crate::indexer::fact_collector) fn push_direct_method_fact(&mut self, fact: MethodFact) {
        let fqn = fact.fqn.clone();
        self.facts.direct.methods.push(fact);
        self.refresh_local_public_method_candidate(&fqn);
    }

    pub(in crate::indexer::fact_collector) fn refresh_local_public_method_candidate(
        &mut self,
        fqn: &FullyQualifiedName,
    ) {
        let mut matching = self
            .facts
            .direct
            .methods
            .iter()
            .filter(|fact| &fact.fqn == fqn)
            .peekable();
        assert!(
            matching.peek().is_some(),
            "INVARIANT VIOLATED: public-method candidate refresh has no matching direct method fact. This is a bug because refresh must run only after insertion or visibility mutation. Fix: pass the exact inserted method FQN to refresh_local_public_method_candidate."
        );
        let proven_public = matching.all(|fact| {
            fact.visibility == MethodVisibility::Public
                && matches!(&fact.availability, MethodAvailability::Available)
        });
        let candidates = Arc::make_mut(&mut self.semantics.public_method_candidates);
        if proven_public {
            candidates.insert(fqn.clone());
        } else {
            candidates.remove(fqn);
        }
    }

    pub fn direct_push_variable_symbol(
        &mut self,
        fqn: FullyQualifiedName,
        kind: SymbolKind,
        location: &ruby_prism::Location<'_>,
    ) {
        self.facts
            .direct
            .symbols
            .push(SymbolFact::new(fqn, kind, self.direct_range(location)));
    }

    pub fn direct_push_assignment_type(
        &mut self,
        subject: TypeSubject,
        ruby_type: RubyType,
        location: &ruby_prism::Location<'_>,
    ) {
        self.direct_push_type(subject, ruby_type, location, TypeProvenance::Assignment);
    }

    pub fn direct_push_type(
        &mut self,
        subject: TypeSubject,
        ruby_type: RubyType,
        location: &ruby_prism::Location<'_>,
        provenance: TypeProvenance,
    ) {
        // Deferred constant equations must retain their exact target even
        // while its value is unresolved, including cycles and late imports.
        if ruby_type == RubyType::Unknown && !matches!(subject, TypeSubject::Constant(_)) {
            return;
        }
        self.facts.direct.types.push(TypeFact::new(
            subject,
            ruby_type,
            self.direct_range(location),
            provenance,
        ));
    }

    pub(in crate::indexer::fact_collector) fn push_direct_expression_fact(
        &mut self,
        fact: TypeFact,
    ) {
        let TypeSubject::Expression(subject_range) = &fact.subject else {
            panic!(
                "INVARIANT VIOLATED: the direct expression index received a named type subject. This is a bug because the range index may only point to TypeSubject::Expression facts. Fix: route named facts through direct_push_type and expression facts through push_direct_expression_fact."
            );
        };
        assert_eq!(
            *subject_range,
            fact.range,
            "INVARIANT VIOLATED: a direct expression subject differs from its fact range. This is a bug because the compact range index uses that identity for exact lookup. Fix: construct both ranges from the same Prism node location."
        );
        let range = *subject_range;
        let index = self.facts.direct.types.len();
        self.facts.direct.types.push(fact);
        self.facts
            .expression_indexes
            .entry(range)
            .or_default()
            .push(index);
    }

    pub(in crate::indexer::fact_collector) fn direct_expression_fact(
        &self,
        range: TextRange,
        provenance: Option<TypeProvenance>,
    ) -> Option<&TypeFact> {
        self.facts.expression_indexes
            .get(&range)?
            .iter()
            .rev()
            .find_map(|index| {
                let fact = self.facts.direct.types.get(*index).expect(
                    "INVARIANT VIOLATED: the direct expression index points outside the append-only fact vector. This is a bug because direct facts are never removed during collection. Fix: record each index only after appending its owning fact and never reorder direct_facts.types.",
                );
                assert!(
                    matches!(&fact.subject, TypeSubject::Expression(subject_range) if *subject_range == range)
                        && fact.range == range,
                    "INVARIANT VIOLATED: the direct expression range index points to a different semantic fact. This is a bug because an indexed lookup would return evidence for the wrong AST node. Fix: update the range index atomically with every expression-fact append."
                );
                provenance
                    .is_none_or(|expected| fact.provenance == expected)
                    .then_some(fact)
            })
    }
}

pub(in crate::indexer::fact_collector) fn ancestry_edge_kind(kind: GraphEdgeKind) -> bool {
    match kind {
        GraphEdgeKind::Superclass
        | GraphEdgeKind::Include
        | GraphEdgeKind::Prepend
        | GraphEdgeKind::Extend => true,
        GraphEdgeKind::ExecutionContextApplication => false,
    }
}
