//! `Solver`: the inference evidence each file owns, the dirty flags that
//! schedule equation solving, and constant-type and method-return equation
//! solving. Each solve is a read-only plan over the engine followed by an
//! `apply` step that writes the solution into the `TypeTable`.

use crate::invariant::ExpectInvariant;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::mem::size_of;

use crate::core::{
    ConstantTypeDependency, ConstantTypeEquation, ConstantTypeProjection, ConstantTypeTarget,
    FullyQualifiedName, GraphNodeKind, InferenceEvidence, InferenceTelemetry, MethodReturnEquation,
    RubyType, SourceFileId, TextRange, TypeFact, TypeInferenceOutcome, TypeProvenance, TypeSubject,
    UnknownReason,
};

use super::types::{TypeInferenceOutcomeRef, TypeTable};
use super::AnalysisEngine;
use crate::core::callables::callable_body::CallableBodySummary;
use crate::engine::AnalysisQuery;
use crate::inference::constant::{
    solve_constant_type_equations, ConstantFactInput, ResolvedConstantDependency,
};
use crate::inference::method::constructor::ConstructorResult;
use crate::inference::method::recursive::{
    solve_method_return_equations_with_telemetry, MethodReturnSolveResult,
};

fn resolve_constant_dependency(
    query: &AnalysisQuery<'_>,
    dependency: &ConstantTypeDependency,
) -> Option<ResolvedConstantDependency> {
    let context = if dependency.absolute {
        &[][..]
    } else {
        dependency.lexical_context.as_slice()
    };
    let resolved = query.resolve_constant_in_context(&dependency.parts, context)?;
    let constant = FullyQualifiedName::constant(resolved.namespace_parts());
    match dependency.projection() {
        ConstantTypeProjection::Value => Some(ResolvedConstantDependency::Value(constant)),
        ConstantTypeProjection::ConstructorInstance => {
            if let Some(value_type) = query.constant_value_type(&constant) {
                return match value_type {
                    RubyType::ClassReference(target) => {
                        let instance = RubyType::Class(target.clone());
                        constructed_dependency(query.constructor_result(&target), instance)
                    }
                    RubyType::Class(_)
                    | RubyType::Module(_)
                    | RubyType::ModuleReference(_)
                    | RubyType::Literal(_)
                    | RubyType::Array(_)
                    | RubyType::Hash(_, _)
                    | RubyType::Shape(_)
                    | RubyType::Union(_)
                    | RubyType::Unknown => None,
                };
            }
            let namespace = FullyQualifiedName::namespace(constant.namespace_parts());
            match query.namespace_node_kind(&namespace) {
                Some(GraphNodeKind::Class) => {
                    let result = query.constructor_result(&constant);
                    constructed_dependency(result, RubyType::Class(constant))
                }
                Some(GraphNodeKind::Module) | None => None,
            }
        }
    }
}

/// Project `Receiver.new` only when it proves a type. A declared factory
/// without a proven result leaves the dependency incomplete, so its target
/// stays Unknown instead of claiming an instance of the receiver.
fn constructed_dependency(
    result: ConstructorResult,
    instance: RubyType,
) -> Option<ResolvedConstantDependency> {
    result
        .into_type_outcome(instance)
        .into_proven_type()
        .map(ResolvedConstantDependency::Projected)
}

pub(in crate::engine) fn resolve_constant_dependency_type(
    query: &AnalysisQuery<'_>,
    dependency: &ConstantTypeDependency,
) -> Option<RubyType> {
    match resolve_constant_dependency(query, dependency)? {
        ResolvedConstantDependency::Value(constant) => query.constant_value_type(&constant),
        ResolvedConstantDependency::Projected(ruby_type) => Some(ruby_type),
    }
}

#[derive(Debug, Clone, Default)]
pub(in crate::engine) struct Solver {
    evidence_by_file: HashMap<SourceFileId, InferenceEvidence>,
    method_return_equations_dirty: bool,
    constant_type_equations_dirty: bool,
    method_return_solution_spans_files: bool,
}

/// The read-only result of planning a constant-type equation solve.
pub(in crate::engine) enum ConstantEquationPlan {
    /// No equation changed since the last solve.
    Clean,
    /// No constant-type equation exists.
    NoEquations,
    /// Solved targets to write, in solver order.
    Solved(Vec<(ConstantTypeTarget, RubyType)>),
}

/// The read-only result of planning a method-return equation solve.
pub(in crate::engine) enum MethodReturnEquationPlan {
    /// No equation changed since the last solve.
    Clean,
    /// One file owns every equation and its file-local solution is current.
    FileLocal,
    /// No method-return equation exists.
    NoEquations,
    /// The solved outcomes for the equations owned by `file_ids`, sorted.
    Solved {
        file_ids: Vec<SourceFileId>,
        result: MethodReturnSolveResult,
    },
}

impl Solver {
    pub(in crate::engine) fn has_evidence(&self, file_id: SourceFileId) -> bool {
        self.evidence_by_file.contains_key(&file_id)
    }

    pub(in crate::engine) fn evidence(&self, file_id: SourceFileId) -> Option<&InferenceEvidence> {
        self.evidence_by_file.get(&file_id)
    }

    /// Before a file's facts replace its previous ones, keep the previous
    /// solved method-return outcomes and telemetry when the file's
    /// method-return equations did not change. Returns whether they changed.
    pub(in crate::engine) fn carry_over_unchanged_solution(
        &self,
        file_id: SourceFileId,
        inference: &mut InferenceEvidence,
        types: &mut [TypeFact],
    ) -> bool {
        let equations_changed = match self.evidence_by_file.get(&file_id) {
            Some(previous) => previous.method_return_equations != inference.method_return_equations,
            None => !inference.method_return_equations.is_empty(),
        };
        if !equations_changed {
            if let Some(previous) = self.evidence_by_file.get(&file_id) {
                inference.method_return_outcomes = previous.method_return_outcomes.clone();
                inference.telemetry = previous.telemetry.clone();
                for fact in types {
                    if fact.provenance != TypeProvenance::Inferred {
                        continue;
                    }
                    let TypeSubject::MethodReturn(method) = &fact.subject else {
                        continue;
                    };
                    if let Some(outcome) = previous.method_return_outcomes.get(method) {
                        fact.ruby_type = outcome.clone().into_ruby_type();
                    }
                }
            }
        }
        equations_changed
    }

    /// Install one file's inference evidence and mark the equations that
    /// need solving.
    pub(in crate::engine) fn replace_file(
        &mut self,
        file_id: SourceFileId,
        evidence: InferenceEvidence,
        equations_changed: bool,
    ) {
        self.evidence_by_file.insert(file_id, evidence);
        self.method_return_equations_dirty |= equations_changed;
        self.constant_type_equations_dirty = self.evidence_by_file.values().any(|evidence| {
            !evidence.constant_type_equations.is_empty()
                || evidence
                    .method_return_equations
                    .iter()
                    .any(|equation| !equation.constant_dependencies().is_empty())
        });
    }

    pub(in crate::engine) fn refresh_retained_shape_telemetry(
        &mut self,
        file_id: SourceFileId,
        types: &TypeTable,
    ) {
        let mut observed = InferenceTelemetry::default();
        for ruby_type in types.ruby_types_in_file(file_id) {
            observed.observe_retained_type(ruby_type);
        }
        if self.has_evidence(file_id) {
            for (_, ruby_type) in types.local_read_type_views_in_file(file_id) {
                observed.observe_retained_type(ruby_type);
            }
        }
        if let Some(evidence) = self.evidence_by_file.get(&file_id) {
            for outcome in evidence.method_return_outcomes.values() {
                if let Some(ruby_type) = outcome.proven_type() {
                    observed.observe_retained_type(ruby_type);
                }
                if let Some(reason) = outcome.unknown_reason() {
                    observed.observe_shape_unknown(reason);
                }
            }
            for (_, reason) in &evidence.expression_unknown_reasons {
                observed.observe_shape_unknown(*reason);
            }
        }
        if let Some(outcomes) = types.call_expression_outcome_views_in_file(file_id) {
            for (_, outcome) in outcomes {
                match outcome {
                    TypeInferenceOutcomeRef::Proven(ruby_type) => {
                        observed.observe_retained_type(ruby_type);
                    }
                    TypeInferenceOutcomeRef::Unknown(reason) => {
                        observed.observe_shape_unknown(reason);
                    }
                }
            }
        }
        self.evidence_by_file
            .get_mut(&file_id)
            .expect_invariant(
                "retained shape telemetry lost its file-owned inference evidence",
                "the refresh runs only after atomic evidence insertion",
                "keep telemetry refresh inside the file replacement lifecycle",
            )
            .telemetry
            .replace_retained_shape_observations(&observed);
    }

    fn has_method_return_constant_dependencies(&self) -> bool {
        self.evidence_by_file.values().any(|evidence| {
            evidence
                .method_return_equations
                .iter()
                .any(|equation| !equation.constant_dependencies().is_empty())
        })
    }

    /// Collect the project's constant-type equations and solve them against
    /// the current constant facts and resolved dependencies. Writes nothing.
    pub(in crate::engine) fn plan_constant_type_equations(
        &self,
        query: &AnalysisQuery<'_>,
        types: &TypeTable,
    ) -> ConstantEquationPlan {
        if !self.constant_type_equations_dirty {
            return ConstantEquationPlan::Clean;
        }
        let mut equations = self
            .evidence_by_file
            .values()
            .flat_map(|evidence| evidence.constant_type_equations.iter().cloned())
            .collect::<Vec<ConstantTypeEquation>>();
        equations.sort();
        equations.dedup();
        if equations.is_empty() {
            return ConstantEquationPlan::NoEquations;
        }

        let mut dependencies = equations
            .iter()
            .flat_map(|equation| equation.dependencies().iter().cloned())
            .collect::<Vec<ConstantTypeDependency>>();
        dependencies.extend(
            self.evidence_by_file
                .values()
                .flat_map(|evidence| evidence.method_return_equations.iter())
                .flat_map(|equation| equation.constant_dependencies().iter().cloned()),
        );
        dependencies.sort();
        dependencies.dedup();
        let resolved_dependencies = dependencies
            .into_iter()
            .map(|dependency| {
                let resolved = resolve_constant_dependency(query, &dependency);
                (dependency, resolved)
            })
            .collect::<BTreeMap<_, _>>();
        let constant_facts = types
            .constant_type_facts()
            .map(|(constant, range, ruby_type)| ConstantFactInput {
                constant: constant.clone(),
                target: ConstantTypeTarget::Fact {
                    subject: TypeSubject::Constant(constant.clone()),
                    range,
                },
                ruby_type: ruby_type.clone(),
                order: (range.file_id.0, range.start_byte, range.end_byte),
            })
            .collect::<Vec<_>>();
        ConstantEquationPlan::Solved(solve_constant_type_equations(
            &equations,
            &constant_facts,
            &resolved_dependencies,
        ))
    }

    /// Write a planned constant-type solution into `types` and schedule the
    /// method-return equations that depend on constants. Returns whether a
    /// solution was written.
    pub(in crate::engine) fn apply_constant_type_equations(
        &mut self,
        types: &mut TypeTable,
        plan: ConstantEquationPlan,
    ) -> bool {
        let outcomes = match plan {
            ConstantEquationPlan::Clean => return false,
            ConstantEquationPlan::NoEquations => {
                if self.has_method_return_constant_dependencies() {
                    self.method_return_equations_dirty = true;
                }
                self.constant_type_equations_dirty = false;
                return false;
            }
            ConstantEquationPlan::Solved(outcomes) => outcomes,
        };
        for (target, ruby_type) in outcomes {
            match target {
                ConstantTypeTarget::Fact { subject, range } => {
                    let updated = types.update_equation_target(&subject, range, ruby_type);
                    invariant!(
                        updated > 0,
                        what = "constant equation target {subject:?} has no matching type fact at {range:?}",
                        why = "equations and facts must be replaced atomically with their file",
                        fix = "emit the equation from the same write path as its target TypeFact",
                        subject = subject,
                        range = range,
                    );
                }
                ConstantTypeTarget::LocalAssignment { name, range } => {
                    let updated =
                        types.update_local_assignment_equation_target(&name, range, ruby_type);
                    invariant!(
                        updated > 0,
                        what = "local constructor equation target {name:?} has no assignment fact at {range:?}",
                        why = "the assignment and its equation are replaced atomically",
                        fix = "emit the fact and equation from the same local-write path",
                        name = name,
                        range = range,
                    );
                }
                ConstantTypeTarget::LocalRead(range) => {
                    types.update_constant_local_read(range, ruby_type);
                }
            }
        }
        if self.has_method_return_constant_dependencies() {
            self.method_return_equations_dirty = true;
        }
        self.constant_type_equations_dirty = false;
        true
    }

    /// Collect the project's method-return equations with their constant
    /// dependencies resolved and solve them. Writes nothing.
    ///
    /// Equations contain no AST nodes and follow the same per-file replacement
    /// lifecycle as the inferred type facts they update.
    pub(in crate::engine) fn plan_method_return_equations(
        &self,
        query: &AnalysisQuery<'_>,
    ) -> MethodReturnEquationPlan {
        if !self.method_return_equations_dirty {
            return MethodReturnEquationPlan::Clean;
        }

        let mut file_ids = self
            .evidence_by_file
            .iter()
            .filter_map(|(file_id, evidence)| {
                (!evidence.method_return_equations.is_empty()).then_some(*file_id)
            })
            .collect::<Vec<_>>();
        file_ids.sort_unstable();

        let has_constant_dependencies = file_ids.iter().any(|file_id| {
            self.evidence_by_file
                .get(file_id)
                .expect_invariant(
                    "a selected method-equation file disappeared while checking constant dependencies",
                    "resolution owns the engine write lock",
                    "keep equation selection and dependency inspection in one immutable phase",
                )
                .method_return_equations
                .iter()
                .any(|equation| !equation.constant_dependencies().is_empty())
        });
        if file_ids.len() == 1
            && !self.method_return_solution_spans_files
            && !has_constant_dependencies
        {
            return MethodReturnEquationPlan::FileLocal;
        }

        let equations = file_ids
            .iter()
            .flat_map(|file_id| {
                self.evidence_by_file
                    .get(file_id)
                    .expect_invariant(
                        "a selected method-equation file disappeared during immutable collection",
                        "resolution owns the engine write lock",
                        "keep equation collection inside one resolution pass",
                    )
                    .method_return_equations
                    .iter()
                    .map(|equation| {
                        equation.with_resolved_constant_types(
                            equation.constant_dependencies().iter().map(|dependency| {
                                resolve_constant_dependency_type(query, dependency)
                            }),
                        )
                    })
            })
            .collect::<Vec<_>>();

        if equations.is_empty() {
            return MethodReturnEquationPlan::NoEquations;
        }

        MethodReturnEquationPlan::Solved {
            file_ids,
            result: solve_method_return_equations_with_telemetry(&equations),
        }
    }

    /// Write a planned method-return solution into `types` and each owning
    /// file's evidence. Returns whether a solution was written.
    pub(in crate::engine) fn apply_method_return_equations(
        &mut self,
        types: &mut TypeTable,
        plan: MethodReturnEquationPlan,
    ) -> bool {
        let (file_ids, solve_result) = match plan {
            MethodReturnEquationPlan::Clean => return false,
            MethodReturnEquationPlan::FileLocal => {
                self.method_return_equations_dirty = false;
                return false;
            }
            MethodReturnEquationPlan::NoEquations => {
                self.method_return_equations_dirty = false;
                self.method_return_solution_spans_files = false;
                return false;
            }
            MethodReturnEquationPlan::Solved { file_ids, result } => (file_ids, result),
        };
        for file_id in &file_ids {
            let methods = self
                .evidence_by_file
                .get(file_id)
                .expect_invariant(
                    "a method-equation owner disappeared before result projection",
                    "resolution owns the engine write lock",
                    "keep equation solving and projection atomic",
                )
                .method_return_equations
                .iter()
                .map(|equation| equation.method().clone())
                .collect::<BTreeSet<_>>();
            let outcomes = methods
                .iter()
                .map(|method| {
                    let outcome = solve_result.outcomes.get(method).unwrap_or_else(|| {
                        unreachable_invariant!(
                            what = "method-return solver omitted equation `{method}`",
                            why = "every grouped method must produce exactly one proof outcome",
                            fix = "keep SCC emission and result insertion exhaustive",
                            method = method,
                        )
                    });
                    (method.clone(), outcome.clone())
                })
                .collect::<BTreeMap<_, _>>();
            types.update_inferred_method_return_types_in_file(
                *file_id,
                outcomes
                    .iter()
                    .map(|(method, outcome)| (method, outcome.clone().into_ruby_type())),
            );
            let evidence = self.evidence_by_file.get_mut(file_id).expect_invariant(
                "a method-equation owner disappeared before evidence replacement",
                "resolution owns the engine write lock",
                "keep equation solving and evidence projection atomic",
            );
            evidence.method_return_outcomes = outcomes;
            let max_live_shape_aliases = evidence.telemetry.max_live_shape_aliases;
            evidence.telemetry = InferenceTelemetry::default();
            evidence.telemetry.observe_max_live_shape_aliases(
                usize::try_from(max_live_shape_aliases).expect_invariant(
                    "retained shape alias telemetry did not fit usize",
                    "the configured alias bound is representable on every supported target",
                    "keep the telemetry representation aligned with MAX_SHAPE_ALIASES",
                ),
            );
        }

        let telemetry_owner = *file_ids.first().expect_invariant(
            "a non-empty method-equation solve has no file owner",
            "equations are collected exclusively from sorted file evidence",
            "preserve the owner while flattening equations",
        );
        let telemetry_evidence = self
            .evidence_by_file
            .get_mut(&telemetry_owner)
            .expect_invariant(
                "the deterministic method-solver telemetry owner disappeared",
                "resolution owns the engine write lock",
                "assign telemetry before leaving the atomic solve pass",
            );
        let max_live_shape_aliases = telemetry_evidence.telemetry.max_live_shape_aliases;
        telemetry_evidence.telemetry = solve_result.telemetry;
        telemetry_evidence.telemetry.observe_max_live_shape_aliases(
            usize::try_from(max_live_shape_aliases).expect_invariant(
                "retained shape alias telemetry did not fit usize",
                "the configured alias bound is representable on every supported target",
                "keep the telemetry representation aligned with MAX_SHAPE_ALIASES",
            ),
        );
        for file_id in &file_ids {
            self.refresh_retained_shape_telemetry(*file_id, types);
        }
        self.method_return_equations_dirty = false;
        self.method_return_solution_spans_files = file_ids.len() > 1;
        true
    }

    /// Telemetry merged over every file in file-id order.
    pub(in crate::engine) fn telemetry(&self) -> InferenceTelemetry {
        let mut file_ids = self.evidence_by_file.keys().copied().collect::<Vec<_>>();
        file_ids.sort_unstable();
        let mut aggregate = InferenceTelemetry::default();
        for file_id in file_ids {
            aggregate.merge(
                &self
                    .evidence_by_file
                    .get(&file_id)
                    .expect_invariant(
                        "inference telemetry file key disappeared during immutable aggregation",
                        "engine queries hold a stable shared borrow",
                        "keep telemetry replacement behind the engine write lock",
                    )
                    .telemetry,
            );
        }
        aggregate
    }

    pub(in crate::engine) fn constant_callable_body(
        &self,
        constant: &FullyQualifiedName,
    ) -> Option<Result<CallableBodySummary, UnknownReason>> {
        let mut summaries = self
            .evidence_by_file
            .values()
            .flat_map(|evidence| evidence.constant_callable_bodies.iter())
            .filter(|fact| &fact.constant == constant)
            .map(|fact| &fact.summary);
        let first = summaries.next()?.clone();
        if summaries.any(|summary| summary != &first) {
            return Some(Err(UnknownReason::AmbiguousCallableValue));
        }
        Some(Ok(first))
    }

    pub(in crate::engine) fn has_method_return_equation(
        &self,
        method: &FullyQualifiedName,
    ) -> bool {
        self.evidence_by_file.values().any(|evidence| {
            evidence
                .method_return_equations
                .iter()
                .any(|equation| equation.method() == method)
        })
    }

    pub(in crate::engine) fn estimated_heap_bytes(&self) -> usize {
        self.evidence_by_file.capacity()
            * (size_of::<SourceFileId>() + size_of::<InferenceEvidence>() + 1)
            + self
                .evidence_by_file
                .values()
                .map(InferenceEvidence::estimated_heap_bytes)
                .sum::<usize>()
    }

    pub(in crate::engine) fn shrink_to_fit(&mut self) {
        self.evidence_by_file.shrink_to_fit();
    }
}

impl AnalysisEngine {
    pub(super) fn resolve_constant_type_equations(&mut self) -> bool {
        let plan = self
            .solver
            .plan_constant_type_equations(&AnalysisQuery::new(self), &self.types);
        self.solver
            .apply_constant_type_equations(&mut self.types, plan)
    }

    /// Solve the complete project-owned method-return equation graph once per
    /// equation change, before any call reference consumes method returns.
    ///
    /// Ordinary `resolve()` calls are O(1) when method bodies did not change.
    pub(super) fn resolve_method_return_equations(&mut self) -> bool {
        let plan = self
            .solver
            .plan_method_return_equations(&AnalysisQuery::new(self));
        self.solver
            .apply_method_return_equations(&mut self.types, plan)
    }

    pub fn inference_telemetry(&self) -> InferenceTelemetry {
        self.solver.telemetry()
    }

    pub fn inference_telemetry_in_file(
        &self,
        file_id: SourceFileId,
    ) -> Option<&InferenceTelemetry> {
        self.solver
            .evidence(file_id)
            .map(|evidence| &evidence.telemetry)
    }

    pub fn method_return_outcomes_in_file(
        &self,
        file_id: SourceFileId,
    ) -> Option<&BTreeMap<FullyQualifiedName, TypeInferenceOutcome>> {
        self.solver
            .evidence(file_id)
            .map(|evidence| &evidence.method_return_outcomes)
    }

    pub fn method_return_equations_in_file(
        &self,
        file_id: SourceFileId,
    ) -> Option<&[MethodReturnEquation]> {
        self.solver
            .evidence(file_id)
            .map(|evidence| evidence.method_return_equations.as_slice())
    }

    pub fn inference_evidence_in_file(&self, file_id: SourceFileId) -> Option<InferenceEvidence> {
        let mut evidence = self.solver.evidence(file_id)?.clone();
        evidence.call_expression_outcomes = self
            .call_expression_outcomes_in_file(file_id)
            .unwrap_or_default();
        Some(evidence)
    }

    pub(in crate::engine) fn constant_callable_body(
        &self,
        constant: &FullyQualifiedName,
    ) -> Option<Result<CallableBodySummary, UnknownReason>> {
        self.solver.constant_callable_body(constant)
    }

    pub(crate) fn has_method_return_equation(&self, method: &FullyQualifiedName) -> bool {
        self.solver.has_method_return_equation(method)
    }

    pub(in crate::engine) fn expression_unknown_reason(
        &self,
        range: TextRange,
    ) -> Option<UnknownReason> {
        self.call_expression_outcome_at(range)
            .and_then(|outcome| match outcome {
                TypeInferenceOutcomeRef::Proven(_) => None,
                TypeInferenceOutcomeRef::Unknown(reason) => Some(reason),
            })
            .or_else(|| {
                let evidence = self.solver.evidence(range.file_id)?;
                evidence
                    .expression_unknown_reasons
                    .binary_search_by_key(&range, |(evidence_range, _)| *evidence_range)
                    .ok()
                    .map(|index| evidence.expression_unknown_reasons[index].1)
            })
    }

    pub(in crate::engine) fn expression_unknown_reasons_in_file(
        &self,
        file_id: SourceFileId,
    ) -> Option<&[(TextRange, UnknownReason)]> {
        self.solver
            .evidence(file_id)
            .map(|evidence| evidence.expression_unknown_reasons.as_slice())
    }
}
