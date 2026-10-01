use std::collections::HashMap;
use std::mem::size_of;

use crate::core::names::fqn_id::ConstLookupId;
use crate::core::names::fqn_id::FqnId;
use crate::core::storage::file_owned::{FileOwned, FileRow};
use crate::core::storage::memory_estimate::{
    map_table_bytes, ruby_type_heap_bytes, vec_payload_bytes,
};
use crate::core::{
    FullyQualifiedName, NamespaceKind, RubyConstant, RubyMethod, RubyType, SourceFileId, TextRange,
};
use smallvec::SmallVec;

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
    pub(crate) caller: Option<FqnId>,
    pub access: MethodReferenceAccess,
}

impl ReferenceFact {
    pub(crate) fn new(range: TextRange, caller: Option<FqnId>) -> Self {
        Self {
            range,
            caller,
            access: MethodReferenceAccess::Normal,
        }
    }

    pub(crate) fn method(
        range: TextRange,
        caller: Option<FqnId>,
        access: MethodReferenceAccess,
    ) -> Self {
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
    /// `Module#instance_method` inspects this namespace's own ancestor chain.
    InstanceMethodReflection,
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
        call_expression_range: Option<TextRange>,
        preferred_definition_range: Option<TextRange>,
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
    /// At least one statically present entry exists in the trailing keyword
    /// hash. For methods without keyword parameters Ruby can pass that syntax
    /// as one positional options hash. This is deliberately distinct from a
    /// keyword splat, whose runtime hash may be empty.
    pub has_nonempty_keyword_hash: bool,
    /// The final positional argument is either proven to be a Hash literal or
    /// has a shape whose value type is not statically known. On Ruby versions
    /// where an options hash can satisfy keyword parameters, required-keyword
    /// diagnostics are therefore inconclusive.
    pub trailing_positional_may_be_options_hash: bool,
    pub keyword_args: Vec<KeywordArgCandidate>,
    pub has_keyword_splat: bool,
}

impl MethodCallSignatureCandidate {
    pub fn is_empty(&self) -> bool {
        self.positional_count == 0
            && !self.has_positional_splat
            && !self.has_nonempty_keyword_hash
            && !self.trailing_positional_may_be_options_hash
            && self.keyword_args.is_empty()
            && !self.has_keyword_splat
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodReferenceDiagnostics {
    pub diagnostic_range: TextRange,
    pub receiver_label: Option<String>,
    /// The exact nested call whose proven result is the receiver for this
    /// dispatch. The engine resolves this dependency after all file-owned
    /// facts are installed; an absent or unproven result keeps the outer call
    /// Unknown instead of guessing from syntax.
    pub receiver_expression_range: Option<TextRange>,
    /// The collector's statically proven receiver type. Engine finalization
    /// revalidates expression receivers against complete flow evidence before
    /// using this fallback; unions resolve as one fail-closed dispatch group.
    /// Kept behind the candidate's existing metadata box so ordinary method
    /// reference candidates retain their compact stored layout.
    pub receiver_type: Option<Box<RubyType>>,
    pub diagnose_unresolved: bool,
    pub allow_unindexed_owner: bool,
    /// The call uses `&.`: a nil receiver skips dispatch and the call yields
    /// nil, so only the non-nil part of `receiver_type` is resolved.
    pub safe_navigation: bool,
    /// Present only when this reference is an invocation with a statically
    /// known call shape. Method objects, aliases, and other references are not
    /// zero-argument calls and therefore retain `None`.
    pub signature: Option<MethodCallSignatureCandidate>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodReferenceCandidate {
    pub owner: Vec<RubyConstant>,
    pub owner_kind: NamespaceKind,
    pub method: RubyMethod,
    pub is_super: bool,
    pub access: MethodReferenceAccess,
    pub caller: Option<FullyQualifiedName>,
    pub call_expression_range: Option<TextRange>,
    pub preferred_definition_range: Option<TextRange>,
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
    pub call_expression_range: Option<TextRange>,
    pub preferred_definition_range: Option<TextRange>,
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
        call_expression_range: Option<TextRange>,
        preferred_definition_range: Option<TextRange>,
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
        call_expression_range: Option<TextRange>,
        preferred_definition_range: Option<TextRange>,
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
                call_expression_range,
                preferred_definition_range,
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
        invariant!(
            !parts.is_empty(),
            what = "constant reference candidate has no parts",
            why = "constant resolution requires at least one constant name",
            fix = "skip empty constant paths before constructing ReferenceCandidate",
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
        if let Some(expression_range) = candidate.call_expression_range {
            invariant!(
                expression_range.file_id == reference_range.file_id
                    && expression_range.start_byte <= reference_range.start_byte
                    && expression_range.end_byte >= reference_range.end_byte,
                what = "method reference range is outside its call expression",
                why = "call-type finalization updates the owning AST call",
                fix = "attach the enclosing CallNode range to the candidate",
            );
        }
        Self {
            range: reference_range,
            kind: ReferenceCandidateKind::Method {
                owner: ConstantPath::from_vec(candidate.owner),
                owner_kind: candidate.owner_kind,
                method: candidate.method,
                is_super: candidate.is_super,
                access: candidate.access,
                caller: candidate.caller,
                call_expression_range: candidate.call_expression_range,
                preferred_definition_range: candidate.preferred_definition_range,
                diagnostics: Some(Box::new(candidate.diagnostics)),
            },
        }
    }

    pub fn method_target(
        reference_range: TextRange,
        owner: Vec<RubyConstant>,
        owner_kind: NamespaceKind,
        method: RubyMethod,
        caller: Option<FullyQualifiedName>,
    ) -> Self {
        invariant!(
            !owner.is_empty(),
            what = "exact method reference target has no owner namespace",
            why = "method resolution requires a concrete owner",
            fix = "validate extension method targets before constructing ReferenceCandidate",
        );
        Self {
            range: reference_range,
            kind: ReferenceCandidateKind::Method {
                owner: ConstantPath::from_vec(owner),
                owner_kind,
                method,
                is_super: false,
                access: MethodReferenceAccess::Normal,
                caller,
                call_expression_range: None,
                preferred_definition_range: None,
                diagnostics: None,
            },
        }
    }
}

impl FileRow for StoredConstantReferenceCandidate {
    fn file_id(&self) -> SourceFileId {
        self.range.file_id
    }
}

impl FileRow for StoredMethodReferenceCandidate {
    fn file_id(&self) -> SourceFileId {
        self.range.file_id
    }
}

impl FileRow for StoredResolvedReferenceCandidate {
    fn file_id(&self) -> SourceFileId {
        self.range.file_id
    }
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
                StoredReferenceCandidateKind::Method {
                    owner,
                    owner_kind,
                    method,
                    is_super,
                    access,
                    caller,
                    call_expression_range,
                    preferred_definition_range,
                    diagnostics,
                } => methods.push(StoredMethodReferenceCandidate {
                    range: candidate.range,
                    owner,
                    owner_kind,
                    method,
                    is_super,
                    access,
                    caller,
                    call_expression_range,
                    preferred_definition_range,
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
            .filter(move |candidate| candidate.method == method)
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
                kind: StoredReferenceCandidateKind::Method {
                    owner: candidate.owner,
                    owner_kind: candidate.owner_kind,
                    method: candidate.method,
                    is_super: candidate.is_super,
                    access: candidate.access,
                    caller: candidate.caller,
                    call_expression_range: candidate.call_expression_range,
                    preferred_definition_range: candidate.preferred_definition_range,
                    diagnostics: candidate.diagnostics.clone(),
                },
            }
        }));
        candidates.extend(self.resolved.rows(file_id).iter().map(|candidate| {
            StoredReferenceCandidate {
                range: candidate.range,
                kind: StoredReferenceCandidateKind::Resolved {
                    target: candidate.target,
                    caller: candidate.caller,
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
                .estimated_heap_bytes(method_reference_candidate_heap_bytes)
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
            invariant!(
                fact.range.file_id == file_id,
                what = "replacement reference fact belongs to a different file id",
                why = "ReferenceStore::replace_file must only receive facts for the target file",
                fix = "partition facts by SourceFileId before replacing",
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
        + diagnostics
            .receiver_type
            .as_deref()
            .map(|ruby_type| size_of::<RubyType>() + ruby_type_heap_bytes(ruby_type))
            .unwrap_or(0)
        + diagnostics
            .signature
            .as_ref()
            .map(|signature| {
                vec_payload_bytes(&signature.keyword_args)
                    + signature
                        .keyword_args
                        .iter()
                        .map(|arg| arg.name.capacity())
                        .sum::<usize>()
            })
            .unwrap_or(0)
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
    fn replace_file_removes_stale_reference_facts_for_same_file_only() {
        let target = FqnId(1);
        let mut store = ReferenceStore::default();
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
