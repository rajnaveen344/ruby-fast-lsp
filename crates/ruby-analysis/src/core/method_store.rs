use std::collections::{HashMap, HashSet};

use super::memory_estimate::{map_table_bytes, string_heap_bytes, vec_payload_bytes};
use crate::{FqnId, FullyQualifiedName, RubyMethod, SourceFileId, TextRange};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MethodVisibility {
    Public,
    Protected,
    Private,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodParamFact {
    pub name: String,
    pub kind: MethodParamKind,
    pub type_label: Option<String>,
    pub documentation: Option<String>,
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
        Self {
            name,
            kind,
            type_label: None,
            documentation: None,
        }
    }

    pub fn with_signature_metadata(
        mut self,
        type_label: Option<String>,
        documentation: Option<String>,
    ) -> Self {
        self.type_label = type_label;
        self.documentation = documentation;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodFact {
    pub fqn: FullyQualifiedName,
    pub owner: FullyQualifiedName,
    pub range: TextRange,
    pub params: Vec<String>,
    pub param_facts: Vec<MethodParamFact>,
    pub delegate_receiver: Option<RubyMethod>,
    pub visibility: MethodVisibility,
    pub documentation: Option<String>,
    pub return_type_label: Option<String>,
}

impl MethodFact {
    pub fn new(fqn: FullyQualifiedName, owner: FullyQualifiedName, range: TextRange) -> Self {
        Self {
            fqn,
            owner,
            range,
            params: Vec::new(),
            param_facts: Vec::new(),
            delegate_receiver: None,
            visibility: MethodVisibility::Public,
            documentation: None,
            return_type_label: None,
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
            delegate_receiver: None,
            visibility: MethodVisibility::Public,
            documentation: None,
            return_type_label: None,
        }
    }

    pub fn with_delegate_receiver(
        fqn: FullyQualifiedName,
        owner: FullyQualifiedName,
        range: TextRange,
        delegate_receiver: RubyMethod,
    ) -> Self {
        Self {
            fqn,
            owner,
            range,
            params: Vec::new(),
            param_facts: Vec::new(),
            delegate_receiver: Some(delegate_receiver),
            visibility: MethodVisibility::Public,
            documentation: None,
            return_type_label: None,
        }
    }

    pub fn with_visibility(mut self, visibility: MethodVisibility) -> Self {
        self.visibility = visibility;
        self
    }

    pub fn with_signature_metadata(
        mut self,
        documentation: Option<String>,
        return_type_label: Option<String>,
    ) -> Self {
        self.documentation = documentation;
        self.return_type_label = return_type_label;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodVisibilityOverrideFact {
    pub owner: FullyQualifiedName,
    pub method: RubyMethod,
    pub visibility: MethodVisibility,
    pub range: TextRange,
}

impl MethodVisibilityOverrideFact {
    pub fn new(
        owner: FullyQualifiedName,
        method: RubyMethod,
        visibility: MethodVisibility,
        range: TextRange,
    ) -> Self {
        Self {
            owner,
            method,
            visibility,
            range,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredMethodFact {
    pub fqn: FqnId,
    pub owner: FqnId,
    pub method: Option<RubyMethod>,
    pub range: TextRange,
    pub params: Vec<String>,
    pub param_facts: Vec<MethodParamFact>,
    pub delegate_receiver: Option<RubyMethod>,
    pub visibility: MethodVisibility,
    pub documentation: Option<String>,
    pub return_type_label: Option<String>,
}

impl StoredMethodFact {
    pub fn new(fqn: FqnId, owner: FqnId, method: Option<RubyMethod>, range: TextRange) -> Self {
        Self {
            fqn,
            owner,
            method,
            range,
            params: Vec::new(),
            param_facts: Vec::new(),
            delegate_receiver: None,
            visibility: MethodVisibility::Public,
            documentation: None,
            return_type_label: None,
        }
    }

    pub fn with_param_facts(
        fqn: FqnId,
        owner: FqnId,
        method: Option<RubyMethod>,
        range: TextRange,
        param_facts: Vec<MethodParamFact>,
    ) -> Self {
        let params = param_facts.iter().map(|param| param.name.clone()).collect();
        Self {
            fqn,
            owner,
            method,
            range,
            params,
            param_facts,
            delegate_receiver: None,
            visibility: MethodVisibility::Public,
            documentation: None,
            return_type_label: None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct MethodStore {
    facts: Vec<Option<StoredMethodFact>>,
    free_facts: Vec<MethodFactId>,
    facts_by_fqn: HashMap<FqnId, Vec<MethodFactId>>,
    facts_by_owner: HashMap<FqnId, Vec<MethodFactId>>,
    facts_by_owner_name: HashMap<(FqnId, RubyMethod), Vec<MethodFactId>>,
    facts_by_file: HashMap<SourceFileId, Vec<MethodFactId>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct MethodFactId(usize);

impl MethodStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, fact: StoredMethodFact) {
        let method_name = fact.method;
        let file_id = fact.range.file_id;
        let fqn = fact.fqn;
        let owner = fact.owner;
        let id = self.insert_fact(fact);
        self.facts_by_fqn.entry(fqn).or_default().push(id);
        sort_method_ids_by_fqn(&self.facts, self.facts_by_fqn.get_mut(&fqn).unwrap());
        self.facts_by_owner.entry(owner).or_default().push(id);
        sort_method_ids_by_owner(&self.facts, self.facts_by_owner.get_mut(&owner).unwrap());
        if let Some(method_name) = method_name {
            let key = (owner, method_name);
            self.facts_by_owner_name.entry(key).or_default().push(id);
            sort_method_ids_by_owner(&self.facts, self.facts_by_owner_name.get_mut(&key).unwrap());
        }
        self.facts_by_file.entry(file_id).or_default().push(id);
        sort_method_ids_by_file(&self.facts, self.facts_by_file.get_mut(&file_id).unwrap());
    }

    pub fn facts_for(&self, fqn: FqnId) -> Vec<StoredMethodFact> {
        self.facts_by_fqn
            .get(&fqn)
            .map(|ids| self.clone_facts(ids))
            .unwrap_or_default()
    }

    pub fn all_facts(&self) -> Vec<StoredMethodFact> {
        self.facts.iter().filter_map(|fact| fact.clone()).collect()
    }

    pub fn fact_count(&self) -> usize {
        self.facts.iter().filter(|fact| fact.is_some()).count()
    }

    pub fn facts_matching_owner(&self, owner: FqnId, partial: &str) -> Vec<StoredMethodFact> {
        self.facts_by_owner
            .get(&owner)
            .into_iter()
            .flat_map(|ids| ids.iter().filter_map(|id| self.fact(*id)))
            .filter(|fact| {
                fact.method
                    .is_some_and(|method| method.get_name().starts_with(partial))
            })
            .cloned()
            .collect()
    }

    pub fn facts_matching_owner_name(
        &self,
        owner: FqnId,
        method: &RubyMethod,
    ) -> Vec<StoredMethodFact> {
        self.facts_by_owner_name
            .get(&(owner, *method))
            .map(|ids| self.clone_facts(ids))
            .unwrap_or_default()
    }

    pub fn method_names_for_owner(&self, owner: FqnId) -> Vec<&'static str> {
        let mut names = Vec::new();
        let mut seen = HashSet::new();
        let Some(facts) = self.facts_by_owner.get(&owner) else {
            return names;
        };
        for fact in facts.iter().filter_map(|id| self.fact(*id)) {
            let Some(method) = fact.method else {
                continue;
            };
            let name = method.as_str();
            if seen.insert(name) {
                names.push(name);
            }
        }
        names
    }

    pub fn facts_in_file(&self, file_id: crate::SourceFileId) -> Vec<StoredMethodFact> {
        self.facts_by_file
            .get(&file_id)
            .map(|ids| self.clone_facts(ids))
            .unwrap_or_default()
    }

    pub fn remove_file(&mut self, file_id: crate::SourceFileId) {
        let Some(stale_facts) = self.facts_by_file.remove(&file_id) else {
            return;
        };
        for stale_id in stale_facts {
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
            if let Some(ids) = self.facts_by_owner.get_mut(&stale.owner) {
                ids.retain(|id| *id != stale_id);
                if ids.is_empty() {
                    self.facts_by_owner.remove(&stale.owner);
                }
            }
            if let Some(method) = stale.method {
                let key = (stale.owner, method);
                if let Some(ids) = self.facts_by_owner_name.get_mut(&key) {
                    ids.retain(|id| *id != stale_id);
                    if ids.is_empty() {
                        self.facts_by_owner_name.remove(&key);
                    }
                }
            }
        }
    }

    pub fn replace_file(
        &mut self,
        file_id: crate::SourceFileId,
        facts: impl IntoIterator<Item = StoredMethodFact>,
    ) {
        self.remove_file(file_id);
        let mut touched_fqns = HashSet::new();
        let mut touched_owners = HashSet::new();
        let mut touched_owner_names = HashSet::new();
        for fact in facts {
            assert!(
                fact.range.file_id == file_id,
                "INVARIANT VIOLATED: replacement method fact belongs to a different file id. \
                 This is a bug because MethodStore::replace_file must only receive facts for the target file. \
                 Fix: partition method facts by SourceFileId before replacing."
            );
            let fqn = fact.fqn;
            let owner = fact.owner;
            let method_name = fact.method;
            let id = self.insert_fact(fact);
            touched_fqns.insert(fqn);
            touched_owners.insert(owner);
            self.facts_by_fqn.entry(fqn).or_default().push(id);
            self.facts_by_owner.entry(owner).or_default().push(id);
            if let Some(method) = method_name {
                let key = (owner, method);
                touched_owner_names.insert(key);
                self.facts_by_owner_name.entry(key).or_default().push(id);
            }
            self.facts_by_file.entry(file_id).or_default().push(id);
        }
        for fqn in touched_fqns {
            if let Some(ids) = self.facts_by_fqn.get_mut(&fqn) {
                sort_method_ids_by_fqn(&self.facts, ids);
                ids.shrink_to_fit();
            }
        }
        for owner in touched_owners {
            if let Some(ids) = self.facts_by_owner.get_mut(&owner) {
                sort_method_ids_by_owner(&self.facts, ids);
                ids.shrink_to_fit();
            }
        }
        for key in touched_owner_names {
            if let Some(ids) = self.facts_by_owner_name.get_mut(&key) {
                sort_method_ids_by_owner(&self.facts, ids);
                ids.shrink_to_fit();
            }
        }
        if let Some(ids) = self.facts_by_file.get_mut(&file_id) {
            sort_method_ids_by_file(&self.facts, ids);
            ids.shrink_to_fit();
        }
    }

    pub fn estimated_heap_bytes(&self) -> usize {
        vec_payload_bytes(&self.facts)
            + vec_payload_bytes(&self.free_facts)
            + self
                .facts
                .iter()
                .filter_map(|fact| fact.as_ref())
                .map(method_fact_heap_bytes)
                .sum::<usize>()
            + map_table_bytes(&self.facts_by_fqn)
            + map_table_bytes(&self.facts_by_owner)
            + map_table_bytes(&self.facts_by_owner_name)
            + map_table_bytes(&self.facts_by_file)
            + self
                .facts_by_fqn
                .values()
                .map(vec_payload_bytes)
                .sum::<usize>()
            + self
                .facts_by_owner
                .values()
                .map(vec_payload_bytes)
                .sum::<usize>()
            + self
                .facts_by_owner_name
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
        self.facts_by_owner.shrink_to_fit();
        self.facts_by_owner_name.shrink_to_fit();
        self.facts_by_file.shrink_to_fit();
        for ids in self.facts_by_fqn.values_mut() {
            ids.shrink_to_fit();
        }
        for ids in self.facts_by_owner.values_mut() {
            ids.shrink_to_fit();
        }
        for ids in self.facts_by_owner_name.values_mut() {
            ids.shrink_to_fit();
        }
        for ids in self.facts_by_file.values_mut() {
            ids.shrink_to_fit();
        }
    }

    fn insert_fact(&mut self, fact: StoredMethodFact) -> MethodFactId {
        if let Some(id) = self.free_facts.pop() {
            let slot = self.facts.get_mut(id.0).expect(
                "INVARIANT VIOLATED: method free list points outside fact arena. \
                 This is a bug because free ids must come from previous arena slots. \
                 Fix: only push ids returned by MethodStore::take_fact.",
            );
            assert!(
                slot.is_none(),
                "INVARIANT VIOLATED: method free list points to occupied fact slot. \
                 This is a bug because free ids must only reference removed facts. \
                 Fix: push each removed method id at most once."
            );
            *slot = Some(fact);
            return id;
        }
        let id = MethodFactId(self.facts.len());
        self.facts.push(Some(fact));
        id
    }

    fn fact(&self, id: MethodFactId) -> Option<&StoredMethodFact> {
        self.facts.get(id.0).and_then(Option::as_ref)
    }

    fn take_fact(&mut self, id: MethodFactId) -> Option<StoredMethodFact> {
        self.facts.get_mut(id.0).and_then(Option::take)
    }

    fn clone_facts(&self, ids: &[MethodFactId]) -> Vec<StoredMethodFact> {
        ids.iter()
            .filter_map(|id| self.fact(*id).cloned())
            .collect()
    }
}

fn method_fact_heap_bytes(fact: &StoredMethodFact) -> usize {
    vec_payload_bytes(&fact.params)
        + fact.params.iter().map(string_heap_bytes).sum::<usize>()
        + vec_payload_bytes(&fact.param_facts)
        + fact
            .param_facts
            .iter()
            .map(|param| {
                string_heap_bytes(&param.name)
                    + param
                        .type_label
                        .as_ref()
                        .map(string_heap_bytes)
                        .unwrap_or(0)
                    + param
                        .documentation
                        .as_ref()
                        .map(string_heap_bytes)
                        .unwrap_or(0)
            })
            .sum::<usize>()
        + fact
            .documentation
            .as_ref()
            .map(string_heap_bytes)
            .unwrap_or(0)
        + fact
            .return_type_label
            .as_ref()
            .map(string_heap_bytes)
            .unwrap_or(0)
}

fn sort_method_ids_by_fqn(facts: &[Option<StoredMethodFact>], ids: &mut [MethodFactId]) {
    ids.sort_by_key(|id| {
        let fact = facts[id.0].as_ref().expect(
            "INVARIANT VIOLATED: method index points to missing fact. \
             This is a bug because indexes must be removed before arena facts. \
             Fix: remove stale ids from every MethodStore index.",
        );
        (
            fact.range.file_id,
            fact.range.start_byte,
            fact.range.end_byte,
            fact.owner,
        )
    });
}

fn sort_method_ids_by_owner(facts: &[Option<StoredMethodFact>], ids: &mut [MethodFactId]) {
    ids.sort_by_key(|id| {
        let fact = facts[id.0].as_ref().expect(
            "INVARIANT VIOLATED: method owner index points to missing fact. \
             This is a bug because indexes must be removed before arena facts. \
             Fix: remove stale ids from every MethodStore index.",
        );
        (
            fact.range.file_id,
            fact.range.start_byte,
            fact.range.end_byte,
            fact.fqn,
        )
    });
}

fn sort_method_ids_by_file(facts: &[Option<StoredMethodFact>], ids: &mut [MethodFactId]) {
    ids.sort_by_key(|id| {
        let fact = facts[id.0].as_ref().expect(
            "INVARIANT VIOLATED: method file index points to missing fact. \
             This is a bug because indexes must be removed before arena facts. \
             Fix: remove stale ids from every MethodStore index.",
        );
        (fact.range.start_byte, fact.range.end_byte)
    });
}

#[cfg(test)]
mod tests {
    use crate::{FqnId, RubyMethod, SourceFileId, TextRange};

    use super::*;

    fn file() -> SourceFileId {
        SourceFileId(1)
    }

    #[test]
    fn replace_file_removes_stale_method_facts_for_same_file_only() {
        let fqn = FqnId(1);
        let other_fqn = FqnId(2);
        let owner = FqnId(3);
        let name = RubyMethod::new("name").unwrap();
        let email = RubyMethod::new("email").unwrap();
        let mut store = MethodStore::new();
        store.add(StoredMethodFact::new(
            fqn,
            owner,
            Some(name),
            TextRange::new(file(), 0, 8),
        ));
        store.add(StoredMethodFact::new(
            other_fqn,
            owner,
            Some(email),
            TextRange::new(SourceFileId(2), 0, 8),
        ));

        store.replace_file(
            file(),
            [StoredMethodFact::new(
                fqn,
                owner,
                Some(name),
                TextRange::new(file(), 10, 18),
            )],
        );

        assert_eq!(store.facts_for(fqn).len(), 1);
        assert_eq!(store.facts_for(fqn)[0].range.start_byte, 10);
        assert_eq!(store.facts_for(other_fqn).len(), 1);
    }
}
