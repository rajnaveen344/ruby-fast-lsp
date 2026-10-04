//! Reference candidates as producers emit them, and the interned form the
//! engine hands to [`super::ReferenceCandidateStore`].

use ustr::Ustr;

use super::method_row::StoredMethodReferenceCandidate;
use super::{ConstantPath, MethodReferenceAccess};
use crate::core::names::fqn_id::{ConstLookupId, FqnId, OptionalFqnId};
use crate::core::storage::file_owned::FileRow;
use crate::core::{
    FullyQualifiedName, NamespaceKind, RubyConstant, RubyMethod, RubyType, SourceFileId, TextRange,
};

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

/// How an unresolved-method diagnostic names the receiver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MethodReceiverLabel {
    /// Identifier text written at the call site, such as a constant path or
    /// `super`.
    Written(String),
    /// The display form of the candidate's own `receiver_type`.
    ReceiverType,
    /// Rendered type text that differs from the stored receiver type.
    Rendered(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodReferenceDiagnostics {
    pub diagnostic_range: TextRange,
    pub receiver_label: Option<MethodReceiverLabel>,
    /// The exact nested call whose proven result is the receiver for this
    /// dispatch. The engine resolves this dependency after all file-owned
    /// facts are installed; an absent or unproven result keeps the outer call
    /// Unknown instead of guessing from syntax.
    pub receiver_expression_range: Option<TextRange>,
    /// The collector's statically proven receiver type. Engine finalization
    /// revalidates expression receivers against complete flow evidence before
    /// using this fallback; unions resolve as one fail-closed dispatch group.
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
    pub name: Ustr,
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
pub struct StoredResolvedReferenceCandidate {
    pub range: TextRange,
    pub target: FqnId,
    caller: OptionalFqnId,
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
    Method(StoredMethodReferenceCandidate),
    Resolved {
        target: FqnId,
        caller: Option<FqnId>,
    },
}

impl StoredResolvedReferenceCandidate {
    pub(super) fn new(range: TextRange, target: FqnId, caller: Option<FqnId>) -> Self {
        Self {
            range,
            target,
            caller: caller.into(),
        }
    }

    pub fn caller(&self) -> Option<FqnId> {
        self.caller.get()
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
