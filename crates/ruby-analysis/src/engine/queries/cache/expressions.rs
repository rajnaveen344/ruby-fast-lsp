//! Exact expression, call outcome, and local-read type queries.

use crate::core::{
    RubyType, SourceFileId, TextRange, TypeInferenceOutcome, TypeResolution, TypeSubject,
    UnknownReason,
};
use crate::engine::queries::View;
use crate::engine::state::TypeInferenceOutcomeRef;
use crate::invariant::ExpectInvariant;

impl<'a> View<'a> {
    /// Return the exact receiver proof retained for a call's message range.
    ///
    /// Sparse flow overrides and explicit Unknown evidence precede the
    /// collector's receiver proof. No neighboring or enclosing expression can
    /// supply a missing local-read type, and conflicting candidates fail closed.
    pub(crate) fn exact_call_receiver_type(
        &self,
        message_range: TextRange,
        receiver_range: TextRange,
    ) -> Option<RubyType> {
        if let Some(ruby_type) = self.exact_expression_type(receiver_range) {
            return Some(ruby_type);
        }
        let mut proven_type = None;
        for candidate in self
            .engine
            .uses
            .candidates()
            .method_candidates_at_exact_range(message_range)
        {
            let Some(diagnostics) = candidate.diagnostics.as_deref() else {
                return None;
            };
            if diagnostics.receiver_expression_range != Some(receiver_range) {
                return None;
            }
            let ruby_type = diagnostics.receiver_type.as_deref()?;
            if proven_type.is_some_and(|previous| previous != ruby_type) {
                return Some(RubyType::Unknown);
            }
            proven_type = Some(ruby_type);
        }
        proven_type.cloned()
    }

    /// Return the proof failure attached to one exact Unknown expression.
    pub fn expression_unknown_reason(&self, range: TextRange) -> Option<UnknownReason> {
        self.engine.expression_unknown_reason(range)
    }

    /// Return Unknown evidence owned by this exact expression range.
    ///
    /// Unlike `expression_unknown_reason_at`, this never inherits an Unknown
    /// result from an enclosing call. Receiver consumers use it to avoid
    /// treating an unknown call return as evidence that its proven receiver
    /// was unknown.
    pub fn exact_expression_unknown_reason(&self, range: TextRange) -> Option<UnknownReason> {
        if let Some(reason) = self.expression_unknown_reason(range) {
            return Some(reason);
        }
        match self.engine.call_expression_outcome_at(range) {
            Some(TypeInferenceOutcomeRef::Unknown(reason)) => Some(reason),
            Some(TypeInferenceOutcomeRef::Proven(_)) | None => None,
        }
    }

    /// Return the type owned by one exact expression range.
    ///
    /// This query never borrows a narrower child or wider enclosing
    /// expression. Deferred receiver resolution uses it so an enclosing call
    /// result cannot be mistaken for the receiver's own type. Local-read and
    /// expression-fact fallbacks are exact-range lookups on the file-owned
    /// sorted indexes; they must not scan every fact in the file.
    pub fn exact_expression_type(&self, range: TextRange) -> Option<RubyType> {
        if let Some(outcome) = self.engine.call_expression_outcome_at(range) {
            return Some(match outcome {
                TypeInferenceOutcomeRef::Proven(ruby_type) => ruby_type.clone(),
                TypeInferenceOutcomeRef::Unknown(_) => RubyType::Unknown,
            });
        }
        if self.engine.expression_unknown_reason(range).is_some() {
            return Some(RubyType::Unknown);
        }
        if let Some(ruby_type) = self.engine.exact_local_read_type_at(range) {
            return Some(ruby_type.clone());
        }
        match self.engine.type_store().type_at(
            &TypeSubject::Expression(range),
            range.file_id,
            range.start_byte,
        ) {
            TypeResolution::Unresolved => None,
            TypeResolution::Resolved(fact) => Some(fact.ruby_type),
            TypeResolution::Ambiguous(facts) => {
                let types = facts
                    .into_iter()
                    .map(|fact| fact.ruby_type)
                    .collect::<Vec<_>>();
                if types
                    .iter()
                    .any(|ruby_type| *ruby_type == RubyType::Unknown)
                {
                    Some(RubyType::Unknown)
                } else {
                    Some(RubyType::union(types))
                }
            }
        }
    }

    /// Return the reason for the most specific Unknown expression covering a
    /// source position. Proven expressions never inherit an enclosing reason.
    pub fn expression_unknown_reason_at(
        &self,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> Option<UnknownReason> {
        if self
            .expression_type_at(file_id, byte_offset)
            .is_some_and(|ruby_type| ruby_type != RubyType::Unknown)
        {
            return None;
        }

        let reasons = self
            .engine
            .expression_unknown_reasons_in_file(file_id)
            .unwrap_or_default();
        let mut best: Option<(u32, TextRange, UnknownReason)> = None;
        let mut ambiguous = false;
        let mut consider = |range: TextRange, reason: UnknownReason| {
            if !range.contains_offset(file_id, byte_offset) {
                return;
            }
            let span = range
                .end_byte
                .checked_sub(range.start_byte)
                .expect_invariant(
                    "an expression Unknown reason has an inverted range",
                    "TextRange producers must emit start <= end",
                    "validate the indexer range before recording proof evidence",
                );
            match best {
                None => {
                    best = Some((span, range, reason));
                    ambiguous = false;
                }
                Some((best_span, _, _)) if span < best_span => {
                    best = Some((span, range, reason));
                    ambiguous = false;
                }
                Some((best_span, best_range, _)) if span == best_span && range != best_range => {
                    ambiguous = true;
                }
                Some(_) => {}
            }
        };
        for (range, reason) in reasons.iter().copied() {
            consider(range, reason);
        }
        if let Some(outcomes) = self.engine.call_expression_outcome_views_in_file(file_id) {
            for (range, outcome) in outcomes {
                if let TypeInferenceOutcomeRef::Unknown(reason) = outcome {
                    consider(range, reason);
                }
            }
        }
        if ambiguous {
            None
        } else {
            best.map(|(_, _, reason)| reason)
        }
    }

    /// Return compact file-owned Unknown evidence for non-call expressions.
    ///
    /// The evidence is range-sorted and contains at most one reason per exact
    /// expression. It intentionally lives outside the general type store so
    /// retaining an unproven read cannot increase graph replacement work.
    pub fn expression_unknown_reasons_in_file(
        &self,
        file_id: SourceFileId,
    ) -> Option<&[(TextRange, UnknownReason)]> {
        self.engine.expression_unknown_reasons_in_file(file_id)
    }

    /// Return the exact expression fact covering a source position.
    ///
    /// Unknown remains observable so adapters cannot fall back to an older or
    /// independently inferred concrete type. If equally specific distinct
    /// expression ranges overlap, the result fails closed to Unknown.
    pub fn expression_type_at(&self, file_id: SourceFileId, byte_offset: u32) -> Option<RubyType> {
        let mut best_span = None;
        let mut best_range = None;
        let mut best_types = Vec::new();
        let mut ambiguous_range = false;

        let mut consider = |range: TextRange, ruby_type: RubyType| {
            if !range.contains_offset(file_id, byte_offset) {
                return;
            }
            let span = range
                .end_byte
                .checked_sub(range.start_byte)
                .expect_invariant(
                    "an expression type fact has an inverted range",
                    "TextRange producers must emit start <= end",
                    "validate the indexer range before inserting the expression fact",
                );
            match best_span {
                None => {
                    best_span = Some(span);
                    best_range = Some(range);
                    best_types.push(ruby_type);
                    ambiguous_range = false;
                }
                Some(best) if span < best => {
                    best_span = Some(span);
                    best_range = Some(range);
                    best_types.clear();
                    best_types.push(ruby_type);
                    ambiguous_range = false;
                }
                Some(best) if span == best && best_range == Some(range) => {
                    best_types.push(ruby_type);
                }
                Some(best) if span == best => {
                    ambiguous_range = true;
                }
                Some(_) => {}
            }
        };

        for fact in self.engine.type_store().facts_in_file(file_id) {
            let TypeSubject::Expression(range) = fact.subject else {
                continue;
            };
            consider(range, fact.ruby_type);
        }
        if let Some(outcomes) = self.engine.call_expression_outcome_views_in_file(file_id) {
            for (range, outcome) in outcomes {
                let ruby_type = match outcome {
                    TypeInferenceOutcomeRef::Proven(ruby_type) => ruby_type.clone(),
                    TypeInferenceOutcomeRef::Unknown(_) => RubyType::Unknown,
                };
                consider(range, ruby_type);
            }
        }
        // Local-variable hover asks the exact range-sorted local query first.
        // Consult it here only when no ordinary expression or call outcome
        // covers the position, keeping method-call hover on its established
        // hot path while preserving the generic local-expression result.
        if best_span.is_none() {
            if let Some(ruby_type) = self.local_read_type_at(file_id, byte_offset) {
                return Some(ruby_type);
            }
        }

        if best_span.is_none() {
            return None;
        }
        if ambiguous_range {
            return Some(RubyType::Unknown);
        }
        Some(RubyType::union(best_types))
    }

    pub fn call_expression_outcomes_in_file(
        &self,
        file_id: SourceFileId,
    ) -> Option<Vec<(TextRange, crate::core::TypeInferenceOutcome)>> {
        self.engine.call_expression_outcomes_in_file(file_id)
    }

    /// Return the proof result for the innermost complete call containing a
    /// source position.
    ///
    /// Method hover uses this query instead of the generic expression query:
    /// identifier-level facts may be more narrowly ranged than the complete
    /// call, but they cannot supersede the call solver's final proof result.
    pub fn call_expression_outcome_at_position(
        &self,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> Option<TypeInferenceOutcome> {
        let outcomes = self.engine.call_expression_outcome_views_in_file(file_id)?;
        let mut best: Option<(u32, TextRange, TypeInferenceOutcome)> = None;
        for (range, outcome) in outcomes {
            if !range.contains_offset(file_id, byte_offset) {
                continue;
            }
            let span = range
                .end_byte
                .checked_sub(range.start_byte)
                .expect_invariant(
                    "a call-expression outcome has an inverted range",
                    "TextRange producers must emit start <= end",
                    "validate the call range before storing its proof outcome",
                );
            let outcome = match outcome {
                TypeInferenceOutcomeRef::Proven(ruby_type) => {
                    TypeInferenceOutcome::proven(ruby_type.clone())
                }
                TypeInferenceOutcomeRef::Unknown(reason) => TypeInferenceOutcome::unknown(reason),
            };
            match &best {
                None => best = Some((span, range, outcome)),
                Some((best_span, _, _)) if span < *best_span => {
                    best = Some((span, range, outcome));
                }
                Some((best_span, best_range, _)) if span == *best_span => {
                    invariant_eq!(
                        range,
                        *best_range,
                        what = "distinct equally specific call ranges overlap one source position",
                        why = "one syntax position cannot belong to two sibling complete calls with identical spans",
                        fix = "emit one normalized call-expression range per AST call",
                    );
                }
                Some(_) => {}
            }
        }
        best.map(|(_, _, outcome)| outcome)
    }

    /// Return exact concrete local-read types solved by the shared flow
    /// tracker. Entries are sorted by range and replaced with their file.
    pub fn local_read_types_in_file(
        &self,
        file_id: SourceFileId,
    ) -> Option<Vec<(TextRange, RubyType)>> {
        self.engine.local_read_types_in_file(file_id)
    }

    pub fn local_read_type_at(&self, file_id: SourceFileId, byte_offset: u32) -> Option<RubyType> {
        self.engine
            .local_read_type_at(file_id, byte_offset)
            .cloned()
    }

    /// Return the authoritative type outcome of the most specific expression
    /// ending at the exact byte boundary.
    ///
    /// A stored call outcome owns its exact range and takes precedence over
    /// expression facts for that same syntax. Explained Unknown evidence is
    /// returned as `RubyType::Unknown` so request-time consumers cannot mistake
    /// it for missing evidence and fall back to an older concrete type.
    pub fn expression_type_ending_at(
        &self,
        file_id: SourceFileId,
        end_byte: u32,
    ) -> Option<RubyType> {
        let expression_facts = self
            .engine
            .type_store()
            .facts_in_file(file_id)
            .into_iter()
            .filter_map(|fact| match fact.subject {
                TypeSubject::Expression(range) if range.end_byte == end_byte => {
                    Some((range, fact.ruby_type))
                }
                TypeSubject::Constant(_)
                | TypeSubject::Local { .. }
                | TypeSubject::InstanceVariable { .. }
                | TypeSubject::ClassVariable { .. }
                | TypeSubject::GlobalVariable(_)
                | TypeSubject::MethodReturn(_)
                | TypeSubject::Parameter { .. }
                | TypeSubject::Expression(_) => None,
            })
            .collect::<Vec<_>>();
        let local_reads = self
            .engine
            .local_read_types_in_file(file_id)
            .into_iter()
            .flatten()
            .filter(|(range, _)| range.end_byte == end_byte)
            .collect::<Vec<_>>();
        let call_ranges = self
            .engine
            .call_expression_outcome_views_in_file(file_id)
            .into_iter()
            .flatten()
            .filter_map(|(range, _)| (range.end_byte == end_byte).then_some(range))
            .collect::<Vec<_>>();
        let unknown_ranges = self
            .engine
            .expression_unknown_reasons_in_file(file_id)
            .into_iter()
            .flatten()
            .filter_map(|(range, _)| (range.end_byte == end_byte).then_some(*range))
            .collect::<Vec<_>>();
        let most_specific_start = expression_facts
            .iter()
            .map(|(range, _)| range.start_byte)
            .chain(local_reads.iter().map(|(range, _)| range.start_byte))
            .chain(call_ranges.iter().map(|range| range.start_byte))
            .chain(unknown_ranges.iter().map(|range| range.start_byte))
            .max()?;
        let range = TextRange::new(file_id, most_specific_start, end_byte);

        if let Some(outcome) = self.engine.call_expression_outcome_at(range) {
            return Some(match outcome {
                TypeInferenceOutcomeRef::Proven(ruby_type) => ruby_type.clone(),
                TypeInferenceOutcomeRef::Unknown(_) => RubyType::Unknown,
            });
        }
        if self.engine.expression_unknown_reason(range).is_some() {
            return Some(RubyType::Unknown);
        }

        let expression_types = expression_facts
            .into_iter()
            .chain(local_reads)
            .filter_map(|(candidate, ruby_type)| (candidate == range).then_some(ruby_type))
            .collect::<Vec<_>>();
        invariant!(
            !expression_types.is_empty(),
            what = "end-boundary selection found {range:?} with no outcome, reason, fact, or local-read type",
            why = "the range came from exactly those stores",
            fix = "keep candidate selection and exact-range projection exhaustive",
            range = range,
        );
        if expression_types
            .iter()
            .any(|ruby_type| *ruby_type == RubyType::Unknown)
        {
            return Some(RubyType::Unknown);
        }
        Some(RubyType::union(expression_types))
    }

    /// Return only a proven expression type at an exact end boundary.
    ///
    /// Inlay hints intentionally omit Unknown outcomes; consumers that must
    /// distinguish Unknown from absent evidence use `expression_type_ending_at`.
    pub fn proven_expression_type_ending_at(
        &self,
        file_id: SourceFileId,
        end_byte: u32,
    ) -> Option<RubyType> {
        self.expression_type_ending_at(file_id, end_byte)
            .filter(|ruby_type| *ruby_type != RubyType::Unknown)
    }
}
