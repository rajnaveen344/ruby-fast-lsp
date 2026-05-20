use std::collections::HashMap;

use crate::{FullyQualifiedName, NamespaceKind, RubyConstant, RubyMethod, SourceFileId, TextRange};

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReferenceCandidateKind {
    Constant {
        parts: Vec<RubyConstant>,
        current_namespace: Vec<RubyConstant>,
        name: String,
    },
    Method {
        owner: Vec<RubyConstant>,
        owner_kind: NamespaceKind,
        method: RubyMethod,
        caller: Option<FullyQualifiedName>,
        diagnostic_range: TextRange,
        receiver_label: Option<String>,
        diagnose_unresolved: bool,
        signature: MethodCallSignatureCandidate,
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

impl ReferenceCandidate {
    pub fn constant(
        range: TextRange,
        parts: Vec<RubyConstant>,
        current_namespace: Vec<RubyConstant>,
        name: impl Into<String>,
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
                parts,
                current_namespace,
                name: name.into(),
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

    pub fn method(
        reference_range: TextRange,
        owner: Vec<RubyConstant>,
        owner_kind: NamespaceKind,
        method: RubyMethod,
        caller: Option<FullyQualifiedName>,
        diagnostic_range: TextRange,
        receiver_label: Option<String>,
        diagnose_unresolved: bool,
        signature: MethodCallSignatureCandidate,
    ) -> Self {
        Self {
            range: reference_range,
            kind: ReferenceCandidateKind::Method {
                owner,
                owner_kind,
                method,
                caller,
                diagnostic_range,
                receiver_label,
                diagnose_unresolved,
                signature,
            },
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ReferenceCandidateStore {
    candidates_by_file: HashMap<SourceFileId, Vec<ReferenceCandidate>>,
}

impl ReferenceCandidateStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn replace_file(
        &mut self,
        file_id: SourceFileId,
        candidates: impl IntoIterator<Item = ReferenceCandidate>,
    ) {
        self.candidates_by_file.remove(&file_id);
        for candidate in candidates {
            assert!(
                candidate.range.file_id == file_id,
                "INVARIANT VIOLATED: replacement reference candidate belongs to a different file id. \
                 This is a bug because ReferenceCandidateStore::replace_file must only receive candidates for the target file. \
                 Fix: partition candidates by SourceFileId before replacing."
            );
            self.candidates_by_file
                .entry(file_id)
                .or_default()
                .push(candidate);
        }
        if let Some(candidates) = self.candidates_by_file.get_mut(&file_id) {
            candidates.sort_by_key(|candidate| {
                (
                    candidate.range.start_byte,
                    candidate.range.end_byte,
                    reference_candidate_rank(&candidate.kind),
                )
            });
        }
    }

    pub fn all_candidates(&self) -> Vec<ReferenceCandidate> {
        self.candidates_by_file
            .values()
            .flat_map(|candidates| candidates.iter().cloned())
            .collect()
    }

    pub fn file_ids(&self) -> Vec<SourceFileId> {
        self.candidates_by_file.keys().copied().collect()
    }
}

fn reference_candidate_rank(kind: &ReferenceCandidateKind) -> u8 {
    match kind {
        ReferenceCandidateKind::Constant { .. } => 0,
        ReferenceCandidateKind::Method { .. } => 1,
        ReferenceCandidateKind::Resolved { .. } => 2,
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

    pub fn clear(&mut self) {
        self.facts.clear();
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
