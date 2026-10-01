//! `TypeTable`: the engine's type facts, the call-expression outcomes and
//! proven local-read types each file owns, the merge of outcomes from resolve
//! passes, and the target writers that apply equation solutions.

use std::collections::HashMap;
use std::mem::size_of;

use crate::core::storage::type_store::{RubyTypeId, TypeStore};
use crate::core::{
    FullyQualifiedName, RubyType, SourceFileId, TextRange, TypeFact, TypeInferenceOutcome,
    TypeResolution, TypeSubject, UnknownReason,
};
use crate::invariant::ExpectInvariant;

use super::AnalysisEngine;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StoredTypeInferenceOutcome {
    Proven(RubyTypeId),
    Unknown(UnknownReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::engine) enum TypeInferenceOutcomeRef<'a> {
    Proven(&'a RubyType),
    Unknown(UnknownReason),
}

impl StoredTypeInferenceOutcome {
    fn from_domain(types: &mut TypeStore, outcome: TypeInferenceOutcome) -> Self {
        match outcome.unknown_reason() {
            Some(reason) => Self::Unknown(reason),
            None => Self::Proven(types.intern_ruby_type(
                outcome.into_proven_type().expect_invariant(
                    "call-expression outcome is neither proven nor Unknown",
                    "TypeInferenceOutcome has exactly those two states",
                    "build outcomes via TypeInferenceOutcome::proven or ::unknown",
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

#[derive(Debug, Clone, Default)]
pub(in crate::engine) struct TypeTable {
    store: TypeStore,
    call_expression_outcomes_by_file:
        HashMap<SourceFileId, Box<[(TextRange, StoredTypeInferenceOutcome)]>>,
    local_read_types_by_file: HashMap<SourceFileId, Box<[(TextRange, RubyTypeId)]>>,
}

impl TypeTable {
    pub(in crate::engine) fn store(&self) -> &TypeStore {
        &self.store
    }

    /// Replace one file's type facts, call-expression outcomes, and proven
    /// local-read types.
    pub(in crate::engine) fn replace_file(
        &mut self,
        file_id: SourceFileId,
        types: Vec<TypeFact>,
        call_expression_outcomes: Vec<(TextRange, TypeInferenceOutcome)>,
        local_read_types: Box<[(TextRange, RubyType)]>,
    ) {
        self.store.replace_file(file_id, types);
        if call_expression_outcomes.is_empty() {
            self.call_expression_outcomes_by_file.remove(&file_id);
        } else {
            let outcomes = call_expression_outcomes
                .into_iter()
                .map(|(range, outcome)| {
                    (
                        range,
                        StoredTypeInferenceOutcome::from_domain(&mut self.store, outcome),
                    )
                })
                .collect::<Vec<_>>()
                .into_boxed_slice();
            self.call_expression_outcomes_by_file
                .insert(file_id, outcomes);
        }
        if local_read_types.is_empty() {
            self.local_read_types_by_file.remove(&file_id);
        } else {
            let local_read_types = local_read_types
                .into_vec()
                .into_iter()
                .map(|(range, ruby_type)| (range, self.store.intern_ruby_type(ruby_type)))
                .collect::<Vec<_>>()
                .into_boxed_slice();
            self.local_read_types_by_file
                .insert(file_id, local_read_types);
        }
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
                    .map(|(range, outcome)| (*range, outcome.as_ref(&self.store)))
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
        Some(outcomes[index].1.as_ref(&self.store))
    }

    /// One file's proven local-read types in range order; empty when the file
    /// retains none.
    pub(in crate::engine) fn local_read_type_views_in_file(
        &self,
        file_id: SourceFileId,
    ) -> impl Iterator<Item = (TextRange, &RubyType)> {
        let reads = self
            .local_read_types_by_file
            .get(&file_id)
            .map_or(&[][..], Box::as_ref);
        reads
            .iter()
            .map(|(range, ruby_type)| (*range, self.store.ruby_type(*ruby_type)))
    }

    /// Every file's proven local-read types, in map order.
    pub(in crate::engine) fn local_read_types_by_file(
        &self,
    ) -> impl Iterator<Item = (SourceFileId, impl Iterator<Item = (TextRange, &RubyType)>)> {
        self.local_read_types_by_file
            .iter()
            .map(|(file_id, reads)| {
                (
                    *file_id,
                    reads
                        .iter()
                        .map(|(range, ruby_type)| (*range, self.store.ruby_type(*ruby_type))),
                )
            })
    }

    pub(in crate::engine) fn exact_local_read_type_at(
        &self,
        range: TextRange,
    ) -> Option<&RubyType> {
        let reads = self.local_read_types_by_file.get(&range.file_id)?;
        let index = reads
            .binary_search_by_key(&range, |(candidate, _)| *candidate)
            .ok()?;
        Some(self.store.ruby_type(reads[index].1))
    }

    pub(in crate::engine) fn local_read_type_at(
        &self,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> Option<&RubyType> {
        let reads = self.local_read_types_by_file.get(&file_id)?;
        let upper = reads.partition_point(|(range, _)| range.start_byte <= byte_offset);
        reads[..upper]
            .iter()
            .rev()
            .find(|(range, _)| range.contains_offset(file_id, byte_offset))
            .map(|(_, ruby_type)| self.store.ruby_type(*ruby_type))
    }

    /// Merge resolved call-expression outcomes into each file's sorted
    /// outcomes, replacing outcomes at equal ranges. `has_evidence` reports
    /// whether a file has installed inference evidence.
    pub(in crate::engine) fn merge_resolved_call_expression_outcomes(
        &mut self,
        mut outcomes: HashMap<TextRange, TypeInferenceOutcome>,
        has_evidence: impl Fn(SourceFileId) -> bool,
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
                &mut self.store,
                outcomes.remove(&first_range).expect_invariant(
                    "sorted call-expression range has no resolved outcome",
                    "the range list is built directly from the owned outcome map",
                    "remove each map entry exactly once while grouping by file",
                ),
            );
            let mut incoming = vec![(first_range, first_outcome)];
            while ordered_ranges
                .peek()
                .is_some_and(|range| range.file_id == file_id)
            {
                let range = ordered_ranges.next().expect_invariant(
                    "a peeked call-expression range disappeared before consumption",
                    "the local iterator is not shared",
                    "keep grouping and consumption in one loop",
                );
                let outcome = StoredTypeInferenceOutcome::from_domain(
                    &mut self.store,
                    outcomes.remove(&range).expect_invariant(
                        "grouped call-expression range has no resolved outcome",
                        "each sorted range must still own one map entry",
                        "remove each map entry exactly once while grouping by file",
                    ),
                );
                incoming.push((range, outcome));
            }

            invariant!(
                has_evidence(file_id),
                what = "resolved call outcome belongs to a file without inference evidence",
                why = "file facts are installed before their method candidates resolve",
                fix = "replace inference evidence atomically with reference candidates",
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
                            std::cmp::Ordering::Less => merged.push(existing.next().expect_invariant(
                                "a peeked existing call outcome disappeared before merge consumption",
                                "the local iterator is not shared",
                                "keep comparison and consumption atomic",
                            )),
                            std::cmp::Ordering::Equal => {
                                existing.next().expect_invariant(
                                    "an equal existing call outcome disappeared before replacement",
                                    "the local iterator is not shared",
                                    "keep comparison and consumption atomic",
                                );
                                merged.push(incoming.next().expect_invariant(
                                    "an equal incoming call outcome disappeared before replacement",
                                    "the local iterator is not shared",
                                    "keep comparison and consumption atomic",
                                ));
                            }
                            std::cmp::Ordering::Greater => merged.push(incoming.next().expect_invariant(
                                "a peeked incoming call outcome disappeared before merge consumption",
                                "the local iterator is not shared",
                                "keep comparison and consumption atomic",
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
        invariant!(
            outcomes.is_empty(),
            what = "resolved call-expression outcomes remained after the complete sorted merge",
            why = "every map key was copied into the ordered range list",
            fix = "keep range collection and map ownership in the same merge operation",
        );
    }

    /// Borrow each value-constant type fact, in arena order.
    pub(in crate::engine) fn constant_type_facts(
        &self,
    ) -> impl Iterator<Item = (&FullyQualifiedName, TextRange, &RubyType)> {
        self.store.constant_type_facts()
    }

    pub(in crate::engine) fn ruby_types_in_file(
        &self,
        file_id: SourceFileId,
    ) -> impl Iterator<Item = &RubyType> {
        self.store.ruby_types_in_file(file_id)
    }

    /// Write a solved type into every fact with this exact subject and range.
    pub(in crate::engine) fn update_equation_target(
        &mut self,
        subject: &TypeSubject,
        range: TextRange,
        ruby_type: RubyType,
    ) -> usize {
        self.store.update_equation_target(subject, range, ruby_type)
    }

    /// Write a solved type into every scope projection of one local assignment.
    pub(in crate::engine) fn update_local_assignment_equation_target(
        &mut self,
        name: &str,
        range: TextRange,
        ruby_type: RubyType,
    ) -> usize {
        self.store
            .update_local_assignment_equation_target(name, range, ruby_type)
    }

    /// Write solved method-return types into one file's inferred facts.
    pub(in crate::engine) fn update_inferred_method_return_types_in_file<'a>(
        &mut self,
        file_id: SourceFileId,
        updates: impl IntoIterator<Item = (&'a FullyQualifiedName, RubyType)>,
    ) -> usize {
        self.store
            .update_inferred_method_return_types_in_file(file_id, updates)
    }

    /// Replace the solved type of one local read; Unknown removes it.
    pub(in crate::engine) fn update_constant_local_read(
        &mut self,
        range: TextRange,
        ruby_type: RubyType,
    ) {
        let mut reads = self
            .local_read_types_by_file
            .remove(&range.file_id)
            .unwrap_or_default()
            .into_vec();
        reads.retain(|(existing, _)| *existing != range);
        if ruby_type != RubyType::Unknown {
            let ruby_type = self.store.intern_ruby_type(ruby_type);
            reads.push((range, ruby_type));
        }
        reads.sort_unstable_by_key(|(range, _)| *range);
        if !reads.is_empty() {
            self.local_read_types_by_file
                .insert(range.file_id, reads.into_boxed_slice());
        }
    }

    pub(in crate::engine) fn fact_count(&self) -> usize {
        self.store.fact_count()
    }

    pub(in crate::engine) fn facts_heap_bytes(&self) -> usize {
        self.store.estimated_heap_bytes()
    }

    /// Heap held by the per-file call-expression outcomes and local-read types.
    pub(in crate::engine) fn file_outcomes_heap_bytes(&self) -> usize {
        self.call_expression_outcomes_by_file.capacity()
            * (size_of::<SourceFileId>()
                + size_of::<Box<[(TextRange, StoredTypeInferenceOutcome)]>>()
                + 1)
            + self
                .call_expression_outcomes_by_file
                .values()
                .map(|outcomes| {
                    outcomes.len() * size_of::<(TextRange, StoredTypeInferenceOutcome)>()
                })
                .sum::<usize>()
            + self.local_read_types_by_file.capacity()
                * (size_of::<SourceFileId>() + size_of::<Box<[(TextRange, RubyTypeId)]>>() + 1)
            + self
                .local_read_types_by_file
                .values()
                .map(|reads| reads.len() * size_of::<(TextRange, RubyTypeId)>())
                .sum::<usize>()
    }

    pub(in crate::engine) fn shrink_to_fit(&mut self) {
        self.store.shrink_to_fit();
        self.call_expression_outcomes_by_file.shrink_to_fit();
        self.local_read_types_by_file.shrink_to_fit();
    }
}

impl AnalysisEngine {
    pub fn type_at(
        &self,
        subject: &TypeSubject,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> TypeResolution {
        self.types.store().type_at(subject, file_id, byte_offset)
    }

    pub fn type_facts_for(&self, subject: &TypeSubject) -> Vec<TypeFact> {
        self.types.store().facts_for(subject)
    }

    pub(crate) fn type_store(&self) -> &TypeStore {
        self.types.store()
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
        self.types.call_expression_outcome_views_in_file(file_id)
    }

    pub(in crate::engine) fn call_expression_outcome_at(
        &self,
        range: TextRange,
    ) -> Option<TypeInferenceOutcomeRef<'_>> {
        self.types.call_expression_outcome_at(range)
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

    /// Local-read types exist only for files with installed inference evidence.
    pub(in crate::engine) fn local_read_type_views_in_file(
        &self,
        file_id: SourceFileId,
    ) -> Option<impl Iterator<Item = (TextRange, &RubyType)>> {
        self.solver.evidence(file_id)?;
        Some(self.types.local_read_type_views_in_file(file_id))
    }

    pub(in crate::engine) fn exact_local_read_type_at(
        &self,
        range: TextRange,
    ) -> Option<&RubyType> {
        self.solver.evidence(range.file_id)?;
        self.types.exact_local_read_type_at(range)
    }

    pub(in crate::engine) fn local_read_type_at(
        &self,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> Option<&RubyType> {
        self.solver.evidence(file_id)?;
        self.types.local_read_type_at(file_id, byte_offset)
    }

    pub(in crate::engine) fn replace_resolved_call_expression_outcomes(
        &mut self,
        outcomes: HashMap<TextRange, TypeInferenceOutcome>,
    ) {
        let solver = &self.solver;
        self.types
            .merge_resolved_call_expression_outcomes(outcomes, |file_id| {
                solver.has_evidence(file_id)
            });
    }
}
