//! File-owned inference evidence: exact proof results, compact equations, and
//! observational telemetry, replaced atomically with the file that produced
//! them.

use std::mem::size_of;
use std::sync::LazyLock;

use crate::core::callables::callable_body::ConstantCallableBodyFact;
use crate::core::storage::memory_estimate::fqn_heap_bytes;
use crate::core::{
    ConstantTypeEquation, FullyQualifiedName, InferenceTelemetry, MethodReturnEquation, TextRange,
    TypeInferenceOutcome, UnknownReason,
};

/// File-owned proof results and their observational solver telemetry.
///
/// The exact outcomes are semantic evidence consumed by both editor and
/// headless adapters. They are replaced atomically with the file that produced
/// them; process/session aggregates intentionally merge only the counters.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InferenceEvidence {
    pub method_return_outcomes: MethodReturnOutcomes,
    /// Compact proof equations retained independently of Prism so the shared
    /// engine can solve recursive components spanning project files.
    pub method_return_equations: Vec<MethodReturnEquation>,
    /// File-owned type equations whose terms contain lexical value-constant
    /// lookups. The engine solves them after the complete namespace graph is
    /// installed and before method-return equations consume their results.
    pub constant_type_equations: Vec<ConstantTypeEquation>,
    /// Capture-free callable constants lowered during the owning file's
    /// ordinary traversal. Cross-file consumers resolve these facts through
    /// `View`; replacement removes them with the source file.
    pub(crate) constant_callable_bodies: Vec<ConstantCallableBodyFact>,
    /// Compact, file-owned results for complete call expressions. These are
    /// resolved from the same method candidates as navigation and diagnostics
    /// instead of duplicating method lookup in the AST visitor.
    pub call_expression_outcomes: Vec<(TextRange, TypeInferenceOutcome)>,
    /// Exact expression ranges whose type is Unknown, paired with the proof
    /// failure that prevented a concrete result.
    pub expression_unknown_reasons: Vec<(TextRange, UnknownReason)>,
    pub telemetry: FileTelemetry,
}

impl InferenceEvidence {
    /// Drop growth slack before the evidence is retained with its file.
    pub(crate) fn shrink_to_fit(&mut self) {
        self.method_return_equations.shrink_to_fit();
        self.constant_type_equations.shrink_to_fit();
        self.constant_callable_bodies.shrink_to_fit();
        self.call_expression_outcomes.shrink_to_fit();
        self.expression_unknown_reasons.shrink_to_fit();
    }

    pub(crate) fn estimated_heap_bytes(&self) -> usize {
        self.method_return_outcomes.estimated_heap_bytes()
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
            + self.telemetry.estimated_heap_bytes()
    }
}

/// One file's method-return outcomes, sorted by method with one outcome each.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MethodReturnOutcomes(Box<[(FullyQualifiedName, TypeInferenceOutcome)]>);

impl MethodReturnOutcomes {
    pub fn get(&self, method: &FullyQualifiedName) -> Option<&TypeInferenceOutcome> {
        self.0
            .binary_search_by(|(candidate, _)| candidate.cmp(method))
            .ok()
            .map(|index| &self.0[index].1)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&FullyQualifiedName, &TypeInferenceOutcome)> {
        self.0.iter().map(|(method, outcome)| (method, outcome))
    }

    pub fn values(&self) -> impl Iterator<Item = &TypeInferenceOutcome> {
        self.0.iter().map(|(_, outcome)| outcome)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn estimated_heap_bytes(&self) -> usize {
        self.0.len() * size_of::<(FullyQualifiedName, TypeInferenceOutcome)>()
            + self
                .0
                .iter()
                .map(|(method, outcome)| fqn_heap_bytes(method) + outcome.estimated_heap_bytes())
                .sum::<usize>()
    }
}

impl FromIterator<(FullyQualifiedName, TypeInferenceOutcome)> for MethodReturnOutcomes {
    fn from_iter<I: IntoIterator<Item = (FullyQualifiedName, TypeInferenceOutcome)>>(
        outcomes: I,
    ) -> Self {
        let mut outcomes = outcomes.into_iter().collect::<Vec<_>>();
        outcomes.sort_by(|(left, _), (right, _)| left.cmp(right));
        for pair in outcomes.windows(2) {
            invariant_ne!(
                pair[0].0,
                pair[1].0,
                what = "method `{method}` has more than one return outcome in one file",
                why = "each method-return equation produces exactly one proof outcome",
                fix = "collect one outcome per method before building file evidence",
                method = pair[0].0,
            );
        }
        Self(outcomes.into_boxed_slice())
    }
}

/// One file's observational telemetry. Most files record nothing, so the
/// empty value owns no allocation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileTelemetry(Option<Box<InferenceTelemetry>>);

static EMPTY_TELEMETRY: LazyLock<InferenceTelemetry> = LazyLock::new(InferenceTelemetry::default);

impl FileTelemetry {
    pub fn get(&self) -> &InferenceTelemetry {
        self.0.as_deref().unwrap_or(&EMPTY_TELEMETRY)
    }

    pub(crate) fn update(&mut self, update: impl FnOnce(&mut InferenceTelemetry)) {
        let telemetry = self.0.get_or_insert_default();
        update(telemetry);
        if **telemetry == *EMPTY_TELEMETRY {
            self.0 = None;
        }
    }

    fn estimated_heap_bytes(&self) -> usize {
        self.0.as_ref().map_or(0, |telemetry| {
            size_of::<InferenceTelemetry>()
                + telemetry.unknown_reasons.len()
                    * (size_of::<UnknownReason>() + size_of::<u64>() + 3 * size_of::<usize>())
        })
    }
}

impl From<InferenceTelemetry> for FileTelemetry {
    fn from(telemetry: InferenceTelemetry) -> Self {
        if telemetry == *EMPTY_TELEMETRY {
            Self(None)
        } else {
            Self(Some(Box::new(telemetry)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{RubyMethod, RubyType};

    fn method(name: &str) -> FullyQualifiedName {
        FullyQualifiedName::method(Vec::new(), RubyMethod::new(name).unwrap())
    }

    #[test]
    fn method_return_outcomes_are_found_from_unsorted_input() {
        let outcomes = [
            (
                method("zeta"),
                TypeInferenceOutcome::proven(RubyType::string()),
            ),
            (
                method("alpha"),
                TypeInferenceOutcome::unknown(UnknownReason::UnprovenRecursiveCycle),
            ),
        ]
        .into_iter()
        .collect::<MethodReturnOutcomes>();

        assert_eq!(outcomes.len(), 2);
        assert_eq!(
            outcomes
                .get(&method("zeta"))
                .and_then(TypeInferenceOutcome::proven_type),
            Some(&RubyType::string())
        );
        assert_eq!(
            outcomes
                .get(&method("alpha"))
                .and_then(TypeInferenceOutcome::unknown_reason),
            Some(UnknownReason::UnprovenRecursiveCycle)
        );
        assert!(outcomes.get(&method("missing")).is_none());
        assert_eq!(
            outcomes
                .iter()
                .map(|(method, _)| method.clone())
                .collect::<Vec<_>>(),
            vec![method("alpha"), method("zeta")]
        );
    }

    #[test]
    #[should_panic(expected = "more than one return outcome")]
    fn duplicate_method_return_outcomes_are_rejected() {
        let outcome = TypeInferenceOutcome::proven(RubyType::string());
        let _ = [
            (method("value"), outcome.clone()),
            (method("value"), outcome),
        ]
        .into_iter()
        .collect::<MethodReturnOutcomes>();
    }

    #[test]
    fn empty_file_telemetry_owns_no_allocation() {
        let empty = FileTelemetry::from(InferenceTelemetry::default());
        assert_eq!(empty, FileTelemetry::default());
        assert_eq!(empty.estimated_heap_bytes(), 0);

        let mut telemetry = FileTelemetry::default();
        telemetry.update(|telemetry| telemetry.observe_max_live_shape_aliases(2));
        assert_eq!(telemetry.get().max_live_shape_aliases, 2);
        assert!(telemetry.estimated_heap_bytes() > 0);

        telemetry.update(|telemetry| *telemetry = InferenceTelemetry::default());
        assert_eq!(telemetry, FileTelemetry::default());
    }
}
