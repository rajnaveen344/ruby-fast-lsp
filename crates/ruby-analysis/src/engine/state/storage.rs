//! The fact arena, conversion between domain facts and their interned stored
//! representations, and storage compaction.

use crate::invariant::ExpectInvariant;

use crate::core::storage::graph_store::StoredGraphEdgeFact;
use crate::core::storage::graph_store::StoredGraphNodeFact;
use crate::core::storage::graph_store::StoredUnresolvedGraphEdgeFact;
use crate::core::storage::reference_store::ConstLookup;
use crate::core::storage::type_store::TypeStore;
use crate::core::{
    ConstantPath, GraphEdgeFact, GraphNodeFact, SourceFileId, TypeFact, TypeResolution,
    TypeSubject, UnresolvedGraphEdgeFact,
};

use super::AnalysisEngine;

#[derive(Debug, Clone, Default)]
pub(in crate::engine) struct FactArena {
    pub(in crate::engine) types: TypeStore,
}

impl AnalysisEngine {
    pub fn type_at(
        &self,
        subject: &TypeSubject,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> TypeResolution {
        self.facts.types.type_at(subject, file_id, byte_offset)
    }

    pub fn type_facts_for(&self, subject: &TypeSubject) -> Vec<TypeFact> {
        self.facts.types.facts_for(subject)
    }

    pub(crate) fn type_store(&self) -> &TypeStore {
        &self.facts.types
    }
}

impl AnalysisEngine {
    pub fn shrink_to_fit(&mut self) {
        self.files.shrink_to_fit();
        self.names.shrink_to_fit();

        self.decls.shrink_to_fit();
        self.facts.types.shrink_to_fit();
        self.graph.shrink_to_fit();
        self.uses.shrink_to_fit();
        self.diagnostics.shrink_to_fit();
        self.inference_by_file.shrink_to_fit();
        self.call_expression_outcomes_by_file.shrink_to_fit();
        self.local_read_types_by_file.shrink_to_fit();
    }
}

impl AnalysisEngine {
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
