//! `UseIndex`: the engine's file-owned reference candidates and the resolved
//! references that resolve passes derive from them.

use crate::core::names::fqn_id::FqnId;
use crate::core::storage::reference_store::{
    ConstLookup, ReferenceCandidateStats, ReferenceCandidateStore, ReferenceFact, ReferenceStore,
    StoredReferenceCandidate,
};
use crate::core::{FullyQualifiedName, ReferenceCandidate, ReferenceCandidateKind, SourceFileId};

use super::names::Names;
use super::Project;

#[derive(Debug, Clone, Default)]
pub(in crate::engine) struct UseIndex {
    candidates: ReferenceCandidateStore,
    resolved: ReferenceStore,
}

impl UseIndex {
    /// Intern one file's reference candidates and replace the file's
    /// previous candidates.
    pub(in crate::engine) fn replace_candidates(
        &mut self,
        names: &mut Names,
        file_id: SourceFileId,
        candidates: Vec<ReferenceCandidate>,
    ) {
        let candidates = intern_reference_candidates(names, candidates);
        self.candidates.replace_file(file_id, candidates);
    }

    /// Drop one file's reference candidates and the resolved references
    /// located in it.
    pub(in crate::engine) fn remove_file(&mut self, file_id: SourceFileId) {
        self.candidates.remove_file(file_id);
        self.resolved.remove_file(file_id);
    }

    pub(in crate::engine) fn candidates(&self) -> &ReferenceCandidateStore {
        &self.candidates
    }

    pub(in crate::engine) fn resolved(&self) -> &ReferenceStore {
        &self.resolved
    }

    pub(in crate::engine) fn facts_for(
        &self,
        names: &Names,
        target: &FullyQualifiedName,
    ) -> &[ReferenceFact] {
        let Some(target_id) = names.fqn_id(target) else {
            return &[];
        };
        self.resolved.facts_for(target_id)
    }

    /// Move the candidates out so the workspace pass can read them while it
    /// writes resolved references; `restore_candidates` puts them back.
    pub(in crate::engine) fn take_candidates(&mut self) -> ReferenceCandidateStore {
        std::mem::take(&mut self.candidates)
    }

    pub(in crate::engine) fn restore_candidates(&mut self, candidates: ReferenceCandidateStore) {
        self.candidates = candidates;
    }

    pub(in crate::engine) fn clear_resolved(&mut self) {
        self.resolved.clear();
    }

    pub(in crate::engine) fn add_resolved(&mut self, target: FqnId, fact: ReferenceFact) {
        self.resolved.add(target, fact);
    }

    pub(in crate::engine) fn sort_resolved(&mut self) {
        self.resolved.sort_all();
    }

    pub(in crate::engine) fn replace_resolved_file(
        &mut self,
        file_id: SourceFileId,
        facts: Vec<(FqnId, ReferenceFact)>,
    ) {
        self.resolved.replace_file(file_id, facts);
    }

    pub(in crate::engine) fn candidate_file_ids(&self) -> Vec<SourceFileId> {
        self.candidates.file_ids()
    }

    pub(in crate::engine) fn candidate_count(&self) -> usize {
        self.candidates.candidate_count()
    }

    pub(in crate::engine) fn candidate_stats(&self) -> ReferenceCandidateStats {
        self.candidates.stats()
    }

    pub(in crate::engine) fn resolved_count(&self) -> usize {
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

impl Project {
    pub fn reference_facts_for(&self, target: &FullyQualifiedName) -> &[ReferenceFact] {
        self.uses.facts_for(&self.names, target)
    }
}

fn intern_reference_candidates(
    names: &mut Names,
    candidates: Vec<ReferenceCandidate>,
) -> Vec<StoredReferenceCandidate> {
    candidates
        .into_iter()
        .map(|candidate| match candidate.kind {
            ReferenceCandidateKind::Constant {
                parts,
                current_namespace,
            } => {
                let context = names.intern_fqn(FullyQualifiedName::namespace(current_namespace));
                let lookup = names.intern_const_lookup(ConstLookup::new(parts, false, context));
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
                let root = names.intern_fqn(FullyQualifiedName::namespace(Vec::new()));
                let owner = names.intern_const_lookup(ConstLookup::new(owner, true, root));
                let caller = caller.map(|caller| names.intern_fqn(caller));
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
                let target = names.intern_fqn(target);
                let caller = caller.map(|caller| names.intern_fqn(caller));
                StoredReferenceCandidate::resolved(candidate.range, target, caller)
            }
        })
        .collect()
}
