use crate::core::storage::method_store::{MethodVisibility, MethodVisibilityOverrideFact};
use crate::core::{
    FullyQualifiedName, GraphEdgeFact, GraphEdgeKind, GraphEdgeProvenance, GraphNodeFact,
    GraphNodeKind, MethodAvailability, MethodFact, MethodParamFact, NamespaceKind, RubyConstant,
    RubyMethod, RubyType, SymbolFact, SymbolKind, TextRange, TypeFact, TypeProvenance, TypeSubject,
    UnresolvedGraphEdgeFact,
};
use crate::indexer::documents::scope_rules::{
    edge_admission, lexical_candidates, resolve_lexical_namespace, EdgeAdmission,
};
use crate::indexer::fact_collector::context::source::u32_offset;
use crate::indexer::fact_collector::FactCollector;
use crate::invariant::ExpectInvariant;
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
        let start = end.checked_sub(name.len()).expect_invariant(
            "constant name is longer than its Prism path location",
            "the terminal name must be contained in the constant path",
            "inspect Prism constant path locations before deriving declaration ranges",
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
        self.facts.analysis.symbols.push(
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
            .analysis
            .graph_nodes
            .push(GraphNodeFact::new(fqn.clone(), kind, range));
        let constant_fqn = FullyQualifiedName::constant(fqn.namespace_parts());
        self.facts.analysis.types.push(TypeFact::new(
            TypeSubject::Constant(constant_fqn.clone()),
            match kind {
                GraphNodeKind::Class => RubyType::ClassReference(constant_fqn.clone()),
                GraphNodeKind::Module => RubyType::ModuleReference(constant_fqn),
            },
            range,
            TypeProvenance::Inferred,
        ));

        let singleton_fqn = fqn.to_singleton_namespace().expect_invariant(
            "namespace fact could not convert to singleton namespace",
            "class/module graph nodes must be namespace FQNs",
            "only call direct_push_namespace_facts with Namespace facts",
        );
        self.semantics
            .known_namespaces
            .insert(singleton_fqn.clone());
        self.facts
            .analysis
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
        resolve_lexical_namespace(parts, absolute, lexical_context, |fqn| {
            self.direct_namespace_is_known(fqn)
        })
    }

    /// The namespace a declaration-time reference (superclass, mixin) binds
    /// when the class body runs: only namespaces declared so far.
    pub fn declared_resolve_namespace_from(
        &self,
        parts: &[RubyConstant],
        absolute: bool,
        lexical_context: &[RubyConstant],
    ) -> Option<FullyQualifiedName> {
        resolve_lexical_namespace(parts, absolute, lexical_context, |fqn| {
            self.direct_namespace_is_declared(fqn)
        })
    }

    pub fn namespace_is_known(&self, fqn: &FullyQualifiedName) -> bool {
        if self.direct_namespace_is_known(fqn) {
            return true;
        }
        self.semantics.project.has_graph_node(fqn)
    }

    pub fn resolve_constant_value_type_from(
        &self,
        parts: &[RubyConstant],
        absolute: bool,
        lexical_context: &[RubyConstant],
    ) -> Option<(FullyQualifiedName, RubyType)> {
        self.first_constant_value_type(lexical_candidates(parts, absolute, lexical_context))
    }

    pub fn first_constant_value_type(
        &self,
        candidates: impl IntoIterator<Item = Vec<RubyConstant>>,
    ) -> Option<(FullyQualifiedName, RubyType)> {
        let candidates = candidates
            .into_iter()
            .map(FullyQualifiedName::constant)
            .collect::<Vec<_>>();
        self.semantics
            .project
            .first_constant_value_type(&candidates, &|constant| {
                self.direct_constant_value_type(constant)
            })
    }

    /// The value a class or module declaration name may alias. A reference
    /// to a namespace no walk knows is a guess from the lexical scope, not
    /// evidence of an alias, so the declaration names its own class.
    pub(in crate::indexer::fact_collector) fn alias_value_type(
        &self,
        candidates: &[Vec<RubyConstant>],
    ) -> Option<RubyType> {
        let (_, ruby_type) = self.first_constant_value_type(candidates.iter().cloned())?;
        match &ruby_type {
            RubyType::ClassReference(target) | RubyType::ModuleReference(target) => {
                let known = target
                    .to_instance_namespace()
                    .is_some_and(|namespace| self.namespace_is_known(&namespace));
                known.then_some(ruby_type)
            }
            RubyType::Class(_)
            | RubyType::Module(_)
            | RubyType::Literal(_)
            | RubyType::Array(_)
            | RubyType::Hash(_, _)
            | RubyType::Shape(_)
            | RubyType::Union(_)
            | RubyType::Unknown => Some(ruby_type),
        }
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
        let lexical_context = self.scope_tracker.get_ns_stack();
        let Some(target) = self.declared_resolve_namespace_from(parts, absolute, &lexical_context)
        else {
            self.facts.analysis.unresolved_graph_edges.push(
                UnresolvedGraphEdgeFact::new(
                    source,
                    parts.to_vec(),
                    absolute,
                    FullyQualifiedName::namespace(lexical_context),
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
        match edge_admission(&self.facts.analysis.graph_edges, &source, &target, kind) {
            EdgeAdmission::Admit => {}
            EdgeAdmission::Duplicate => return true,
            EdgeAdmission::ConflictingSuperclass(existing) => {
                let message = format!(
                    "Class `{source}` already inherits `{existing}` and cannot also inherit `{target}`"
                );
                self.push_error_diagnostic(range, "conflicting-superclass", message);
                return false;
            }
            EdgeAdmission::Cycle => {
                self.push_error_diagnostic(
                    range,
                    "cyclic-inheritance",
                    format!("Inheritance edge `{source}` -> `{target}` creates a cycle"),
                );
                return false;
            }
        }

        self.facts
            .analysis
            .graph_edges
            .push(GraphEdgeFact::new(source, target, kind, range).with_provenance(provenance));
        true
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
            self.scope_tracker.current_visibility(),
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
        visibility: MethodVisibility,
    ) {
        let fqn = FullyQualifiedName::method(namespace.clone(), method);
        let owner = FullyQualifiedName::namespace_with_kind(namespace, owner_kind);
        self.push_direct_method_fact(
            MethodFact::with_param_facts(fqn, owner, range, params)
                .with_name_range(name_range)
                .with_signature_metadata(documentation, return_type_label)
                .with_availability(availability)
                .with_visibility(visibility),
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
            self.scope_tracker.method_definition_context().0,
            self.scope_tracker.current_macro_definition_context(),
        );
        self.facts
            .analysis
            .method_visibility_overrides
            .push(MethodVisibilityOverrideFact::new(
                owner.clone(),
                method,
                visibility,
                range,
            ));
        let mut changed_direct_fact = false;
        for fact in &mut self.facts.analysis.methods {
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
        self.facts.analysis.methods.push(fact);
        self.refresh_local_public_method_candidate(&fqn);
    }

    pub(in crate::indexer::fact_collector) fn refresh_local_public_method_candidate(
        &mut self,
        fqn: &FullyQualifiedName,
    ) {
        let mut matching = self
            .facts
            .analysis
            .methods
            .iter()
            .filter(|fact| &fact.fqn == fqn)
            .peekable();
        invariant!(
            matching.peek().is_some(),
            what = "public-method candidate refresh has no matching direct method fact",
            why = "refresh must run only after insertion or visibility mutation",
            fix = "pass the exact inserted method FQN to refresh_local_public_method_candidate",
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
            .analysis
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
        self.facts.analysis.types.push(TypeFact::new(
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
            unreachable_invariant!(
                what = "the direct expression index received a named type subject",
                why = "the range index points only at TypeSubject::Expression facts",
                fix = "route named facts via direct_push_type, expressions via push_direct_expression_fact",
            );
        };
        invariant_eq!(
            *subject_range,
            fact.range,
            what = "a direct expression subject differs from its fact range",
            why = "the compact range index uses that identity for exact lookup",
            fix = "construct both ranges from the same Prism node location",
        );
        let range = *subject_range;
        let index = self.facts.analysis.types.len();
        self.facts.analysis.types.push(fact);
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
                let fact = self.facts.analysis.types.get(*index).expect_invariant(
                    "the direct expression index points outside the fact vector",
                    "direct facts are never removed during collection",
                    "record each index after appending its fact; never reorder analysis.types",
                );
                invariant!(
                    matches!(&fact.subject, TypeSubject::Expression(subject_range) if *subject_range == range)
                        && fact.range == range,
                    what = "the direct expression range index points to a different semantic fact",
                    why = "an indexed lookup would return evidence for the wrong AST node",
                    fix = "update the range index atomically with every expression-fact append",
                );
                provenance
                    .is_none_or(|expected| fact.provenance == expected)
                    .then_some(fact)
            })
    }
}
