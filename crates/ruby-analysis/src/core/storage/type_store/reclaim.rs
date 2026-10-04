//! Reclaiming interned subjects and Ruby types that no fact references.
//!
//! Replacing or removing a file and overwriting a solved type retire the
//! interned values those facts used. The interners keep them until enough
//! references have retired to repay a full scan, so each retirement costs
//! amortized O(1) and dead values stay bounded by about half the live facts.

use crate::core::storage::interner::IdRemap;

use super::compact::{RubyTypeId, StoredTypeSubject, TypeSubjectId};
use super::TypeStore;

/// Retirements that always pay for a scan, however small the store.
const MIN_RETIRED_BEFORE_RECLAIM: usize = 4_096;

/// Old to new Ruby type ids after a reclamation dropped unused types.
#[derive(Debug)]
pub(crate) struct RubyTypeRemap(IdRemap);

impl RubyTypeRemap {
    pub(crate) fn apply(&self, id: RubyTypeId) -> RubyTypeId {
        RubyTypeId::from_index(self.0.id(id.index()))
    }
}

impl TypeStore {
    /// Count references to interned types that an owner outside the store
    /// dropped.
    pub(crate) fn retire(&mut self, references: usize) {
        self.retired = self.retired.saturating_add(references);
    }

    /// Whether any reference retired since the last reclamation.
    pub(crate) fn has_retired(&self) -> bool {
        self.retired > 0
    }

    /// Whether enough references retired to repay a reclamation scan.
    pub(crate) fn reclaim_due(&self) -> bool {
        let live_facts = self.facts.len() - self.free_facts.len();
        self.retired >= MIN_RETIRED_BEFORE_RECLAIM.max(live_facts / 2)
    }

    /// Drop every appended subject no fact uses and every appended Ruby type
    /// that neither a fact nor `external` references. Returns the type id map
    /// owners of `external` ids must apply, or None when no type moved.
    pub(crate) fn reclaim_interned(
        &mut self,
        external: impl IntoIterator<Item = RubyTypeId>,
    ) -> Option<RubyTypeRemap> {
        self.retired = 0;
        // Every fact with an interned subject is indexed under it, and empty
        // buckets are removed, so the keys are exactly the live subjects.
        let mut live_subjects = vec![false; self.subjects.len()];
        for subject in self.facts_by_subject.keys() {
            live_subjects[subject.index()] = true;
        }
        let mut live_types = vec![false; self.ruby_types.len()];
        for fact in self.facts.iter().flatten() {
            live_types[fact.ruby_type.index()] = true;
        }
        for ruby_type in external {
            live_types[ruby_type.index()] = true;
        }

        if let Some(remap) = self.subjects.retain_own(&live_subjects) {
            let new_id = |id: TypeSubjectId| TypeSubjectId::from_index(remap.id(id.index()));
            for fact in self.facts.iter_mut().flatten() {
                if let Some(id) = fact.subject.interned_id() {
                    fact.subject = StoredTypeSubject::interned(new_id(id));
                }
            }
            self.facts_by_subject = self
                .facts_by_subject
                .drain()
                .map(|(id, facts)| (new_id(id), facts))
                .collect();
        }
        let remap = RubyTypeRemap(self.ruby_types.retain_own(&live_types)?);
        for fact in self.facts.iter_mut().flatten() {
            fact.ruby_type = remap.apply(fact.ruby_type);
        }
        Some(remap)
    }
}
