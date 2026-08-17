//! Proof status for a type-inference result.
//!
//! `RubyType::Unknown` is the editor-facing type projection. This module keeps
//! the reason that a concrete type was withheld so non-LSP consumers can make
//! the same decision and explain it without reimplementing inference policy.

use crate::core::{
    ConstantCallableBodyFact, ConstantTypeEquation, FullyQualifiedName, MethodReturnEquation,
    RubyType, TextRange,
};
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
    /// A shape exceeded a fixed field, depth, variant, alias, or solve bound.
    ShapeBoundExceeded,
    /// A mutable shape escaped or crossed an unsupported mutation boundary.
    MutableShapeInvalidated,
    /// A call has a block/callable shape that the static model cannot represent.
    UnsupportedCallable,
    /// A callable signature did not prove every block input type.
    IncompleteBlockInput,
    /// A reachable block exit did not prove a result type.
    IncompleteBlockResult,
    /// Generic variables required by the callable result were not fully solved.
    IncompleteGenericSubstitution,
    /// More than one compatible callable overload produced a distinct result.
    AmbiguousCallableOverload,
    /// Higher-order solving exceeded a reviewed overload, variable, depth, or union bound.
    HigherOrderBoundExceeded,
    /// Block control flow changes the enclosing call result and is not yet modeled exactly.
    UnsupportedBlockFlow,
    /// The callable body contains syntax outside the reviewed summary domain.
    UnsupportedCallableBody,
    /// At least one callable argument has no complete proven type.
    IncompleteCallableInput,
    /// A source-ordered captured binding has no complete proven type.
    IncompleteCallableCapture,
    /// More than one callable identity reaches the invocation.
    AmbiguousCallableValue,
    /// The callable crossed an unsupported storage or invocation boundary.
    EscapedCallableValue,
    /// Callable lowering or evaluation exceeded a reviewed fixed bound.
    CallableBodyBoundExceeded,
    /// Recursive callable instantiation is deliberately unsupported.
    CallableRecursionUnsupported,
    /// Callable-local control flow cannot be represented completely.
    UnsupportedCallableFlow,
}

impl UnknownReason {
    pub const ALL: [Self; 25] = [
        Self::NoReachingAssignment,
        Self::UnresolvedAssignmentValue,
        Self::AmbiguousReachingAssignment,
        Self::UnknownReceiver,
        Self::InvalidMethodName,
        Self::UnresolvedMethodReturn,
        Self::IncompleteUnionMember,
        Self::UnprovenRecursiveCycle,
        Self::ShapeBoundExceeded,
        Self::MutableShapeInvalidated,
        Self::UnsupportedCallable,
        Self::IncompleteBlockInput,
        Self::IncompleteBlockResult,
        Self::IncompleteGenericSubstitution,
        Self::AmbiguousCallableOverload,
        Self::HigherOrderBoundExceeded,
        Self::UnsupportedBlockFlow,
        Self::UnsupportedCallableBody,
        Self::IncompleteCallableInput,
        Self::IncompleteCallableCapture,
        Self::AmbiguousCallableValue,
        Self::EscapedCallableValue,
        Self::CallableBodyBoundExceeded,
        Self::CallableRecursionUnsupported,
        Self::UnsupportedCallableFlow,
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
            Self::ShapeBoundExceeded => "shape_bound_exceeded",
            Self::MutableShapeInvalidated => "mutable_shape_invalidated",
            Self::UnsupportedCallable => "unsupported_callable",
            Self::IncompleteBlockInput => "incomplete_block_input",
            Self::IncompleteBlockResult => "incomplete_block_result",
            Self::IncompleteGenericSubstitution => "incomplete_generic_substitution",
            Self::AmbiguousCallableOverload => "ambiguous_callable_overload",
            Self::HigherOrderBoundExceeded => "higher_order_bound_exceeded",
            Self::UnsupportedBlockFlow => "unsupported_block_flow",
            Self::UnsupportedCallableBody => "unsupported_callable_body",
            Self::IncompleteCallableInput => "incomplete_callable_input",
            Self::IncompleteCallableCapture => "incomplete_callable_capture",
            Self::AmbiguousCallableValue => "ambiguous_callable_value",
            Self::EscapedCallableValue => "escaped_callable_value",
            Self::CallableBodyBoundExceeded => "callable_body_bound_exceeded",
            Self::CallableRecursionUnsupported => "callable_recursion_unsupported",
            Self::UnsupportedCallableFlow => "unsupported_callable_flow",
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
            Self::ShapeBoundExceeded => {
                "shape inference exceeded a fixed field, depth, variant, alias, or solve bound"
            }
            Self::MutableShapeInvalidated => {
                "the mutable shape crossed an unresolved mutation or escape boundary"
            }
            Self::UnsupportedCallable => {
                "the block or callable shape is not supported by static higher-order inference"
            }
            Self::IncompleteBlockInput => {
                "at least one block input type is not proven by the callable signature"
            }
            Self::IncompleteBlockResult => {
                "at least one reachable block exit does not have a proven result type"
            }
            Self::IncompleteGenericSubstitution => {
                "the callable result depends on a generic variable that was not fully solved"
            }
            Self::AmbiguousCallableOverload => {
                "compatible callable overloads do not prove one canonical result"
            }
            Self::HigherOrderBoundExceeded => {
                "higher-order inference exceeded a fixed overload, variable, depth, or union bound"
            }
            Self::UnsupportedBlockFlow => {
                "block control flow changes the enclosing call result and is not modeled exactly"
            }
            Self::UnsupportedCallableBody => {
                "the callable body contains syntax outside the reviewed static summary domain"
            }
            Self::IncompleteCallableInput => {
                "at least one callable argument does not have a complete proven type"
            }
            Self::IncompleteCallableCapture => {
                "at least one captured binding does not have a complete source-ordered type proof"
            }
            Self::AmbiguousCallableValue => {
                "more than one incompatible callable identity reaches the invocation"
            }
            Self::EscapedCallableValue => {
                "the callable crossed an unsupported storage or invocation boundary"
            }
            Self::CallableBodyBoundExceeded => {
                "callable-body inference exceeded a reviewed fixed bound"
            }
            Self::CallableRecursionUnsupported => {
                "recursive callable instantiation is not supported"
            }
            Self::UnsupportedCallableFlow => {
                "callable-local control flow cannot be represented completely"
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
    /// Shape occurrences retained by file-owned proof values. This is an
    /// occurrence count across facts/outcomes, not a claim about runtime Hash
    /// instances or allocator-unique objects.
    pub retained_shape_occurrences: u64,
    pub retained_shape_fields: u64,
    pub max_retained_shape_fields: u64,
    pub max_retained_shape_depth: u64,
    pub retained_shape_unions: u64,
    pub retained_shape_union_variants: u64,
    pub max_retained_shape_union_variants: u64,
    /// Largest number of simultaneously visible local aliases/containments
    /// observed for one mutable abstract Hash identity in a file.
    pub max_live_shape_aliases: u64,
    /// Exact retained proof outcomes withheld for shape-specific reasons.
    pub shape_invalidated_outcomes: u64,
    pub shape_bound_exceeded_outcomes: u64,
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
    /// File-owned type equations whose terms contain lexical value-constant
    /// lookups. The engine solves them after the complete namespace graph is
    /// installed and before method-return equations consume their results.
    pub constant_type_equations: Vec<ConstantTypeEquation>,
    /// Capture-free callable constants lowered during the owning file's
    /// ordinary traversal. Cross-file consumers resolve these facts through
    /// `AnalysisQuery`; replacement removes them with the source file.
    pub(crate) constant_callable_bodies: Vec<ConstantCallableBodyFact>,
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
            + self.constant_type_equations.capacity() * size_of::<ConstantTypeEquation>()
            + self.constant_callable_bodies.capacity() * size_of::<ConstantCallableBodyFact>()
            + self
                .constant_callable_bodies
                .iter()
                .map(ConstantCallableBodyFact::estimated_heap_bytes)
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
        self.retained_shape_occurrences = checked_add(
            self.retained_shape_occurrences,
            other.retained_shape_occurrences,
            "aggregated retained shape occurrence count",
        );
        self.retained_shape_fields = checked_add(
            self.retained_shape_fields,
            other.retained_shape_fields,
            "aggregated retained shape field count",
        );
        self.max_retained_shape_fields = self
            .max_retained_shape_fields
            .max(other.max_retained_shape_fields);
        self.max_retained_shape_depth = self
            .max_retained_shape_depth
            .max(other.max_retained_shape_depth);
        self.retained_shape_unions = checked_add(
            self.retained_shape_unions,
            other.retained_shape_unions,
            "aggregated retained shape union count",
        );
        self.retained_shape_union_variants = checked_add(
            self.retained_shape_union_variants,
            other.retained_shape_union_variants,
            "aggregated retained shape union variant count",
        );
        self.max_retained_shape_union_variants = self
            .max_retained_shape_union_variants
            .max(other.max_retained_shape_union_variants);
        self.max_live_shape_aliases = self
            .max_live_shape_aliases
            .max(other.max_live_shape_aliases);
        self.shape_invalidated_outcomes = checked_add(
            self.shape_invalidated_outcomes,
            other.shape_invalidated_outcomes,
            "aggregated shape-invalidated outcome count",
        );
        self.shape_bound_exceeded_outcomes = checked_add(
            self.shape_bound_exceeded_outcomes,
            other.shape_bound_exceeded_outcomes,
            "aggregated shape-bound outcome count",
        );
        for (reason, incoming) in &other.unknown_reasons {
            let count = self.unknown_reasons.entry(*reason).or_default();
            *count = checked_add(*count, *incoming, "aggregated Unknown reason count");
        }
    }

    /// Observe one retained proof value recursively. Shape unions count only
    /// their direct Shape members so unrelated nominal union alternatives do
    /// not inflate structural variant width.
    pub fn observe_retained_type(&mut self, ruby_type: &RubyType) {
        match ruby_type {
            RubyType::Shape(shape) => {
                self.retained_shape_occurrences = checked_add(
                    self.retained_shape_occurrences,
                    1,
                    "retained shape occurrence count",
                );
                let fields = u64::try_from(shape.fields().len()).expect(
                    "INVARIANT VIOLATED: retained shape field count did not fit u64. This is a bug because shape width is bounded far below u64. Fix: keep MAX_SHAPE_FIELDS representable by telemetry.",
                );
                self.retained_shape_fields = checked_add(
                    self.retained_shape_fields,
                    fields,
                    "retained shape field count",
                );
                self.max_retained_shape_fields = self.max_retained_shape_fields.max(fields);
                let depth = u64::try_from(shape.depth()).expect(
                    "INVARIANT VIOLATED: retained shape depth did not fit u64. This is a bug because shape depth is bounded far below u64. Fix: keep MAX_SHAPE_DEPTH representable by telemetry.",
                );
                self.max_retained_shape_depth = self.max_retained_shape_depth.max(depth);
                for field in shape.fields() {
                    self.observe_retained_type(field.value());
                }
                if let Some(rest) = shape.rest() {
                    self.observe_retained_type(rest.key());
                    self.observe_retained_type(rest.value());
                }
            }
            RubyType::Union(members) => {
                let shape_variants = members
                    .iter()
                    .filter(|member| matches!(member, RubyType::Shape(_)))
                    .count();
                if shape_variants > 1 {
                    self.retained_shape_unions =
                        checked_add(self.retained_shape_unions, 1, "retained shape union count");
                    let variants = u64::try_from(shape_variants).expect(
                        "INVARIANT VIOLATED: retained shape-union width did not fit u64. This is a bug because union width is bounded far below u64. Fix: keep MAX_SHAPE_UNION_VARIANTS representable by telemetry.",
                    );
                    self.retained_shape_union_variants = checked_add(
                        self.retained_shape_union_variants,
                        variants,
                        "retained shape union variant count",
                    );
                    self.max_retained_shape_union_variants =
                        self.max_retained_shape_union_variants.max(variants);
                }
                for member in members {
                    self.observe_retained_type(member);
                }
            }
            RubyType::Array(elements) => {
                for element in elements {
                    self.observe_retained_type(element);
                }
            }
            RubyType::Hash(keys, values) => {
                for key in keys {
                    self.observe_retained_type(key);
                }
                for value in values {
                    self.observe_retained_type(value);
                }
            }
            RubyType::Class(_)
            | RubyType::Module(_)
            | RubyType::ClassReference(_)
            | RubyType::ModuleReference(_)
            | RubyType::Literal(_)
            | RubyType::Unknown => {}
        }
    }

    pub fn observe_shape_unknown(&mut self, reason: UnknownReason) {
        match reason {
            UnknownReason::MutableShapeInvalidated => {
                self.shape_invalidated_outcomes = checked_add(
                    self.shape_invalidated_outcomes,
                    1,
                    "shape-invalidated outcome count",
                );
            }
            UnknownReason::ShapeBoundExceeded => {
                self.shape_bound_exceeded_outcomes = checked_add(
                    self.shape_bound_exceeded_outcomes,
                    1,
                    "shape-bound outcome count",
                );
            }
            UnknownReason::NoReachingAssignment
            | UnknownReason::UnresolvedAssignmentValue
            | UnknownReason::AmbiguousReachingAssignment
            | UnknownReason::UnknownReceiver
            | UnknownReason::InvalidMethodName
            | UnknownReason::UnresolvedMethodReturn
            | UnknownReason::IncompleteUnionMember
            | UnknownReason::UnprovenRecursiveCycle
            | UnknownReason::UnsupportedCallable
            | UnknownReason::IncompleteBlockInput
            | UnknownReason::IncompleteBlockResult
            | UnknownReason::IncompleteGenericSubstitution
            | UnknownReason::AmbiguousCallableOverload
            | UnknownReason::HigherOrderBoundExceeded
            | UnknownReason::UnsupportedBlockFlow
            | UnknownReason::UnsupportedCallableBody
            | UnknownReason::IncompleteCallableInput
            | UnknownReason::IncompleteCallableCapture
            | UnknownReason::AmbiguousCallableValue
            | UnknownReason::EscapedCallableValue
            | UnknownReason::CallableBodyBoundExceeded
            | UnknownReason::CallableRecursionUnsupported
            | UnknownReason::UnsupportedCallableFlow => {}
        }
    }

    pub fn observe_max_live_shape_aliases(&mut self, aliases: usize) {
        let aliases = u64::try_from(aliases).expect(
            "INVARIANT VIOLATED: live shape alias count did not fit u64. This is a bug because aliases are bounded far below u64. Fix: keep MAX_SHAPE_ALIASES representable by telemetry.",
        );
        self.max_live_shape_aliases = self.max_live_shape_aliases.max(aliases);
    }

    /// Replace metrics derived from final file-owned proof storage while
    /// retaining traversal/solver observations from the same file.
    pub(crate) fn replace_retained_shape_observations(&mut self, observed: &Self) {
        self.retained_shape_occurrences = observed.retained_shape_occurrences;
        self.retained_shape_fields = observed.retained_shape_fields;
        self.max_retained_shape_fields = observed.max_retained_shape_fields;
        self.max_retained_shape_depth = observed.max_retained_shape_depth;
        self.retained_shape_unions = observed.retained_shape_unions;
        self.retained_shape_union_variants = observed.retained_shape_union_variants;
        self.max_retained_shape_union_variants = observed.max_retained_shape_union_variants;
        self.shape_invalidated_outcomes = observed.shape_invalidated_outcomes;
        self.shape_bound_exceeded_outcomes = observed.shape_bound_exceeded_outcomes;
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
    /// Construct a proven result. Passing `RubyType::Unknown` — exactly or as
    /// a union member — is an invariant violation because it would erase the
    /// distinction this type enforces: `RubyType::union` flattens unions and
    /// absorbs `Unknown`, so a `Union` containing `Unknown` is not proof.
    pub fn proven(ruby_type: RubyType) -> Self {
        assert!(
            !RubyType::union_members_contain_unknown(&ruby_type),
            "INVARIANT VIOLATED: TypeInferenceOutcome::proven received RubyType::Unknown (exactly \
             or inside a union member). This is a bug because a proven result must contain a \
             concrete type and Unknown must retain a reason. Fix: construct \
             TypeInferenceOutcome::unknown with the precise UnknownReason instead."
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
    use crate::core::{LiteralKey, ShapeExactness, ShapeField, ShapeStability, ShapeType};

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

    #[test]
    #[should_panic(
        expected = "INVARIANT VIOLATED: TypeInferenceOutcome::proven received RubyType::Unknown"
    )]
    fn union_with_unknown_member_cannot_be_constructed_as_proven() {
        let _ = TypeInferenceOutcome::proven(RubyType::Union(vec![
            RubyType::Unknown,
            RubyType::string(),
        ]));
    }

    #[test]
    fn retained_shape_telemetry_counts_nested_occurrences_and_correlated_unions() {
        let nested = RubyType::Shape(Box::new(
            ShapeType::try_new(
                [ShapeField::required(
                    LiteralKey::symbol("name"),
                    RubyType::string(),
                )],
                None,
                ShapeExactness::Exact,
                ShapeStability::TrackedMutable,
            )
            .unwrap(),
        ));
        let first = RubyType::Shape(Box::new(
            ShapeType::try_new(
                [
                    ShapeField::required(LiteralKey::symbol("id"), RubyType::integer()),
                    ShapeField::required(LiteralKey::symbol("profile"), nested),
                ],
                None,
                ShapeExactness::Exact,
                ShapeStability::TrackedMutable,
            )
            .unwrap(),
        ));
        let second = RubyType::Shape(Box::new(
            ShapeType::try_new(
                [ShapeField::required(
                    LiteralKey::symbol("error"),
                    RubyType::string(),
                )],
                None,
                ShapeExactness::Exact,
                ShapeStability::TrackedMutable,
            )
            .unwrap(),
        ));

        let mut telemetry = InferenceTelemetry::default();
        telemetry.observe_retained_type(&RubyType::union([first, second]));
        telemetry.observe_max_live_shape_aliases(3);
        telemetry.observe_shape_unknown(UnknownReason::MutableShapeInvalidated);
        telemetry.observe_shape_unknown(UnknownReason::ShapeBoundExceeded);

        assert_eq!(telemetry.retained_shape_occurrences, 3);
        assert_eq!(telemetry.retained_shape_fields, 4);
        assert_eq!(telemetry.max_retained_shape_fields, 2);
        assert_eq!(telemetry.max_retained_shape_depth, 2);
        assert_eq!(telemetry.retained_shape_unions, 1);
        assert_eq!(telemetry.retained_shape_union_variants, 2);
        assert_eq!(telemetry.max_retained_shape_union_variants, 2);
        assert_eq!(telemetry.max_live_shape_aliases, 3);
        assert_eq!(telemetry.shape_invalidated_outcomes, 1);
        assert_eq!(telemetry.shape_bound_exceeded_outcomes, 1);
    }

    #[test]
    fn shape_telemetry_merge_adds_occurrences_and_preserves_maxima() {
        let mut left = InferenceTelemetry {
            retained_shape_occurrences: 2,
            retained_shape_fields: 3,
            max_retained_shape_fields: 2,
            max_retained_shape_depth: 2,
            retained_shape_unions: 1,
            retained_shape_union_variants: 2,
            max_retained_shape_union_variants: 2,
            max_live_shape_aliases: 2,
            shape_invalidated_outcomes: 1,
            ..Default::default()
        };
        let right = InferenceTelemetry {
            retained_shape_occurrences: 4,
            retained_shape_fields: 12,
            max_retained_shape_fields: 5,
            max_retained_shape_depth: 3,
            retained_shape_unions: 2,
            retained_shape_union_variants: 6,
            max_retained_shape_union_variants: 4,
            max_live_shape_aliases: 4,
            shape_bound_exceeded_outcomes: 2,
            ..Default::default()
        };

        left.merge(&right);

        assert_eq!(left.retained_shape_occurrences, 6);
        assert_eq!(left.retained_shape_fields, 15);
        assert_eq!(left.max_retained_shape_fields, 5);
        assert_eq!(left.max_retained_shape_depth, 3);
        assert_eq!(left.retained_shape_unions, 3);
        assert_eq!(left.retained_shape_union_variants, 8);
        assert_eq!(left.max_retained_shape_union_variants, 4);
        assert_eq!(left.max_live_shape_aliases, 4);
        assert_eq!(left.shape_invalidated_outcomes, 1);
        assert_eq!(left.shape_bound_exceeded_outcomes, 2);
    }
}
