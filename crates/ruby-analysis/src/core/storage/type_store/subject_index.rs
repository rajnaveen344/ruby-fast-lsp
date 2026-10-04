//! Type facts indexed by interned subject.
//!
//! Most subjects have exactly one fact, so each subject id owns a dense slot
//! holding that fact inline. Only subjects with several facts keep a bucket in
//! a map, ordered by file, range, and provenance like every subject bucket.

use std::collections::HashMap;
use std::slice;

use crate::invariant::ExpectInvariant;

use crate::core::storage::memory_estimate::{map_table_bytes, vec_payload_bytes};

use super::compact::{TypeFactId, TypeSubjectId};

#[derive(Debug, Clone, Default)]
pub(super) struct SubjectIndex {
    /// Per subject id: its only fact, `TypeFactId::MANY`, or
    /// `TypeFactId::NONE`.
    slots: Vec<TypeFactId>,
    /// Buckets of the subjects whose slot is `MANY`, each holding two or
    /// more facts.
    many: HashMap<TypeSubjectId, Vec<TypeFactId>>,
}

impl SubjectIndex {
    /// The facts indexed under `subject`, or None when it has none.
    pub(super) fn get(&self, subject: TypeSubjectId) -> Option<&[TypeFactId]> {
        let slot = self.slots.get(subject.index())?;
        match *slot {
            TypeFactId::NONE => None,
            TypeFactId::MANY => Some(self.bucket(subject)),
            _ => Some(slice::from_ref(slot)),
        }
    }

    pub(super) fn push(&mut self, subject: TypeSubjectId, fact: TypeFactId) {
        if self.slots.len() <= subject.index() {
            self.slots.resize(subject.index() + 1, TypeFactId::NONE);
        }
        let slot = &mut self.slots[subject.index()];
        match *slot {
            TypeFactId::NONE => *slot = fact,
            TypeFactId::MANY => self.bucket_mut(subject).push(fact),
            only => {
                *slot = TypeFactId::MANY;
                self.many.insert(subject, vec![only, fact]);
            }
        }
    }

    pub(super) fn remove(&mut self, subject: TypeSubjectId, fact: TypeFactId) {
        let Some(slot) = self.slots.get_mut(subject.index()) else {
            return;
        };
        match *slot {
            TypeFactId::NONE => {}
            TypeFactId::MANY => {
                let bucket = self.bucket_mut(subject);
                bucket.retain(|id| *id != fact);
                if let [only] = bucket[..] {
                    self.many.remove(&subject);
                    self.slots[subject.index()] = only;
                }
            }
            only if only == fact => *slot = TypeFactId::NONE,
            _ => {}
        }
    }

    /// The bucket of a subject with several facts. A subject with at most one
    /// fact has no order to restore and returns None.
    pub(super) fn many_mut(&mut self, subject: TypeSubjectId) -> Option<&mut Vec<TypeFactId>> {
        self.many.get_mut(&subject)
    }

    /// Every subject with at least one fact.
    pub(super) fn subjects(&self) -> impl Iterator<Item = TypeSubjectId> + '_ {
        self.slots
            .iter()
            .enumerate()
            .filter(|(_, slot)| **slot != TypeFactId::NONE)
            .map(|(index, _)| TypeSubjectId::from_index(index))
    }

    /// Renumber subjects after the interner dropped unused ones. Every
    /// indexed subject must survive.
    pub(super) fn renumber(&mut self, new_id: impl Fn(TypeSubjectId) -> TypeSubjectId) {
        let mut slots = Vec::with_capacity(self.slots.len());
        for (index, slot) in self.slots.iter().enumerate() {
            if *slot == TypeFactId::NONE {
                continue;
            }
            let subject = new_id(TypeSubjectId::from_index(index)).index();
            if slots.len() <= subject {
                slots.resize(subject + 1, TypeFactId::NONE);
            }
            slots[subject] = *slot;
        }
        self.slots = slots;
        self.many = self
            .many
            .drain()
            .map(|(subject, bucket)| (new_id(subject), bucket))
            .collect();
    }

    /// Indexed subjects.
    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.subjects().count()
    }

    pub(super) fn estimated_heap_bytes(&self) -> usize {
        vec_payload_bytes(&self.slots)
            + map_table_bytes(&self.many)
            + self.many.values().map(vec_payload_bytes).sum::<usize>()
    }

    pub(super) fn shrink_to_fit(&mut self) {
        self.slots.shrink_to_fit();
        self.many.shrink_to_fit();
        for bucket in self.many.values_mut() {
            bucket.shrink_to_fit();
        }
    }

    fn bucket(&self, subject: TypeSubjectId) -> &[TypeFactId] {
        self.many.get(&subject).expect_invariant(
            "a subject marked with several facts has no bucket",
            "push moves a subject to the map when it gains a second fact",
            "update the slot and the map together in SubjectIndex",
        )
    }

    fn bucket_mut(&mut self, subject: TypeSubjectId) -> &mut Vec<TypeFactId> {
        self.many.get_mut(&subject).expect_invariant(
            "a subject marked with several facts has no bucket",
            "push moves a subject to the map when it gains a second fact",
            "update the slot and the map together in SubjectIndex",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn subject(index: usize) -> TypeSubjectId {
        TypeSubjectId::from_index(index)
    }

    fn fact(index: usize) -> TypeFactId {
        TypeFactId::from_index(index)
    }

    #[test]
    fn subjects_move_between_inline_slots_and_buckets() {
        let mut index = SubjectIndex::default();
        index.push(subject(3), fact(7));
        assert_eq!(index.get(subject(3)), Some(&[fact(7)][..]));
        assert_eq!(index.get(subject(1)), None);
        assert_eq!(index.get(subject(9)), None);
        assert!(index.many_mut(subject(3)).is_none());

        index.push(subject(3), fact(8));
        index.push(subject(3), fact(9));
        assert_eq!(
            index.get(subject(3)),
            Some(&[fact(7), fact(8), fact(9)][..])
        );

        index.remove(subject(3), fact(7));
        index.remove(subject(3), fact(9));
        assert_eq!(index.get(subject(3)), Some(&[fact(8)][..]));
        assert!(index.many.is_empty());

        index.remove(subject(3), fact(5));
        assert_eq!(index.get(subject(3)), Some(&[fact(8)][..]));
        index.remove(subject(3), fact(8));
        assert_eq!(index.get(subject(3)), None);
        assert_eq!(index.len(), 0);
    }

    #[test]
    fn renumbering_moves_inline_slots_and_buckets() {
        let mut index = SubjectIndex::default();
        index.push(subject(2), fact(0));
        index.push(subject(5), fact(1));
        index.push(subject(5), fact(2));

        index.renumber(|id| subject(if id == subject(2) { 1 } else { 0 }));

        assert_eq!(index.get(subject(1)), Some(&[fact(0)][..]));
        assert_eq!(index.get(subject(0)), Some(&[fact(1), fact(2)][..]));
        assert_eq!(index.get(subject(2)), None);
        assert_eq!(index.get(subject(5)), None);
        assert_eq!(
            index.subjects().collect::<Vec<_>>(),
            vec![subject(0), subject(1)]
        );
    }
}
