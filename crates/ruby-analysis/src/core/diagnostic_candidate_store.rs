use std::collections::HashMap;

use super::memory_estimate::{
    map_table_bytes, ruby_type_heap_bytes, string_heap_bytes, vec_payload_bytes,
};
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

    pub fn candidates_in_file(&self, file_id: SourceFileId) -> Vec<DiagnosticCandidate> {
        self.candidates_by_file
            .get(&file_id)
            .cloned()
            .unwrap_or_default()
    }

    pub fn iter_candidates(&self) -> impl Iterator<Item = &DiagnosticCandidate> {
        self.candidates_by_file
            .values()
            .flat_map(|candidates| candidates.iter())
    }

    pub fn candidate_count(&self) -> usize {
        self.candidates_by_file.values().map(Vec::len).sum()
    }

    pub fn file_ids(&self) -> Vec<SourceFileId> {
        self.candidates_by_file.keys().copied().collect()
    }

    pub fn estimated_heap_bytes(&self) -> usize {
        map_table_bytes(&self.candidates_by_file)
            + self
                .candidates_by_file
                .values()
                .map(|candidates| {
                    vec_payload_bytes(candidates)
                        + candidates
                            .iter()
                            .map(diagnostic_candidate_heap_bytes)
                            .sum::<usize>()
                })
                .sum::<usize>()
    }

    pub fn shrink_to_fit(&mut self) {
        self.candidates_by_file.shrink_to_fit();
        for candidates in self.candidates_by_file.values_mut() {
            candidates.shrink_to_fit();
        }
    }
}

fn diagnostic_candidate_rank(kind: &DiagnosticCandidateKind) -> u8 {
    match kind {
        DiagnosticCandidateKind::RaiseNonException { .. } => 0,
        DiagnosticCandidateKind::BadSplat { .. } => 1,
    }
}

fn diagnostic_candidate_heap_bytes(candidate: &DiagnosticCandidate) -> usize {
    match &candidate.kind {
        DiagnosticCandidateKind::RaiseNonException { arg_repr, arg } => {
            string_heap_bytes(arg_repr) + raise_arg_heap_bytes(arg)
        }
        DiagnosticCandidateKind::BadSplat {
            operator,
            arg_repr,
            expected,
        } => {
            string_heap_bytes(operator) + string_heap_bytes(arg_repr) + string_heap_bytes(expected)
        }
    }
}

fn raise_arg_heap_bytes(arg: &RaiseArgCandidate) -> usize {
    match arg {
        RaiseArgCandidate::StringLiteral
        | RaiseArgCandidate::NonExceptionLiteral
        | RaiseArgCandidate::Unknown => 0,
        RaiseArgCandidate::Constant(name) => string_heap_bytes(name),
        RaiseArgCandidate::Type(ruby_type) => ruby_type_heap_bytes(ruby_type),
        RaiseArgCandidate::BareMethodReturn {
            current_namespace, ..
        } => vec_payload_bytes(current_namespace),
    }
}
