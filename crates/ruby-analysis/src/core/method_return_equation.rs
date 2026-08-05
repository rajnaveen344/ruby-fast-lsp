//! File-owned proof equations for inferred method returns.
//!
//! The parser-facing traversal emits this compact, editor-independent IR. A
//! solver may combine equations from one file or a complete project without
//! retaining Prism nodes or reimplementing Ruby method lookup.

use std::collections::BTreeSet;
use std::mem::size_of;

use super::memory_estimate::{fqn_heap_bytes, ruby_type_heap_bytes};
use super::{FullyQualifiedName, RubyType, TypeInferenceOutcome, UnknownReason};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum MethodReturnBase {
    Bottom,
    Proven(RubyType),
    Unknown(UnknownReason),
}

/// Compact equation `method = base | dependencies...`.
///
/// `Bottom` is private solver state and is never represented by
/// `RubyType::Unknown` in a published result.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct MethodReturnEquation {
    method: FullyQualifiedName,
    base: MethodReturnBase,
    dependencies: BTreeSet<FullyQualifiedName>,
}

impl MethodReturnEquation {
    pub(crate) fn new(
        method: FullyQualifiedName,
        base: MethodReturnBase,
        dependencies: BTreeSet<FullyQualifiedName>,
    ) -> Self {
        Self {
            method,
            base,
            dependencies,
        }
    }

    pub(crate) fn proven(method: FullyQualifiedName, ruby_type: RubyType) -> Self {
        assert!(
            !RubyType::union_members_contain_unknown(&ruby_type),
            "INVARIANT VIOLATED: a proven method-return equation contains Unknown (exactly or \
             inside a union member). This is a bug because Unknown cannot be a concrete equation \
             base and union members absorb Unknown. Fix: construct an unknown equation with its \
             precise reason."
        );
        Self::new(method, MethodReturnBase::Proven(ruby_type), BTreeSet::new())
    }

    pub(crate) fn from_ruby_type(
        method: FullyQualifiedName,
        ruby_type: RubyType,
        unknown_reason: UnknownReason,
    ) -> Self {
        if RubyType::union_members_contain_unknown(&ruby_type) {
            Self::new(
                method,
                MethodReturnBase::Unknown(unknown_reason),
                BTreeSet::new(),
            )
        } else {
            Self::proven(method, ruby_type)
        }
    }

    pub(crate) fn method(&self) -> &FullyQualifiedName {
        &self.method
    }

    pub(crate) fn base(&self) -> &MethodReturnBase {
        &self.base
    }

    pub(crate) fn dependencies(&self) -> &BTreeSet<FullyQualifiedName> {
        &self.dependencies
    }

    /// Project the equation before its complete dependency graph is solved.
    ///
    /// A dependency-free proven base is immediately safe to expose to later
    /// methods in the same traversal. Any dependency stays Unknown until an
    /// SCC solve has every participating equation.
    pub(crate) fn immediate_outcome(&self) -> TypeInferenceOutcome {
        match (&self.base, self.dependencies.is_empty()) {
            (MethodReturnBase::Proven(ruby_type), true) => {
                TypeInferenceOutcome::proven(ruby_type.clone())
            }
            (MethodReturnBase::Unknown(reason), _) => TypeInferenceOutcome::unknown(*reason),
            (MethodReturnBase::Bottom | MethodReturnBase::Proven(_), false)
            | (MethodReturnBase::Bottom, true) => {
                TypeInferenceOutcome::unknown(UnknownReason::UnresolvedMethodReturn)
            }
        }
    }

    pub(crate) fn estimated_heap_bytes(&self) -> usize {
        fqn_heap_bytes(&self.method)
            + match &self.base {
                MethodReturnBase::Bottom | MethodReturnBase::Unknown(_) => 0,
                MethodReturnBase::Proven(ruby_type) => ruby_type_heap_bytes(ruby_type),
            }
            + self.dependencies.len() * (size_of::<FullyQualifiedName>() + 3 * size_of::<usize>())
            + self.dependencies.iter().map(fqn_heap_bytes).sum::<usize>()
    }
}
