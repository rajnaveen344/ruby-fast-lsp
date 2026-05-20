use std::collections::HashMap;

use crate::{FullyQualifiedName, TextRange};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MethodParamKind {
    Required,
    Optional,
    Rest,
    RequiredKeyword,
    OptionalKeyword,
    KeywordRest,
    Block,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodParamFact {
    pub name: String,
    pub kind: MethodParamKind,
}

impl MethodParamFact {
    pub fn new(name: impl Into<String>, kind: MethodParamKind) -> Self {
        let name = name.into();
        assert!(
            !name.is_empty(),
            "INVARIANT VIOLATED: method parameter fact name is empty. \
             This is a bug because parameter facts must identify a Ruby parameter. \
             Fix: skip anonymous parameters or assign a valid generated name before inserting."
        );
        Self { name, kind }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodFact {
    pub fqn: FullyQualifiedName,
    pub owner: FullyQualifiedName,
    pub range: TextRange,
    pub params: Vec<String>,
    pub param_facts: Vec<MethodParamFact>,
}

impl MethodFact {
    pub fn new(fqn: FullyQualifiedName, owner: FullyQualifiedName, range: TextRange) -> Self {
        Self {
            fqn,
            owner,
            range,
            params: Vec::new(),
            param_facts: Vec::new(),
        }
    }

    pub fn with_params(
        fqn: FullyQualifiedName,
        owner: FullyQualifiedName,
        range: TextRange,
        params: Vec<String>,
    ) -> Self {
        let param_facts = params
            .iter()
            .map(|name| MethodParamFact::new(name.clone(), MethodParamKind::Required))
            .collect();
        Self::with_param_facts(fqn, owner, range, param_facts)
    }

    pub fn with_param_facts(
        fqn: FullyQualifiedName,
        owner: FullyQualifiedName,
        range: TextRange,
        param_facts: Vec<MethodParamFact>,
    ) -> Self {
        let params = param_facts.iter().map(|param| param.name.clone()).collect();
        Self {
            fqn,
            owner,
            range,
            params,
            param_facts,
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
