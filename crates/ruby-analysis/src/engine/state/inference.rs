//! Equation solving and the inference evidence owned by each file.

use crate::invariant::ExpectInvariant;
use std::collections::BTreeMap;

use crate::core::{
    ConstantTypeDependency, ConstantTypeEquation, ConstantTypeProjection, ConstantTypeTarget,
    FullyQualifiedName, GraphNodeKind, InferenceEvidence, InferenceTelemetry, RubyType,
    SourceFileId, TextRange, TypeInferenceOutcome, TypeSubject, UnknownReason,
};

use super::types::TypeInferenceOutcomeRef;
use super::AnalysisEngine;
use crate::engine::AnalysisQuery;
use crate::inference::constant::{
    solve_constant_type_equations, ConstantFactInput, ResolvedConstantDependency,
};
use crate::inference::method::constructor::ConstructorResult;
use crate::inference::method::recursive::solve_method_return_equations_with_telemetry;

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

impl AnalysisEngine {
    pub(super) fn resolve_constant_type_equations(&mut self) -> bool {
        if !self.constant_type_equations_dirty {
            return false;
        }
        let mut equations = self
            .inference_by_file
            .values()
            .flat_map(|evidence| evidence.constant_type_equations.iter().cloned())
            .collect::<Vec<ConstantTypeEquation>>();
        equations.sort();
        equations.dedup();
        if equations.is_empty() {
            if self.inference_by_file.values().any(|evidence| {
                evidence
                    .method_return_equations
                    .iter()
                    .any(|equation| !equation.constant_dependencies().is_empty())
            }) {
                self.method_return_equations_dirty = true;
            }
            self.constant_type_equations_dirty = false;
            return false;
        }

        let mut dependencies = equations
            .iter()
            .flat_map(|equation| equation.dependencies().iter().cloned())
            .collect::<Vec<ConstantTypeDependency>>();
        dependencies.extend(
            self.inference_by_file
                .values()
                .flat_map(|evidence| evidence.method_return_equations.iter())
                .flat_map(|equation| equation.constant_dependencies().iter().cloned()),
        );
        dependencies.sort();
        dependencies.dedup();
        let query = AnalysisQuery::new(self);
        let resolved_dependencies = dependencies
            .into_iter()
            .map(|dependency| {
                let resolved = resolve_constant_dependency(&query, &dependency);
                (dependency, resolved)
            })
            .collect::<BTreeMap<_, _>>();
        let constant_facts = self
            .types
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
        let outcomes =
            solve_constant_type_equations(&equations, &constant_facts, &resolved_dependencies);
        for (target, ruby_type) in outcomes {
            match target {
                ConstantTypeTarget::Fact { subject, range } => {
                    let updated = self
                        .types
                        .update_equation_target(&subject, range, ruby_type);
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
                    let updated = self
                        .types
                        .update_local_assignment_equation_target(&name, range, ruby_type);
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
                    self.types.update_constant_local_read(range, ruby_type);
                }
            }
        }
        if self.inference_by_file.values().any(|evidence| {
            evidence
                .method_return_equations
                .iter()
                .any(|equation| !equation.constant_dependencies().is_empty())
        }) {
            self.method_return_equations_dirty = true;
        }
        self.constant_type_equations_dirty = false;
        true
    }

    /// Solve the complete project-owned method-return equation graph once per
    /// equation change, before any call reference consumes method returns.
    ///
    /// Ordinary `resolve()` calls are O(1) when method bodies did not change.
    /// Equations contain no AST nodes and follow the same per-file replacement
    /// lifecycle as the inferred type facts they update.
    pub(super) fn resolve_method_return_equations(&mut self) -> bool {
        if !self.method_return_equations_dirty {
            return false;
        }

        let mut file_ids = self
            .inference_by_file
            .iter()
            .filter_map(|(file_id, evidence)| {
                (!evidence.method_return_equations.is_empty()).then_some(*file_id)
            })
            .collect::<Vec<_>>();
        file_ids.sort_unstable();

        let has_constant_dependencies = file_ids.iter().any(|file_id| {
            self.inference_by_file
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
            self.method_return_equations_dirty = false;
            return false;
        }

        let equations = file_ids
            .iter()
            .flat_map(|file_id| {
                self.inference_by_file
                    .get(file_id)
                    .expect_invariant(
                        "a selected method-equation file disappeared during immutable collection",
                        "resolution owns the engine write lock",
                        "keep equation collection inside one resolution pass",
                    )
                    .method_return_equations
                    .iter()
                    .map(|equation| {
                        let query = AnalysisQuery::new(self);
                        equation.with_resolved_constant_types(
                            equation.constant_dependencies().iter().map(|dependency| {
                                resolve_constant_dependency_type(&query, dependency)
                            }),
                        )
                    })
            })
            .collect::<Vec<_>>();

        if equations.is_empty() {
            self.method_return_equations_dirty = false;
            self.method_return_solution_spans_files = false;
            return false;
        }

        let solve_result = solve_method_return_equations_with_telemetry(&equations);
        for file_id in &file_ids {
            let methods = self
                .inference_by_file
                .get(file_id)
                .expect_invariant(
                    "a method-equation owner disappeared before result projection",
                    "resolution owns the engine write lock",
                    "keep equation solving and projection atomic",
                )
                .method_return_equations
                .iter()
                .map(|equation| equation.method().clone())
                .collect::<std::collections::BTreeSet<_>>();
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
            self.types.update_inferred_method_return_types_in_file(
                *file_id,
                outcomes
                    .iter()
                    .map(|(method, outcome)| (method, outcome.clone().into_ruby_type())),
            );
            let evidence = self.inference_by_file.get_mut(file_id).expect_invariant(
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
            .inference_by_file
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
            self.refresh_retained_shape_telemetry(*file_id);
        }
        self.method_return_equations_dirty = false;
        self.method_return_solution_spans_files = file_ids.len() > 1;
        true
    }

    pub fn inference_telemetry(&self) -> InferenceTelemetry {
        let mut file_ids = self.inference_by_file.keys().copied().collect::<Vec<_>>();
        file_ids.sort_unstable();
        let mut aggregate = InferenceTelemetry::default();
        for file_id in file_ids {
            aggregate.merge(
                &self
                    .inference_by_file
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

    pub fn inference_telemetry_in_file(
        &self,
        file_id: SourceFileId,
    ) -> Option<&InferenceTelemetry> {
        self.inference_by_file
            .get(&file_id)
            .map(|evidence| &evidence.telemetry)
    }

    pub fn method_return_outcomes_in_file(
        &self,
        file_id: SourceFileId,
    ) -> Option<&BTreeMap<FullyQualifiedName, TypeInferenceOutcome>> {
        self.inference_by_file
            .get(&file_id)
            .map(|evidence| &evidence.method_return_outcomes)
    }

    pub fn method_return_equations_in_file(
        &self,
        file_id: SourceFileId,
    ) -> Option<&[crate::core::MethodReturnEquation]> {
        self.inference_by_file
            .get(&file_id)
            .map(|evidence| evidence.method_return_equations.as_slice())
    }

    pub fn inference_evidence_in_file(&self, file_id: SourceFileId) -> Option<InferenceEvidence> {
        let mut evidence = self.inference_by_file.get(&file_id)?.clone();
        evidence.call_expression_outcomes = self
            .call_expression_outcomes_in_file(file_id)
            .unwrap_or_default();
        Some(evidence)
    }

    pub(in crate::engine) fn constant_callable_body(
        &self,
        constant: &FullyQualifiedName,
    ) -> Option<Result<crate::core::callables::callable_body::CallableBodySummary, UnknownReason>>
    {
        let mut summaries = self
            .inference_by_file
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

    pub(crate) fn has_method_return_equation(&self, method: &FullyQualifiedName) -> bool {
        self.inference_by_file.values().any(|evidence| {
            evidence
                .method_return_equations
                .iter()
                .any(|equation| equation.method() == method)
        })
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
                let evidence = self.inference_by_file.get(&range.file_id)?;
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
        self.inference_by_file
            .get(&file_id)
            .map(|evidence| evidence.expression_unknown_reasons.as_slice())
    }
}
