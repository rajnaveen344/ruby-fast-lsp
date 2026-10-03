use crate::core::storage::file_owned::{FileOwned, FileRow};
use crate::core::storage::memory_estimate::{
    ruby_type_heap_bytes, string_heap_bytes, vec_payload_bytes,
};
use crate::core::{RubyConstant, RubyMethod, RubyType, SourceFileId, TextRange};

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
    /// A local receiver whose exact read proof is checked after flow resolution.
    NilCall {
        local_read: TextRange,
        variable: String,
        method: String,
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
    /// Exact local-variable read to validate only after the shared flow solver
    /// has installed a proven read type. Missing evidence remains Unknown.
    LocalRead(TextRange),
    Unknown,
}

impl FileRow for DiagnosticCandidate {
    fn file_id(&self) -> SourceFileId {
        self.range.file_id
    }
}

#[derive(Debug, Clone, Default)]
pub struct DiagnosticCandidateStore {
    candidates: FileOwned<DiagnosticCandidate>,
}

impl DiagnosticCandidateStore {
    pub fn remove_file(&mut self, file_id: SourceFileId) {
        self.candidates.remove(file_id);
    }

    pub fn replace_file(
        &mut self,
        file_id: SourceFileId,
        candidates: impl IntoIterator<Item = DiagnosticCandidate>,
    ) {
        self.candidates.replace(file_id, candidates, |left, right| {
            let key = |candidate: &DiagnosticCandidate| {
                (
                    candidate.range.start_byte,
                    candidate.range.end_byte,
                    diagnostic_candidate_rank(&candidate.kind),
                )
            };
            key(left).cmp(&key(right))
        });
    }

    pub fn candidates_in_file(&self, file_id: SourceFileId) -> Vec<DiagnosticCandidate> {
        self.candidates.rows(file_id).to_vec()
    }

    pub fn iter_candidates(&self) -> impl Iterator<Item = &DiagnosticCandidate> {
        self.candidates.iter()
    }

    pub fn candidate_count(&self) -> usize {
        self.candidates.len()
    }

    pub fn file_ids(&self) -> Vec<SourceFileId> {
        self.candidates.files().collect()
    }

    pub fn estimated_heap_bytes(&self) -> usize {
        self.candidates
            .estimated_heap_bytes(diagnostic_candidate_heap_bytes)
    }

    pub fn shrink_to_fit(&mut self) {
        self.candidates.shrink_to_fit();
    }
}

fn diagnostic_candidate_rank(kind: &DiagnosticCandidateKind) -> u8 {
    match kind {
        DiagnosticCandidateKind::RaiseNonException { .. } => 0,
        DiagnosticCandidateKind::BadSplat { .. } => 1,
        DiagnosticCandidateKind::NilCall { .. } => 2,
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
        DiagnosticCandidateKind::NilCall {
            variable, method, ..
        } => string_heap_bytes(variable) + string_heap_bytes(method),
    }
}

fn raise_arg_heap_bytes(arg: &RaiseArgCandidate) -> usize {
    match arg {
        RaiseArgCandidate::StringLiteral
        | RaiseArgCandidate::NonExceptionLiteral
        | RaiseArgCandidate::LocalRead(_)
        | RaiseArgCandidate::Unknown => 0,
        RaiseArgCandidate::Constant(name) => string_heap_bytes(name),
        RaiseArgCandidate::Type(ruby_type) => ruby_type_heap_bytes(ruby_type),
        RaiseArgCandidate::BareMethodReturn {
            current_namespace, ..
        } => vec_payload_bytes(current_namespace),
    }
}
