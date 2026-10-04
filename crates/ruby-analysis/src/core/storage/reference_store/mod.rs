mod candidate;
mod method_row;
#[cfg(test)]
mod tests;

use std::collections::HashMap;

use crate::core::names::fqn_id::{FqnId, OptionalFqnId};
use crate::core::storage::file_owned::FileOwned;
use crate::core::storage::memory_estimate::{map_table_bytes, vec_payload_bytes};
use crate::core::{RubyConstant, RubyMethod, SourceFileId, TextRange};
use smallvec::SmallVec;

pub use candidate::{
    KeywordArgCandidate, MethodCallSignatureCandidate, MethodReceiverLabel,
    MethodReferenceCandidate, MethodReferenceDiagnostics, ReferenceCandidate,
    ReferenceCandidateKind, StoredConstantReferenceCandidate, StoredReferenceCandidate,
    StoredReferenceCandidateKind, StoredReferenceCandidateRef, StoredResolvedReferenceCandidate,
};
pub use method_row::{
    MethodCallDiagnostics, MethodCallSignature, StoredMethodReferenceCandidate,
    StoredReceiverLabel, StoredReceiverType,
};

pub type ConstantPath = SmallVec<[RubyConstant; 4]>;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ConstLookup {
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
    caller: OptionalFqnId,
    pub access: MethodReferenceAccess,
}

impl ReferenceFact {
    pub(crate) fn new(range: TextRange, caller: Option<FqnId>) -> Self {
        Self::method(range, caller, MethodReferenceAccess::Normal)
    }

    pub(crate) fn method(
        range: TextRange,
        caller: Option<FqnId>,
        access: MethodReferenceAccess,
    ) -> Self {
        Self {
            range,
            caller: caller.into(),
            access,
        }
    }

    pub(crate) fn caller(&self) -> Option<FqnId> {
        self.caller.get()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MethodReferenceAccess {
    Normal,
    ExplicitReceiver,
    VisibilityBypass,
    /// `Module#instance_method` inspects this namespace's own ancestor chain.
    InstanceMethodReflection,
}

#[derive(Debug, Clone, Default)]
pub struct ReferenceCandidateStore {
    constants: FileOwned<StoredConstantReferenceCandidate>,
    methods: FileOwned<StoredMethodReferenceCandidate>,
    resolved: FileOwned<StoredResolvedReferenceCandidate>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReferenceCandidateStats {
    pub constants: usize,
    pub methods: usize,
    pub resolved: usize,
}

/// Candidates of one kind are ordered by their source range.
fn by_range<T>(range: impl Fn(&T) -> TextRange) -> impl Fn(&T, &T) -> std::cmp::Ordering {
    move |left, right| {
        let (left, right) = (range(left), range(right));
        (left.start_byte, left.end_byte).cmp(&(right.start_byte, right.end_byte))
    }
}

impl ReferenceCandidateStore {
    pub fn remove_file(&mut self, file_id: SourceFileId) {
        self.constants.remove(file_id);
        self.methods.remove(file_id);
        self.resolved.remove(file_id);
    }

    pub fn replace_file(
        &mut self,
        file_id: SourceFileId,
        candidates: impl IntoIterator<Item = StoredReferenceCandidate>,
    ) {
        let mut constants = Vec::new();
        let mut methods = Vec::new();
        let mut resolved = Vec::new();
        for candidate in candidates {
            match candidate.kind {
                StoredReferenceCandidateKind::Constant { lookup } => {
                    constants.push(StoredConstantReferenceCandidate {
                        range: candidate.range,
                        lookup,
                    })
                }
                StoredReferenceCandidateKind::Method(row) => methods.push(row),
                StoredReferenceCandidateKind::Resolved { target, caller } => resolved.push(
                    StoredResolvedReferenceCandidate::new(candidate.range, target, caller),
                ),
            }
        }
        self.constants.replace(
            file_id,
            constants,
            by_range(|candidate: &StoredConstantReferenceCandidate| candidate.range),
        );
        self.methods.replace(
            file_id,
            methods,
            by_range(|candidate: &StoredMethodReferenceCandidate| candidate.range),
        );
        self.resolved.replace(
            file_id,
            resolved,
            by_range(|candidate: &StoredResolvedReferenceCandidate| candidate.range),
        );
    }

    pub(crate) fn method_candidates_in_file(
        &self,
        file_id: SourceFileId,
    ) -> impl Iterator<Item = &StoredMethodReferenceCandidate> {
        self.methods.rows(file_id).iter()
    }

    pub fn method_candidates_named(
        &self,
        method: RubyMethod,
    ) -> impl Iterator<Item = &StoredMethodReferenceCandidate> {
        self.methods
            .iter()
            .filter(move |candidate| candidate.method() == method)
    }

    pub fn candidates_in_file(&self, file_id: SourceFileId) -> Vec<StoredReferenceCandidate> {
        let mut candidates = Vec::new();
        candidates.extend(self.constants.rows(file_id).iter().map(|candidate| {
            StoredReferenceCandidate {
                range: candidate.range,
                kind: StoredReferenceCandidateKind::Constant {
                    lookup: candidate.lookup,
                },
            }
        }));
        candidates.extend(self.methods.rows(file_id).iter().map(|candidate| {
            StoredReferenceCandidate {
                range: candidate.range,
                kind: StoredReferenceCandidateKind::Method(candidate.clone()),
            }
        }));
        candidates.extend(self.resolved.rows(file_id).iter().map(|candidate| {
            StoredReferenceCandidate {
                range: candidate.range,
                kind: StoredReferenceCandidateKind::Resolved {
                    target: candidate.target,
                    caller: candidate.caller(),
                },
            }
        }));
        candidates.sort_by_key(|candidate| (candidate.range.start_byte, candidate.range.end_byte));
        candidates
    }

    pub fn iter_candidates(&self) -> impl Iterator<Item = StoredReferenceCandidateRef<'_>> {
        self.constants
            .iter()
            .map(StoredReferenceCandidateRef::Constant)
            .chain(self.methods.iter().map(StoredReferenceCandidateRef::Method))
            .chain(
                self.resolved
                    .iter()
                    .map(StoredReferenceCandidateRef::Resolved),
            )
    }

    pub(crate) fn method_candidates_at_exact_range(
        &self,
        range: TextRange,
    ) -> &[StoredMethodReferenceCandidate] {
        let candidates = self.methods.rows(range.file_id);
        let key = (range.start_byte, range.end_byte);
        let start = candidates.partition_point(|candidate| {
            (candidate.range.start_byte, candidate.range.end_byte) < key
        });
        let end = candidates.partition_point(|candidate| {
            (candidate.range.start_byte, candidate.range.end_byte) <= key
        });
        &candidates[start..end]
    }

    pub fn candidate_count(&self) -> usize {
        self.constants.len() + self.methods.len() + self.resolved.len()
    }

    pub fn stats(&self) -> ReferenceCandidateStats {
        ReferenceCandidateStats {
            constants: self.constants.len(),
            methods: self.methods.len(),
            resolved: self.resolved.len(),
        }
    }

    pub fn file_ids(&self) -> Vec<SourceFileId> {
        let mut file_ids = self
            .constants
            .files()
            .chain(self.methods.files())
            .chain(self.resolved.files())
            .collect::<Vec<_>>();
        file_ids.sort();
        file_ids.dedup();
        file_ids
    }

    pub fn estimated_heap_bytes(&self) -> usize {
        self.constants.estimated_heap_bytes(|_| 0)
            + self
                .methods
                .estimated_heap_bytes(StoredMethodReferenceCandidate::heap_bytes)
            + self.resolved.estimated_heap_bytes(|_| 0)
    }

    pub fn shrink_to_fit(&mut self) {
        self.constants.shrink_to_fit();
        self.methods.shrink_to_fit();
        self.resolved.shrink_to_fit();
    }
}

#[derive(Debug, Clone, Default)]
pub struct ReferenceStore {
    facts: HashMap<FqnId, Vec<ReferenceFact>>,
    targets_by_file: HashMap<SourceFileId, Vec<FqnId>>,
}

impl ReferenceStore {
    pub fn add(&mut self, target: FqnId, fact: ReferenceFact) {
        let file_id = fact.range.file_id;
        let facts = self.facts.entry(target).or_default();
        facts.push(fact);
        self.targets_by_file
            .entry(file_id)
            .or_default()
            .push(target);
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

    /// Return every semantic target resolved for one exact source range.
    ///
    /// A grouped receiver dispatch can intentionally resolve one call range
    /// to multiple method identities. Keeping this reverse lookup in the
    /// resolved store lets navigation consume the same proof as references,
    /// diagnostics, hover, and the check CLI instead of recomputing a receiver
    /// from syntax.
    pub fn targets_for_exact_range(&self, range: TextRange) -> Vec<FqnId> {
        let Some(targets) = self.targets_by_file.get(&range.file_id) else {
            return Vec::new();
        };
        targets
            .iter()
            .copied()
            .filter(|target| {
                self.facts
                    .get(target)
                    .is_some_and(|facts| facts.iter().any(|fact| fact.range == range))
            })
            .collect()
    }

    pub fn remove_file(&mut self, file_id: SourceFileId) {
        self.remove_file_targets(file_id);
    }

    /// Remove `file_id`'s facts and return the targets they referenced.
    /// Retaining keeps every remaining list sorted.
    fn remove_file_targets(&mut self, file_id: SourceFileId) -> Vec<FqnId> {
        let Some(stale_targets) = self.targets_by_file.remove(&file_id) else {
            return Vec::new();
        };
        for target in &stale_targets {
            if let Some(facts) = self.facts.get_mut(target) {
                facts.retain(|fact| fact.range.file_id != file_id);
                if facts.is_empty() {
                    self.facts.remove(target);
                }
            }
        }
        stale_targets
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
        let mut touched = self.remove_file_targets(file_id);
        for (target, fact) in facts {
            invariant!(
                fact.range.file_id == file_id,
                what = "replacement reference fact belongs to a different file id",
                why = "ReferenceStore::replace_file must only receive facts for the target file",
                fix = "partition facts by SourceFileId before replacing",
            );
            self.add(target, fact);
        }
        // Only the targets this file referenced before or now changed; every
        // other list is still sorted and compact.
        if let Some(targets) = self.targets_by_file.get_mut(&file_id) {
            targets.sort_unstable();
            targets.dedup();
            targets.shrink_to_fit();
            touched.extend_from_slice(targets);
        }
        touched.sort_unstable();
        touched.dedup();
        for target in touched {
            if let Some(facts) = self.facts.get_mut(&target) {
                sort_reference_facts(facts);
                facts.shrink_to_fit();
            }
        }
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
