use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use crate::core::{
    FullyQualifiedName, GraphNodeKind, MethodFact, NamespaceKind, ResolvedMethodCallee,
    RubyConstant, RubyMethod, RubyType, SourceFileId, SourceKind, TextRange, TypeFact,
    TypeInferenceOutcome, TypeResolution, TypeSubject, UnknownReason,
};
use parking_lot::Mutex;

use crate::engine::state::TypeInferenceOutcomeRef;

const MAX_RESOLVED_METHOD_CACHE_ENTRIES_PER_SOURCE: usize = 256;
const MAX_METHOD_SIGNATURE_FACT_CACHE_ENTRIES_PER_SOURCE: usize = 256;
const MAX_THREAD_METHOD_RETURN_CACHE_ENTRIES: usize = 8192;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(in crate::engine) enum MethodReturnQueryAccess {
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
    method_signature_facts: HashMap<MethodReturnQueryKey, Arc<Vec<MethodFact>>>,
}

impl AnalysisQueryCacheState {
    fn bind_engine_identity(&mut self, engine_identity: (u64, u64)) {
        if self.engine_identity != Some(engine_identity) {
            self.engine_identity = Some(engine_identity);
            self.method_returns.clear();
            self.method_callees.clear();
            self.method_signature_facts.clear();
        }
    }
}

/// Thread-local memo of engine method-return lookups for one engine identity.
///
/// Parallel project collection keeps this cache on the worker thread so identical
/// `Integer#to_s` / `User#new` lookups reuse the same bounded result without
/// sharing `AnalysisQueryCache` across a file batch. Explicit calls use the
/// public cache when the receiver chain has no private/protected method of that
/// name, so caller namespace is not part of the hot key. Identity changes drop
/// the entries. A 256-entry cap left ~52k misses and ~4s of engine return
/// lookup on the JRuby project visitor; 8192 holds one worker's unique
/// receiver/method set. Hits are O(1); eviction is insertion-order FIFO. Do
/// not raise the cap further without an RSS measurement; do not key this
/// cache only on method name.
struct ThreadMethodReturnCache {
    identity: Option<(u64, u64)>,
    entries: HashMap<MethodReturnQueryKey, Option<RubyType>>,
    order: VecDeque<MethodReturnQueryKey>,
    non_public: HashMap<(FullyQualifiedName, RubyMethod), bool>,
    non_public_order: VecDeque<(FullyQualifiedName, RubyMethod)>,
}

impl Default for ThreadMethodReturnCache {
    fn default() -> Self {
        Self {
            identity: None,
            entries: HashMap::new(),
            order: VecDeque::new(),
            non_public: HashMap::new(),
            non_public_order: VecDeque::new(),
        }
    }
}

impl ThreadMethodReturnCache {
    fn bind(&mut self, identity: (u64, u64)) {
        if self.identity != Some(identity) {
            self.identity = Some(identity);
            self.entries.clear();
            self.order.clear();
            self.non_public.clear();
            self.non_public_order.clear();
        }
    }

    fn get(&mut self, key: &MethodReturnQueryKey) -> Option<Option<RubyType>> {
        self.entries.get(key).cloned()
    }

    fn insert(&mut self, key: MethodReturnQueryKey, value: Option<RubyType>) {
        if self.entries.contains_key(&key) {
            self.entries.insert(key, value);
            return;
        }
        while self.entries.len() >= MAX_THREAD_METHOD_RETURN_CACHE_ENTRIES {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            self.entries.remove(&oldest);
        }
        self.order.push_back(key.clone());
        self.entries.insert(key, value);
    }

    fn get_non_public(&mut self, key: &(FullyQualifiedName, RubyMethod)) -> Option<bool> {
        self.non_public.get(key).copied()
    }

    fn insert_non_public(&mut self, key: (FullyQualifiedName, RubyMethod), value: bool) {
        if self.non_public.contains_key(&key) {
            self.non_public.insert(key, value);
            return;
        }
        while self.non_public.len() >= MAX_THREAD_METHOD_RETURN_CACHE_ENTRIES {
            let Some(oldest) = self.non_public_order.pop_front() else {
                break;
            };
            self.non_public.remove(&oldest);
        }
        self.non_public_order.push_back(key.clone());
        self.non_public.insert(key, value);
    }
}

thread_local! {
    static THREAD_METHOD_RETURN_CACHE: RefCell<ThreadMethodReturnCache> =
        RefCell::new(ThreadMethodReturnCache::default());
}

fn thread_method_return_get(
    identity: (u64, u64),
    key: &MethodReturnQueryKey,
) -> Option<Option<RubyType>> {
    THREAD_METHOD_RETURN_CACHE.with(|cell| {
        let mut cache = cell.borrow_mut();
        cache.bind(identity);
        cache.get(key)
    })
}

fn thread_method_return_insert(
    identity: (u64, u64),
    key: MethodReturnQueryKey,
    value: Option<RubyType>,
) {
    THREAD_METHOD_RETURN_CACHE.with(|cell| {
        let mut cache = cell.borrow_mut();
        cache.bind(identity);
        cache.insert(key, value);
    });
}

fn thread_receiver_has_non_public(
    identity: (u64, u64),
    namespace: &FullyQualifiedName,
    method: RubyMethod,
    compute: impl FnOnce() -> bool,
) -> bool {
    let key = (namespace.clone(), method);
    if let Some(hit) = THREAD_METHOD_RETURN_CACHE.with(|cell| {
        let mut cache = cell.borrow_mut();
        cache.bind(identity);
        cache.get_non_public(&key)
    }) {
        return hit;
    }

    let value = compute();
    THREAD_METHOD_RETURN_CACHE.with(|cell| {
        let mut cache = cell.borrow_mut();
        cache.bind(identity);
        cache.insert_non_public(key, value);
    });
    value
}

/// Bounded-lifetime memoization for repeated semantic queries while collecting
/// one source product.
///
/// The cache binds itself to one exact engine instance and semantic revision.
/// Replacements and engine clones receive different identities, so callers
/// cannot accidentally reuse results across isolated project engines.
/// Engine method-return lookups additionally reuse a thread-local 8192-entry
/// FIFO keyed by that same identity so parallel file collection can share
/// identical receiver/method results without one `AnalysisQueryCache` mutex
/// spanning the batch. Explicit `foo.bar` lookups reuse the public-access
/// entry when the receiver chain has no private/protected method of that
/// name, so caller namespace is not part of the hot key. Hits are O(1). Do
/// not raise the cap further without an RSS measurement.
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
        if let Some(hit) = thread_method_return_get(engine_identity, &key) {
            return hit;
        }

        {
            let mut state = self.state.lock();
            state.bind_engine_identity(engine_identity);
            if let Some(cached) = state.method_returns.get(&key) {
                let result = cached.clone();
                drop(state);
                thread_method_return_insert(engine_identity, key, result.clone());
                return result;
            }
        }

        let result = compute();
        thread_method_return_insert(engine_identity, key.clone(), result.clone());
        let mut state = self.state.lock();
        if state.engine_identity == Some(engine_identity) {
            state.method_returns.insert(key, result.clone());
        }
        result
    }

    pub(in crate::engine) fn method_callees(
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
            state.bind_engine_identity(engine_identity);
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

    pub(in crate::engine) fn method_signature_facts(
        &self,
        engine_identity: (u64, u64),
        namespace: &FullyQualifiedName,
        method: RubyMethod,
        access: MethodReturnQueryAccess,
        compute: impl FnOnce() -> Vec<MethodFact>,
    ) -> Arc<Vec<MethodFact>> {
        let key = MethodReturnQueryKey {
            namespace: namespace.clone(),
            method,
            access,
        };
        {
            let mut state = self.state.lock();
            state.bind_engine_identity(engine_identity);
            if let Some(cached) = state.method_signature_facts.get(&key) {
                return cached.clone();
            }
        }

        let result = Arc::new(compute());
        let mut state = self.state.lock();
        if state.engine_identity == Some(engine_identity)
            && state.method_signature_facts.len()
                < MAX_METHOD_SIGNATURE_FACT_CACHE_ENTRIES_PER_SOURCE
        {
            state.method_signature_facts.insert(key, result.clone());
        }
        result
    }

    #[cfg(test)]
    pub(crate) fn valid_entry_counts_for_test(&self) -> (usize, usize, usize) {
        let state = self.state.lock();
        (
            state.method_returns.len(),
            state.method_callees.len(),
            state.method_signature_facts.len(),
        )
    }
}
use crate::engine::queries::lookup::types::{ConstantHover, ConstantHoverKind, VariableTypeKind};
use crate::engine::queries::AnalysisQuery;
use crate::engine::resolution::{
    chain_has_custom_method_missing, execution_context_application_targets, method_facts_in_chain,
    method_lookup_chain, method_missing_method, module_instance_receivers, namespace_target_exists,
};

type MethodVisitKey = (FullyQualifiedName, SourceFileId, u32, u32);

impl<'a> AnalysisQuery<'a> {
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
            .reference_candidate_store()
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
        self.engine.latest_graph_node_kind(namespace_fqn)
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

    pub(crate) fn constant_dependency_type(
        &self,
        dependency: &crate::core::ConstantTypeDependency,
    ) -> Option<RubyType> {
        crate::engine::state::resolve_constant_dependency_type(self, dependency)
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
        let identity = self.engine.query_cache_identity();
        if !thread_receiver_has_non_public(identity, namespace_fqn, *method, || {
            self.receiver_method_has_non_public(namespace_fqn, method)
        }) {
            return self.method_return_type_for_public_receiver_cached(
                namespace_fqn,
                method,
                cache,
            );
        }
        let key = MethodReturnQueryKey {
            namespace: namespace_fqn.clone(),
            method: *method,
            access: MethodReturnQueryAccess::Protected(caller_namespace_fqn.clone()),
        };
        cache.method_return(identity, key, || {
            self.method_return_type_for_protected_receiver(
                namespace_fqn,
                method,
                caller_namespace_fqn,
            )
        })
    }

    fn receiver_method_has_non_public(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> bool {
        let ancestor_chain = method_lookup_chain(self.engine, namespace_fqn);
        let private_facts = method_facts_in_chain(self.engine, &ancestor_chain, method, true, None);
        let public_facts = method_facts_in_chain(self.engine, &ancestor_chain, method, false, None);
        match (private_facts, public_facts) {
            (None, None) => false,
            (Some((private_owner, private_facts)), Some((public_owner, public_facts))) => {
                private_owner != public_owner
                    || private_facts
                        .iter()
                        .map(|fact| fact.range)
                        .ne(public_facts.iter().map(|fact| fact.range))
            }
            (Some(_), None) | (None, Some(_)) => true,
        }
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

        let receivers = module_instance_receivers(self.engine, namespace_fqn);
        if !receivers.is_empty() {
            // Sibling receivers may reach the same inherited declaration.
            // Only revisiting a fact within one branch is a recursion cycle.
            return RubyType::union_from_proven(receivers, |receiver| {
                self.method_return_type_for_receiver_inner(
                    &receiver,
                    method,
                    allow_private,
                    protected_caller,
                    &mut seen.clone(),
                )
            });
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

        // Default BasicObject#method_missing is language fallback, not a proven
        // return. Navigation already treats that stub as Missing; walking it
        // here redoes MRO on every unresolved call.
        if *method != method_missing_method()
            && chain_has_custom_method_missing(self.engine, &ancestor_chain)
        {
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

#[cfg(test)]
mod exact_receiver_tests {
    use super::*;
    use crate::core::{
        InferenceEvidence, MethodReferenceCandidate, MethodReferenceDiagnostics, ReferenceCandidate,
    };
    use crate::engine::{AnalysisEngine, FileFacts, ResolveMode, SourceFileInput};

    fn fixture() -> (
        AnalysisEngine,
        SourceFileId,
        TextRange,
        TextRange,
        FileFacts,
    ) {
        let mut engine = AnalysisEngine::new();
        let file_id = engine.register_file(SourceFileInput {
            path: "receiver.rb".into(),
            content: "x.upcase".to_string(),
            kind: SourceKind::Project,
        });
        let receiver = TextRange::new(file_id, 0, 1);
        let message = TextRange::new(file_id, 2, 8);
        let facts = FileFacts {
            reference_candidates: vec![ReferenceCandidate::method(
                message,
                MethodReferenceCandidate {
                    owner: vec![RubyConstant::new("NilClass").expect("valid language class")],
                    owner_kind: NamespaceKind::Instance,
                    method: RubyMethod::new("upcase").expect("valid Ruby method"),
                    is_super: false,
                    access: crate::core::MethodReferenceAccess::ExplicitReceiver,
                    caller: None,
                    call_expression_range: None,
                    preferred_definition_range: None,
                    diagnostics: MethodReferenceDiagnostics {
                        diagnostic_range: message,
                        receiver_label: Some("NilClass".to_string()),
                        receiver_expression_range: Some(receiver),
                        receiver_type: Some(Box::new(RubyType::nil_class())),
                        diagnose_unresolved: true,
                        allow_unindexed_owner: false,
                        signature: None,
                    },
                },
            )],
            ..FileFacts::default()
        };
        (engine, file_id, receiver, message, facts)
    }

    #[test]
    fn exact_call_receiver_type_keeps_unknown_and_union_proof_barriers() {
        let (mut engine, file_id, receiver, message, facts) = fixture();
        engine.replace_facts(file_id, facts.clone(), ResolveMode::Deferred);
        assert_eq!(
            engine.query().exact_call_receiver_type(message, receiver),
            Some(RubyType::nil_class())
        );

        let unknown = FileFacts {
            inference: InferenceEvidence {
                expression_unknown_reasons: vec![(
                    receiver,
                    UnknownReason::UnresolvedAssignmentValue,
                )],
                ..InferenceEvidence::default()
            },
            ..facts.clone()
        };
        engine.replace_facts(file_id, unknown, ResolveMode::Deferred);
        assert_eq!(
            engine.query().exact_call_receiver_type(message, receiver),
            Some(RubyType::Unknown)
        );

        let union = RubyType::union(vec![RubyType::nil_class(), RubyType::string()]);
        engine.replace_facts(
            file_id,
            FileFacts {
                local_read_types: vec![(receiver, union.clone())].into_boxed_slice(),
                ..facts
            },
            ResolveMode::Deferred,
        );
        assert_eq!(
            engine.query().exact_call_receiver_type(message, receiver),
            Some(union)
        );
    }

    #[test]
    fn exact_call_receiver_type_rejects_other_message_and_receiver_ranges() {
        let (mut engine, file_id, receiver, message, facts) = fixture();
        engine.replace_facts(file_id, facts, ResolveMode::Deferred);
        assert_eq!(
            engine
                .query()
                .exact_call_receiver_type(TextRange::new(file_id, 2, 7), receiver),
            None
        );
        assert_eq!(
            engine
                .query()
                .exact_call_receiver_type(TextRange::new(file_id, 3, 8), receiver),
            None
        );
        assert_eq!(
            engine
                .query()
                .exact_call_receiver_type(message, TextRange::new(file_id, 0, 2)),
            None
        );
    }
}
