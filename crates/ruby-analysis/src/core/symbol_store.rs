use std::collections::{HashMap, HashSet};

use super::file_owned_index::place_appended_file_facts;
use super::memory_estimate::{map_table_bytes, vec_payload_bytes};
use crate::{FqnId, FullyQualifiedName, SourceFileId, TextRange};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SymbolKind {
    Class,
    Module,
    Method,
    Constant,
    LocalVariable,
    InstanceVariable,
    ClassVariable,
    GlobalVariable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolFact {
    pub fqn: FullyQualifiedName,
    pub kind: SymbolKind,
    /// Exact identifier token that declares this symbol.
    pub name_range: TextRange,
    /// Full declaration range used for navigation and presentation.
    pub range: TextRange,
}

impl SymbolFact {
    pub fn new(fqn: FullyQualifiedName, kind: SymbolKind, range: TextRange) -> Self {
        Self {
            fqn,
            kind,
            name_range: range,
            range,
        }
    }

    pub fn with_name_range(mut self, name_range: TextRange) -> Self {
        assert!(
            name_range.file_id == self.range.file_id
                && name_range.start_byte >= self.range.start_byte
                && name_range.end_byte <= self.range.end_byte,
            "INVARIANT VIOLATED: symbol name range is outside its declaration range. \
             This is a bug because rename edits must target a token within the declaration. \
             Fix: derive name_range from the declaring Prism node."
        );
        self.name_range = name_range;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoredSymbolFact {
    pub fqn: FqnId,
    pub kind: SymbolKind,
    pub name_range: TextRange,
    pub range: TextRange,
}

impl StoredSymbolFact {
    pub fn new(fqn: FqnId, kind: SymbolKind, range: TextRange) -> Self {
        Self {
            fqn,
            kind,
            name_range: range,
            range,
        }
    }

    pub fn with_name_range(mut self, name_range: TextRange) -> Self {
        assert!(
            name_range.file_id == self.range.file_id
                && name_range.start_byte >= self.range.start_byte
                && name_range.end_byte <= self.range.end_byte,
            "INVARIANT VIOLATED: stored symbol name range is outside its declaration range. \
             This is a bug because interned facts must preserve declaration token boundaries. \
             Fix: intern SymbolFact::name_range without changing offsets."
        );
        self.name_range = name_range;
        self
    }
}

#[derive(Debug, Clone, Default)]
pub struct SymbolStore {
    facts: Vec<Option<StoredSymbolFact>>,
    free_facts: Vec<SymbolFactId>,
    facts_by_fqn: HashMap<FqnId, Vec<SymbolFactId>>,
    facts_by_file: HashMap<SourceFileId, Vec<SymbolFactId>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct SymbolFactId(usize);

impl SymbolStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, fact: StoredSymbolFact) {
        let file_id = fact.range.file_id;
        let fqn = fact.fqn;
        let id = self.insert_fact(fact);
        self.facts_by_fqn.entry(fqn).or_default().push(id);
        sort_symbol_ids(&self.facts, self.facts_by_fqn.get_mut(&fqn).unwrap());
        self.facts_by_file.entry(file_id).or_default().push(id);
        sort_symbol_ids_by_file(&self.facts, self.facts_by_file.get_mut(&file_id).unwrap());
    }

    pub fn facts_for(&self, fqn: FqnId) -> Vec<StoredSymbolFact> {
        self.facts_by_fqn
            .get(&fqn)
            .map(|ids| self.clone_facts(ids))
            .unwrap_or_default()
    }

    pub fn all_facts(&self) -> Vec<StoredSymbolFact> {
        self.facts.iter().filter_map(|fact| *fact).collect()
    }

    pub fn fact_count(&self) -> usize {
        self.facts.iter().filter(|fact| fact.is_some()).count()
    }

    pub fn known_namespace_fqns(&self) -> HashSet<FqnId> {
        self.facts
            .iter()
            .filter_map(|fact| *fact)
            .filter(|fact| matches!(fact.kind, SymbolKind::Class | SymbolKind::Module))
            .map(|fact| fact.fqn)
            .collect()
    }

    pub fn facts_in_file(&self, file_id: crate::SourceFileId) -> Vec<StoredSymbolFact> {
        self.facts_by_file
            .get(&file_id)
            .map(|ids| self.clone_facts(ids))
            .unwrap_or_default()
    }

    pub fn remove_file(&mut self, file_id: crate::SourceFileId) {
        let Some(stale_ids) = self.facts_by_file.remove(&file_id) else {
            return;
        };
        for stale_id in stale_ids {
            let Some(stale) = self.take_fact(stale_id) else {
                continue;
            };
            self.free_facts.push(stale_id);
            if let Some(ids) = self.facts_by_fqn.get_mut(&stale.fqn) {
                ids.retain(|id| *id != stale_id);
                if ids.is_empty() {
                    self.facts_by_fqn.remove(&stale.fqn);
                }
            }
        }
    }

    pub fn replace_file(
        &mut self,
        file_id: crate::SourceFileId,
        facts: impl IntoIterator<Item = StoredSymbolFact>,
    ) {
        self.remove_file(file_id);
        let mut touched_fqns = Vec::new();
        for fact in facts {
            assert!(
                fact.range.file_id == file_id,
                "INVARIANT VIOLATED: replacement symbol fact belongs to a different file id. \
                 This is a bug because SymbolStore::replace_file must only receive facts for the target file. \
                 Fix: partition facts by SourceFileId before replacing."
            );
            let key = fact.fqn;
            if let Some((_, appended_count)) =
                touched_fqns.iter_mut().find(|(touched, _)| *touched == key)
            {
                *appended_count += 1;
            } else {
                touched_fqns.push((key, 1));
            }
            let id = self.insert_fact(fact);
            self.facts_by_fqn.entry(key).or_default().push(id);
            self.facts_by_file.entry(file_id).or_default().push(id);
        }
        for (fqn, appended_count) in touched_fqns {
            if let Some(ids) = self.facts_by_fqn.get_mut(&fqn) {
                place_appended_file_facts(
                    ids,
                    appended_count,
                    file_id,
                    |id| {
                        self.facts[id.0]
                            .as_ref()
                            .expect(
                                "INVARIANT VIOLATED: symbol index points to missing fact. \
                                 This is a bug because indexes must be removed before arena facts. \
                                 Fix: remove stale ids from every SymbolStore index.",
                            )
                            .range
                            .file_id
                    },
                    |appended| sort_symbol_ids(&self.facts, appended),
                );
                ids.shrink_to_fit();
            }
        }
        if let Some(ids) = self.facts_by_file.get_mut(&file_id) {
            sort_symbol_ids_by_file(&self.facts, ids);
            ids.shrink_to_fit();
        }
    }

    pub fn estimated_heap_bytes(&self) -> usize {
        vec_payload_bytes(&self.facts)
            + vec_payload_bytes(&self.free_facts)
            + map_table_bytes(&self.facts_by_fqn)
            + map_table_bytes(&self.facts_by_file)
            + self
                .facts_by_fqn
                .values()
                .map(vec_payload_bytes)
                .sum::<usize>()
            + self
                .facts_by_file
                .values()
                .map(vec_payload_bytes)
                .sum::<usize>()
    }

    pub fn shrink_to_fit(&mut self) {
        self.facts.shrink_to_fit();
        self.free_facts.shrink_to_fit();
        self.facts_by_fqn.shrink_to_fit();
        self.facts_by_file.shrink_to_fit();
        for ids in self.facts_by_fqn.values_mut() {
            ids.shrink_to_fit();
        }
        for ids in self.facts_by_file.values_mut() {
            ids.shrink_to_fit();
        }
    }

    fn insert_fact(&mut self, fact: StoredSymbolFact) -> SymbolFactId {
        if let Some(id) = self.free_facts.pop() {
            let slot = self.facts.get_mut(id.0).expect(
                "INVARIANT VIOLATED: symbol free list points outside fact arena. \
                 This is a bug because free ids must come from previous arena slots. \
                 Fix: only push ids returned by SymbolStore::take_fact.",
            );
            assert!(
                slot.is_none(),
                "INVARIANT VIOLATED: symbol free list points to occupied fact slot. \
                 This is a bug because free ids must only reference removed facts. \
                 Fix: push each removed symbol id at most once."
            );
            *slot = Some(fact);
            return id;
        }
        let id = SymbolFactId(self.facts.len());
        self.facts.push(Some(fact));
        id
    }

    fn fact(&self, id: SymbolFactId) -> Option<StoredSymbolFact> {
        self.facts.get(id.0).and_then(|fact| *fact)
    }

    fn take_fact(&mut self, id: SymbolFactId) -> Option<StoredSymbolFact> {
        self.facts.get_mut(id.0).and_then(Option::take)
    }

    fn clone_facts(&self, ids: &[SymbolFactId]) -> Vec<StoredSymbolFact> {
        ids.iter().filter_map(|id| self.fact(*id)).collect()
    }
}

fn sort_symbol_ids(facts: &[Option<StoredSymbolFact>], ids: &mut [SymbolFactId]) {
    ids.sort_by_key(|id| {
        let fact = facts[id.0].as_ref().expect(
            "INVARIANT VIOLATED: symbol index points to missing fact. \
             This is a bug because indexes must be removed before arena facts. \
             Fix: remove stale ids from every SymbolStore index.",
        );
        (
            fact.range.file_id,
            fact.range.start_byte,
            fact.range.end_byte,
            fact.kind,
        )
    });
}

fn sort_symbol_ids_by_file(facts: &[Option<StoredSymbolFact>], ids: &mut [SymbolFactId]) {
    ids.sort_by_key(|id| {
        let fact = facts[id.0].as_ref().expect(
            "INVARIANT VIOLATED: symbol file index points to missing fact. \
             This is a bug because indexes must be removed before arena facts. \
             Fix: remove stale ids from every SymbolStore index.",
        );
        (fact.range.start_byte, fact.range.end_byte, fact.kind)
    });
}

#[cfg(test)]
mod tests {
    use crate::{FqnId, SourceFileId, TextRange};

    use super::*;

    fn file() -> SourceFileId {
        SourceFileId(1)
    }

    #[test]
    fn replace_file_removes_stale_symbol_facts_for_same_file_only() {
        let fqn = FqnId(1);
        let other_fqn = FqnId(2);
        let mut store = SymbolStore::new();
        store.add(StoredSymbolFact::new(
            fqn,
            SymbolKind::Constant,
            TextRange::new(file(), 0, 8),
        ));
        store.add(StoredSymbolFact::new(
            other_fqn,
            SymbolKind::Constant,
            TextRange::new(SourceFileId(2), 0, 8),
        ));

        store.replace_file(
            file(),
            [StoredSymbolFact::new(
                fqn,
                SymbolKind::Constant,
                TextRange::new(file(), 10, 18),
            )],
        );

        assert_eq!(store.facts_for(fqn).len(), 1);
        assert_eq!(store.facts_for(fqn)[0].range.start_byte, 10);
        assert_eq!(store.facts_for(other_fqn).len(), 1);
    }
}
