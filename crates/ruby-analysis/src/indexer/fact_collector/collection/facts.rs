use crate::core::storage::type_store::TypeStore;
use crate::core::{
    DiagnosticFact, DiagnosticSeverity, ExecutionContextFact, FileAnalysis, GraphEdgeFact,
    GraphNodeFact, InferenceEvidence, ReferenceCandidate, SymbolFact, TextRange, TypeFact,
    TypeSubject,
};
use crate::indexer::fact_collector::FactCollector;
use crate::indexer::RubyDocument;
use ruby_fast_lsp_extension_api::IndexPatch;
use std::collections::HashMap;

#[derive(Default)]
pub(in crate::indexer::fact_collector) struct CollectedFacts {
    /// Every file-owned fact this pass produces; `finish()` adds the inference
    /// evidence and local read types before handing it to the caller.
    pub(in crate::indexer::fact_collector) analysis: FileAnalysis,
    /// Flow and extension type facts. They stay separate from the declaration
    /// types in `analysis` until the file processor merges them by slot.
    pub(in crate::indexer::fact_collector) flow_types: TypeStore,
    /// Append-only range index into `analysis.types` for expression facts.
    ///
    /// Recursive receiver inference consults expressions frequently while a
    /// file is being traversed. Retaining compact vector indexes avoids an
    /// O(expressions × all prior type facts) scan without creating a second
    /// semantic store or duplicating RubyType payloads.
    pub(in crate::indexer::fact_collector) expression_indexes:
        HashMap<TextRange, smallvec::SmallVec<[usize; 1]>>,
    pub(in crate::indexer::fact_collector) extension_patches: Vec<IndexPatch>,
}

/// Completed output of one file traversal, ready for source-specific composition.
///
/// This owns facts and the updated document, never mutable engine stores or
/// traversal stacks. The file processor applies source policy, merges its
/// declaration seed, extension patches, and flow types into `analysis`, and
/// then replaces the file through the analysis engine.
pub struct FactCollectorOutput {
    pub analysis: FileAnalysis,
    pub flow_types: Vec<TypeFact>,
    pub extension_patches: Vec<IndexPatch>,
    pub document: RubyDocument,
}

impl FactCollector {
    /// Consume this pass after traversal. Collection and solving are performed
    /// by the ordinary visitor; finishing only packages their existing output.
    pub fn finish(self) -> FactCollectorOutput {
        let local_read_types = self.local_read_type_evidence();
        let inference = self.inference_evidence();
        let flow_types = self.type_facts();
        let mut analysis = self.facts.analysis;
        analysis.inference = inference;
        analysis.local_read_types = local_read_types;
        FactCollectorOutput {
            analysis,
            flow_types,
            extension_patches: self.facts.extension_patches,
            document: self.document,
        }
    }

    /// File-owned facts collected so far in this pass.
    pub fn analysis(&self) -> &FileAnalysis {
        &self.facts.analysis
    }

    pub fn reference_candidates(&self) -> &[ReferenceCandidate] {
        &self.facts.analysis.reference_candidates
    }

    pub fn diagnostics(&self) -> &[DiagnosticFact] {
        &self.facts.analysis.diagnostics
    }

    pub fn add_symbol_fact(&mut self, fact: SymbolFact) {
        self.facts.analysis.symbols.push(fact);
    }

    pub fn add_graph_node_fact(&mut self, fact: GraphNodeFact) {
        self.facts.analysis.graph_nodes.push(fact);
    }

    pub fn add_graph_edge_fact(&mut self, fact: GraphEdgeFact) {
        self.facts.analysis.graph_edges.push(fact);
    }

    /// Retain a type fact for direct composition without changing its provenance.
    pub fn add_direct_type_fact(&mut self, fact: TypeFact) {
        self.facts.analysis.types.push(fact);
    }

    pub fn add_reference_candidate(&mut self, candidate: ReferenceCandidate) {
        self.facts.analysis.reference_candidates.push(candidate);
    }

    pub fn add_execution_context_fact(&mut self, fact: ExecutionContextFact) {
        self.facts.analysis.execution_contexts.push(fact);
    }

    pub fn record_extension_patch(&mut self, patch: IndexPatch) {
        self.facts.extension_patches.push(patch);
    }

    /// Record a type fact emitted by an extension or runtime during this file pass.
    /// The caller also retains any direct fact needed for final file composition.
    pub fn add_type_fact(&mut self, fact: TypeFact) {
        self.facts.flow_types.add(fact);
    }

    /// Collected type facts in the same order used for file composition.
    pub fn type_facts(&self) -> Vec<TypeFact> {
        self.facts.flow_types.all_facts()
    }

    /// Collected facts for one subject, preserving their source order.
    pub fn type_facts_for(&self, subject: &TypeSubject) -> Vec<TypeFact> {
        self.facts.flow_types.facts_for(subject)
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
        self.facts.analysis.diagnostics.push(DiagnosticFact::new(
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
        self.facts.analysis.diagnostics.push(DiagnosticFact::new(
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
            .analysis
            .reference_candidates
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
            invariant!(
                adjacent[0] != adjacent[1],
                what = "one call expression produced multiple deferred method-return candidates",
                why =
                    "final resolution cannot choose one runtime dispatch from competing candidates",
                fix = "attach exactly one method candidate to each CallNode outcome",
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
            invariant!(
                adjacent[0].0 != adjacent[1].0,
                what = "one call expression produced more than one immediate proof outcome",
                why = "one AST call has exactly one result",
                fix = "classify an immediate call once; defer all others to the engine",
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
            invariant!(
                adjacent[0].0 != adjacent[1].0,
                what = "one expression range produced more than one Unknown reason",
                why = "one AST expression has exactly one proof result",
                fix = "record expression evidence once during its node-entry callback",
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
            method_return_outcomes: self
                .method_returns
                .outcomes
                .iter()
                .map(|(method, outcome)| (method.clone(), outcome.clone()))
                .collect(),
            method_return_equations,
            constant_type_equations: self.constants.equations.clone(),
            constant_callable_bodies: self.constants.callable_bodies.clone(),
            call_expression_outcomes,
            expression_unknown_reasons,
            telemetry: self.inference_telemetry().into(),
        }
    }
}
