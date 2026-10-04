//! `Diagnostics`: the engine's file-owned diagnostic candidates and resolved
//! diagnostic facts. Resolve passes read candidates and rebuild the derived
//! diagnostics; indexer and require diagnostics survive the rebuild.

use super::policy::{is_resolve_derived, UNRESOLVED_REQUIRE_CODE};
use crate::core::storage::diagnostics::candidates::DiagnosticCandidateStore;
use crate::core::storage::diagnostics::resolved::DiagnosticStore;
use crate::core::{DiagnosticCandidate, DiagnosticFact, SourceFileId};
use crate::engine::View;

#[derive(Debug, Clone, Default)]
pub(in crate::engine) struct Diagnostics {
    candidates: DiagnosticCandidateStore,
    resolved: DiagnosticStore,
}

impl Diagnostics {
    /// Install one file's diagnostic candidates and indexer diagnostics,
    /// replacing whatever the file owned before.
    pub(in crate::engine) fn replace_file(
        &mut self,
        file_id: SourceFileId,
        candidates: Vec<DiagnosticCandidate>,
        diagnostics: Vec<DiagnosticFact>,
    ) {
        self.candidates.replace_file(file_id, candidates);
        self.resolved.replace_file(file_id, diagnostics);
    }

    /// Drop one file's diagnostic candidates and resolved diagnostics.
    pub(in crate::engine) fn remove_file(&mut self, file_id: SourceFileId) {
        self.candidates.remove_file(file_id);
        self.resolved.remove_file(file_id);
    }

    /// Swap one file's `unresolved-require` diagnostics, keeping all others.
    pub(in crate::engine) fn replace_unresolved_require(
        &mut self,
        file_id: SourceFileId,
        require_diagnostics: Vec<DiagnosticFact>,
    ) {
        let mut diagnostics = self
            .resolved
            .facts_in_file(file_id)
            .into_iter()
            .filter(|fact| fact.code != UNRESOLVED_REQUIRE_CODE)
            .collect::<Vec<_>>();
        diagnostics.extend(require_diagnostics);
        self.resolved.replace_file(file_id, diagnostics);
    }

    /// Replace one file's resolve-derived diagnostics with `derived`, keeping
    /// indexer and require diagnostics.
    pub(in crate::engine) fn rebuild_resolved(
        &mut self,
        file_id: SourceFileId,
        derived: Vec<DiagnosticFact>,
    ) {
        let mut diagnostics = self
            .resolved
            .facts_in_file(file_id)
            .into_iter()
            .filter(|fact| !is_resolve_derived(&fact.code))
            .collect::<Vec<_>>();
        diagnostics.extend(derived);
        self.resolved.replace_file(file_id, diagnostics);
    }

    pub(in crate::engine) fn candidate_file_ids(&self) -> Vec<SourceFileId> {
        self.candidates.file_ids()
    }

    pub(in crate::engine) fn candidates(&self) -> impl Iterator<Item = &DiagnosticCandidate> {
        self.candidates.iter_candidates()
    }

    pub(in crate::engine) fn candidates_in_file(
        &self,
        file_id: SourceFileId,
    ) -> Vec<DiagnosticCandidate> {
        self.candidates.candidates_in_file(file_id)
    }

    pub(in crate::engine) fn facts_in_file(&self, file_id: SourceFileId) -> Vec<DiagnosticFact> {
        self.resolved.facts_in_file(file_id)
    }

    pub(in crate::engine) fn all_facts(&self) -> Vec<DiagnosticFact> {
        self.resolved.all_facts()
    }

    pub(in crate::engine) fn candidate_count(&self) -> usize {
        self.candidates.candidate_count()
    }

    pub(in crate::engine) fn fact_count(&self) -> usize {
        self.resolved.fact_count()
    }

    pub(in crate::engine) fn candidates_heap_bytes(&self) -> usize {
        self.candidates.estimated_heap_bytes()
    }

    pub(in crate::engine) fn resolved_heap_bytes(&self) -> usize {
        self.resolved.estimated_heap_bytes()
    }

    pub(in crate::engine) fn shrink_to_fit(&mut self) {
        self.candidates.shrink_to_fit();
        self.resolved.shrink_to_fit();
    }
}

impl View<'_> {
    pub fn diagnostic_facts_in_file(&self, file_id: SourceFileId) -> Vec<DiagnosticFact> {
        self.engine.diagnostics.facts_in_file(file_id)
    }

    pub fn all_diagnostic_facts(&self) -> Vec<DiagnosticFact> {
        self.engine.diagnostics.all_facts()
    }
}
