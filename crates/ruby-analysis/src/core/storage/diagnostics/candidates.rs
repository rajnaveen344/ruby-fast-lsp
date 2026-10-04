use std::mem::size_of;

use ustr::Ustr;

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

/// Candidates are kept for every indexed file, and nil-call candidates are
/// emitted for every local-receiver call, so the common variant stays inline
/// and the rarer `raise` payload lives behind one box.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticCandidateKind {
    RaiseNonException(Box<RaiseCandidate>),
    BadSplat {
        operator: SplatOperator,
        arg_repr: Box<str>,
    },
    /// A local receiver whose exact read proof is checked after flow resolution.
    NilCall {
        local_read: TextRange,
        variable: Ustr,
        method: Ustr,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaiseCandidate {
    pub arg_repr: Box<str>,
    pub arg: RaiseArgCandidate,
}

/// The splat form at a call site; the expected container follows from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplatOperator {
    /// `*expr`, which expects an Array.
    Positional,
    /// `**expr`, which expects a Hash.
    Keyword,
}

impl SplatOperator {
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Positional => "*",
            Self::Keyword => "**",
        }
    }

    pub fn expected(self) -> &'static str {
        match self {
            Self::Positional => "Array",
            Self::Keyword => "Hash",
        }
    }
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
        DiagnosticCandidateKind::RaiseNonException(raise) => {
            size_of::<RaiseCandidate>() + raise.arg_repr.len() + raise_arg_heap_bytes(&raise.arg)
        }
        DiagnosticCandidateKind::BadSplat { arg_repr, .. } => arg_repr.len(),
        // Interned names are shared process-wide.
        DiagnosticCandidateKind::NilCall { .. } => 0,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn range(start: u32, end: u32) -> TextRange {
        TextRange::new(SourceFileId(1), start, end)
    }

    #[test]
    fn nil_call_candidate_is_compact_and_owns_no_heap() {
        let candidate = DiagnosticCandidate::new(
            range(4, 7),
            DiagnosticCandidateKind::NilCall {
                local_read: range(0, 3),
                variable: Ustr::from("value"),
                method: Ustr::from("upcase"),
            },
        );
        assert!(size_of::<DiagnosticCandidate>() <= 48);
        assert_eq!(diagnostic_candidate_heap_bytes(&candidate), 0);
    }

    #[test]
    fn splat_operator_names_its_symbol_and_expected_container() {
        assert_eq!(SplatOperator::Positional.symbol(), "*");
        assert_eq!(SplatOperator::Positional.expected(), "Array");
        assert_eq!(SplatOperator::Keyword.symbol(), "**");
        assert_eq!(SplatOperator::Keyword.expected(), "Hash");
    }

    #[test]
    fn rare_payload_heap_counts_its_box_and_text() {
        let raise = DiagnosticCandidate::new(
            range(0, 2),
            DiagnosticCandidateKind::RaiseNonException(Box::new(RaiseCandidate {
                arg_repr: "42".into(),
                arg: RaiseArgCandidate::NonExceptionLiteral,
            })),
        );
        assert_eq!(
            diagnostic_candidate_heap_bytes(&raise),
            size_of::<RaiseCandidate>() + 2
        );
        let splat = DiagnosticCandidate::new(
            range(0, 6),
            DiagnosticCandidateKind::BadSplat {
                operator: SplatOperator::Keyword,
                arg_repr: "value".into(),
            },
        );
        assert_eq!(diagnostic_candidate_heap_bytes(&splat), 5);
    }
}
