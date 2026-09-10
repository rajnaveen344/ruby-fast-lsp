use crate::core::{DiagnosticCandidate, DiagnosticCandidateKind};
use ruby_prism::CallNode;

use crate::indexer::fact_collector::FactCollector;

impl FactCollector {
    /// Retain syntax identity only; the engine owns the receiver's flow proof.
    pub(in crate::indexer::fact_collector) fn collect_nil_call_candidate(
        &mut self,
        node: &CallNode<'_>,
    ) {
        if !self.options.diagnostics_enabled {
            return;
        }
        let Some(receiver) = node.receiver() else {
            return;
        };
        let Some(local) = receiver.as_local_variable_read_node() else {
            return;
        };
        let Some(message) = node.message_loc() else {
            return;
        };
        self.facts
            .diagnostic_candidates
            .push(DiagnosticCandidate::new(
                self.document.prism_location_to_text_range(&message),
                DiagnosticCandidateKind::NilCall {
                    local_read: self
                        .document
                        .prism_location_to_text_range(&local.location()),
                    variable: crate::indexer::utf8_str(local.name().as_slice()).to_string(),
                    method: crate::indexer::utf8_str(node.name().as_slice()).to_string(),
                },
            ));
    }
}
