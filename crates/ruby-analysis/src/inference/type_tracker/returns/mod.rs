//! Method return evidence and bounded recursive return solving.

use crate::core::method_return_equation::MethodReturnBase;
use crate::core::{
    ConstantTypeDependency, FullyQualifiedName, RubyType, TypeInferenceOutcome, UnknownReason,
};
use std::collections::{BTreeSet, HashMap, HashSet};

pub(in crate::inference::type_tracker) mod dependencies;
pub(in crate::inference::type_tracker) mod methods;

/// Private lattice value used while solving a recursive method return.
///
/// `Bottom` is the empty approximation for a recursive type variable. It is
/// intentionally not a `RubyType` variant, so it cannot escape into engine
/// facts, hover, inlay hints, or CLI output. `Unknown` is the opposite: a
/// required premise was not proven and therefore absorbs the equation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::inference::type_tracker) enum RecursiveReturnApproximation {
    Bottom,
    Proven(RubyType),
    Unknown,
}

impl RecursiveReturnApproximation {
    pub(in crate::inference::type_tracker) fn from_ruby_type(ruby_type: RubyType) -> Self {
        if ruby_type == RubyType::Unknown {
            Self::Unknown
        } else {
            Self::Proven(ruby_type)
        }
    }

    pub(in crate::inference::type_tracker) fn as_ruby_type(&self) -> RubyType {
        match self {
            Self::Proven(ruby_type) => ruby_type.clone(),
            Self::Bottom | Self::Unknown => RubyType::Unknown,
        }
    }

    pub(in crate::inference::type_tracker) fn into_outcome(
        self,
        unknown_reason: UnknownReason,
    ) -> TypeInferenceOutcome {
        match self {
            Self::Proven(ruby_type) => TypeInferenceOutcome::proven(ruby_type),
            Self::Bottom | Self::Unknown => TypeInferenceOutcome::unknown(unknown_reason),
        }
    }

    pub(in crate::inference::type_tracker) fn into_equation_base(self) -> MethodReturnBase {
        match self {
            Self::Bottom => MethodReturnBase::Bottom,
            Self::Proven(ruby_type) => MethodReturnBase::Proven(ruby_type),
            Self::Unknown => MethodReturnBase::Unknown(UnknownReason::UnresolvedMethodReturn),
        }
    }
}

/// Private return terms and proof evidence retained during one method solve.
#[derive(Default)]
pub(in crate::inference::type_tracker) struct ReturnEvidence {
    /// Private same-file return dependencies carried through straight-line
    /// local aliases such as `value = helper; value`.
    ///
    /// Control-flow nodes clear this map conservatively until dependency terms
    /// participate in the full branch environment. It must never become a
    /// public Ruby type or survive a method pass.
    pub(in crate::inference::type_tracker) local_terms:
        HashMap<String, (FullyQualifiedName, RecursiveReturnApproximation)>,
    /// Dependency aliases are retained only through straight-line statements.
    /// Branch/loop environments do not yet join private dependency terms, so
    /// aliases created inside them are deliberately discarded.
    pub(in crate::inference::type_tracker) inside_control_flow: bool,
    /// Current private approximation for direct calls back to the method being
    /// solved. This value must never be projected as a concrete `RubyType`.
    pub(in crate::inference::type_tracker) approximation: Option<RecursiveReturnApproximation>,
    /// Exact same-file calls observed as explicit or fallthrough return terms.
    pub(in crate::inference::type_tracker) dependencies: BTreeSet<FullyQualifiedName>,
    /// Value-constant terms returned by the current method. These stay
    /// private until attached to the file-owned method equation.
    pub(in crate::inference::type_tracker) constant_dependencies: BTreeSet<ConstantTypeDependency>,
    /// Call locations whose result was proven directly from a modeled block or
    /// proc body. These are expression proofs, not dependencies on the
    /// ordinary return equation of the invoked method.
    pub(in crate::inference::type_tracker) direct_call_proofs: HashSet<usize>,
    /// Explicit `return` values found during the current method pass. Ruby
    /// methods return from these paths as well as from their fallthrough tail.
    pub(in crate::inference::type_tracker) explicit_values: Vec<RecursiveReturnApproximation>,
    /// Set while the ordinary method traversal observes a direct recursive
    /// call. This avoids a separate pre-scan of every method body.
    pub(in crate::inference::type_tracker) saw_direct_recursion: bool,
}
