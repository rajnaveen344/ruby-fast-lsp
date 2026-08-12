use std::collections::{HashMap, HashSet};

use crate::core::{
    FullyQualifiedName, GraphNodeKind, MethodFact, NamespaceKind, ResolvedMethodCallee,
    RubyConstant, RubyMethod, RubyType, SourceFileId, SourceKind, TextRange, TypeFact,
    TypeInferenceOutcome, TypeResolution, TypeSubject, UnknownReason,
};
use parking_lot::Mutex;

use super::state::TypeInferenceOutcomeRef;

const MAX_RESOLVED_METHOD_CACHE_ENTRIES_PER_SOURCE: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) enum MethodReturnQueryAccess {
    Private,
    Public,
    Protected(FullyQualifiedName),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct MethodReturnQueryKey {
    namespace: FullyQualifiedName,
    method: RubyMethod,
    access: MethodReturnQueryAccess,
}

#[derive(Debug, Default)]
struct AnalysisQueryCacheState {
    engine_identity: Option<(u64, u64)>,
    method_returns: HashMap<MethodReturnQueryKey, Option<RubyType>>,
    method_callees: HashMap<MethodReturnQueryKey, Option<Vec<ResolvedMethodCallee>>>,
}

/// Bounded-lifetime memoization for repeated semantic queries while collecting
/// one source product.
///
/// The cache binds itself to one exact engine instance and semantic revision.
/// Replacements and engine clones receive different identities, so callers
/// cannot accidentally reuse results across isolated project engines.
#[derive(Debug, Default)]
pub struct AnalysisQueryCache {
    state: Mutex<AnalysisQueryCacheState>,
}

impl AnalysisQueryCache {
    fn method_return(
        &self,
        engine_identity: (u64, u64),
        key: MethodReturnQueryKey,
        compute: impl FnOnce() -> Option<RubyType>,
    ) -> Option<RubyType> {
        {
            let mut state = self.state.lock();
            if state.engine_identity != Some(engine_identity) {
                state.engine_identity = Some(engine_identity);
                state.method_returns.clear();
                state.method_callees.clear();
            }
            if let Some(cached) = state.method_returns.get(&key) {
                return cached.clone();
            }
        }

        let result = compute();
        let mut state = self.state.lock();
        if state.engine_identity == Some(engine_identity) {
            state.method_returns.insert(key, result.clone());
        }
        result
    }

    pub(super) fn method_callees(
        &self,
        engine_identity: (u64, u64),
        namespace: &FullyQualifiedName,
        method: RubyMethod,
        access: MethodReturnQueryAccess,
        compute: impl FnOnce() -> Option<Vec<ResolvedMethodCallee>>,
    ) -> Option<Vec<ResolvedMethodCallee>> {
        let key = MethodReturnQueryKey {
            namespace: namespace.clone(),
            method,
            access,
        };
        {
            let mut state = self.state.lock();
            if state.engine_identity != Some(engine_identity) {
                state.engine_identity = Some(engine_identity);
                state.method_returns.clear();
                state.method_callees.clear();
            }
            if let Some(cached) = state.method_callees.get(&key) {
                return cached.clone();
            }
        }

        let result = compute();
        let mut state = self.state.lock();
        if state.engine_identity == Some(engine_identity)
            && state.method_callees.len() < MAX_RESOLVED_METHOD_CACHE_ENTRIES_PER_SOURCE
        {
            state.method_callees.insert(key, result.clone());
        }
        result
    }

    #[cfg(test)]
    pub(crate) fn valid_entry_counts_for_test(&self) -> (usize, usize) {
        let state = self.state.lock();
        (state.method_returns.len(), state.method_callees.len())
    }
}
use crate::engine::lookup_types::{ConstantHover, ConstantHoverKind, VariableTypeKind};
use crate::engine::query::AnalysisQuery;
use crate::engine::resolution::{
    execution_context_application_targets, method_facts_in_chain, method_lookup_chain,
    method_missing_method, namespace_target_exists,
};

type MethodVisitKey = (FullyQualifiedName, SourceFileId, u32, u32);

impl<'a> AnalysisQuery<'a> {
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
    /// result cannot be mistaken for the receiver's own type.
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
        if let Some(reads) = self.engine.local_read_type_views_in_file(range.file_id) {
            let exact = reads
                .filter_map(|(candidate, ruby_type)| {
                    (candidate == range).then_some(ruby_type.clone())
                })
                .collect::<Vec<_>>();
            if !exact.is_empty() {
                return Some(RubyType::union(exact));
            }
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
            let span = range.end_byte.checked_sub(range.start_byte).expect(
                "INVARIANT VIOLATED: an expression Unknown reason has an inverted range. This is a bug because TextRange producers must emit start <= end. Fix: validate the indexer range before recording proof evidence.",
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
            let span = range.end_byte.checked_sub(range.start_byte).expect(
                "INVARIANT VIOLATED: an expression type fact has an inverted range. This is a bug because TextRange producers must emit start <= end. Fix: validate the indexer range before inserting the expression fact.",
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
            let span = range.end_byte.checked_sub(range.start_byte).expect(
                "INVARIANT VIOLATED: a call-expression outcome has an inverted range. This is a bug because TextRange producers must emit start <= end. Fix: validate the call range before storing its proof outcome.",
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
                    assert_eq!(
                        range, *best_range,
                        "INVARIANT VIOLATED: distinct equally specific call ranges overlap one source position. This is a bug because one syntax position cannot belong to two sibling complete calls with identical spans. Fix: emit one normalized call-expression range per AST call."
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
        assert!(
            !expression_types.is_empty(),
            "INVARIANT VIOLATED: expression end-boundary selection found range {range:?} without a call outcome, Unknown reason, expression fact, or local-read type. This is a bug because the selected range came from exactly those stores. Fix: keep candidate selection and exact-range projection exhaustive."
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

    pub fn method_return_type_at(
        &self,
        name: &str,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> Option<RubyType> {
        self.method_return_type_at_with_kind_filter(name, None, file_id, byte_offset)
    }

    pub fn method_return_type_at_with_kind(
        &self,
        name: &str,
        namespace_kind: NamespaceKind,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> Option<RubyType> {
        self.method_return_type_at_with_kind_filter(
            name,
            Some(namespace_kind),
            file_id,
            byte_offset,
        )
    }

    fn method_return_type_at_with_kind_filter(
        &self,
        name: &str,
        namespace_kind: Option<NamespaceKind>,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> Option<RubyType> {
        let method_fact = self
            .engine
            .method_facts_in_file(file_id)
            .into_iter()
            .find(|fact| {
                let FullyQualifiedName::Method(_, method) = &fact.fqn else {
                    return false;
                };
                method.as_str() == name
                    && namespace_kind
                        .map(|kind| fact.owner.namespace_kind() == Some(kind))
                        .unwrap_or(true)
                    && fact.range.start_byte <= byte_offset
                    && byte_offset <= fact.range.end_byte
            })?;

        self.engine
            .type_store()
            .facts_in_file(file_id)
            .into_iter()
            .filter_map(|fact| match &fact.subject {
                TypeSubject::MethodReturn(method) if method == &method_fact.fqn => Some(fact),
                TypeSubject::Constant(_)
                | TypeSubject::Local { .. }
                | TypeSubject::InstanceVariable { .. }
                | TypeSubject::ClassVariable { .. }
                | TypeSubject::GlobalVariable(_)
                | TypeSubject::MethodReturn(_)
                | TypeSubject::Parameter { .. }
                | TypeSubject::Expression(_) => None,
            })
            .filter(|fact| {
                method_fact.range.file_id == fact.range.file_id
                    && method_fact.range.start_byte <= fact.range.start_byte
                    && fact.range.end_byte <= method_fact.range.end_byte
            })
            .max_by_key(|fact| fact.range.start_byte)
            .map(|fact| fact.ruby_type)
            .or_else(|| self.method_return_type(&method_fact))
    }

    pub fn parameter_type_at(
        &self,
        method_name: &str,
        param_name: &str,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> Option<RubyType> {
        let method_fact = self
            .engine
            .method_facts_in_file(file_id)
            .into_iter()
            .find(|fact| {
                let FullyQualifiedName::Method(_, method) = &fact.fqn else {
                    return false;
                };
                method.as_str() == method_name
                    && fact.range.start_byte <= byte_offset
                    && byte_offset <= fact.range.end_byte
            })?;

        self.engine
            .type_store()
            .facts_in_file(file_id)
            .into_iter()
            .filter_map(|fact| match &fact.subject {
                TypeSubject::Parameter { method, name }
                    if method == &method_fact.fqn
                        && name == param_name
                        && fact.ruby_type != RubyType::Unknown =>
                {
                    Some(fact)
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
            .max_by_key(|fact| fact.range.start_byte)
            .map(|fact| fact.ruby_type)
    }

    /// Return one complete project-RBS parameter contract for a Ruby method.
    ///
    /// Declaration ownership and source kind are part of the proof: a Ruby
    /// implementation fact cannot accidentally become its own contract, and
    /// instance/singleton homonyms remain isolated even though their method
    /// subjects share one Ruby name FQN.
    pub fn rbs_parameter_contract_type(
        &self,
        method: &FullyQualifiedName,
        owner: &FullyQualifiedName,
        parameter_name: &str,
    ) -> Option<RubyType> {
        let signature_methods = self
            .engine
            .method_facts_for(method)
            .into_iter()
            .filter(|fact| {
                fact.owner == *owner
                    && self
                        .engine
                        .file(fact.range.file_id)
                        .is_some_and(|file| file.kind == SourceKind::Signature)
            })
            .collect::<Vec<_>>();
        if signature_methods.is_empty() {
            return None;
        }

        let mut contract_types = Vec::with_capacity(signature_methods.len());
        for signature in signature_methods {
            let contract = self
                .engine
                .type_store()
                .facts_in_file(signature.range.file_id)
                .into_iter()
                .find_map(|fact| match &fact.subject {
                    TypeSubject::Parameter { method, name }
                        if method == &signature.fqn
                            && name == parameter_name
                            && signature.range.start_byte <= fact.range.start_byte
                            && fact.range.end_byte <= signature.range.end_byte
                            && fact.ruby_type != RubyType::Unknown =>
                    {
                        Some(fact.ruby_type)
                    }
                    TypeSubject::Constant(_)
                    | TypeSubject::Local { .. }
                    | TypeSubject::InstanceVariable { .. }
                    | TypeSubject::ClassVariable { .. }
                    | TypeSubject::GlobalVariable(_)
                    | TypeSubject::MethodReturn(_)
                    | TypeSubject::Parameter { .. }
                    | TypeSubject::Expression(_) => None,
                });
            contract_types.push(contract?);
        }

        let contract = RubyType::union(contract_types);
        (!RubyType::contains_unknown(&contract)).then_some(contract)
    }

    /// Return the exhaustive project-RBS return contract for one owner.
    /// Every matching signature declaration must carry a complete type fact;
    /// otherwise diagnostics and body inference stay fail-closed.
    pub fn rbs_return_contract_type(
        &self,
        method: &FullyQualifiedName,
        owner: &FullyQualifiedName,
    ) -> Option<RubyType> {
        let signature_methods = self
            .engine
            .method_facts_for(method)
            .into_iter()
            .filter(|fact| {
                fact.owner == *owner
                    && self
                        .engine
                        .file(fact.range.file_id)
                        .is_some_and(|file| file.kind == SourceKind::Signature)
            })
            .collect::<Vec<_>>();
        if signature_methods.is_empty() {
            return None;
        }

        let mut contract_types = Vec::with_capacity(signature_methods.len());
        for signature in signature_methods {
            let contract = self
                .engine
                .type_store()
                .facts_in_file(signature.range.file_id)
                .into_iter()
                .find_map(|fact| match &fact.subject {
                    TypeSubject::MethodReturn(method)
                        if method == &signature.fqn
                            && signature.range.start_byte <= fact.range.start_byte
                            && fact.range.end_byte <= signature.range.end_byte
                            && fact.ruby_type != RubyType::Unknown =>
                    {
                        Some(fact.ruby_type)
                    }
                    TypeSubject::Constant(_)
                    | TypeSubject::Local { .. }
                    | TypeSubject::InstanceVariable { .. }
                    | TypeSubject::ClassVariable { .. }
                    | TypeSubject::GlobalVariable(_)
                    | TypeSubject::MethodReturn(_)
                    | TypeSubject::Parameter { .. }
                    | TypeSubject::Expression(_) => None,
                });
            contract_types.push(contract?);
        }

        let contract = RubyType::union(contract_types);
        (!RubyType::contains_unknown(&contract)).then_some(contract)
    }

    pub fn variable_type_before_in_owner(
        &self,
        kind: VariableTypeKind,
        name: &str,
        owner: &FullyQualifiedName,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> Option<RubyType> {
        assert!(
            matches!(
                kind,
                VariableTypeKind::Instance
                    | VariableTypeKind::Class
                    | VariableTypeKind::Global
            ),
            "INVARIANT VIOLATED: an owner-aware variable query received a local or constant kind. This is a bug because locals require a lexical scope and constants require lexical constant resolution. Fix: use local_variable_type_at or the constant query instead."
        );

        let matching = self
            .engine
            .type_store()
            .facts_in_file(file_id)
            .into_iter()
            .filter(|fact| fact.range.start_byte <= byte_offset)
            .filter(|fact| match (&fact.subject, kind) {
                (
                    TypeSubject::InstanceVariable {
                        owner: fact_owner,
                        name: fact_name,
                    },
                    VariableTypeKind::Instance,
                ) => fact_name == name && fact_owner == owner,
                (
                    TypeSubject::ClassVariable {
                        owner: fact_owner,
                        name: fact_name,
                    },
                    VariableTypeKind::Class,
                ) => fact_name == name && fact_owner.namespace_parts() == owner.namespace_parts(),
                (TypeSubject::GlobalVariable(fact_name), VariableTypeKind::Global) => {
                    fact_name == name
                }
                (
                    TypeSubject::Constant(_)
                    | TypeSubject::Local { .. }
                    | TypeSubject::InstanceVariable { .. }
                    | TypeSubject::ClassVariable { .. }
                    | TypeSubject::GlobalVariable(_)
                    | TypeSubject::MethodReturn(_)
                    | TypeSubject::Parameter { .. }
                    | TypeSubject::Expression(_),
                    VariableTypeKind::Local
                    | VariableTypeKind::Instance
                    | VariableTypeKind::Class
                    | VariableTypeKind::Global
                    | VariableTypeKind::Constant,
                ) => false,
            });

        Self::latest_unambiguous_concrete_type(matching)
    }

    /// Return the type fact attached to one exact variable write token.
    ///
    /// Unlike a flow lookup, an assignment inlay must describe this write's
    /// right-hand side. Falling back to an earlier concrete write when this
    /// exact write is Unknown would publish a type with no proof. Duplicate
    /// producers may agree on the same fact; any conflicting payload fails
    /// closed to Unknown.
    pub fn variable_assignment_type_at(
        &self,
        kind: VariableTypeKind,
        name: &str,
        file_id: SourceFileId,
        name_start_offset: u32,
        name_end_offset: u32,
    ) -> Option<RubyType> {
        assert!(
            name_start_offset <= name_end_offset,
            "INVARIANT VIOLATED: a variable assignment name range is reversed. This is a bug because exact-write type queries require a normalized source range. Fix: pass the Prism name location without swapping its offsets."
        );
        let mut best_span = None;
        let mut best_type = None;
        let mut conflicting_best_type = false;
        for fact in self
            .engine
            .type_store()
            .facts_in_file(file_id)
            .into_iter()
            .filter(|fact| {
                fact.range.start_byte <= name_start_offset && name_end_offset <= fact.range.end_byte
            })
        {
            let matches = match (&fact.subject, kind) {
                (
                    TypeSubject::Local {
                        scope_id: _,
                        name: fact_name,
                    },
                    VariableTypeKind::Local,
                ) => fact_name == name,
                (
                    TypeSubject::InstanceVariable {
                        owner: _,
                        name: fact_name,
                    },
                    VariableTypeKind::Instance,
                ) => fact_name == name,
                (
                    TypeSubject::ClassVariable {
                        owner: _,
                        name: fact_name,
                    },
                    VariableTypeKind::Class,
                ) => fact_name == name,
                (TypeSubject::GlobalVariable(fact_name), VariableTypeKind::Global) => {
                    fact_name == name
                }
                (TypeSubject::Constant(fqn), VariableTypeKind::Constant) => fqn.name() == name,
                (
                    TypeSubject::Constant(_)
                    | TypeSubject::Local { .. }
                    | TypeSubject::InstanceVariable { .. }
                    | TypeSubject::ClassVariable { .. }
                    | TypeSubject::GlobalVariable(_)
                    | TypeSubject::MethodReturn(_)
                    | TypeSubject::Parameter { .. }
                    | TypeSubject::Expression(_),
                    VariableTypeKind::Local
                    | VariableTypeKind::Instance
                    | VariableTypeKind::Class
                    | VariableTypeKind::Global
                    | VariableTypeKind::Constant,
                ) => false,
            };
            if !matches {
                continue;
            }
            let span = fact.range.end_byte.checked_sub(fact.range.start_byte).expect(
                    "INVARIANT VIOLATED: a stored type fact range is reversed. This is a bug because TypeFact ranges must remain normalized. Fix: construct type facts through TextRange::new and preserve that invariant during replacement.",
                );
            match best_span {
                None => {
                    best_span = Some(span);
                    best_type = Some(fact.ruby_type);
                }
                Some(current_span) if span < current_span => {
                    best_span = Some(span);
                    best_type = Some(fact.ruby_type);
                    conflicting_best_type = false;
                }
                Some(current_span) if span == current_span => {
                    if best_type.as_ref() != Some(&fact.ruby_type) {
                        conflicting_best_type = true;
                    }
                }
                Some(_) => {}
            }
        }

        if conflicting_best_type {
            Some(RubyType::Unknown)
        } else {
            best_type
        }
    }

    pub fn local_variable_type_at(
        &self,
        name: &str,
        scope_id: u32,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> Option<RubyType> {
        match self.engine.type_store().type_at(
            &TypeSubject::Local {
                scope_id,
                name: name.to_string(),
            },
            file_id,
            byte_offset,
        ) {
            TypeResolution::Resolved(fact) => return Some(fact.ruby_type),
            TypeResolution::Ambiguous(_) => return None,
            TypeResolution::Unresolved => {}
        }

        self.engine
            .type_store()
            .facts_in_file(file_id)
            .into_iter()
            .filter(|fact| fact.range.start_byte <= byte_offset)
            .filter_map(|fact| match &fact.subject {
                TypeSubject::Parameter {
                    method: _,
                    name: fact_name,
                } if fact_name == name && fact.ruby_type != RubyType::Unknown => Some(fact),
                TypeSubject::Constant(_)
                | TypeSubject::Local { .. }
                | TypeSubject::InstanceVariable { .. }
                | TypeSubject::ClassVariable { .. }
                | TypeSubject::GlobalVariable(_)
                | TypeSubject::MethodReturn(_)
                | TypeSubject::Parameter { .. }
                | TypeSubject::Expression(_) => None,
            })
            .max_by_key(|fact| fact.range.start_byte)
            .map(|fact| fact.ruby_type)
    }

    pub fn namespace_node_kind(&self, namespace_fqn: &FullyQualifiedName) -> Option<GraphNodeKind> {
        self.engine
            .graph_nodes_for(namespace_fqn)
            .iter()
            .max_by_key(|fact| {
                (
                    fact.range.file_id,
                    fact.range.start_byte,
                    fact.range.end_byte,
                )
            })
            .map(|fact| fact.kind)
    }

    pub fn namespace_exists(&self, namespace_fqn: &FullyQualifiedName) -> bool {
        self.namespace_node_kind(namespace_fqn).is_some()
    }

    pub fn namespace_type(&self, namespace_fqn: &FullyQualifiedName) -> Option<RubyType> {
        match self.namespace_node_kind(namespace_fqn)? {
            GraphNodeKind::Class => Some(RubyType::Class(namespace_fqn.clone())),
            GraphNodeKind::Module => Some(RubyType::Module(namespace_fqn.clone())),
        }
    }

    pub fn constant_reference_type(&self, path: &[RubyConstant]) -> Option<RubyType> {
        let namespace_fqn = FullyQualifiedName::namespace(path.to_vec());
        let constant_fqn = FullyQualifiedName::constant(path.to_vec());
        match self.namespace_node_kind(&namespace_fqn)? {
            GraphNodeKind::Class => Some(RubyType::ClassReference(constant_fqn)),
            GraphNodeKind::Module => Some(RubyType::ModuleReference(constant_fqn)),
        }
    }

    pub fn type_to_namespace(&self, ruby_type: &RubyType) -> Option<FullyQualifiedName> {
        match ruby_type {
            RubyType::Class(fqn) | RubyType::Module(fqn) => {
                Some(FullyQualifiedName::namespace_with_kind(
                    fqn.namespace_parts(),
                    crate::core::NamespaceKind::Instance,
                ))
            }
            RubyType::ClassReference(fqn) | RubyType::ModuleReference(fqn) => {
                Some(FullyQualifiedName::namespace_with_kind(
                    fqn.namespace_parts(),
                    crate::core::NamespaceKind::Singleton,
                ))
            }
            RubyType::Array(_) => Some(FullyQualifiedName::namespace_with_kind(
                vec![RubyConstant::new("Array").expect(
                    "INVARIANT VIOLATED: built-in constant `Array` is invalid. \
                     This is a bug because Ruby built-in constants must be valid Ruby constants. \
                     Fix: correct the hard-coded built-in constant name.",
                )],
                crate::core::NamespaceKind::Instance,
            )),
            RubyType::Hash(_, _) => Some(FullyQualifiedName::namespace_with_kind(
                vec![RubyConstant::new("Hash").expect(
                    "INVARIANT VIOLATED: built-in constant `Hash` is invalid. \
                     This is a bug because Ruby built-in constants must be valid Ruby constants. \
                     Fix: correct the hard-coded built-in constant name.",
                )],
                crate::core::NamespaceKind::Instance,
            )),
            RubyType::Shape(_) => self.type_to_namespace(&RubyType::Hash(
                vec![RubyType::Unknown],
                vec![RubyType::Unknown],
            )),
            RubyType::Literal(value) => self.type_to_namespace(&value.widened_type()),
            RubyType::Union(_) | RubyType::Unknown => None,
        }
    }

    pub fn constructor_return_type_for_namespace(
        &self,
        namespace_fqn: &FullyQualifiedName,
    ) -> Option<RubyType> {
        if namespace_fqn.namespace_kind() != Some(crate::core::NamespaceKind::Singleton) {
            return None;
        }

        Some(RubyType::Class(FullyQualifiedName::constant(
            namespace_fqn.namespace_parts(),
        )))
    }

    pub fn constant_value_type(&self, constant_fqn: &FullyQualifiedName) -> Option<RubyType> {
        self.engine
            .type_store()
            .facts_for(&TypeSubject::Constant(constant_fqn.clone()))
            .iter()
            .filter(|fact| fact.ruby_type != RubyType::Unknown)
            .max_by_key(|fact| {
                (
                    fact.range.file_id,
                    fact.range.start_byte,
                    fact.range.end_byte,
                )
            })
            .map(|fact| fact.ruby_type.clone())
    }

    pub fn constant_hover(&self, path: &[RubyConstant]) -> Option<ConstantHover> {
        let namespace_fqn = FullyQualifiedName::namespace(path.to_vec());
        let constant_fqn = FullyQualifiedName::constant(path.to_vec());
        let name = path
            .iter()
            .map(|constant| constant.to_string())
            .collect::<Vec<_>>()
            .join("::");

        match self.namespace_node_kind(&namespace_fqn) {
            Some(GraphNodeKind::Class) => {
                return Some(ConstantHover {
                    name,
                    kind: ConstantHoverKind::Class,
                });
            }
            Some(GraphNodeKind::Module) => {
                return Some(ConstantHover {
                    name,
                    kind: ConstantHoverKind::Module,
                });
            }
            None => {}
        }

        self.constant_value_type(&constant_fqn)
            .map(|ruby_type| ConstantHover {
                name,
                kind: ConstantHoverKind::Value(ruby_type),
            })
    }

    pub fn known_namespace_fqns(&self) -> HashSet<FullyQualifiedName> {
        self.engine
            .symbol_store()
            .known_namespace_fqns()
            .into_iter()
            .filter_map(|id| self.engine.fqn_for_id(id).cloned())
            .collect()
    }

    pub fn method_return_type(&self, fact: &MethodFact) -> Option<crate::core::RubyType> {
        let mut seen = HashSet::new();
        self.method_return_type_inner(fact, &mut seen)
    }

    /// Resolve return types for a callee that has already passed ordinary MRO,
    /// visibility, execution-context, and `method_missing` resolution.
    ///
    /// Callers that already own a [`ResolvedMethodCallee`] must use this path
    /// instead of resolving the callee owner as a fresh receiver. Re-running
    /// receiver lookup is both redundant and subtly different for module
    /// includers because the callee already identifies the winning definition
    /// ranges.
    pub fn method_return_type_for_callee(
        &self,
        callee: &ResolvedMethodCallee,
    ) -> Option<crate::core::RubyType> {
        if callee.definition_ranges.is_empty() {
            return None;
        }

        let mut facts = self
            .engine
            .method_facts_matching_owner_name(&callee.owner, &callee.method)
            .into_iter()
            .filter(|fact| callee.definition_ranges.contains(&fact.range))
            .collect::<Vec<_>>();
        facts.sort_by_key(|fact| {
            (
                fact.range.file_id,
                fact.range.start_byte,
                fact.range.end_byte,
                fact.fqn.to_string(),
            )
        });
        facts.dedup();

        let mut seen = HashSet::new();
        RubyType::union_from_proven(facts.iter(), |fact| {
            self.method_return_type_inner(fact, &mut seen)
        })
    }

    fn method_return_type_inner(
        &self,
        fact: &MethodFact,
        seen: &mut HashSet<MethodVisitKey>,
    ) -> Option<crate::core::RubyType> {
        if !seen.insert((
            fact.fqn.clone(),
            fact.range.file_id,
            fact.range.start_byte,
            fact.range.end_byte,
        )) {
            return None;
        }

        match self.engine.type_at(
            &TypeSubject::MethodReturn(fact.fqn.clone()),
            fact.range.file_id,
            fact.range.end_byte,
        ) {
            TypeResolution::Resolved(type_fact) => return Some(type_fact.ruby_type),
            TypeResolution::Ambiguous(_) | TypeResolution::Unresolved => {}
        }

        let FullyQualifiedName::Method(_, method) = &fact.fqn else {
            panic!(
                "INVARIANT VIOLATED: method return lookup received a non-method fact {}. \
                 This is a bug because MethodFact FQNs must always use the Method variant. \
                 Fix: validate method facts before engine insertion.",
                fact.fqn
            );
        };
        let signatures = self
            .engine
            .method_facts_matching_owner_name(&fact.owner, method)
            .into_iter()
            .filter(|signature| {
                self.engine
                    .file(signature.range.file_id)
                    .expect(
                        "INVARIANT VIOLATED: RBS method fact references an unregistered source file. \
                         This is a bug because type overlay requires stable signature metadata. \
                         Fix: remove signature facts through per-file replacement.",
                    )
                    .kind
                    == crate::core::SourceKind::Signature
            })
            .collect::<Vec<_>>();
        if !signatures.is_empty() {
            return RubyType::union_from_proven(signatures, |signature| {
                match self.engine.type_at(
                    &TypeSubject::MethodReturn(signature.fqn),
                    signature.range.file_id,
                    signature.range.end_byte,
                ) {
                    TypeResolution::Resolved(type_fact) => Some(type_fact.ruby_type),
                    TypeResolution::Ambiguous(_) | TypeResolution::Unresolved => None,
                }
            });
        }

        self.delegate_method_return_type(fact, seen)
    }

    pub fn method_return_type_for_receiver(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Option<crate::core::RubyType> {
        let mut seen = HashSet::new();
        self.method_return_type_for_receiver_inner(namespace_fqn, method, true, None, &mut seen)
    }

    pub fn method_return_type_for_receiver_cached(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
        cache: &AnalysisQueryCache,
    ) -> Option<crate::core::RubyType> {
        let key = MethodReturnQueryKey {
            namespace: namespace_fqn.clone(),
            method: *method,
            access: MethodReturnQueryAccess::Private,
        };
        cache.method_return(self.engine.query_cache_identity(), key, || {
            self.method_return_type_for_receiver(namespace_fqn, method)
        })
    }

    pub fn method_return_type_for_public_receiver(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Option<crate::core::RubyType> {
        let mut seen = HashSet::new();
        self.method_return_type_for_receiver_inner(namespace_fqn, method, false, None, &mut seen)
    }

    pub fn method_return_type_for_public_receiver_cached(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
        cache: &AnalysisQueryCache,
    ) -> Option<crate::core::RubyType> {
        let key = MethodReturnQueryKey {
            namespace: namespace_fqn.clone(),
            method: *method,
            access: MethodReturnQueryAccess::Public,
        };
        cache.method_return(self.engine.query_cache_identity(), key, || {
            self.method_return_type_for_public_receiver(namespace_fqn, method)
        })
    }

    pub fn method_return_type_for_protected_receiver(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
        caller_namespace_fqn: &FullyQualifiedName,
    ) -> Option<crate::core::RubyType> {
        let mut seen = HashSet::new();
        self.method_return_type_for_receiver_inner(
            namespace_fqn,
            method,
            false,
            Some(caller_namespace_fqn),
            &mut seen,
        )
    }

    pub fn method_return_type_for_protected_receiver_cached(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
        caller_namespace_fqn: &FullyQualifiedName,
        cache: &AnalysisQueryCache,
    ) -> Option<crate::core::RubyType> {
        let key = MethodReturnQueryKey {
            namespace: namespace_fqn.clone(),
            method: *method,
            access: MethodReturnQueryAccess::Protected(caller_namespace_fqn.clone()),
        };
        cache.method_return(self.engine.query_cache_identity(), key, || {
            self.method_return_type_for_protected_receiver(
                namespace_fqn,
                method,
                caller_namespace_fqn,
            )
        })
    }

    fn method_return_type_for_receiver_inner(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
        allow_private: bool,
        protected_caller: Option<&FullyQualifiedName>,
        seen: &mut HashSet<MethodVisitKey>,
    ) -> Option<crate::core::RubyType> {
        if !namespace_target_exists(self.engine, namespace_fqn) {
            return None;
        }

        let ancestor_chain = method_lookup_chain(self.engine, namespace_fqn);
        if let Some((_owner, facts)) = method_facts_in_chain(
            self.engine,
            &ancestor_chain,
            method,
            allow_private,
            protected_caller,
        ) {
            return RubyType::union_from_proven(facts, |fact| {
                self.method_return_type_inner(&fact, seen)
            });
        }

        let applications = execution_context_application_targets(self.engine, namespace_fqn);
        if !applications.is_empty() {
            return RubyType::union_from_proven(applications, |application| {
                self.method_return_type_for_receiver_inner(
                    &application,
                    method,
                    allow_private,
                    protected_caller,
                    seen,
                )
            });
        }

        if *method != method_missing_method() {
            return self.method_return_type_for_receiver_inner(
                namespace_fqn,
                &method_missing_method(),
                allow_private,
                protected_caller,
                seen,
            );
        }

        None
    }

    fn delegate_method_return_type(
        &self,
        fact: &MethodFact,
        seen: &mut HashSet<MethodVisitKey>,
    ) -> Option<RubyType> {
        let FullyQualifiedName::Method(_, delegated_method) = &fact.fqn else {
            return None;
        };
        let receiver_method = fact.delegate_receiver?;
        let receiver_type = self.method_return_type_for_receiver_inner(
            &fact.owner,
            &receiver_method,
            true,
            None,
            seen,
        )?;

        RubyType::union_from_proven(
            AnalysisQuery::receiver_type_to_method_namespaces(&receiver_type),
            |namespace| {
                self.method_return_type_for_receiver_inner(
                    &namespace,
                    delegated_method,
                    true,
                    None,
                    seen,
                )
            },
        )
    }

    fn latest_unambiguous_concrete_type(facts: impl Iterator<Item = TypeFact>) -> Option<RubyType> {
        let mut latest_start = None;
        let mut latest_type = None;
        let mut ambiguous = false;
        for fact in facts {
            match latest_start {
                None => {
                    latest_start = Some(fact.range.start_byte);
                    latest_type = Some(fact.ruby_type);
                }
                Some(start) if fact.range.start_byte > start => {
                    latest_start = Some(fact.range.start_byte);
                    latest_type = Some(fact.ruby_type);
                    ambiguous = false;
                }
                Some(start) if fact.range.start_byte == start => {
                    if latest_type.as_ref() != Some(&fact.ruby_type) {
                        ambiguous = true;
                    }
                }
                Some(_) => {}
            }
        }

        let latest_type = latest_type?;
        if latest_type == RubyType::Unknown || ambiguous {
            None
        } else {
            Some(latest_type)
        }
    }
}
