use std::collections::HashMap;

use crate::{FullyQualifiedName, TextRange};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodFact {
    pub fqn: FullyQualifiedName,
    pub owner: FullyQualifiedName,
    pub range: TextRange,
    pub params: Vec<String>,
}

impl MethodFact {
    pub fn new(fqn: FullyQualifiedName, owner: FullyQualifiedName, range: TextRange) -> Self {
        Self {
            fqn,
            owner,
            range,
            params: Vec::new(),
        }
    }

    pub fn with_params(
        fqn: FullyQualifiedName,
        owner: FullyQualifiedName,
        range: TextRange,
        params: Vec<String>,
    ) -> Self {
        Self {
            fqn,
            owner,
            range,
            params,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct MethodStore {
    facts: HashMap<FullyQualifiedName, Vec<MethodFact>>,
}

impl MethodStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, fact: MethodFact) {
        let facts = self.facts.entry(fact.fqn.clone()).or_default();
        facts.push(fact);
        facts.sort_by_key(|fact| {
            (
                fact.range.file_id,
                fact.range.start_byte,
                fact.range.end_byte,
                fact.owner.to_string(),
            )
        });
    }

    pub fn facts_for(&self, fqn: &FullyQualifiedName) -> &[MethodFact] {
        self.facts.get(fqn).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn all_facts(&self) -> Vec<MethodFact> {
        self.facts
            .values()
            .flat_map(|facts| facts.iter().cloned())
            .collect()
    }

    pub fn facts_in_file(&self, file_id: crate::SourceFileId) -> Vec<MethodFact> {
        self.facts
            .values()
            .flat_map(|facts| facts.iter())
            .filter(|fact| fact.range.file_id == file_id)
            .cloned()
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
        facts: impl IntoIterator<Item = MethodFact>,
    ) {
        self.remove_file(file_id);
        for fact in facts {
            assert!(
                fact.range.file_id == file_id,
                "INVARIANT VIOLATED: replacement method fact belongs to a different file id. \
                 This is a bug because MethodStore::replace_file must only receive facts for the target file. \
                 Fix: partition method facts by SourceFileId before replacing."
            );
            self.add(fact);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{FullyQualifiedName, RubyConstant, RubyMethod, SourceFileId, TextRange};

    use super::*;

    fn file() -> SourceFileId {
        SourceFileId(1)
    }

    fn method_fqn(name: &str) -> FullyQualifiedName {
        FullyQualifiedName::method(
            vec![RubyConstant::new("User").unwrap()],
            RubyMethod::new(name).unwrap(),
        )
    }

    fn owner() -> FullyQualifiedName {
        FullyQualifiedName::namespace(vec![RubyConstant::new("User").unwrap()])
    }

    #[test]
    fn replace_file_removes_stale_method_facts_for_same_file_only() {
        let fqn = method_fqn("name");
        let other_fqn = method_fqn("email");
        let mut store = MethodStore::new();
        store.add(MethodFact::new(
            fqn.clone(),
            owner(),
            TextRange::new(file(), 0, 8),
        ));
        store.add(MethodFact::new(
            other_fqn.clone(),
            owner(),
            TextRange::new(SourceFileId(2), 0, 8),
        ));

        store.replace_file(
            file(),
            [MethodFact::new(
                fqn.clone(),
                owner(),
                TextRange::new(file(), 10, 18),
            )],
        );

        assert_eq!(store.facts_for(&fqn).len(), 1);
        assert_eq!(store.facts_for(&fqn)[0].range.start_byte, 10);
        assert_eq!(store.facts_for(&other_fqn).len(), 1);
    }
}
