use std::collections::HashMap;

use super::memory_estimate::{map_table_bytes, vec_payload_bytes};
use crate::{
    ConstLookupId, FqnId, FullyQualifiedName, NamespaceKind, RubyConstant, RubyMethod,
    SourceFileId, TextRange,
};
use smallvec::SmallVec;

pub type ConstantPath = SmallVec<[RubyConstant; 4]>;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ConstLookup {
    pub path: ConstantPath,
    pub absolute: bool,
    pub context: FqnId,
}

impl ConstLookup {
    pub fn new(path: ConstantPath, absolute: bool, context: FqnId) -> Self {
        Self {
            path,
            absolute,
            context,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceFact {
    pub range: TextRange,
    pub caller: Option<FqnId>,
    pub access: MethodReferenceAccess,
}

impl ReferenceFact {
    pub fn new(range: TextRange, caller: Option<FqnId>) -> Self {
        Self {
            range,
            caller,
            access: MethodReferenceAccess::Normal,
        }
    }

    pub fn method(range: TextRange, caller: Option<FqnId>, access: MethodReferenceAccess) -> Self {
        Self {
            range,
            caller,
            access,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MethodReferenceAccess {
    Normal,
    ExplicitReceiver,
    VisibilityBypass,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReferenceCandidateKind {
    Constant {
        parts: ConstantPath,
        current_namespace: ConstantPath,
    },
    Method {
        owner: ConstantPath,
        owner_kind: NamespaceKind,
        method: RubyMethod,
        is_super: bool,
        access: MethodReferenceAccess,
        caller: Option<FullyQualifiedName>,
        diagnostics: Option<Box<MethodReferenceDiagnostics>>,
    },
    Resolved {
        target: FullyQualifiedName,
        caller: Option<FullyQualifiedName>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MethodCallSignatureCandidate {
    pub positional_count: usize,
    pub has_positional_splat: bool,
    pub keyword_args: Vec<KeywordArgCandidate>,
    pub has_keyword_splat: bool,
}

impl MethodCallSignatureCandidate {
    pub fn is_empty(&self) -> bool {
        self.positional_count == 0
            && !self.has_positional_splat
            && self.keyword_args.is_empty()
            && !self.has_keyword_splat
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodReferenceDiagnostics {
    pub diagnostic_range: TextRange,
    pub receiver_label: Option<String>,
    pub diagnose_unresolved: bool,
    pub signature: MethodCallSignatureCandidate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodReferenceCandidate {
    pub owner: Vec<RubyConstant>,
    pub owner_kind: NamespaceKind,
    pub method: RubyMethod,
    pub is_super: bool,
    pub access: MethodReferenceAccess,
    pub caller: Option<FullyQualifiedName>,
    pub diagnostics: MethodReferenceDiagnostics,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeywordArgCandidate {
    pub name: String,
    pub range: TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceCandidate {
    pub range: TextRange,
    pub kind: ReferenceCandidateKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredReferenceCandidate {
    pub range: TextRange,
    pub kind: StoredReferenceCandidateKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredConstantReferenceCandidate {
    pub range: TextRange,
    pub lookup: ConstLookupId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredMethodReferenceCandidate {
    pub range: TextRange,
    pub owner: ConstLookupId,
    pub owner_kind: NamespaceKind,
    pub method: RubyMethod,
    pub is_super: bool,
    pub access: MethodReferenceAccess,
    pub caller: Option<FqnId>,
    pub diagnostics: Option<Box<MethodReferenceDiagnostics>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredResolvedReferenceCandidate {
    pub range: TextRange,
    pub target: FqnId,
    pub caller: Option<FqnId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoredReferenceCandidateRef<'a> {
    Constant(&'a StoredConstantReferenceCandidate),
    Method(&'a StoredMethodReferenceCandidate),
    Resolved(&'a StoredResolvedReferenceCandidate),
}

impl StoredReferenceCandidateRef<'_> {
    pub fn range(&self) -> TextRange {
        match self {
            StoredReferenceCandidateRef::Constant(candidate) => candidate.range,
            StoredReferenceCandidateRef::Method(candidate) => candidate.range,
            StoredReferenceCandidateRef::Resolved(candidate) => candidate.range,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoredReferenceCandidateKind {
    Constant {
        lookup: ConstLookupId,
    },
    Method {
        owner: ConstLookupId,
        owner_kind: NamespaceKind,
        method: RubyMethod,
        is_super: bool,
        access: MethodReferenceAccess,
        caller: Option<FqnId>,
        diagnostics: Option<Box<MethodReferenceDiagnostics>>,
    },
    Resolved {
        target: FqnId,
        caller: Option<FqnId>,
    },
}

impl StoredReferenceCandidate {
    pub fn constant(range: TextRange, lookup: ConstLookupId) -> Self {
        Self {
            range,
            kind: StoredReferenceCandidateKind::Constant { lookup },
        }
    }

    pub fn method(
        range: TextRange,
        owner: ConstLookupId,
        owner_kind: NamespaceKind,
        method: RubyMethod,
        is_super: bool,
        access: MethodReferenceAccess,
        caller: Option<FqnId>,
        diagnostics: Option<Box<MethodReferenceDiagnostics>>,
    ) -> Self {
        Self {
            range,
            kind: StoredReferenceCandidateKind::Method {
                owner,
                owner_kind,
                method,
                is_super,
                access,
                caller,
                diagnostics,
            },
        }
    }

    pub fn resolved(range: TextRange, target: FqnId, caller: Option<FqnId>) -> Self {
        Self {
            range,
            kind: StoredReferenceCandidateKind::Resolved { target, caller },
        }
    }
}

impl ReferenceCandidate {
    pub fn constant(
        range: TextRange,
        parts: Vec<RubyConstant>,
        current_namespace: Vec<RubyConstant>,
    ) -> Self {
        assert!(
            !parts.is_empty(),
            "INVARIANT VIOLATED: constant reference candidate has no parts. \
             This is a bug because constant resolution requires at least one constant name. \
             Fix: skip empty constant paths before constructing ReferenceCandidate."
        );
        Self {
            range,
            kind: ReferenceCandidateKind::Constant {
                parts: ConstantPath::from_vec(parts),
                current_namespace: ConstantPath::from_vec(current_namespace),
            },
        }
    }

    pub fn resolved(
        range: TextRange,
        target: FullyQualifiedName,
        caller: Option<FullyQualifiedName>,
    ) -> Self {
        Self {
            range,
            kind: ReferenceCandidateKind::Resolved { target, caller },
        }
    }

    pub fn method(reference_range: TextRange, candidate: MethodReferenceCandidate) -> Self {
        Self {
            range: reference_range,
            kind: ReferenceCandidateKind::Method {
                owner: ConstantPath::from_vec(candidate.owner),
                owner_kind: candidate.owner_kind,
                method: candidate.method,
                is_super: candidate.is_super,
                access: candidate.access,
                caller: candidate.caller,
                diagnostics: Some(Box::new(candidate.diagnostics)),
            },
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ReferenceCandidateStore {
    constants_by_file: HashMap<SourceFileId, Vec<StoredConstantReferenceCandidate>>,
    methods_by_file: HashMap<SourceFileId, Vec<StoredMethodReferenceCandidate>>,
    resolved_by_file: HashMap<SourceFileId, Vec<StoredResolvedReferenceCandidate>>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReferenceCandidateStats {
    pub constants: usize,
    pub methods: usize,
    pub resolved: usize,
}

impl ReferenceCandidateStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn replace_file(
        &mut self,
        file_id: SourceFileId,
        candidates: impl IntoIterator<Item = StoredReferenceCandidate>,
    ) {
        self.constants_by_file.remove(&file_id);
        self.methods_by_file.remove(&file_id);
        self.resolved_by_file.remove(&file_id);

        let mut constants = Vec::new();
        let mut methods = Vec::new();
        let mut resolved = Vec::new();
        for candidate in candidates {
            assert!(
                candidate.range.file_id == file_id,
                "INVARIANT VIOLATED: replacement reference candidate belongs to a different file id. \
                 This is a bug because ReferenceCandidateStore::replace_file must only receive candidates for the target file. \
                 Fix: partition candidates by SourceFileId before replacing."
            );
            match candidate.kind {
                StoredReferenceCandidateKind::Constant { lookup } => {
                    constants.push(StoredConstantReferenceCandidate {
                        range: candidate.range,
                        lookup,
                    })
                }
                StoredReferenceCandidateKind::Method {
                    owner,
                    owner_kind,
                    method,
                    is_super,
                    access,
                    caller,
                    diagnostics,
                } => methods.push(StoredMethodReferenceCandidate {
                    range: candidate.range,
                    owner,
                    owner_kind,
                    method,
                    is_super,
                    access,
                    caller,
                    diagnostics,
                }),
                StoredReferenceCandidateKind::Resolved { target, caller } => {
                    resolved.push(StoredResolvedReferenceCandidate {
                        range: candidate.range,
                        target,
                        caller,
                    });
                }
            }
        }

        if !constants.is_empty() {
            constants
                .sort_by_key(|candidate| (candidate.range.start_byte, candidate.range.end_byte));
            constants.shrink_to_fit();
            self.constants_by_file.insert(file_id, constants);
        }
        if !methods.is_empty() {
            methods.sort_by_key(|candidate| (candidate.range.start_byte, candidate.range.end_byte));
            methods.shrink_to_fit();
            self.methods_by_file.insert(file_id, methods);
        }
        if !resolved.is_empty() {
            resolved
                .sort_by_key(|candidate| (candidate.range.start_byte, candidate.range.end_byte));
            resolved.shrink_to_fit();
            self.resolved_by_file.insert(file_id, resolved);
        }
    }

    pub fn all_candidates(&self) -> Vec<StoredReferenceCandidate> {
        self.iter_candidates()
            .map(|candidate| match candidate {
                StoredReferenceCandidateRef::Constant(candidate) => StoredReferenceCandidate {
                    range: candidate.range,
                    kind: StoredReferenceCandidateKind::Constant {
                        lookup: candidate.lookup,
                    },
                },
                StoredReferenceCandidateRef::Method(candidate) => StoredReferenceCandidate {
                    range: candidate.range,
                    kind: StoredReferenceCandidateKind::Method {
                        owner: candidate.owner,
                        owner_kind: candidate.owner_kind,
                        method: candidate.method,
                        is_super: candidate.is_super,
                        access: candidate.access,
                        caller: candidate.caller,
                        diagnostics: candidate.diagnostics.clone(),
                    },
                },
                StoredReferenceCandidateRef::Resolved(candidate) => StoredReferenceCandidate {
                    range: candidate.range,
                    kind: StoredReferenceCandidateKind::Resolved {
                        target: candidate.target,
                        caller: candidate.caller,
                    },
                },
            })
            .collect()
    }

    pub fn candidates_in_file(&self, file_id: SourceFileId) -> Vec<StoredReferenceCandidate> {
        let mut candidates = Vec::new();
        if let Some(constants) = self.constants_by_file.get(&file_id) {
            candidates.extend(constants.iter().map(|candidate| StoredReferenceCandidate {
                range: candidate.range,
                kind: StoredReferenceCandidateKind::Constant {
                    lookup: candidate.lookup,
                },
            }));
        }
        if let Some(methods) = self.methods_by_file.get(&file_id) {
            candidates.extend(methods.iter().map(|candidate| StoredReferenceCandidate {
                range: candidate.range,
                kind: StoredReferenceCandidateKind::Method {
                    owner: candidate.owner,
                    owner_kind: candidate.owner_kind,
                    method: candidate.method,
                    is_super: candidate.is_super,
                    access: candidate.access,
                    caller: candidate.caller,
                    diagnostics: candidate.diagnostics.clone(),
                },
            }));
        }
        if let Some(resolved) = self.resolved_by_file.get(&file_id) {
            candidates.extend(resolved.iter().map(|candidate| StoredReferenceCandidate {
                range: candidate.range,
                kind: StoredReferenceCandidateKind::Resolved {
                    target: candidate.target,
                    caller: candidate.caller,
                },
            }));
        }
        candidates.sort_by_key(|candidate| (candidate.range.start_byte, candidate.range.end_byte));
        candidates
    }

    pub fn iter_candidates(&self) -> impl Iterator<Item = StoredReferenceCandidateRef<'_>> {
        self.constants_by_file
            .values()
            .flat_map(|candidates| candidates.iter().map(StoredReferenceCandidateRef::Constant))
            .chain(
                self.methods_by_file.values().flat_map(|candidates| {
                    candidates.iter().map(StoredReferenceCandidateRef::Method)
                }),
            )
            .chain(self.resolved_by_file.values().flat_map(|candidates| {
                candidates.iter().map(StoredReferenceCandidateRef::Resolved)
            }))
    }

    pub fn candidate_count(&self) -> usize {
        self.constants_by_file.values().map(Vec::len).sum::<usize>()
            + self.methods_by_file.values().map(Vec::len).sum::<usize>()
            + self.resolved_by_file.values().map(Vec::len).sum::<usize>()
    }

    pub fn stats(&self) -> ReferenceCandidateStats {
        ReferenceCandidateStats {
            constants: self.constants_by_file.values().map(Vec::len).sum(),
            methods: self.methods_by_file.values().map(Vec::len).sum(),
            resolved: self.resolved_by_file.values().map(Vec::len).sum(),
        }
    }

    pub fn file_ids(&self) -> Vec<SourceFileId> {
        let mut file_ids = self
            .constants_by_file
            .keys()
            .chain(self.methods_by_file.keys())
            .chain(self.resolved_by_file.keys())
            .copied()
            .collect::<Vec<_>>();
        file_ids.sort();
        file_ids.dedup();
        file_ids
    }

    pub fn estimated_heap_bytes(&self) -> usize {
        map_table_bytes(&self.constants_by_file)
            + map_table_bytes(&self.methods_by_file)
            + map_table_bytes(&self.resolved_by_file)
            + self
                .constants_by_file
                .values()
                .map(vec_payload_bytes)
                .sum::<usize>()
            + self
                .methods_by_file
                .values()
                .map(|candidates| {
                    vec_payload_bytes(candidates)
                        + candidates
                            .iter()
                            .map(method_reference_candidate_heap_bytes)
                            .sum::<usize>()
                })
                .sum::<usize>()
            + self
                .resolved_by_file
                .values()
                .map(vec_payload_bytes)
                .sum::<usize>()
    }

    pub fn shrink_to_fit(&mut self) {
        self.constants_by_file.shrink_to_fit();
        self.methods_by_file.shrink_to_fit();
        self.resolved_by_file.shrink_to_fit();
        for candidates in self.constants_by_file.values_mut() {
            candidates.shrink_to_fit();
        }
        for candidates in self.methods_by_file.values_mut() {
            candidates.shrink_to_fit();
        }
        for candidates in self.resolved_by_file.values_mut() {
            candidates.shrink_to_fit();
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ReferenceStore {
    facts: HashMap<FqnId, Vec<ReferenceFact>>,
    targets_by_file: HashMap<SourceFileId, Vec<FqnId>>,
}

impl ReferenceStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, target: FqnId, fact: ReferenceFact) {
        let file_id = fact.range.file_id;
        let facts = self.facts.entry(target).or_default();
        facts.push(fact);
        self.targets_by_file
            .entry(file_id)
            .or_default()
            .push(target);
    }

    pub fn add_sorted(&mut self, target: FqnId, fact: ReferenceFact) {
        self.add(target, fact);
        self.sort_all();
    }

    pub fn sort_all(&mut self) {
        for facts in self.facts.values_mut() {
            sort_reference_facts(facts);
            facts.shrink_to_fit();
        }
        for targets in self.targets_by_file.values_mut() {
            targets.sort();
            targets.dedup();
            targets.shrink_to_fit();
        }
    }

    pub fn facts_for(&self, target: FqnId) -> &[ReferenceFact] {
        self.facts.get(&target).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn all_facts(&self) -> Vec<ReferenceFact> {
        self.facts
            .values()
            .flat_map(|facts| facts.iter().cloned())
            .collect()
    }

    pub fn iter_facts_with_targets(&self) -> impl Iterator<Item = (FqnId, &ReferenceFact)> {
        self.facts
            .iter()
            .flat_map(|(target, facts)| facts.iter().map(move |fact| (*target, fact)))
    }

    pub fn fact_count(&self) -> usize {
        self.facts.values().map(Vec::len).sum()
    }

    pub fn facts_in_file(&self, file_id: SourceFileId) -> Vec<ReferenceFact> {
        let Some(targets) = self.targets_by_file.get(&file_id) else {
            return Vec::new();
        };
        targets
            .iter()
            .filter_map(|target| self.facts.get(target))
            .flat_map(|facts| facts.iter())
            .filter(|fact| fact.range.file_id == file_id)
            .cloned()
            .collect()
    }

    pub fn facts_for_caller(&self, caller: FqnId) -> Vec<ReferenceFact> {
        self.facts
            .values()
            .flat_map(|facts| facts.iter())
            .filter(|fact| fact.caller == Some(caller))
            .cloned()
            .collect()
    }

    pub fn remove_file(&mut self, file_id: SourceFileId) {
        let Some(stale_targets) = self.targets_by_file.remove(&file_id) else {
            return;
        };
        for target in stale_targets {
            if let Some(facts) = self.facts.get_mut(&target) {
                facts.retain(|fact| fact.range.file_id != file_id);
                if facts.is_empty() {
                    self.facts.remove(&target);
                }
            }
        }
    }

    pub fn clear(&mut self) {
        self.facts.clear();
        self.targets_by_file.clear();
    }

    pub fn replace_file(
        &mut self,
        file_id: SourceFileId,
        facts: impl IntoIterator<Item = (FqnId, ReferenceFact)>,
    ) {
        self.remove_file(file_id);
        for (target, fact) in facts {
            assert!(
                fact.range.file_id == file_id,
                "INVARIANT VIOLATED: replacement reference fact belongs to a different file id. \
                 This is a bug because ReferenceStore::replace_file must only receive facts for the target file. \
                 Fix: partition facts by SourceFileId before replacing."
            );
            self.add(target, fact);
        }
        self.sort_all();
    }

    pub fn estimated_heap_bytes(&self) -> usize {
        map_table_bytes(&self.facts)
            + map_table_bytes(&self.targets_by_file)
            + self.facts.values().map(vec_payload_bytes).sum::<usize>()
            + self
                .targets_by_file
                .values()
                .map(vec_payload_bytes)
                .sum::<usize>()
    }

    pub fn shrink_to_fit(&mut self) {
        self.facts.shrink_to_fit();
        self.targets_by_file.shrink_to_fit();
        for facts in self.facts.values_mut() {
            facts.shrink_to_fit();
        }
        for targets in self.targets_by_file.values_mut() {
            targets.shrink_to_fit();
        }
    }
}

fn sort_reference_facts(facts: &mut [ReferenceFact]) {
    facts.sort_by_key(|fact| {
        (
            fact.range.file_id,
            fact.range.start_byte,
            fact.range.end_byte,
        )
    });
}

fn method_reference_candidate_heap_bytes(candidate: &StoredMethodReferenceCandidate) -> usize {
    candidate
        .diagnostics
        .as_deref()
        .map(method_reference_diagnostics_heap_bytes)
        .unwrap_or(0)
}

fn method_reference_diagnostics_heap_bytes(diagnostics: &MethodReferenceDiagnostics) -> usize {
    diagnostics
        .receiver_label
        .as_ref()
        .map(String::capacity)
        .unwrap_or(0)
        + vec_payload_bytes(&diagnostics.signature.keyword_args)
        + diagnostics
            .signature
            .keyword_args
            .iter()
            .map(|arg| arg.name.capacity())
            .sum::<usize>()
}

#[cfg(test)]
mod tests {
    use crate::{FqnId, SourceFileId, TextRange};

    use super::*;

    fn file() -> SourceFileId {
        SourceFileId(1)
    }

    #[test]
    fn replace_file_removes_stale_reference_facts_for_same_file_only() {
        let target = FqnId(1);
        let mut store = ReferenceStore::new();
        store.add(
            target,
            ReferenceFact::new(TextRange::new(file(), 0, 4), None),
        );
        store.add(
            target,
            ReferenceFact::new(TextRange::new(SourceFileId(2), 0, 4), None),
        );

        store.replace_file(
            file(),
            [(
                target,
                ReferenceFact::new(TextRange::new(file(), 10, 14), None),
            )],
        );

        let facts = store.facts_for(target);
        assert_eq!(facts.len(), 2);
        assert!(facts
            .iter()
            .any(|fact| fact.range.file_id == file() && fact.range.start_byte == 10));
        assert!(facts
            .iter()
            .any(|fact| fact.range.file_id == SourceFileId(2)));
    }
}
