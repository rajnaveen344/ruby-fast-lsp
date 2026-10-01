//! File-owned fact removal and replacement with ordered index maintenance.

use crate::core::storage::file_owned_index::place_appended_file_facts;

use super::{provenance_rank, SourceFileId, StoredTypeFact, TypeFact, TypeFactId, TypeStore};

impl TypeStore {
    pub fn remove_file(&mut self, file_id: SourceFileId) {
        let Some(stale_ids) = self.facts_by_file.remove(&file_id) else {
            return;
        };
        for stale_id in stale_ids {
            let Some(stale) = self.take_fact(stale_id) else {
                continue;
            };
            self.free_facts.push(stale_id);
            if let Some(subject_id) = stale.subject.interned_id() {
                if let Some(ids) = self.facts_by_subject.get_mut(&subject_id) {
                    ids.retain(|id| *id != stale_id);
                    if ids.is_empty() {
                        self.facts_by_subject.remove(&subject_id);
                    }
                }
            }
        }
    }

    pub fn replace_file(
        &mut self,
        file_id: SourceFileId,
        facts: impl IntoIterator<Item = TypeFact>,
    ) {
        self.remove_file(file_id);
        let mut touched_subjects = Vec::new();
        for fact in facts {
            assert!(
                fact.range.file_id == file_id,
                "INVARIANT VIOLATED: replacement fact belongs to a different file id. \
                 This is a bug because TypeStore::replace_file must only receive facts for the target file. \
                 Fix: partition facts by SourceFileId before replacing."
            );
            let subject = self.store_subject(fact.subject, fact.range);
            if let Some(subject_id) = subject.interned_id() {
                if let Some((_, appended_count)) = touched_subjects
                    .iter_mut()
                    .find(|(touched, _)| *touched == subject_id)
                {
                    *appended_count += 1;
                } else {
                    touched_subjects.push((subject_id, 1));
                }
            }
            let ruby_type = self.intern_ruby_type(fact.ruby_type);
            let id = self.insert_fact(StoredTypeFact {
                subject,
                ruby_type,
                range: fact.range,
                provenance: fact.provenance,
            });
            if let Some(subject_id) = subject.interned_id() {
                self.facts_by_subject
                    .entry(subject_id)
                    .or_default()
                    .push(id);
            }
            self.facts_by_file.entry(file_id).or_default().push(id);
        }
        for (subject, appended_count) in touched_subjects {
            if let Some(ids) = self.facts_by_subject.get_mut(&subject) {
                if self.file_owned_indexes_ordered {
                    place_appended_file_facts(
                        ids,
                        appended_count,
                        file_id,
                        |id| {
                            self.facts[id.index()]
                                .as_ref()
                                .expect(
                                    "INVARIANT VIOLATED: type index points to missing fact. \
                                     This is a bug because indexes must be removed before arena facts. \
                                     Fix: remove stale ids from every TypeStore index.",
                                )
                                .range
                                .file_id
                        },
                        |appended| sort_type_ids(&self.facts, appended),
                    );
                } else {
                    sort_type_ids(&self.facts, ids);
                }
                ids.shrink_to_fit();
            }
        }
        if let Some(ids) = self.facts_by_file.get_mut(&file_id) {
            sort_type_ids_by_file(&self.facts, ids);
            ids.shrink_to_fit();
        }
    }
}

fn sort_type_ids(facts: &[Option<StoredTypeFact>], ids: &mut [TypeFactId]) {
    ids.sort_by_key(|id| {
        let fact = facts[id.index()].as_ref().expect(
            "INVARIANT VIOLATED: type index points to missing fact. \
             This is a bug because indexes must be removed before arena facts. \
             Fix: remove stale ids from every TypeStore index.",
        );
        (
            fact.range.file_id,
            fact.range.start_byte,
            fact.range.end_byte,
            provenance_rank(fact.provenance),
        )
    });
}

fn sort_type_ids_by_file(facts: &[Option<StoredTypeFact>], ids: &mut [TypeFactId]) {
    ids.sort_by_key(|id| {
        let fact = facts[id.index()].as_ref().expect(
            "INVARIANT VIOLATED: type file index points to missing fact. \
             This is a bug because indexes must be removed before arena facts. \
             Fix: remove stale ids from every TypeStore index.",
        );
        (
            fact.range.start_byte,
            fact.range.end_byte,
            provenance_rank(fact.provenance),
        )
    });
}
