//! Deterministic method-return equation solving.
//!
//! The AST traversal emits compact equations consisting of proven base return
//! types and same-file method dependencies. This module solves those equations
//! without revisiting Prism nodes. Recursive components start at a private
//! bottom value and iterate synchronously to a bounded least fixed point.

use std::collections::{BTreeMap, BTreeSet};

use crate::core::method_return_equation::MethodReturnBase;
use crate::core::{
    FullyQualifiedName, InferenceTelemetry, MethodReturnEquation, RubyType, TypeInferenceOutcome,
    UnknownReason,
};

pub(crate) const MAX_RECURSIVE_RETURN_ITERATIONS: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Approximation {
    Bottom,
    Proven(RubyType),
    Unknown(UnknownReason),
}

impl Approximation {
    fn into_outcome(self, recursive: bool) -> TypeInferenceOutcome {
        match self {
            Self::Proven(ruby_type) => TypeInferenceOutcome::proven(ruby_type),
            Self::Bottom | Self::Unknown(_) if recursive => {
                TypeInferenceOutcome::unknown(UnknownReason::UnprovenRecursiveCycle)
            }
            Self::Bottom => TypeInferenceOutcome::unknown(UnknownReason::UnresolvedMethodReturn),
            Self::Unknown(reason) => TypeInferenceOutcome::unknown(reason),
        }
    }
}

/// Solve all method equations by deterministic strongly connected component.
///
/// Calls to methods outside `equations` are incomplete evidence and remain
/// Unknown. Duplicate definitions are evaluated as one exhaustive method
/// result: every reopened body must resolve before the group can publish a
/// concrete union.
#[cfg(test)]
pub(crate) fn solve_method_return_equations(
    equations: &[MethodReturnEquation],
) -> BTreeMap<FullyQualifiedName, TypeInferenceOutcome> {
    solve_method_return_equations_with_telemetry(equations).outcomes
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MethodReturnSolveResult {
    pub(crate) outcomes: BTreeMap<FullyQualifiedName, TypeInferenceOutcome>,
    pub(crate) telemetry: InferenceTelemetry,
}

pub(crate) fn solve_method_return_equations_with_telemetry(
    equations: &[MethodReturnEquation],
) -> MethodReturnSolveResult {
    let mut grouped: BTreeMap<FullyQualifiedName, Vec<&MethodReturnEquation>> = BTreeMap::new();
    for equation in equations {
        grouped
            .entry(equation.method().clone())
            .or_default()
            .push(equation);
    }

    let graph = grouped
        .iter()
        .map(|(method, definitions)| {
            let dependencies = definitions
                .iter()
                .flat_map(|definition| definition.dependencies().iter())
                .filter(|dependency| grouped.contains_key(*dependency))
                .cloned()
                .collect::<BTreeSet<_>>();
            (method.clone(), dependencies)
        })
        .collect::<BTreeMap<_, _>>();

    let components = strongly_connected_components(&graph);
    let mut solved = BTreeMap::new();
    let mut telemetry = InferenceTelemetry::default();

    for component in components {
        let component_set = component.iter().cloned().collect::<BTreeSet<_>>();
        let recursive = component.len() > 1
            || component.iter().any(|method| {
                graph
                    .get(method)
                    .is_some_and(|dependencies| dependencies.contains(method))
            });

        if !recursive {
            let method = component.first().expect(
                "INVARIANT VIOLATED: the SCC solver produced an empty component. This is a bug because every component must own at least one method. Fix: keep Tarjan component emission paired with a popped root node.",
            );
            let approximation =
                evaluate_method(method, &grouped, &component_set, &BTreeMap::new(), &solved);
            solved.insert(method.clone(), approximation.into_outcome(false));
            continue;
        }

        telemetry.recursive_components = increment(
            telemetry.recursive_components,
            "recursive method-return component count",
        );
        telemetry.recursive_methods = telemetry
            .recursive_methods
            .checked_add(u64::try_from(component.len()).expect(
                "INVARIANT VIOLATED: recursive component size exceeded u64. This is a bug because one process cannot retain that many methods. Fix: bound equation collection below u64::MAX.",
            ))
            .expect(
                "INVARIANT VIOLATED: recursive method count exhausted u64. This is a bug because telemetry must remain exact. Fix: reset file-owned telemetry or widen the counter before overflow.",
            );

        let mut approximations = component
            .iter()
            .cloned()
            .map(|method| (method, Approximation::Bottom))
            .collect::<BTreeMap<_, _>>();
        let mut converged = false;

        for _iteration in 0..MAX_RECURSIVE_RETURN_ITERATIONS {
            telemetry.solver_iterations = increment(
                telemetry.solver_iterations,
                "method-return solver iteration count",
            );
            let next = component
                .iter()
                .map(|method| {
                    (
                        method.clone(),
                        evaluate_method(method, &grouped, &component_set, &approximations, &solved),
                    )
                })
                .collect::<BTreeMap<_, _>>();
            if next == approximations {
                approximations = next;
                converged = true;
                break;
            }
            approximations = next;
        }

        if !converged {
            telemetry.solver_bound_hits = increment(
                telemetry.solver_bound_hits,
                "method-return solver bound-hit count",
            );
        }

        for method in component {
            let outcome = if converged {
                approximations
                    .remove(&method)
                    .expect(
                        "INVARIANT VIOLATED: a recursive component lost a method approximation. This is a bug because every synchronous iteration must preserve the component key set. Fix: build each iteration from the complete sorted component.",
                    )
                    .into_outcome(true)
            } else {
                TypeInferenceOutcome::unknown(UnknownReason::UnprovenRecursiveCycle)
            };
            solved.insert(method, outcome);
        }
    }

    for outcome in solved.values() {
        telemetry.observe_method_return(outcome);
    }
    MethodReturnSolveResult {
        outcomes: solved,
        telemetry,
    }
}

fn increment(value: u64, counter: &str) -> u64 {
    value.checked_add(1).unwrap_or_else(|| {
        panic!(
            "INVARIANT VIOLATED: {counter} exhausted u64. This is a bug because telemetry must remain exact. Fix: reset file-owned telemetry or widen the counter before overflow."
        )
    })
}

fn evaluate_method(
    method: &FullyQualifiedName,
    grouped: &BTreeMap<FullyQualifiedName, Vec<&MethodReturnEquation>>,
    component: &BTreeSet<FullyQualifiedName>,
    approximations: &BTreeMap<FullyQualifiedName, Approximation>,
    solved: &BTreeMap<FullyQualifiedName, TypeInferenceOutcome>,
) -> Approximation {
    let definitions = grouped.get(method).expect(
        "INVARIANT VIOLATED: the return solver evaluated a method without an equation. This is a bug because SCC nodes must be derived from the grouped equation keys. Fix: keep graph and equation construction atomic.",
    );
    let mut alternatives = Vec::new();

    for definition in definitions {
        match definition.base() {
            MethodReturnBase::Bottom => {}
            MethodReturnBase::Proven(ruby_type) => alternatives.push(ruby_type.clone()),
            MethodReturnBase::Unknown(reason) => return Approximation::Unknown(*reason),
        }

        for dependency in definition.dependencies() {
            let dependency_approximation = if component.contains(dependency) {
                approximations.get(dependency).cloned().unwrap_or_else(|| {
                    panic!(
                        "INVARIANT VIOLATED: recursive dependency `{dependency}` has no approximation. This is a bug because the component key set must be initialized before evaluation. Fix: seed every SCC member with private bottom."
                    )
                })
            } else if let Some(outcome) = solved.get(dependency) {
                match outcome.proven_type() {
                    Some(ruby_type) => Approximation::Proven(ruby_type.clone()),
                    None => Approximation::Unknown(
                        outcome
                            .unknown_reason()
                            .unwrap_or(UnknownReason::UnresolvedMethodReturn),
                    ),
                }
            } else {
                Approximation::Unknown(UnknownReason::UnresolvedMethodReturn)
            };

            match dependency_approximation {
                Approximation::Bottom => {}
                Approximation::Proven(ruby_type) => alternatives.push(ruby_type),
                Approximation::Unknown(reason) => return Approximation::Unknown(reason),
            }
        }
    }

    if alternatives.is_empty() {
        Approximation::Bottom
    } else {
        let joined = RubyType::union(alternatives);
        if joined == RubyType::Unknown {
            Approximation::Unknown(UnknownReason::UnresolvedMethodReturn)
        } else {
            Approximation::Proven(joined)
        }
    }
}

fn strongly_connected_components(
    graph: &BTreeMap<FullyQualifiedName, BTreeSet<FullyQualifiedName>>,
) -> Vec<Vec<FullyQualifiedName>> {
    struct Tarjan<'a> {
        graph: &'a BTreeMap<FullyQualifiedName, BTreeSet<FullyQualifiedName>>,
        next_index: usize,
        indexes: BTreeMap<FullyQualifiedName, usize>,
        lowlinks: BTreeMap<FullyQualifiedName, usize>,
        stack: Vec<FullyQualifiedName>,
        on_stack: BTreeSet<FullyQualifiedName>,
        components: Vec<Vec<FullyQualifiedName>>,
    }

    impl Tarjan<'_> {
        fn visit(&mut self, method: FullyQualifiedName) {
            let index = self.next_index;
            self.next_index = self.next_index.checked_add(1).expect(
                "INVARIANT VIOLATED: the return-equation DFS index exhausted usize. This is a bug because the method graph cannot exceed addressable memory. Fix: bound collected method equations below usize::MAX.",
            );
            self.indexes.insert(method.clone(), index);
            self.lowlinks.insert(method.clone(), index);
            self.stack.push(method.clone());
            self.on_stack.insert(method.clone());

            let dependencies = self.graph.get(&method).expect(
                "INVARIANT VIOLATED: the SCC traversal reached a method missing from its graph. This is a bug because dependencies must be filtered to grouped equation keys. Fix: construct the deterministic graph before traversal.",
            );
            for dependency in dependencies {
                if !self.indexes.contains_key(dependency) {
                    self.visit(dependency.clone());
                    let dependency_lowlink = *self.lowlinks.get(dependency).expect(
                        "INVARIANT VIOLATED: a visited dependency has no lowlink. This is a bug because Tarjan must assign a lowlink before recursion returns. Fix: keep lowlink insertion before dependency traversal.",
                    );
                    let lowlink = self.lowlinks.get_mut(&method).expect(
                        "INVARIANT VIOLATED: the active Tarjan method lost its lowlink. This is a bug because active stack entries must remain indexed. Fix: do not remove lowlinks during traversal.",
                    );
                    *lowlink = (*lowlink).min(dependency_lowlink);
                } else if self.on_stack.contains(dependency) {
                    let dependency_index = *self.indexes.get(dependency).expect(
                        "INVARIANT VIOLATED: an on-stack dependency has no DFS index. This is a bug because stack membership begins after index insertion. Fix: update these structures atomically.",
                    );
                    let lowlink = self.lowlinks.get_mut(&method).expect(
                        "INVARIANT VIOLATED: the active Tarjan method lost its lowlink. This is a bug because active stack entries must remain indexed. Fix: do not remove lowlinks during traversal.",
                    );
                    *lowlink = (*lowlink).min(dependency_index);
                }
            }

            if self.lowlinks.get(&method) != self.indexes.get(&method) {
                return;
            }

            let mut component = Vec::new();
            loop {
                let member = self.stack.pop().expect(
                    "INVARIANT VIOLATED: Tarjan reached a component root with an empty stack. This is a bug because the root itself must remain active until component emission. Fix: pop only through the current root.",
                );
                self.on_stack.remove(&member);
                let finished = member == method;
                component.push(member);
                if finished {
                    break;
                }
            }
            component.sort();
            self.components.push(component);
        }
    }

    let mut tarjan = Tarjan {
        graph,
        next_index: 0,
        indexes: BTreeMap::new(),
        lowlinks: BTreeMap::new(),
        stack: Vec::new(),
        on_stack: BTreeSet::new(),
        components: Vec::new(),
    };
    for method in graph.keys() {
        if !tarjan.indexes.contains_key(method) {
            tarjan.visit(method.clone());
        }
    }
    tarjan.components
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{RubyConstant, RubyMethod};

    fn method(name: &str) -> FullyQualifiedName {
        FullyQualifiedName::method(
            vec![RubyConstant::new("Parity").unwrap()],
            RubyMethod::new(name).unwrap(),
        )
    }

    #[test]
    fn mutual_component_reaches_the_least_fixed_point() {
        let even = method("even");
        let odd = method("odd");
        let equations = [
            MethodReturnEquation::new(
                even.clone(),
                MethodReturnBase::Proven(RubyType::true_class()),
                [odd.clone()].into_iter().collect(),
            ),
            MethodReturnEquation::new(
                odd.clone(),
                MethodReturnBase::Proven(RubyType::false_class()),
                [even.clone()].into_iter().collect(),
            ),
        ];

        let solved = solve_method_return_equations(&equations);
        let boolean = RubyType::boolean();

        assert_eq!(solved[&even].proven_type(), Some(&boolean));
        assert_eq!(solved[&odd].proven_type(), Some(&boolean));
    }

    #[test]
    fn non_recursive_dependency_is_solved_before_its_consumer() {
        let provider = method("provider");
        let consumer = method("consumer");
        let equations = [
            MethodReturnEquation::new(
                consumer.clone(),
                MethodReturnBase::Bottom,
                [provider.clone()].into_iter().collect(),
            ),
            MethodReturnEquation::proven(provider.clone(), RubyType::string()),
        ];

        let solved = solve_method_return_equations(&equations);

        assert_eq!(solved[&provider].proven_type(), Some(&RubyType::string()));
        assert_eq!(solved[&consumer].proven_type(), Some(&RubyType::string()));
    }

    #[test]
    fn base_free_component_remains_unknown() {
        let left = method("left");
        let right = method("right");
        let equations = [
            MethodReturnEquation::new(
                left.clone(),
                MethodReturnBase::Bottom,
                [right.clone()].into_iter().collect(),
            ),
            MethodReturnEquation::new(
                right.clone(),
                MethodReturnBase::Bottom,
                [left.clone()].into_iter().collect(),
            ),
        ];

        let solved = solve_method_return_equations(&equations);

        assert_eq!(
            solved[&left].unknown_reason(),
            Some(UnknownReason::UnprovenRecursiveCycle)
        );
        assert_eq!(
            solved[&right].unknown_reason(),
            Some(UnknownReason::UnprovenRecursiveCycle)
        );
    }

    #[test]
    fn equation_order_does_not_change_a_mutual_solution() {
        let even = method("even");
        let odd = method("odd");
        let even_equation = MethodReturnEquation::new(
            even.clone(),
            MethodReturnBase::Proven(RubyType::true_class()),
            [odd.clone()].into_iter().collect(),
        );
        let odd_equation = MethodReturnEquation::new(
            odd.clone(),
            MethodReturnBase::Proven(RubyType::false_class()),
            [even.clone()].into_iter().collect(),
        );

        let forward = solve_method_return_equations(&[even_equation.clone(), odd_equation.clone()]);
        let reverse = solve_method_return_equations(&[odd_equation, even_equation]);

        assert_eq!(forward, reverse);
    }

    #[test]
    fn incomplete_recursive_base_poisoning_fails_closed() {
        let left = method("left");
        let right = method("right");
        let equations = [
            MethodReturnEquation::new(
                left.clone(),
                MethodReturnBase::Unknown(UnknownReason::UnresolvedMethodReturn),
                [right.clone()].into_iter().collect(),
            ),
            MethodReturnEquation::new(
                right.clone(),
                MethodReturnBase::Proven(RubyType::integer()),
                [left.clone()].into_iter().collect(),
            ),
        ];

        let solved = solve_method_return_equations(&equations);

        assert_eq!(
            solved[&left].unknown_reason(),
            Some(UnknownReason::UnprovenRecursiveCycle)
        );
        assert_eq!(
            solved[&right].unknown_reason(),
            Some(UnknownReason::UnprovenRecursiveCycle)
        );
    }

    #[test]
    fn union_with_unknown_member_equation_stays_unknown_instead_of_panicking() {
        // A YARD `@return [Array [Array, String]]` can produce an untyped
        // member beside a known one. The equation boundary must treat the
        // whole union as failed proof; the solver must never receive a
        // `Proven(Unknown)` approximation.
        let target = method("ambiguous");
        let equations = [MethodReturnEquation::from_ruby_type(
            target.clone(),
            RubyType::Union(vec![RubyType::Unknown, RubyType::string()]),
            UnknownReason::UnresolvedMethodReturn,
        )];

        let solved = solve_method_return_equations(&equations);

        assert_eq!(
            solved[&target].unknown_reason(),
            Some(UnknownReason::UnresolvedMethodReturn)
        );
        assert_eq!(solved[&target].proven_type(), None);
    }
}
