//! Equation solving and the stored inference outcomes, expression types, and
//! local-read types owned by each file.

use std::collections::{BTreeMap, HashMap};

use crate::core::{
    ConstantTypeDependency, ConstantTypeEquation, ConstantTypeProjection, ConstantTypeTarget,
    FullyQualifiedName, GraphNodeKind, InferenceEvidence, InferenceTelemetry, RubyType,
    SourceFileId, TextRange, TypeInferenceOutcome, TypeStore, TypeSubject, UnknownReason,
};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StoredTypeInferenceOutcome {
    Proven(crate::core::storage::type_store::RubyTypeId),
    Unknown(UnknownReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::engine) enum TypeInferenceOutcomeRef<'a> {
    Proven(&'a RubyType),
    Unknown(UnknownReason),
}

impl StoredTypeInferenceOutcome {
    pub(super) fn from_domain(types: &mut TypeStore, outcome: TypeInferenceOutcome) -> Self {
        match outcome.unknown_reason() {
            Some(reason) => Self::Unknown(reason),
            None => Self::Proven(types.intern_ruby_type(
                outcome.into_proven_type().expect(
                    "INVARIANT VIOLATED: call-expression outcome is neither proven nor Unknown. This is a bug because TypeInferenceOutcome has exactly those two states. Fix: construct outcomes only through TypeInferenceOutcome::proven or TypeInferenceOutcome::unknown.",
                ),
            )),
        }
    }

    fn as_ref<'a>(self, types: &'a TypeStore) -> TypeInferenceOutcomeRef<'a> {
        match self {
            Self::Proven(ruby_type) => TypeInferenceOutcomeRef::Proven(types.ruby_type(ruby_type)),
            Self::Unknown(reason) => TypeInferenceOutcomeRef::Unknown(reason),
        }
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
            .facts
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
                        .facts
                        .types
                        .update_equation_target(&subject, range, ruby_type);
                    assert!(
                        updated > 0,
                        "INVARIANT VIOLATED: constant equation target {subject:?} has no matching type fact at {range:?}. This is a bug because equations and facts must be replaced atomically with their file. Fix: emit the equation from the same write path as its target TypeFact."
                    );
                }
                ConstantTypeTarget::LocalAssignment { name, range } => {
                    let updated = self
                        .facts
                        .types
                        .update_local_assignment_equation_target(&name, range, ruby_type);
                    assert!(
                        updated > 0,
                        "INVARIANT VIOLATED: local constructor equation target {name:?} has no matching assignment fact at {range:?}. This is a bug because the stable source assignment and its equation must be replaced atomically. Fix: emit the target fact and equation from the same local-write path."
                    );
                }
                ConstantTypeTarget::LocalRead(range) => {
                    self.update_constant_local_read(range, ruby_type);
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

    fn update_constant_local_read(&mut self, range: TextRange, ruby_type: RubyType) {
        let mut reads = self
            .local_read_types_by_file
            .remove(&range.file_id)
            .unwrap_or_default()
            .into_vec();
        reads.retain(|(existing, _)| *existing != range);
        if ruby_type != RubyType::Unknown {
            let ruby_type = self.facts.types.intern_ruby_type(ruby_type);
            reads.push((range, ruby_type));
        }
        reads.sort_unstable_by_key(|(range, _)| *range);
        if !reads.is_empty() {
            self.local_read_types_by_file
                .insert(range.file_id, reads.into_boxed_slice());
        }
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
                .expect(
                    "INVARIANT VIOLATED: a selected method-equation file disappeared while checking constant dependencies. This is a bug because resolution owns the engine write lock. Fix: keep equation selection and dependency inspection in one immutable phase.",
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
                    .expect(
                        "INVARIANT VIOLATED: a selected method-equation file disappeared during immutable collection. This is a bug because resolution owns the engine write lock. Fix: keep equation collection inside one resolution pass.",
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
                .expect(
                    "INVARIANT VIOLATED: a method-equation owner disappeared before result projection. This is a bug because resolution owns the engine write lock. Fix: keep equation solving and projection atomic.",
                )
                .method_return_equations
                .iter()
                .map(|equation| equation.method().clone())
                .collect::<std::collections::BTreeSet<_>>();
            let outcomes = methods
                .iter()
                .map(|method| {
                    let outcome = solve_result.outcomes.get(method).unwrap_or_else(|| {
                        panic!(
                            "INVARIANT VIOLATED: method-return solver omitted equation `{method}`. This is a bug because every grouped method must produce exactly one proof outcome. Fix: keep SCC emission and result insertion exhaustive."
                        )
                    });
                    (method.clone(), outcome.clone())
                })
                .collect::<BTreeMap<_, _>>();
            self.facts
                .types
                .update_inferred_method_return_types_in_file(
                    *file_id,
                    outcomes
                        .iter()
                        .map(|(method, outcome)| (method, outcome.clone().into_ruby_type())),
                );
            let evidence = self.inference_by_file.get_mut(file_id).expect(
                "INVARIANT VIOLATED: a method-equation owner disappeared before evidence replacement. This is a bug because resolution owns the engine write lock. Fix: keep equation solving and evidence projection atomic.",
            );
            evidence.method_return_outcomes = outcomes;
            let max_live_shape_aliases = evidence.telemetry.max_live_shape_aliases;
            evidence.telemetry = InferenceTelemetry::default();
            evidence
                .telemetry
                .observe_max_live_shape_aliases(usize::try_from(max_live_shape_aliases).expect(
                    "INVARIANT VIOLATED: retained shape alias telemetry did not fit usize. This is a bug because the configured alias bound is representable on every supported target. Fix: keep the telemetry representation aligned with MAX_SHAPE_ALIASES.",
                ));
        }

        let telemetry_owner = *file_ids.first().expect(
            "INVARIANT VIOLATED: a non-empty method-equation solve has no file owner. This is a bug because equations are collected exclusively from sorted file evidence. Fix: preserve the owner while flattening equations.",
        );
        let telemetry_evidence = self
            .inference_by_file
            .get_mut(&telemetry_owner)
            .expect(
                "INVARIANT VIOLATED: the deterministic method-solver telemetry owner disappeared. This is a bug because resolution owns the engine write lock. Fix: assign telemetry before leaving the atomic solve pass.",
            );
        let max_live_shape_aliases = telemetry_evidence.telemetry.max_live_shape_aliases;
        telemetry_evidence.telemetry = solve_result.telemetry;
        telemetry_evidence
            .telemetry
            .observe_max_live_shape_aliases(usize::try_from(max_live_shape_aliases).expect(
                "INVARIANT VIOLATED: retained shape alias telemetry did not fit usize. This is a bug because the configured alias bound is representable on every supported target. Fix: keep the telemetry representation aligned with MAX_SHAPE_ALIASES.",
            ));
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
            aggregate.merge(&self.inference_by_file.get(&file_id).expect(
                "INVARIANT VIOLATED: inference telemetry file key disappeared during immutable aggregation. This is a bug because engine queries hold a stable shared borrow. Fix: keep telemetry replacement behind the engine write lock.",
            ).telemetry);
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
    ) -> Option<Result<crate::core::CallableBodySummary, UnknownReason>> {
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

    pub(in crate::engine) fn call_expression_outcomes_in_file(
        &self,
        file_id: SourceFileId,
    ) -> Option<Vec<(TextRange, TypeInferenceOutcome)>> {
        self.call_expression_outcome_views_in_file(file_id)
            .map(|outcomes| {
                outcomes
                    .map(|(range, outcome)| {
                        let outcome = match outcome {
                            TypeInferenceOutcomeRef::Proven(ruby_type) => {
                                TypeInferenceOutcome::proven(ruby_type.clone())
                            }
                            TypeInferenceOutcomeRef::Unknown(reason) => {
                                TypeInferenceOutcome::unknown(reason)
                            }
                        };
                        (range, outcome)
                    })
                    .collect()
            })
    }

    pub(in crate::engine) fn call_expression_outcome_views_in_file(
        &self,
        file_id: SourceFileId,
    ) -> Option<impl Iterator<Item = (TextRange, TypeInferenceOutcomeRef<'_>)>> {
        self.call_expression_outcomes_by_file
            .get(&file_id)
            .map(|outcomes| {
                outcomes
                    .iter()
                    .map(|(range, outcome)| (*range, outcome.as_ref(&self.facts.types)))
            })
    }

    pub(in crate::engine) fn call_expression_outcome_at(
        &self,
        range: TextRange,
    ) -> Option<TypeInferenceOutcomeRef<'_>> {
        let outcomes = self.call_expression_outcomes_by_file.get(&range.file_id)?;
        let index = outcomes
            .binary_search_by_key(&range, |(outcome_range, _)| *outcome_range)
            .ok()?;
        Some(outcomes[index].1.as_ref(&self.facts.types))
    }

    pub(in crate::engine) fn local_read_types_in_file(
        &self,
        file_id: SourceFileId,
    ) -> Option<Vec<(TextRange, RubyType)>> {
        self.local_read_type_views_in_file(file_id).map(|reads| {
            reads
                .map(|(range, ruby_type)| (range, ruby_type.clone()))
                .collect()
        })
    }

    pub(in crate::engine) fn local_read_type_views_in_file(
        &self,
        file_id: SourceFileId,
    ) -> Option<impl Iterator<Item = (TextRange, &RubyType)>> {
        self.inference_by_file.get(&file_id)?;
        let reads = self
            .local_read_types_by_file
            .get(&file_id)
            .map_or(&[][..], Box::as_ref);
        Some(
            reads
                .iter()
                .map(|(range, ruby_type)| (*range, self.facts.types.ruby_type(*ruby_type))),
        )
    }

    pub(in crate::engine) fn exact_local_read_type_at(
        &self,
        range: TextRange,
    ) -> Option<&RubyType> {
        self.inference_by_file.get(&range.file_id)?;
        let reads = self.local_read_types_by_file.get(&range.file_id)?;
        let index = reads
            .binary_search_by_key(&range, |(candidate, _)| *candidate)
            .ok()?;
        Some(self.facts.types.ruby_type(reads[index].1))
    }

    pub(in crate::engine) fn local_read_type_at(
        &self,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> Option<&RubyType> {
        self.inference_by_file.get(&file_id)?;
        let reads = self.local_read_types_by_file.get(&file_id)?;
        let upper = reads.partition_point(|(range, _)| range.start_byte <= byte_offset);
        reads[..upper]
            .iter()
            .rev()
            .find(|(range, _)| range.contains_offset(file_id, byte_offset))
            .map(|(_, ruby_type)| self.facts.types.ruby_type(*ruby_type))
    }

    pub(in crate::engine) fn replace_resolved_call_expression_outcomes(
        &mut self,
        mut outcomes: HashMap<TextRange, TypeInferenceOutcome>,
    ) {
        // Sorting only the compact ranges avoids materializing a second
        // Vec<(TextRange, TypeInferenceOutcome)> while the resolve map and the
        // previous file-owned outcomes are both still live. Move each outcome
        // out of the map only when its file is merged.
        let mut ordered_ranges = outcomes.keys().copied().collect::<Vec<_>>();
        ordered_ranges.sort_unstable();
        let mut ordered_ranges = ordered_ranges.into_iter().peekable();
        while let Some(first_range) = ordered_ranges.next() {
            let file_id = first_range.file_id;
            let first_outcome = StoredTypeInferenceOutcome::from_domain(
                &mut self.facts.types,
                outcomes.remove(&first_range).expect(
                    "INVARIANT VIOLATED: sorted call-expression range has no resolved outcome. This is a bug because the range list is built directly from the owned outcome map. Fix: remove each map entry exactly once while grouping by file.",
                ),
            );
            let mut incoming = vec![(first_range, first_outcome)];
            while ordered_ranges
                .peek()
                .is_some_and(|range| range.file_id == file_id)
            {
                let range = ordered_ranges.next().expect(
                    "INVARIANT VIOLATED: a peeked call-expression range disappeared before consumption. This is a bug because the local iterator is not shared. Fix: keep grouping and consumption in one loop.",
                );
                let outcome = StoredTypeInferenceOutcome::from_domain(
                    &mut self.facts.types,
                    outcomes.remove(&range).expect(
                        "INVARIANT VIOLATED: grouped call-expression range has no resolved outcome. This is a bug because each sorted range must still own one map entry. Fix: remove each map entry exactly once while grouping by file.",
                    ),
                );
                incoming.push((range, outcome));
            }

            self.inference_by_file.get(&file_id).expect(
                "INVARIANT VIOLATED: resolved call outcome belongs to a file without inference evidence. This is a bug because file facts are installed before their method candidates resolve. Fix: replace inference evidence atomically with reference candidates.",
            );
            let mut existing = self
                .call_expression_outcomes_by_file
                .remove(&file_id)
                .unwrap_or_default()
                .into_vec()
                .into_iter()
                .peekable();
            let mut incoming = incoming.into_iter().peekable();
            let mut merged = Vec::with_capacity(existing.len() + incoming.len());
            loop {
                match (existing.peek(), incoming.peek()) {
                    (Some((existing_range, _)), Some((incoming_range, _))) => {
                        match existing_range.cmp(incoming_range) {
                            std::cmp::Ordering::Less => merged.push(existing.next().expect(
                                "INVARIANT VIOLATED: a peeked existing call outcome disappeared before merge consumption. This is a bug because the local iterator is not shared. Fix: keep comparison and consumption atomic.",
                            )),
                            std::cmp::Ordering::Equal => {
                                existing.next().expect(
                                    "INVARIANT VIOLATED: an equal existing call outcome disappeared before replacement. This is a bug because the local iterator is not shared. Fix: keep comparison and consumption atomic.",
                                );
                                merged.push(incoming.next().expect(
                                    "INVARIANT VIOLATED: an equal incoming call outcome disappeared before replacement. This is a bug because the local iterator is not shared. Fix: keep comparison and consumption atomic.",
                                ));
                            }
                            std::cmp::Ordering::Greater => merged.push(incoming.next().expect(
                                "INVARIANT VIOLATED: a peeked incoming call outcome disappeared before merge consumption. This is a bug because the local iterator is not shared. Fix: keep comparison and consumption atomic.",
                            )),
                        }
                    }
                    (Some(_), None) => {
                        merged.extend(existing);
                        break;
                    }
                    (None, Some(_)) => {
                        merged.extend(incoming);
                        break;
                    }
                    (None, None) => break,
                }
            }
            self.call_expression_outcomes_by_file
                .insert(file_id, merged.into_boxed_slice());
        }
        assert!(
            outcomes.is_empty(),
            "INVARIANT VIOLATED: resolved call-expression outcomes remained after the complete sorted merge. This is a bug because every map key was copied into the ordered range list. Fix: keep range collection and map ownership in the same merge operation."
        );
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
