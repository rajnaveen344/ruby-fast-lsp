//! Deterministic fixed-point solving for value-constant type equations.

use std::collections::{BTreeMap, HashMap};

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

pub(crate) fn solve_constant_type_equations(
    equations: &[ConstantTypeEquation],
    constant_facts: &[ConstantFactInput],
    resolved_dependencies: &BTreeMap<ConstantTypeDependency, Option<ResolvedConstantDependency>>,
) -> Vec<(ConstantTypeTarget, RubyType)> {
    if equations.is_empty() {
        return Vec::new();
    }

    let mut equation_by_target = HashMap::with_capacity(equations.len());
    for (index, equation) in equations.iter().enumerate() {
        if let Some(previous) = equation_by_target.insert(equation.target().clone(), index) {
            assert_eq!(
                equations[previous], *equation,
                "INVARIANT VIOLATED: one exact type target has conflicting constant equations. This is a bug because one AST value owns one compact equation. Fix: merge dependency terms before publishing file evidence."
            );
        }
    }

    let mut latest_fact_by_constant: BTreeMap<FullyQualifiedName, ConstantFactInput> =
        BTreeMap::new();
    for fact in constant_facts {
        match latest_fact_by_constant.get(&fact.constant) {
            Some(previous) if previous.order >= fact.order => {}
            Some(_) | None => {
                latest_fact_by_constant.insert(fact.constant.clone(), fact.clone());
            }
        }
    }

    let mut values = vec![None; equations.len()];
    let iteration_bound = equations.len().checked_add(1).expect(
        "INVARIANT VIOLATED: constant equation iteration bound overflowed usize. This is a bug because retained equations already fit addressable memory. Fix: reject an equation set at usize::MAX entries.",
    );
    let mut converged = false;
    for _iteration in 0..iteration_bound {
        let mut next = Vec::with_capacity(equations.len());
        for equation in equations {
            let mut members = Vec::new();
            let mut waiting_on_bottom = false;
            let mut incomplete = false;
            for dependency in equation.dependencies() {
                let Some(Some(resolved)) = resolved_dependencies.get(dependency) else {
                    incomplete = true;
                    break;
                };
                if let ResolvedConstantDependency::Projected(ruby_type) = resolved {
                    assert_ne!(
                        *ruby_type,
                        RubyType::Unknown,
                        "INVARIANT VIOLATED: a projected constant dependency contains Unknown. This is a bug because unresolved projections must be represented by None. Fix: publish Projected only for a proven namespace/value type."
                    );
                    members.push(ruby_type.clone());
                    continue;
                }
                let ResolvedConstantDependency::Value(constant) = resolved else {
                    panic!(
                        "INVARIANT VIOLATED: an explicitly handled projected constant dependency reached value lookup. This is a bug because every dependency projection must have one exhaustive solver branch. Fix: add the missing explicit projection branch."
                    );
                };
                let Some(fact) = latest_fact_by_constant.get(constant) else {
                    incomplete = true;
                    break;
                };
                let value = match equation_by_target.get(&fact.target) {
                    Some(index) => values[*index].clone(),
                    None => (fact.ruby_type != RubyType::Unknown).then(|| fact.ruby_type.clone()),
                };
                match value {
                    Some(ruby_type) if ruby_type != RubyType::Unknown => members.push(ruby_type),
                    Some(_) => {
                        incomplete = true;
                        break;
                    }
                    None => waiting_on_bottom = true,
                }
            }
            next.push(if incomplete {
                Some(RubyType::Unknown)
            } else if waiting_on_bottom {
                None
            } else if members.is_empty() {
                None
            } else {
                Some(RubyType::union(members))
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
