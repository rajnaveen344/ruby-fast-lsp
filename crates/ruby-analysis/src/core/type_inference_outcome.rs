//! Proof status for a type-inference result.
//!
//! `RubyType::Unknown` is the editor-facing type projection. This module keeps
//! the reason that a concrete type was withheld so non-LSP consumers can make
//! the same decision and explain it without reimplementing inference policy.

use crate::core::{FullyQualifiedName, MethodReturnEquation, RubyType, TextRange};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt::{self, Display, Formatter};
use std::mem::size_of;

use super::memory_estimate::{fqn_heap_bytes, ruby_type_heap_bytes};

/// Stable, machine-readable reasons that inference withheld a concrete type.
///
/// Variant names are internal Rust API. Consumers that persist or transmit a
/// reason must use [`UnknownReason::code`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnknownReason {
    /// No source-ordered assignment reaches a variable read.
    NoReachingAssignment,
    /// The latest source-ordered assignment has no proven value type.
    UnresolvedAssignmentValue,
    /// More than one incompatible assignment is equally eligible at the read.
    AmbiguousReachingAssignment,
    /// The receiver itself has no proven type.
    UnknownReceiver,
    /// A parsed call name could not be represented as a Ruby method name.
    InvalidMethodName,
    /// Method lookup or return inference did not prove a return type.
    UnresolvedMethodReturn,
    /// At least one reachable member of a union did not prove the call result.
    IncompleteUnionMember,
    /// A recursive return equation had no concrete least fixed point within
    /// the bounded solver.
    UnprovenRecursiveCycle,
}

impl UnknownReason {
    pub const ALL: [Self; 8] = [
        Self::NoReachingAssignment,
        Self::UnresolvedAssignmentValue,
        Self::AmbiguousReachingAssignment,
        Self::UnknownReceiver,
        Self::InvalidMethodName,
        Self::UnresolvedMethodReturn,
        Self::IncompleteUnionMember,
        Self::UnprovenRecursiveCycle,
    ];

    /// Stable identifier for CLI/JSON output and scorecard expectations.
    pub const fn code(self) -> &'static str {
        match self {
            Self::NoReachingAssignment => "no_reaching_assignment",
            Self::UnresolvedAssignmentValue => "unresolved_assignment_value",
            Self::AmbiguousReachingAssignment => "ambiguous_reaching_assignment",
            Self::UnknownReceiver => "unknown_receiver",
            Self::InvalidMethodName => "invalid_method_name",
            Self::UnresolvedMethodReturn => "unresolved_method_return",
            Self::IncompleteUnionMember => "incomplete_union_member",
            Self::UnprovenRecursiveCycle => "unproven_recursive_cycle",
        }
    }

    /// Concise explanation suitable for humans and deterministic snapshots.
    pub const fn explanation(self) -> &'static str {
        match self {
            Self::NoReachingAssignment => "no source-ordered assignment reaches this variable read",
            Self::UnresolvedAssignmentValue => {
                "the reaching assignment value does not have a proven type"
            }
            Self::AmbiguousReachingAssignment => {
                "multiple incompatible assignments are equally eligible at this read"
            }
            Self::UnknownReceiver => "the receiver type is not proven",
            Self::InvalidMethodName => "the call name is not a valid Ruby method name",
            Self::UnresolvedMethodReturn => {
                "method lookup and available signatures do not prove a return type"
            }
            Self::IncompleteUnionMember => {
                "at least one reachable union member does not prove the call result"
            }
            Self::UnprovenRecursiveCycle => {
                "the recursive return equation did not converge to a concrete proof"
            }
        }
    }
}

impl Display for UnknownReason {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.explanation())
    }
}

/// A proof-carrying inference result.
///
/// The representation is private so a caller cannot publish
/// `Proven(RubyType::Unknown)`. Existing consumers may project this to
/// `Option<RubyType>` or `RubyType`, while CLI/reporting consumers retain the
/// structured reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeInferenceOutcome {
    state: TypeInferenceState,
}

/// Deterministic, file-owned evidence about method-return inference work.
///
/// These counters are observational: they never participate in semantic
/// fingerprints or type decisions. Counts are replaced with their owning file
/// and aggregated by engine/CLI consumers without process-global atomics.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InferenceTelemetry {
    pub method_return_outcomes: u64,
    pub proven_method_returns: u64,
    pub unknown_method_returns: u64,
    pub unknown_reasons: BTreeMap<UnknownReason, u64>,
    pub recursive_components: u64,
    pub recursive_methods: u64,
    pub solver_iterations: u64,
    pub solver_bound_hits: u64,
}

/// File-owned proof results and their observational solver telemetry.
///
/// The exact outcomes are semantic evidence consumed by both editor and
/// headless adapters. They are replaced atomically with the file that produced
/// them; process/session aggregates intentionally merge only the counters.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InferenceEvidence {
    pub method_return_outcomes: BTreeMap<FullyQualifiedName, TypeInferenceOutcome>,
    /// Compact proof equations retained independently of Prism so the shared
    /// engine can solve recursive components spanning project files.
    pub method_return_equations: Vec<MethodReturnEquation>,
    /// Compact, file-owned results for complete call expressions. These are
    /// resolved from the same method candidates as navigation and diagnostics
    /// instead of duplicating method lookup in the AST visitor.
    pub call_expression_outcomes: Vec<(TextRange, TypeInferenceOutcome)>,
    /// Exact expression ranges whose type is Unknown, paired with the proof
    /// failure that prevented a concrete result.
    pub expression_unknown_reasons: Vec<(TextRange, UnknownReason)>,
    pub telemetry: InferenceTelemetry,
}

impl InferenceEvidence {
    pub(crate) fn estimated_heap_bytes(&self) -> usize {
        self.method_return_outcomes.len()
            * (size_of::<FullyQualifiedName>()
                + size_of::<TypeInferenceOutcome>()
                + 3 * size_of::<usize>())
            + self
                .method_return_outcomes
                .iter()
                .map(|(method, outcome)| fqn_heap_bytes(method) + outcome.estimated_heap_bytes())
                .sum::<usize>()
            + self.method_return_equations.capacity() * size_of::<MethodReturnEquation>()
            + self
                .method_return_equations
                .iter()
                .map(MethodReturnEquation::estimated_heap_bytes)
                .sum::<usize>()
            + self.call_expression_outcomes.capacity()
                * size_of::<(TextRange, TypeInferenceOutcome)>()
            + self
                .call_expression_outcomes
                .iter()
                .map(|(_, outcome)| outcome.estimated_heap_bytes())
                .sum::<usize>()
            + self.expression_unknown_reasons.capacity() * size_of::<(TextRange, UnknownReason)>()
            + self.telemetry.unknown_reasons.len()
                * (size_of::<UnknownReason>() + size_of::<u64>() + 3 * size_of::<usize>())
    }
}

impl InferenceTelemetry {
    pub fn observe_method_return(&mut self, outcome: &TypeInferenceOutcome) {
        self.method_return_outcomes = checked_add(
            self.method_return_outcomes,
            1,
            "method-return outcome count",
        );
        match outcome.unknown_reason() {
            None => {
                self.proven_method_returns =
                    checked_add(self.proven_method_returns, 1, "proven method-return count");
            }
            Some(reason) => {
                self.unknown_method_returns = checked_add(
                    self.unknown_method_returns,
                    1,
                    "Unknown method-return count",
                );
                let count = self.unknown_reasons.entry(reason).or_default();
                *count = checked_add(*count, 1, "Unknown reason count");
            }
        }
    }

    pub fn merge(&mut self, other: &Self) {
        self.method_return_outcomes = checked_add(
            self.method_return_outcomes,
            other.method_return_outcomes,
            "aggregated method-return outcome count",
        );
        self.proven_method_returns = checked_add(
            self.proven_method_returns,
            other.proven_method_returns,
            "aggregated proven method-return count",
        );
        self.unknown_method_returns = checked_add(
            self.unknown_method_returns,
            other.unknown_method_returns,
            "aggregated Unknown method-return count",
        );
        self.recursive_components = checked_add(
            self.recursive_components,
            other.recursive_components,
            "aggregated recursive component count",
        );
        self.recursive_methods = checked_add(
            self.recursive_methods,
            other.recursive_methods,
            "aggregated recursive method count",
        );
        self.solver_iterations = checked_add(
            self.solver_iterations,
            other.solver_iterations,
            "aggregated solver iteration count",
        );
        self.solver_bound_hits = checked_add(
            self.solver_bound_hits,
            other.solver_bound_hits,
            "aggregated solver bound-hit count",
        );
        for (reason, incoming) in &other.unknown_reasons {
            let count = self.unknown_reasons.entry(*reason).or_default();
            *count = checked_add(*count, *incoming, "aggregated Unknown reason count");
        }
    }
}

fn checked_add(left: u64, right: u64, counter: &str) -> u64 {
    left.checked_add(right).unwrap_or_else(|| {
        panic!(
            "INVARIANT VIOLATED: {counter} exhausted u64. This is a bug because one process cannot observe more events than addressable work. Fix: reset telemetry at a bounded file/session lifecycle or widen the counter before overflow."
        )
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TypeInferenceState {
    Proven(RubyType),
    Unknown(UnknownReason),
}

impl TypeInferenceOutcome {
    /// Construct a proven result. Passing `RubyType::Unknown` is an invariant
    /// violation because it would erase the distinction this type enforces.
    pub fn proven(ruby_type: RubyType) -> Self {
        assert!(
            ruby_type != RubyType::Unknown,
            "INVARIANT VIOLATED: TypeInferenceOutcome::proven received RubyType::Unknown. \
             This is a bug because a proven result must contain a concrete type and Unknown \
             must retain a reason. Fix: construct TypeInferenceOutcome::unknown with the \
             precise UnknownReason instead."
        );
        Self {
            state: TypeInferenceState::Proven(ruby_type),
        }
    }

    pub const fn unknown(reason: UnknownReason) -> Self {
        Self {
            state: TypeInferenceState::Unknown(reason),
        }
    }

    /// Convert an existing optional result without allowing a reasonless
    /// `RubyType::Unknown` to pass as proof.
    pub fn from_optional(ruby_type: Option<RubyType>, reason: UnknownReason) -> Self {
        match ruby_type {
            Some(RubyType::Unknown) | None => Self::unknown(reason),
            Some(ruby_type) => Self::proven(ruby_type),
        }
    }

    pub fn proven_type(&self) -> Option<&RubyType> {
        match &self.state {
            TypeInferenceState::Proven(ruby_type) => Some(ruby_type),
            TypeInferenceState::Unknown(_) => None,
        }
    }

    pub fn into_proven_type(self) -> Option<RubyType> {
        match self.state {
            TypeInferenceState::Proven(ruby_type) => Some(ruby_type),
            TypeInferenceState::Unknown(_) => None,
        }
    }

    pub fn unknown_reason(&self) -> Option<UnknownReason> {
        match self.state {
            TypeInferenceState::Proven(_) => None,
            TypeInferenceState::Unknown(reason) => Some(reason),
        }
    }

    /// Project into the legacy/editor type representation.
    pub fn into_ruby_type(self) -> RubyType {
        match self.state {
            TypeInferenceState::Proven(ruby_type) => ruby_type,
            TypeInferenceState::Unknown(_) => RubyType::Unknown,
        }
    }

    pub(crate) fn estimated_heap_bytes(&self) -> usize {
        match &self.state {
            TypeInferenceState::Proven(ruby_type) => ruby_type_heap_bytes(ruby_type),
            TypeInferenceState::Unknown(_) => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optional_unknown_retains_the_supplied_reason() {
        let outcome = TypeInferenceOutcome::from_optional(
            Some(RubyType::Unknown),
            UnknownReason::UnresolvedMethodReturn,
        );

        assert_eq!(
            outcome.unknown_reason(),
            Some(UnknownReason::UnresolvedMethodReturn)
        );
        assert_eq!(outcome.into_ruby_type(), RubyType::Unknown);
    }

    #[test]
    #[should_panic(
        expected = "INVARIANT VIOLATED: TypeInferenceOutcome::proven received RubyType::Unknown"
    )]
    fn unknown_cannot_be_constructed_as_proven() {
        let _ = TypeInferenceOutcome::proven(RubyType::Unknown);
    }
}
