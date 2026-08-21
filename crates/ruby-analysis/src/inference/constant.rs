//! Deterministic fixed-point solving for value-constant type equations.
//!
//! Working-set identity is interned before Jacobi iteration. Latest facts are
//! ordered by `FullyQualifiedName`, equation targets by `ConstantTypeTarget`,
//! and the iteration itself uses integer equation indexes. FQN `Ord`/clone and
//! `ConstantTypeTarget` hashing belong only to intern construction.

use std::collections::BTreeMap;

use crate::core::{
    ConstantTypeDependency, ConstantTypeEquation, ConstantTypeTarget, FullyQualifiedName, RubyType,
};

#[derive(Debug, Clone)]
pub(crate) struct ConstantFactInput {
    pub constant: FullyQualifiedName,
    pub target: ConstantTypeTarget,
    pub ruby_type: RubyType,
    pub order: (u32, u32, u32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ResolvedConstantDependency {
    Value(FullyQualifiedName),
    Projected(RubyType),
}

#[derive(Clone, Copy)]
enum PreparedDependency<'a> {
    Incomplete,
    Projected(&'a RubyType),
    Equation(usize),
    Known(&'a RubyType),
}

pub(crate) fn solve_constant_type_equations(
    equations: &[ConstantTypeEquation],
    constant_facts: &[ConstantFactInput],
    resolved_dependencies: &BTreeMap<ConstantTypeDependency, Option<ResolvedConstantDependency>>,
) -> Vec<(ConstantTypeTarget, RubyType)> {
    if equations.is_empty() {
        return Vec::new();
    }

    let equation_indexes = equation_indexes_by_target(equations);
    let latest = latest_facts(constant_facts);
    let prepared =
        prepare_dependencies(equations, &latest, &equation_indexes, resolved_dependencies);

    let mut values: Vec<Option<RubyType>> = vec![None; equations.len()];
    let iteration_bound = equations.len().checked_add(1).expect(
        "INVARIANT VIOLATED: constant equation iteration bound overflowed usize. This is a bug because retained equations already fit addressable memory. Fix: reject an equation set at usize::MAX entries.",
    );
    let mut converged = false;
    for _iteration in 0..iteration_bound {
        let mut next = Vec::with_capacity(equations.len());
        for members in &prepared {
            let mut union_members = Vec::new();
            let mut waiting_on_bottom = false;
            let mut incomplete = false;
            for dependency in members {
                match *dependency {
                    PreparedDependency::Incomplete => {
                        incomplete = true;
                        break;
                    }
                    PreparedDependency::Projected(ruby_type)
                    | PreparedDependency::Known(ruby_type) => {
                        union_members.push(ruby_type.clone());
                    }
                    PreparedDependency::Equation(index) => match &values[index] {
                        Some(ruby_type) if *ruby_type != RubyType::Unknown => {
                            union_members.push(ruby_type.clone());
                        }
                        Some(_) => {
                            incomplete = true;
                            break;
                        }
                        None => waiting_on_bottom = true,
                    },
                }
            }
            next.push(if incomplete {
                Some(RubyType::Unknown)
            } else if waiting_on_bottom {
                None
            } else if union_members.is_empty() {
                None
            } else {
                Some(RubyType::union(union_members))
            });
        }
        if next == values {
            converged = true;
            break;
        }
        values = next;
    }
    assert!(
        converged,
        "INVARIANT VIOLATED: monotone constant type equations did not converge within N+1 iterations. This is a bug because every dependency step can expose at most one previously Bottom target. Fix: keep the equation domain monotone or replace the bound with a proven SCC solver."
    );

    equations
        .iter()
        .zip(values)
        .map(|(equation, value)| {
            (
                equation.target().clone(),
                value.unwrap_or(RubyType::Unknown),
            )
        })
        .collect()
}

fn equation_indexes_by_target(equations: &[ConstantTypeEquation]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..equations.len()).collect();
    order.sort_by(|&left, &right| {
        equations[left]
            .target()
            .cmp(equations[right].target())
            .then(left.cmp(&right))
    });
    let mut unique: Vec<usize> = Vec::new();
    for index in order {
        if let Some(&previous) = unique.last() {
            if equations[previous].target() == equations[index].target() {
                assert_eq!(
                    equations[previous], equations[index],
                    "INVARIANT VIOLATED: one exact type target has conflicting constant equations. This is a bug because one AST value owns one compact equation. Fix: merge dependency terms before publishing file evidence."
                );
                *unique.last_mut().expect(
                    "INVARIANT VIOLATED: duplicate constant equation target compaction lost the previous index. This is a bug because last() was Some immediately before replacement. Fix: compact unique targets in one pass.",
                ) = index;
                continue;
            }
        }
        unique.push(index);
    }
    unique
}

fn equation_index_for_target(
    equations: &[ConstantTypeEquation],
    indexes: &[usize],
    target: &ConstantTypeTarget,
) -> Option<usize> {
    indexes
        .binary_search_by(|&index| equations[index].target().cmp(target))
        .ok()
        .map(|slot| indexes[slot])
}

fn latest_facts(facts: &[ConstantFactInput]) -> Vec<&ConstantFactInput> {
    let mut order: Vec<usize> = (0..facts.len()).collect();
    order.sort_by(|&left, &right| {
        facts[left]
            .constant
            .cmp(&facts[right].constant)
            .then(facts[left].order.cmp(&facts[right].order))
            .then(left.cmp(&right))
    });
    let mut latest: Vec<&ConstantFactInput> = Vec::new();
    for index in order {
        let fact = &facts[index];
        match latest.last_mut() {
            Some(previous) if previous.constant == fact.constant => {
                if fact.order > previous.order {
                    *previous = fact;
                }
            }
            Some(_) | None => latest.push(fact),
        }
    }
    latest
}

fn latest_fact_for<'a>(
    latest: &[&'a ConstantFactInput],
    constant: &FullyQualifiedName,
) -> Option<&'a ConstantFactInput> {
    latest
        .binary_search_by(|fact| fact.constant.cmp(constant))
        .ok()
        .map(|index| latest[index])
}

fn prepare_dependencies<'a>(
    equations: &'a [ConstantTypeEquation],
    latest: &[&'a ConstantFactInput],
    equation_indexes: &[usize],
    resolved_dependencies: &'a BTreeMap<ConstantTypeDependency, Option<ResolvedConstantDependency>>,
) -> Vec<Vec<PreparedDependency<'a>>> {
    equations
        .iter()
        .map(|equation| {
            equation
                .dependencies()
                .iter()
                .map(|dependency| match resolved_dependencies.get(dependency) {
                    Some(Some(ResolvedConstantDependency::Projected(ruby_type))) => {
                        assert_ne!(
                            *ruby_type,
                            RubyType::Unknown,
                            "INVARIANT VIOLATED: a projected constant dependency contains Unknown. This is a bug because unresolved projections must be represented by None. Fix: publish Projected only for a proven namespace/value type."
                        );
                        PreparedDependency::Projected(ruby_type)
                    }
                    Some(Some(ResolvedConstantDependency::Value(constant))) => {
                        let Some(fact) = latest_fact_for(latest, constant) else {
                            return PreparedDependency::Incomplete;
                        };
                        match equation_index_for_target(equations, equation_indexes, &fact.target) {
                            Some(index) => PreparedDependency::Equation(index),
                            None if fact.ruby_type != RubyType::Unknown => {
                                PreparedDependency::Known(&fact.ruby_type)
                            }
                            None => PreparedDependency::Incomplete,
                        }
                    }
                    Some(None) | None => PreparedDependency::Incomplete,
                })
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::core::{RubyConstant, SourceFileId, TextRange, TypeSubject};

    fn constant(name: &str) -> FullyQualifiedName {
        FullyQualifiedName::constant(vec![RubyConstant::new(name).unwrap()])
    }

    fn value_dep(name: &str) -> ConstantTypeDependency {
        ConstantTypeDependency::new(vec![RubyConstant::new(name).unwrap()], true, Vec::new())
    }

    fn fact_target(name: &str, start: u32) -> ConstantTypeTarget {
        ConstantTypeTarget::Fact {
            subject: TypeSubject::Constant(constant(name)),
            range: TextRange::new(SourceFileId(1), start, start + 4),
        }
    }

    fn fact(name: &str, start: u32, ruby_type: RubyType) -> ConstantFactInput {
        ConstantFactInput {
            constant: constant(name),
            target: fact_target(name, start),
            ruby_type,
            order: (1, start, start + 4),
        }
    }

    fn equation(name: &str, start: u32, dependency: &str) -> ConstantTypeEquation {
        ConstantTypeEquation::dependency(fact_target(name, start), value_dep(dependency))
    }

    fn resolved_value(name: &str) -> (ConstantTypeDependency, Option<ResolvedConstantDependency>) {
        (
            value_dep(name),
            Some(ResolvedConstantDependency::Value(constant(name))),
        )
    }

    fn outcome_map(
        outcomes: Vec<(ConstantTypeTarget, RubyType)>,
    ) -> BTreeMap<ConstantTypeTarget, RubyType> {
        outcomes.into_iter().collect()
    }

    #[test]
    fn unsorted_facts_still_solve_a_non_recursive_chain() {
        let equations = [equation("Zeta", 30, "Mu"), equation("Mu", 20, "Alpha")];
        let facts = [
            fact("Zeta", 30, RubyType::Unknown),
            fact("Mu", 20, RubyType::Unknown),
            fact("Alpha", 10, RubyType::integer()),
        ];
        let resolved = BTreeMap::from([resolved_value("Mu"), resolved_value("Alpha")]);

        let solved = outcome_map(solve_constant_type_equations(&equations, &facts, &resolved));

        assert_eq!(solved[&fact_target("Mu", 20)], RubyType::integer());
        assert_eq!(solved[&fact_target("Zeta", 30)], RubyType::integer());
    }

    #[test]
    fn external_dependency_without_a_fact_stays_unknown() {
        let equations = [equation("Present", 10, "Absent")];
        let facts = [fact("Present", 10, RubyType::Unknown)];
        let resolved = BTreeMap::from([resolved_value("Absent")]);

        let solved = outcome_map(solve_constant_type_equations(&equations, &facts, &resolved));

        assert_eq!(solved[&fact_target("Present", 10)], RubyType::Unknown);
    }

    #[test]
    fn later_range_fact_wins_for_the_same_constant() {
        let equations = [equation("Alias", 40, "Source")];
        let facts = [
            fact("Source", 20, RubyType::string()),
            fact("Source", 0, RubyType::integer()),
            fact("Alias", 40, RubyType::Unknown),
        ];
        let resolved = BTreeMap::from([resolved_value("Source")]);

        let solved = outcome_map(solve_constant_type_equations(&equations, &facts, &resolved));

        assert_eq!(solved[&fact_target("Alias", 40)], RubyType::string());
    }

    #[test]
    fn equal_order_keeps_the_first_fact() {
        let equations = [equation("Alias", 40, "Source")];
        let first = ConstantFactInput {
            constant: constant("Source"),
            target: fact_target("Source", 10),
            ruby_type: RubyType::integer(),
            order: (1, 10, 14),
        };
        let second = ConstantFactInput {
            constant: constant("Source"),
            target: fact_target("Source", 20),
            ruby_type: RubyType::string(),
            order: (1, 10, 14),
        };
        let facts = [first, second, fact("Alias", 40, RubyType::Unknown)];
        let resolved = BTreeMap::from([resolved_value("Source")]);

        let solved = outcome_map(solve_constant_type_equations(&equations, &facts, &resolved));

        assert_eq!(solved[&fact_target("Alias", 40)], RubyType::integer());
    }

    #[test]
    fn projected_dependency_uses_the_proven_type() {
        let dependency = ConstantTypeDependency::constructor(
            vec![RubyConstant::new("User").unwrap()],
            true,
            Vec::new(),
        );
        let target = fact_target("Built", 10);
        let equations = [ConstantTypeEquation::dependency(
            target.clone(),
            dependency.clone(),
        )];
        let facts = [fact("Built", 10, RubyType::Unknown)];
        let user = constant("User");
        let resolved = BTreeMap::from([(
            dependency,
            Some(ResolvedConstantDependency::Projected(RubyType::Class(
                user.clone(),
            ))),
        )]);

        let solved = outcome_map(solve_constant_type_equations(&equations, &facts, &resolved));

        assert_eq!(solved[&target], RubyType::Class(user));
    }

    #[test]
    fn equation_consumes_another_equation_after_one_jacobi_step() {
        let equations = [equation("Mid", 20, "Base"), equation("Leaf", 30, "Mid")];
        let facts = [
            fact("Base", 10, RubyType::string()),
            fact("Mid", 20, RubyType::Unknown),
            fact("Leaf", 30, RubyType::Unknown),
        ];
        let resolved = BTreeMap::from([resolved_value("Base"), resolved_value("Mid")]);

        let solved = outcome_map(solve_constant_type_equations(&equations, &facts, &resolved));

        assert_eq!(solved[&fact_target("Mid", 20)], RubyType::string());
        assert_eq!(solved[&fact_target("Leaf", 30)], RubyType::string());
    }
}
