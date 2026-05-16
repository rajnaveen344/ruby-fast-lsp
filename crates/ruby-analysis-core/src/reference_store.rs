use std::collections::HashMap;

use crate::{FullyQualifiedName, SourceFileId, TextRange};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceFact {
    pub target: FullyQualifiedName,
    pub range: TextRange,
    pub caller: Option<FullyQualifiedName>,
}

impl ReferenceFact {
    pub fn new(
        target: FullyQualifiedName,
        range: TextRange,
        caller: Option<FullyQualifiedName>,
    ) -> Self {
        Self {
            target,
            range,
            caller,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ReferenceStore {
    facts: HashMap<FullyQualifiedName, Vec<ReferenceFact>>,
}

impl ReferenceStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, fact: ReferenceFact) {
        let facts = self.facts.entry(fact.target.clone()).or_default();
        facts.push(fact);
        facts.sort_by_key(|fact| {
            (
                fact.range.file_id,
                fact.range.start_byte,
                fact.range.end_byte,
            )
        });
    }

    pub fn facts_for(&self, target: &FullyQualifiedName) -> &[ReferenceFact] {
        self.facts.get(target).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn all_facts(&self) -> Vec<ReferenceFact> {
        self.facts
            .values()
            .flat_map(|facts| facts.iter().cloned())
            .collect()
    }

    pub fn facts_in_file(&self, file_id: SourceFileId) -> Vec<ReferenceFact> {
        self.facts
            .values()
            .flat_map(|facts| facts.iter())
            .filter(|fact| fact.range.file_id == file_id)
            .cloned()
            .collect()
    }

    pub fn remove_file(&mut self, file_id: SourceFileId) {
        self.facts.retain(|_, facts| {
            facts.retain(|fact| fact.range.file_id != file_id);
            !facts.is_empty()
        });
    }

    pub fn replace_file(
        &mut self,
        file_id: SourceFileId,
        facts: impl IntoIterator<Item = ReferenceFact>,
    ) {
        self.remove_file(file_id);
        for fact in facts {
            assert!(
                fact.range.file_id == file_id,
                "INVARIANT VIOLATED: replacement reference fact belongs to a different file id. \
                 This is a bug because ReferenceStore::replace_file must only receive facts for the target file. \
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

    fn fqn(name: &str) -> FullyQualifiedName {
        FullyQualifiedName::constant(vec![RubyConstant::new(name).unwrap()])
    }

    #[test]
    fn replace_file_removes_stale_reference_facts_for_same_file_only() {
        let target = fqn("User");
        let mut store = ReferenceStore::new();
        store.add(ReferenceFact::new(
            target.clone(),
            TextRange::new(file(), 0, 4),
            None,
        ));
        store.add(ReferenceFact::new(
            target.clone(),
            TextRange::new(SourceFileId(2), 0, 4),
            None,
        ));

        store.replace_file(
            file(),
            [ReferenceFact::new(
                target.clone(),
                TextRange::new(file(), 10, 14),
                None,
            )],
        );

        let facts = store.facts_for(&target);
        assert_eq!(facts.len(), 2);
        assert!(facts
            .iter()
            .any(|fact| fact.range.file_id == file() && fact.range.start_byte == 10));
        assert!(facts
            .iter()
            .any(|fact| fact.range.file_id == SourceFileId(2)));
    }
}
