use super::FactCollector;
use crate::core::{
    DiagnosticCandidate, DiagnosticFact, DiagnosticSeverity, ExecutionContextFact, GraphEdgeFact,
    GraphNodeFact, InferenceEvidence, ReferenceCandidate, RubyType, SymbolFact, TextRange,
    TypeFact, TypeStore, TypeSubject,
};
use crate::indexer::{AnalysisIndex, RubyDocument};
use ruby_fast_lsp_extension_api::IndexPatch;
use std::collections::HashMap;

#[derive(Default)]
pub(super) struct CollectedFacts {
    pub(super) diagnostics: Vec<DiagnosticFact>,
    pub(super) types: TypeStore,
    pub(super) references: Vec<ReferenceCandidate>,
    pub(super) diagnostic_candidates: Vec<DiagnosticCandidate>,
    pub(super) direct: AnalysisIndex,
    /// Append-only range index into `direct.types` for expression facts.
    ///
    /// Recursive receiver inference consults expressions frequently while a
    /// file is being traversed. Retaining compact vector indexes avoids an
    /// O(expressions × all prior type facts) scan without creating a second
    /// semantic store or duplicating RubyType payloads.
    pub(super) expression_indexes: HashMap<TextRange, smallvec::SmallVec<[usize; 1]>>,
    pub(super) extension_patches: Vec<IndexPatch>,
    pub(super) execution_contexts: Vec<ExecutionContextFact>,
}

/// Completed output of one file traversal, ready for source-specific composition.
///
/// This owns facts and the updated document, never mutable engine stores or
/// traversal stacks. The file processor applies source policy and merges its
/// declaration seed before replacing the file through the analysis engine.
pub struct CollectedFile {
    pub document: RubyDocument,
    pub direct_facts: AnalysisIndex,
    pub type_facts: Vec<TypeFact>,
    pub reference_candidates: Vec<ReferenceCandidate>,
    pub diagnostic_candidates: Vec<DiagnosticCandidate>,
    pub diagnostics: Vec<DiagnosticFact>,
    pub extension_patches: Vec<IndexPatch>,
    pub execution_contexts: Vec<ExecutionContextFact>,
    pub inference: InferenceEvidence,
    pub local_read_types: Box<[(TextRange, RubyType)]>,
}

impl FactCollector {
    /// Consume this pass after traversal. Collection and solving are performed
    /// by the ordinary visitor; finishing only packages their existing output.
    pub fn finish(self) -> CollectedFile {
        let local_read_types = self.local_read_type_evidence();
        let inference = self.inference_evidence();
        let type_facts = self.type_facts();
        CollectedFile {
            document: self.document,
            direct_facts: self.facts.direct,
            type_facts,
            reference_candidates: self.facts.references,
            diagnostic_candidates: self.facts.diagnostic_candidates,
            diagnostics: self.facts.diagnostics,
            extension_patches: self.facts.extension_patches,
            execution_contexts: self.facts.execution_contexts,
            inference,
            local_read_types,
        }
    }

    pub fn direct_facts(&self) -> &AnalysisIndex {
        &self.facts.direct
    }

    pub fn reference_candidates(&self) -> &[ReferenceCandidate] {
        &self.facts.references
    }

    pub fn diagnostics(&self) -> &[DiagnosticFact] {
        &self.facts.diagnostics
    }

    pub fn add_symbol_fact(&mut self, fact: SymbolFact) {
        self.facts.direct.symbols.push(fact);
    }

    pub fn add_graph_node_fact(&mut self, fact: GraphNodeFact) {
        self.facts.direct.graph_nodes.push(fact);
    }

    pub fn add_graph_edge_fact(&mut self, fact: GraphEdgeFact) {
        self.facts.direct.graph_edges.push(fact);
    }

    /// Retain a type fact for direct composition without changing its provenance.
    pub fn add_direct_type_fact(&mut self, fact: TypeFact) {
        self.facts.direct.types.push(fact);
    }

    pub fn add_reference_candidate(&mut self, candidate: ReferenceCandidate) {
        self.facts.references.push(candidate);
    }

    pub fn add_execution_context_fact(&mut self, fact: ExecutionContextFact) {
        self.facts.execution_contexts.push(fact);
    }

    pub fn record_extension_patch(&mut self, patch: IndexPatch) {
        self.facts.extension_patches.push(patch);
    }

    /// Record a type fact emitted by an extension or runtime during this file pass.
    /// The caller also retains any direct fact needed for final file composition.
    pub fn add_type_fact(&mut self, fact: TypeFact) {
        self.facts.types.add(fact);
    }

    /// Collected type facts in the same order used for file composition.
    pub fn type_facts(&self) -> Vec<TypeFact> {
        self.facts.types.all_facts()
    }

    /// Collected facts for one subject, preserving their source order.
    pub fn type_facts_for(&self, subject: &TypeSubject) -> Vec<TypeFact> {
        self.facts.types.facts_for(subject)
    }

    pub fn push_warning_diagnostic(
        &mut self,
        range: TextRange,
        code: &'static str,
        message: String,
    ) {
        if !self.options.diagnostics_enabled {
            return;
        }
        self.facts.diagnostics.push(DiagnosticFact::new(
            range,
            DiagnosticSeverity::Warning,
            code,
            message,
        ));
    }

    pub fn push_error_diagnostic(&mut self, range: TextRange, code: &'static str, message: String) {
        if !self.options.diagnostics_enabled {
            return;
        }
        self.facts.diagnostics.push(DiagnosticFact::new(
            range,
            DiagnosticSeverity::Error,
            code,
            message,
        ));
    }

    /// Return the exact file-owned proof results consumed by all adapters.
    pub fn inference_evidence(&self) -> InferenceEvidence {
        let mut deferred_call_ranges = self
            .facts
            .references
            .iter()
            .filter_map(|candidate| match &candidate.kind {
                crate::core::ReferenceCandidateKind::Method {
                    call_expression_range,
                    ..
                } => *call_expression_range,
                crate::core::ReferenceCandidateKind::Constant { .. }
                | crate::core::ReferenceCandidateKind::Resolved { .. } => None,
            })
            .collect::<Vec<_>>();
        deferred_call_ranges.sort_unstable();
        for adjacent in deferred_call_ranges.windows(2) {
            assert!(
                adjacent[0] != adjacent[1],
                "INVARIANT VIOLATED: one call expression produced multiple deferred method-return candidates. This is a bug because final resolution cannot choose one runtime dispatch from competing candidates. Fix: attach exactly one method candidate to each CallNode outcome."
            );
        }

        let mut call_expression_outcomes = self
            .expressions
            .call_outcomes
            .iter()
            .map(|(range, outcome)| (*range, outcome.clone()))
            .collect::<Vec<_>>();
        call_expression_outcomes.sort_unstable_by_key(|(range, _)| *range);
        for adjacent in call_expression_outcomes.windows(2) {
            assert!(
                adjacent[0].0 != adjacent[1].0,
                "INVARIANT VIOLATED: one call expression produced more than one immediate proof outcome. This is a bug because one AST call has exactly one result. Fix: classify an immediate call once and leave all other calls to deferred engine resolution."
            );
        }

        let mut expression_unknown_reasons = self
            .expressions
            .unknown_reasons
            .iter()
            .map(|(range, reason)| (*range, *reason))
            .collect::<Vec<_>>();
        expression_unknown_reasons.sort_unstable();
        for adjacent in expression_unknown_reasons.windows(2) {
            assert!(
                adjacent[0].0 != adjacent[1].0,
                "INVARIANT VIOLATED: one expression range produced more than one Unknown reason. This is a bug because one AST expression has exactly one proof result. Fix: record expression evidence once during its node-entry callback."
            );
        }
        let mut method_return_equations = self
            .method_returns
            .equations
            .values()
            .flatten()
            .cloned()
            .collect::<Vec<_>>();
        method_return_equations.sort_unstable();

        InferenceEvidence {
            method_return_outcomes: self.method_returns.outcomes.clone(),
            method_return_equations,
            constant_type_equations: self.constants.equations.clone(),
            constant_callable_bodies: self.constants.callable_bodies.clone(),
            call_expression_outcomes,
            expression_unknown_reasons,
            telemetry: self.inference_telemetry(),
        }
    }
}
