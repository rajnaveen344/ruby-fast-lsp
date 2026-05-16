use std::collections::HashMap;

use crate::{FullyQualifiedName, TextRange};

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
    pub range: TextRange,
}

impl SymbolFact {
    pub fn new(fqn: FullyQualifiedName, kind: SymbolKind, range: TextRange) -> Self {
        Self { fqn, kind, range }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SymbolStore {
    facts: HashMap<FullyQualifiedName, Vec<SymbolFact>>,
}

impl SymbolStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, fact: SymbolFact) {
        let facts = self.facts.entry(fact.fqn.clone()).or_default();
        facts.push(fact);
        facts.sort_by_key(|fact| {
            (
                fact.range.file_id,
                fact.range.start_byte,
                fact.range.end_byte,
                fact.kind,
            )
        });
    }

    pub fn facts_for(&self, fqn: &FullyQualifiedName) -> &[SymbolFact] {
        self.facts.get(fqn).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn all_facts(&self) -> Vec<SymbolFact> {
        self.facts
            .values()
            .flat_map(|facts| facts.iter().cloned())
            .collect()
    }

    pub fn remove_file(&mut self, file_id: crate::SourceFileId) {
        self.facts.retain(|_, facts| {
            facts.retain(|fact| fact.range.file_id != file_id);
            !facts.is_empty()
        });
    }

    pub fn replace_file(
        &mut self,
        file_id: crate::SourceFileId,
        facts: impl IntoIterator<Item = SymbolFact>,
    ) {
        self.remove_file(file_id);
        for fact in facts {
            assert!(
                fact.range.file_id == file_id,
                "INVARIANT VIOLATED: replacement symbol fact belongs to a different file id. \
                 This is a bug because SymbolStore::replace_file must only receive facts for the target file. \
                 Fix: partition facts by SourceFileId before replacing."
            );
            self.add(fact);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{FullyQualifiedName, RubyConstant, SourceFileId, TextRange};

    use super::*;

    fn file() -> SourceFileId {
        SourceFileId(1)
    }

    fn constant_fqn(name: &str) -> FullyQualifiedName {
        FullyQualifiedName::constant(vec![RubyConstant::new(name).unwrap()])
    }

    #[test]
    fn replace_file_removes_stale_symbol_facts_for_same_file_only() {
        let fqn = constant_fqn("VALUE");
        let other_fqn = constant_fqn("OTHER");
        let mut store = SymbolStore::new();
        store.add(SymbolFact::new(
            fqn.clone(),
            SymbolKind::Constant,
            TextRange::new(file(), 0, 8),
        ));
        store.add(SymbolFact::new(
            other_fqn.clone(),
            SymbolKind::Constant,
            TextRange::new(SourceFileId(2), 0, 8),
        ));

        store.replace_file(
            file(),
            [SymbolFact::new(
                fqn.clone(),
                SymbolKind::Constant,
                TextRange::new(file(), 10, 18),
            )],
        );

        assert_eq!(store.facts_for(&fqn).len(), 1);
        assert_eq!(store.facts_for(&fqn)[0].range.start_byte, 10);
        assert_eq!(store.facts_for(&other_fqn).len(), 1);
    }
}
