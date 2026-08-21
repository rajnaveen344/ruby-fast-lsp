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
///
/// Working-set identity is a dense `MethodSolveId` assigned in
/// `FullyQualifiedName` sort order. Tarjan, component membership, and the
/// Jacobi iteration must not `Ord` or clone FQNs; those compares belong only
/// to intern construction and the public outcome map.
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

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct MethodSolveId(u32);

impl MethodSolveId {
    fn index(self) -> usize {
        usize::try_from(self.0).expect(
            "INVARIANT VIOLATED: method-return solve id does not fit usize. This is a bug because interned ids are assigned from a memory-resident equation set. Fix: keep MethodSolveId as a dense index into that set.",
        )
    }
}

pub(crate) fn solve_method_return_equations_with_telemetry(
    equations: &[MethodReturnEquation],
) -> MethodReturnSolveResult {
    let (methods, grouped, definition_dependencies, graph) = intern_method_equations(equations);
    let components = strongly_connected_components(&graph);
    let mut solved = vec![None; methods.len()];
    let mut telemetry = InferenceTelemetry::default();
    let empty_component: [MethodSolveId; 0] = [];
    let empty_approximations = Vec::new();

    for component in components {
        let recursive = component.len() > 1
            || component
                .first()
                .is_some_and(|method| graph[method.index()].contains(method));

        if !recursive {
            let method = *component.first().expect(
                "INVARIANT VIOLATED: the SCC solver produced an empty component. This is a bug because every component must own at least one method. Fix: keep Tarjan component emission paired with a popped root node.",
            );
            let approximation = evaluate_method(
                method,
                &grouped,
                &definition_dependencies,
                &empty_component,
                &empty_approximations,
                &solved,
            );
            solved[method.index()] = Some(approximation.into_outcome(false));
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

        let mut approximations = vec![Approximation::Bottom; component.len()];
        let mut converged = false;

        for _iteration in 0..MAX_RECURSIVE_RETURN_ITERATIONS {
            telemetry.solver_iterations = increment(
                telemetry.solver_iterations,
                "method-return solver iteration count",
            );
            let mut next = vec![Approximation::Bottom; component.len()];
            for (slot, method) in component.iter().enumerate() {
                next[slot] = evaluate_method(
                    *method,
                    &grouped,
                    &definition_dependencies,
                    &component,
                    &approximations,
                    &solved,
                );
            }
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

        for (slot, method) in component.into_iter().enumerate() {
            let outcome = if converged {
                std::mem::replace(&mut approximations[slot], Approximation::Bottom)
                    .into_outcome(true)
            } else {
                TypeInferenceOutcome::unknown(UnknownReason::UnprovenRecursiveCycle)
            };
            solved[method.index()] = Some(outcome);
        }
    }

    let mut outcomes = BTreeMap::new();
    for (index, method) in methods.into_iter().enumerate() {
        let outcome = solved[index].take().unwrap_or_else(|| {
            panic!(
                "INVARIANT VIOLATED: method-return solver omitted interned method `{method}`. This is a bug because every grouped method must produce exactly one proof outcome. Fix: keep SCC emission and result insertion exhaustive."
            )
        });
        telemetry.observe_method_return(&outcome);
        outcomes.insert(method.clone(), outcome);
    }
    MethodReturnSolveResult {
        outcomes,
        telemetry,
    }
}

fn intern_method_equations(
    equations: &[MethodReturnEquation],
) -> (
    Vec<&FullyQualifiedName>,
    Vec<Vec<&MethodReturnEquation>>,
    Vec<Vec<Vec<Option<MethodSolveId>>>>,
    Vec<BTreeSet<MethodSolveId>>,
) {
    let mut ordered = equations.iter().collect::<Vec<_>>();
    ordered.sort_by(|left, right| left.method().cmp(right.method()));

    let mut methods = Vec::new();
    let mut grouped: Vec<Vec<&MethodReturnEquation>> = Vec::new();
    for equation in ordered {
        match methods.last() {
            Some(method) if *method == equation.method() => {
                grouped
                    .last_mut()
                    .expect(
                        "INVARIANT VIOLATED: interned method-return grouping has a method without a definition list. This is a bug because a new interned method always pushes an empty group. Fix: push the method and its group together.",
                    )
                    .push(equation);
            }
            Some(_) | None => {
                let _id = u32::try_from(methods.len()).expect(
                    "INVARIANT VIOLATED: interned method-return equations exceeded u32::MAX distinct methods. This is a bug because one process cannot retain that many equations. Fix: bound equation collection below u32::MAX.",
                );
                methods.push(equation.method());
                grouped.push(vec![equation]);
            }
        }
    }

    let definition_dependencies = grouped
        .iter()
        .map(|definitions| {
            definitions
                .iter()
                .map(|definition| {
                    definition
                        .dependencies()
                        .iter()
                        .map(|dependency| method_solve_id(&methods, dependency))
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let graph = definition_dependencies
        .iter()
        .map(|definitions| {
            definitions
                .iter()
                .flatten()
                .filter_map(|dependency| *dependency)
                .collect::<BTreeSet<_>>()
        })
        .collect();
    (methods, grouped, definition_dependencies, graph)
}

fn method_solve_id(
    methods: &[&FullyQualifiedName],
    method: &FullyQualifiedName,
) -> Option<MethodSolveId> {
    methods.binary_search(&method).ok().map(|index| {
        MethodSolveId(u32::try_from(index).expect(
            "INVARIANT VIOLATED: interned method index exceeded u32. This is a bug because intern construction rejects sets larger than u32::MAX. Fix: keep MethodSolveId assignment aligned with intern_method_equations.",
        ))
    })
}

fn increment(value: u64, counter: &str) -> u64 {
    value.checked_add(1).unwrap_or_else(|| {
        panic!(
            "INVARIANT VIOLATED: {counter} exhausted u64. This is a bug because telemetry must remain exact. Fix: reset file-owned telemetry or widen the counter before overflow."
        )
    })
}

fn evaluate_method(
    method: MethodSolveId,
    grouped: &[Vec<&MethodReturnEquation>],
    definition_dependencies: &[Vec<Vec<Option<MethodSolveId>>>],
    component: &[MethodSolveId],
    approximations: &[Approximation],
    solved: &[Option<TypeInferenceOutcome>],
) -> Approximation {
    let definitions = grouped.get(method.index()).expect(
        "INVARIANT VIOLATED: the return solver evaluated a method without an equation. This is a bug because SCC nodes must be derived from the interned equation keys. Fix: keep graph and equation construction atomic.",
    );
    let interned_dependencies = definition_dependencies.get(method.index()).expect(
        "INVARIANT VIOLATED: interned method-return dependencies are missing for an equation group. This is a bug because dependency ids are assigned with the grouped definitions. Fix: keep intern_method_equations atomic.",
    );
    assert_eq!(
        definitions.len(),
        interned_dependencies.len(),
        "INVARIANT VIOLATED: interned method-return definitions and dependency lists drifted. This is a bug because both vectors are built from the same grouped equations. Fix: keep intern_method_equations atomic."
    );
    let mut alternatives = Vec::new();

    for (definition, dependencies) in definitions.iter().zip(interned_dependencies) {
        match definition.base() {
            MethodReturnBase::Bottom => {}
            MethodReturnBase::Proven(ruby_type) => alternatives.push(ruby_type.clone()),
            MethodReturnBase::Unknown(reason) => return Approximation::Unknown(*reason),
        }

        for dependency_id in dependencies {
            let Some(dependency_id) = *dependency_id else {
                return Approximation::Unknown(UnknownReason::UnresolvedMethodReturn);
            };
            let dependency_approximation = if let Ok(slot) = component.binary_search(&dependency_id)
            {
                approximations.get(slot).cloned().unwrap_or_else(|| {
                    panic!(
                        "INVARIANT VIOLATED: recursive dependency has no approximation. This is a bug because the component key set must be initialized before evaluation. Fix: seed every SCC member with private bottom."
                    )
                })
            } else if let Some(Some(outcome)) = solved.get(dependency_id.index()) {
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

fn strongly_connected_components(graph: &[BTreeSet<MethodSolveId>]) -> Vec<Vec<MethodSolveId>> {
    struct Tarjan<'a> {
        graph: &'a [BTreeSet<MethodSolveId>],
        next_index: usize,
        indexes: Vec<Option<usize>>,
        lowlinks: Vec<usize>,
        stack: Vec<MethodSolveId>,
        on_stack: Vec<bool>,
        components: Vec<Vec<MethodSolveId>>,
    }

    impl Tarjan<'_> {
        fn visit(&mut self, method: MethodSolveId) {
            let index = self.next_index;
            self.next_index = self.next_index.checked_add(1).expect(
                "INVARIANT VIOLATED: the return-equation DFS index exhausted usize. This is a bug because the method graph cannot exceed addressable memory. Fix: bound collected method equations below usize::MAX.",
            );
            let slot = method.index();
            self.indexes[slot] = Some(index);
            self.lowlinks[slot] = index;
            self.stack.push(method);
            self.on_stack[slot] = true;

            for dependency in &self.graph[slot] {
                let dependency_slot = dependency.index();
                if self.indexes[dependency_slot].is_none() {
                    self.visit(*dependency);
                    self.lowlinks[slot] = self.lowlinks[slot].min(self.lowlinks[dependency_slot]);
                } else if self.on_stack[dependency_slot] {
                    let dependency_index = self.indexes[dependency_slot].expect(
                        "INVARIANT VIOLATED: an on-stack dependency has no DFS index. This is a bug because stack membership begins after index insertion. Fix: update these structures atomically.",
                    );
                    self.lowlinks[slot] = self.lowlinks[slot].min(dependency_index);
                }
            }

            if self.lowlinks[slot] != self.indexes[slot].expect(
                "INVARIANT VIOLATED: the active Tarjan method lost its DFS index. This is a bug because active stack entries must remain indexed. Fix: do not clear indexes during traversal.",
            ) {
                return;
            }

            let mut component = Vec::new();
            loop {
                let member = self.stack.pop().expect(
                    "INVARIANT VIOLATED: Tarjan reached a component root with an empty stack. This is a bug because the root itself must remain active until component emission. Fix: pop only through the current root.",
                );
                self.on_stack[member.index()] = false;
                let finished = member == method;
                component.push(member);
                if finished {
                    break;
                }
            }
            component.sort_unstable();
            self.components.push(component);
        }
    }

    let method_count = graph.len();
    let mut tarjan = Tarjan {
        graph,
        next_index: 0,
        indexes: vec![None; method_count],
        lowlinks: vec![0; method_count],
        stack: Vec::new(),
        on_stack: vec![false; method_count],
        components: Vec::new(),
    };
    for index in 0..method_count {
        let method = MethodSolveId(u32::try_from(index).expect(
            "INVARIANT VIOLATED: interned method index exceeded u32 during SCC roots. This is a bug because intern construction rejects sets larger than u32::MAX. Fix: keep MethodSolveId assignment aligned with intern_method_equations.",
        ));
        if tarjan.indexes[index].is_none() {
            tarjan.visit(method);
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

    #[test]
    fn unsorted_input_still_solves_a_non_recursive_chain() {
        let alpha = method("alpha");
        let mu = method("mu");
        let zeta = method("zeta");
        let equations = [
            MethodReturnEquation::new(
                zeta.clone(),
                MethodReturnBase::Bottom,
                [mu.clone()].into_iter().collect(),
            ),
            MethodReturnEquation::new(
                mu.clone(),
                MethodReturnBase::Bottom,
                [alpha.clone()].into_iter().collect(),
            ),
            MethodReturnEquation::proven(alpha.clone(), RubyType::integer()),
        ];

        let solved = solve_method_return_equations(&equations);

        assert_eq!(solved[&alpha].proven_type(), Some(&RubyType::integer()));
        assert_eq!(solved[&mu].proven_type(), Some(&RubyType::integer()));
        assert_eq!(solved[&zeta].proven_type(), Some(&RubyType::integer()));
    }

    #[test]
    fn external_dependency_without_an_equation_stays_unknown() {
        let present = method("present");
        let absent = method("absent");
        let equations = [MethodReturnEquation::new(
            present.clone(),
            MethodReturnBase::Bottom,
            [absent].into_iter().collect(),
        )];

        let solved = solve_method_return_equations(&equations);

        assert_eq!(
            solved[&present].unknown_reason(),
            Some(UnknownReason::UnresolvedMethodReturn)
        );
        assert!(!solved.contains_key(&method("absent")));
    }

    #[test]
    fn reopened_definitions_join_as_one_method() {
        let target = method("reopened");
        let equations = [
            MethodReturnEquation::proven(target.clone(), RubyType::integer()),
            MethodReturnEquation::proven(target.clone(), RubyType::string()),
        ];

        let solved = solve_method_return_equations(&equations);

        assert_eq!(
            solved[&target].proven_type(),
            Some(&RubyType::union([RubyType::integer(), RubyType::string()]))
        );
    }
}
