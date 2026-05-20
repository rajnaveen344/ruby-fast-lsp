use std::collections::HashMap;

use crate::{RubyConstant, RubyMethod, RubyType, SourceFileId, TextRange};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticCandidate {
    pub range: TextRange,
    pub kind: DiagnosticCandidateKind,
}

impl DiagnosticCandidate {
    pub fn new(range: TextRange, kind: DiagnosticCandidateKind) -> Self {
        Self { range, kind }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticCandidateKind {
    RaiseNonException {
        arg_repr: String,
        arg: RaiseArgCandidate,
    },
    BadSplat {
        operator: String,
        arg_repr: String,
        expected: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RaiseArgCandidate {
    StringLiteral,
    NonExceptionLiteral,
    Constant(String),
    Type(RubyType),
    BareMethodReturn {
        current_namespace: Vec<RubyConstant>,
        method: RubyMethod,
    },
    Unknown,
}

#[derive(Debug, Clone, Default)]
pub struct DiagnosticCandidateStore {
    candidates_by_file: HashMap<SourceFileId, Vec<DiagnosticCandidate>>,
}

impl DiagnosticCandidateStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn replace_file(
        &mut self,
        file_id: SourceFileId,
        candidates: impl IntoIterator<Item = DiagnosticCandidate>,
    ) {
        self.candidates_by_file.remove(&file_id);
        for candidate in candidates {
            assert!(
                candidate.range.file_id == file_id,
                "INVARIANT VIOLATED: replacement diagnostic candidate belongs to a different file id. \
                 This is a bug because DiagnosticCandidateStore::replace_file must only receive candidates for the target file. \
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
                    diagnostic_candidate_rank(&candidate.kind),
                )
            });
        }
    }

    pub fn all_candidates(&self) -> Vec<DiagnosticCandidate> {
        self.candidates_by_file
            .values()
            .flat_map(|candidates| candidates.iter().cloned())
            .collect()
    }

    pub fn file_ids(&self) -> Vec<SourceFileId> {
        self.candidates_by_file.keys().copied().collect()
    }
}

fn diagnostic_candidate_rank(kind: &DiagnosticCandidateKind) -> u8 {
    match kind {
        DiagnosticCandidateKind::RaiseNonException { .. } => 0,
        DiagnosticCandidateKind::BadSplat { .. } => 1,
    }
}
