use std::cmp::Ordering;
use std::collections::HashSet;

use crate::core::names::fqn_id::FqnId;
use crate::core::storage::file_owned::arena::{FileArena, FileIndex};
use crate::core::storage::file_owned::FileRow;
use crate::core::{FullyQualifiedName, SourceFileId, TextRange};

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
        invariant!(
            name_range.file_id == self.range.file_id
                && name_range.start_byte >= self.range.start_byte
                && name_range.end_byte <= self.range.end_byte,
            what = "symbol name range is outside its declaration range",
            why = "rename edits must target a token within the declaration",
            fix = "derive name_range from the declaring Prism node",
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
        invariant!(
            name_range.file_id == self.range.file_id
                && name_range.start_byte >= self.range.start_byte
                && name_range.end_byte <= self.range.end_byte,
            what = "stored symbol name range is outside its declaration range",
            why = "interned facts must preserve declaration token boundaries",
            fix = "intern SymbolFact::name_range without changing offsets",
        );
        self.name_range = name_range;
        self
    }
}

impl FileRow for StoredSymbolFact {
    fn file_id(&self) -> SourceFileId {
        self.range.file_id
    }
}

#[derive(Debug, Clone, Default)]
pub struct SymbolStore {
    facts: FileArena<StoredSymbolFact>,
    facts_by_fqn: FileIndex<FqnId>,
}

/// Symbols of one file are ordered by declaration range, then kind.
fn by_range(left: &StoredSymbolFact, right: &StoredSymbolFact) -> Ordering {
    let key = |fact: &StoredSymbolFact| (fact.range.start_byte, fact.range.end_byte, fact.kind);
    key(left).cmp(&key(right))
}

impl SymbolStore {
    pub fn facts_for(&self, fqn: FqnId) -> Vec<StoredSymbolFact> {
        self.facts_by_fqn
            .get(&fqn)
            .iter()
            .map(|id| *self.facts.get(*id))
            .collect()
    }

    pub fn has_facts(&self, fqn: FqnId) -> bool {
        !self.facts_by_fqn.get(&fqn).is_empty()
    }

    pub fn all_facts(&self) -> Vec<StoredSymbolFact> {
        self.facts.iter().copied().collect()
    }

    pub fn fact_count(&self) -> usize {
        self.facts.len()
    }

    pub fn known_namespace_fqns(&self) -> HashSet<FqnId> {
        self.facts
            .iter()
            .filter(|fact| matches!(fact.kind, SymbolKind::Class | SymbolKind::Module))
            .map(|fact| fact.fqn)
            .collect()
    }

    pub fn facts_in_file(&self, file_id: SourceFileId) -> Vec<StoredSymbolFact> {
        self.facts.rows_in_file(file_id).copied().collect()
    }

    pub fn replace_file(
        &mut self,
        file_id: SourceFileId,
        facts: impl IntoIterator<Item = StoredSymbolFact>,
    ) {
        self.facts.remove_file(file_id, |arena, stale| {
            self.facts_by_fqn.unlink(stale.fqn, file_id, arena)
        });
        let ids = self.facts.insert_file(file_id, facts, by_range);
        self.facts_by_fqn.link(
            file_id,
            ids.iter().map(|id| (self.facts.get(*id).fqn, *id)),
            &self.facts,
            by_range,
        );
    }

    pub fn estimated_heap_bytes(&self) -> usize {
        self.facts.estimated_heap_bytes() + self.facts_by_fqn.estimated_heap_bytes()
    }

    pub fn shrink_to_fit(&mut self) {
        self.facts.shrink_to_fit();
        self.facts_by_fqn.shrink_to_fit();
    }
}

#[cfg(test)]
mod tests {
    use crate::core::names::fqn_id::FqnId;
    use crate::core::{SourceFileId, TextRange};

    use super::*;

    fn file() -> SourceFileId {
        SourceFileId(1)
    }

    #[test]
    fn replace_file_removes_stale_symbol_facts_for_same_file_only() {
        let fqn = FqnId(1);
        let other_fqn = FqnId(2);
        let mut store = SymbolStore::default();
        store.replace_file(
            file(),
            [StoredSymbolFact::new(
                fqn,
                SymbolKind::Constant,
                TextRange::new(file(), 0, 8),
            )],
        );
        store.replace_file(
            SourceFileId(2),
            [StoredSymbolFact::new(
                other_fqn,
                SymbolKind::Constant,
                TextRange::new(SourceFileId(2), 0, 8),
            )],
        );

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
        assert!(store.has_facts(fqn));
        assert!(store.has_facts(other_fqn));
        assert!(!store.has_facts(FqnId(3)));
    }
}
