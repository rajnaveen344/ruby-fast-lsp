//! The fact arena, conversion between domain facts and their interned stored
//! representations, and storage compaction.

use crate::invariant::ExpectInvariant;

use crate::core::storage::graph_store::StoredGraphEdgeFact;
use crate::core::storage::graph_store::StoredGraphNodeFact;
use crate::core::storage::graph_store::StoredUnresolvedGraphEdgeFact;
use crate::core::storage::method_store::MethodStore;
use crate::core::storage::method_store::StoredMethodFact;
use crate::core::storage::reference_store::ConstLookup;
use crate::core::storage::reference_store::ReferenceCandidateStore;
use crate::core::storage::reference_store::ReferenceStore;
use crate::core::storage::reference_store::StoredReferenceCandidate;
use crate::core::storage::symbol_store::StoredSymbolFact;
use crate::core::storage::symbol_store::SymbolStore;
use crate::core::storage::type_store::TypeStore;
use crate::core::{
    ConstantPath, FullyQualifiedName, GraphEdgeFact, GraphNodeFact, MethodFact, ReferenceCandidate,
    ReferenceCandidateKind, SymbolFact, UnresolvedGraphEdgeFact,
};

use super::AnalysisEngine;

#[derive(Debug, Clone, Default)]
pub(in crate::engine) struct FactArena {
    pub(in crate::engine) definitions: DefinitionFacts,
    pub(in crate::engine) references: ReferenceFacts,
    pub(in crate::engine) types: TypeStore,
}

#[derive(Debug, Clone, Default)]
pub(in crate::engine) struct DefinitionFacts {
    pub(in crate::engine) symbols: SymbolStore,
    pub(in crate::engine) methods: MethodStore,
}

#[derive(Debug, Clone, Default)]
pub(in crate::engine) struct ReferenceFacts {
    pub(in crate::engine) candidates: ReferenceCandidateStore,
    pub(in crate::engine) resolved: ReferenceStore,
}

impl AnalysisEngine {
    pub fn shrink_to_fit(&mut self) {
        self.files.shrink_to_fit();
        self.names.shrink_to_fit();

        self.facts.definitions.symbols.shrink_to_fit();
        self.facts.definitions.methods.shrink_to_fit();
        self.facts.types.shrink_to_fit();
        self.graph.shrink_to_fit();
        self.facts.references.candidates.shrink_to_fit();
        self.facts.references.resolved.shrink_to_fit();
        self.diagnostics.shrink_to_fit();
        self.inference_by_file.shrink_to_fit();
        self.call_expression_outcomes_by_file.shrink_to_fit();
        self.local_read_types_by_file.shrink_to_fit();
    }
}

impl AnalysisEngine {
    pub(super) fn intern_reference_candidates(
        &mut self,
        candidates: Vec<ReferenceCandidate>,
    ) -> Vec<StoredReferenceCandidate> {
        candidates
            .into_iter()
            .map(|candidate| match candidate.kind {
                ReferenceCandidateKind::Constant {
                    parts,
                    current_namespace,
                } => {
                    let context = self
                        .names
                        .intern_fqn(FullyQualifiedName::namespace(current_namespace));
                    let lookup = self
                        .names
                        .intern_const_lookup(ConstLookup::new(parts, false, context));
                    StoredReferenceCandidate::constant(candidate.range, lookup)
                }
                ReferenceCandidateKind::Method {
                    owner,
                    owner_kind,
                    method,
                    is_super,
                    access,
                    caller,
                    call_expression_range,
                    preferred_definition_range,
                    diagnostics,
                } => {
                    let root = self
                        .names
                        .intern_fqn(FullyQualifiedName::namespace(Vec::new()));
                    let owner = self
                        .names
                        .intern_const_lookup(ConstLookup::new(owner, true, root));
                    let caller = caller.map(|caller| self.names.intern_fqn(caller));
                    StoredReferenceCandidate::method(
                        candidate.range,
                        owner,
                        owner_kind,
                        method,
                        is_super,
                        access,
                        caller,
                        call_expression_range,
                        preferred_definition_range,
                        diagnostics,
                    )
                }
                ReferenceCandidateKind::Resolved { target, caller } => {
                    let target = self.names.intern_fqn(target);
                    let caller = caller.map(|caller| self.names.intern_fqn(caller));
                    StoredReferenceCandidate::resolved(candidate.range, target, caller)
                }
            })
            .collect()
    }

    pub(super) fn intern_symbol_facts(&mut self, facts: Vec<SymbolFact>) -> Vec<StoredSymbolFact> {
        facts
            .into_iter()
            .map(|fact| {
                let fqn = self.names.intern_fqn(fact.fqn);
                StoredSymbolFact::new(fqn, fact.kind, fact.range).with_name_range(fact.name_range)
            })
            .collect()
    }

    pub(super) fn intern_method_facts(&mut self, facts: Vec<MethodFact>) -> Vec<StoredMethodFact> {
        facts
            .into_iter()
            .map(|fact| {
                let method = match &fact.fqn {
                    FullyQualifiedName::Method(_, method) => Some(*method),
                    FullyQualifiedName::Namespace(_, _)
                    | FullyQualifiedName::Constant(_)
                    | FullyQualifiedName::LocalVariable(_)
                    | FullyQualifiedName::InstanceVariable(_)
                    | FullyQualifiedName::ClassVariable(_)
                    | FullyQualifiedName::GlobalVariable(_) => None,
                };
                let fqn = self.names.intern_fqn(fact.fqn);
                let owner = self.names.intern_fqn(fact.owner);
                StoredMethodFact {
                    fqn,
                    owner,
                    method,
                    range: fact.range,
                    name_range: fact.name_range,
                    params: fact.params,
                    param_facts: fact.param_facts,
                    parameter_shape_complete: fact.parameter_shape_complete,
                    delegate_receiver: fact.delegate_receiver,
                    visibility: fact.visibility,
                    availability: fact.availability,
                    documentation: fact.documentation,
                    return_type_label: fact.return_type_label,
                    higher_order: fact.higher_order,
                }
            })
            .collect()
    }

    pub(super) fn intern_graph_node_facts(
        &mut self,
        facts: Vec<GraphNodeFact>,
    ) -> Vec<StoredGraphNodeFact> {
        facts
            .into_iter()
            .map(|fact| {
                let fqn = self.names.intern_fqn(fact.fqn);
                StoredGraphNodeFact::new(fqn, fact.kind, fact.range)
            })
            .collect()
    }

    pub(super) fn intern_graph_edge_facts(
        &mut self,
        facts: Vec<GraphEdgeFact>,
    ) -> Vec<StoredGraphEdgeFact> {
        facts
            .into_iter()
            .map(|fact| {
                let source = self.names.intern_fqn(fact.source);
                let target = self.names.intern_fqn(fact.target);
                StoredGraphEdgeFact::new(source, target, fact.kind, fact.range)
                    .with_provenance(fact.provenance)
            })
            .collect()
    }

    pub(super) fn intern_unresolved_graph_edge_facts(
        &mut self,
        facts: Vec<UnresolvedGraphEdgeFact>,
    ) -> Vec<StoredUnresolvedGraphEdgeFact> {
        facts
            .into_iter()
            .map(|fact| {
                let source = self.names.intern_fqn(fact.source);
                let context = self.names.intern_fqn(fact.context);
                let target = self.names.intern_const_lookup(ConstLookup::new(
                    ConstantPath::from_vec(fact.target_parts),
                    fact.absolute,
                    context,
                ));
                StoredUnresolvedGraphEdgeFact::new(source, target, fact.kind, fact.range)
                    .with_provenance(fact.provenance)
            })
            .collect()
    }

    pub(super) fn expand_symbol_fact(&self, fact: StoredSymbolFact) -> SymbolFact {
        let fqn = self
            .names
            .fqn(fact.fqn)
            .expect_invariant(
                "symbol fact points to missing FQN id",
                "symbol facts must only store interned FQN ids",
                "intern symbol FQNs before inserting facts",
            )
            .clone();
        SymbolFact::new(fqn, fact.kind, fact.range).with_name_range(fact.name_range)
    }

    pub(super) fn expand_method_fact(&self, fact: StoredMethodFact) -> MethodFact {
        let fqn = self
            .names
            .fqn(fact.fqn)
            .expect_invariant(
                "method fact points to missing FQN id",
                "method facts must only store interned FQN ids",
                "intern method FQNs before inserting facts",
            )
            .clone();
        let owner = self
            .names
            .fqn(fact.owner)
            .expect_invariant(
                "method fact points to missing owner FQN id",
                "method facts must only store interned owner FQN ids",
                "intern method owners before inserting facts",
            )
            .clone();
        MethodFact {
            fqn,
            owner,
            range: fact.range,
            name_range: fact.name_range,
            params: fact.params,
            param_facts: fact.param_facts,
            parameter_shape_complete: fact.parameter_shape_complete,
            delegate_receiver: fact.delegate_receiver,
            visibility: fact.visibility,
            availability: fact.availability,
            documentation: fact.documentation,
            return_type_label: fact.return_type_label,
            higher_order: fact.higher_order,
        }
    }

    pub(super) fn expand_graph_node_fact(&self, fact: StoredGraphNodeFact) -> GraphNodeFact {
        let fqn = self
            .names
            .fqn(fact.fqn)
            .expect_invariant(
                "graph node points to missing FQN id",
                "graph nodes must only store interned FQN ids",
                "intern graph node FQNs before inserting facts",
            )
            .clone();
        GraphNodeFact::new(fqn, fact.kind, fact.range)
    }

    pub(super) fn expand_graph_edge_fact(&self, fact: StoredGraphEdgeFact) -> GraphEdgeFact {
        GraphEdgeFact::new(
            self.names.expand_interned_fqn(fact.source),
            self.names.expand_interned_fqn(fact.target),
            fact.kind,
            fact.range,
        )
        .with_provenance(fact.provenance)
    }

    pub(super) fn expand_unresolved_graph_edge_fact(
        &self,
        fact: StoredUnresolvedGraphEdgeFact,
    ) -> UnresolvedGraphEdgeFact {
        let source = self
            .names
            .fqn(fact.source)
            .expect_invariant(
                "unresolved graph edge points to missing source FQN id",
                "unresolved graph edges must only store interned source FQN ids",
                "intern unresolved graph edge sources before inserting facts",
            )
            .clone();
        let lookup = self.names.const_lookup(fact.target).expect_invariant(
            "unresolved graph edge points to missing constant lookup id",
            "unresolved graph edges must only store interned constant lookup ids",
            "intern unresolved graph edge targets before inserting facts",
        );
        let context = self
            .names
            .fqn(lookup.context)
            .expect_invariant(
                "unresolved graph edge lookup points to missing context FQN id",
                "constant lookups must only store interned context FQN ids",
                "intern constant lookup contexts before inserting facts",
            )
            .clone();
        UnresolvedGraphEdgeFact::new(
            source,
            lookup.path.to_vec(),
            lookup.absolute,
            context,
            fact.kind,
            fact.range,
        )
        .with_provenance(fact.provenance)
    }
}
