use std::collections::{HashMap, HashSet};
use std::time::Instant;

use log::debug;

use crate::core::method_store::MethodVisibility;
use crate::core::{
    ConstLookup, ConstLookupId, ConstantPath, DiagnosticCandidate, DiagnosticCandidateKind,
    DiagnosticFact, FqnId, FullyQualifiedName, GraphEdgeKind, GraphNodeKind, MethodAvailability,
    MethodCallSignatureCandidate, MethodCalleeResolution, MethodFact, MethodReferenceAccess,
    NamespaceKind, RaiseArgCandidate, ReferenceFact, ResolvedMethodCallee, RubyConstant,
    RubyMethod, RubyType, SourceFileId, StoredReferenceCandidateKind, StoredReferenceCandidateRef,
    TextRange, TypeInferenceOutcome, UnknownReason,
};
use crate::engine::diagnostic_helpers::{
    arity_mismatch, closest_keyword, levenshtein, suggestion_threshold, MethodArity,
    EXCEPTION_WHITELIST, NON_EXCEPTION_TYPES,
};
use crate::engine::resolution::{
    effective_method_visibility_for_chain, method_lookup_chain, protected_method_visible_from,
    MethodLookupChainCache, MethodLookupResult,
};
use crate::engine::state::{elapsed_ns, ResolvePassStats, TypeInferenceOutcomeRef};
use crate::{AnalysisEngine, AnalysisQuery};

type MethodReferenceCacheKey = (ConstLookupId, NamespaceKind, RubyMethod, bool);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CachedMethodVisibility {
    Public,
    Protected(FqnId),
    Private,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum AmbiguousMethodReturnAccess {
    Private,
    Public,
    Protected(FqnId),
}

#[derive(Default)]
struct MethodCallOutcomeCaches {
    returns: HashMap<(FqnId, TextRange), Option<RubyType>>,
    visibilities: HashMap<MethodReferenceCacheKey, CachedMethodVisibility>,
    ambiguous_returns:
        HashMap<(MethodReferenceCacheKey, AmbiguousMethodReturnAccess), Option<RubyType>>,
    return_hits: usize,
    return_misses: usize,
    visibility_hits: usize,
    visibility_misses: usize,
    ambiguous_return_hits: usize,
    ambiguous_return_misses: usize,
}

#[derive(Default)]
struct MethodChainCompletenessCache {
    results: HashMap<FullyQualifiedName, bool>,
    dynamic_mixin_hooks: HashMap<FullyQualifiedName, bool>,
    ambiguous_superclasses: HashMap<FullyQualifiedName, bool>,
}

impl AnalysisEngine {
    pub(super) fn resolve_reference_candidates(&mut self, stats: &mut ResolvePassStats) {
        let mut candidate_file_ids = self.facts.references.candidates.file_ids();
        for file_id in self.facts.diagnostics.candidates.file_ids() {
            if !candidate_file_ids.contains(&file_id) {
                candidate_file_ids.push(file_id);
            }
        }

        let reference_candidate_store = std::mem::take(&mut self.facts.references.candidates);
        let diagnostic_seed_started = Instant::now();
        let mut unresolved_constants = self.resolve_diagnostic_candidates();
        stats.diagnostic_seed_ns = elapsed_ns(diagnostic_seed_started);
        let mut method_fact_cache: HashMap<MethodReferenceCacheKey, MethodLookupResult> =
            HashMap::new();
        let mut method_namespace_exists_cache: HashMap<FullyQualifiedName, bool> = HashMap::new();
        let mut method_suggestion_cache: HashMap<(FullyQualifiedName, RubyMethod), Option<String>> =
            HashMap::new();
        let mut constant_target_cache: HashMap<ConstLookupId, Option<FqnId>> = HashMap::new();
        let mut method_lookup_chain_cache = MethodLookupChainCache::new();
        let unresolved_method_edge_sources = self.unresolved_method_edge_sources();
        let mut method_chain_completeness_cache = MethodChainCompletenessCache::default();
        let mut resolved_call_outcomes = HashMap::new();
        let mut call_outcome_caches = MethodCallOutcomeCaches::default();
        self.facts.references.resolved.clear();
        let candidate_loop_started = Instant::now();
        for candidate in reference_candidate_store.iter_candidates() {
            match candidate {
                StoredReferenceCandidateRef::Resolved(candidate) => {
                    self.facts.references.resolved.add(
                        candidate.target,
                        ReferenceFact::new(candidate.range, candidate.caller),
                    );
                }
                StoredReferenceCandidateRef::Constant(candidate) => {
                    let lookup = self.names.const_lookup(candidate.lookup).expect(
                        "INVARIANT VIOLATED: reference candidate points to missing constant lookup. \
                         This is a bug because stored reference candidates must only contain interned lookup ids. \
                         Fix: intern constant lookups before inserting candidates.",
                    );
                    let parts = lookup.path.to_vec();
                    let context = self.names.fqn(lookup.context).expect(
                        "INVARIANT VIOLATED: constant lookup points to missing context FQN id. \
                         This is a bug because constant lookups must only store interned context FQN ids. \
                         Fix: intern lookup contexts before inserting candidates.",
                    );
                    let target = if let Some(target) = constant_target_cache.get(&candidate.lookup)
                    {
                        stats.constant_cache_hits = stats.constant_cache_hits.checked_add(1).expect(
                            "INVARIANT VIOLATED: constant resolve-cache hit counter overflowed usize. \
                             This is a bug because one resolve pass cannot exceed addressable memory operations. \
                             Fix: inspect corrupt resolve instrumentation.",
                        );
                        *target
                    } else {
                        stats.constant_cache_misses =
                            stats.constant_cache_misses.checked_add(1).expect(
                                "INVARIANT VIOLATED: constant resolve-cache miss counter overflowed usize. \
                                 This is a bug because one resolve pass cannot exceed addressable memory operations. \
                                 Fix: inspect corrupt resolve instrumentation.",
                            );
                        let target = self
                            .resolve_constant_reference(
                                &parts,
                                &if lookup.absolute {
                                    Vec::new()
                                } else {
                                    context.namespace_parts()
                                },
                            )
                            .map(|target| self.names.intern_fqn(target));
                        constant_target_cache.insert(candidate.lookup, target);
                        target
                    };
                    if let Some(target) = target {
                        self.facts
                            .references
                            .resolved
                            .add(target, ReferenceFact::new(candidate.range, None));
                    } else {
                        unresolved_constants
                            .entry(candidate.range.file_id)
                            .or_default()
                            .push(DiagnosticFact::new(
                                candidate.range,
                                crate::core::DiagnosticSeverity::Error,
                                "unresolved-constant",
                                format!("Unresolved constant `{}`", constant_name(&parts)),
                            ));
                    }
                }
                StoredReferenceCandidateRef::Method(candidate) => {
                    let deferred_receiver_range = candidate
                        .diagnostics
                        .as_deref()
                        .and_then(|diagnostics| diagnostics.receiver_expression_range);
                    let solved_receiver_type = deferred_receiver_range.and_then(|range| {
                        self.proven_deferred_receiver_type(range, &resolved_call_outcomes)
                    });
                    let receiver_is_explicitly_unknown =
                        deferred_receiver_range.is_some_and(|range| {
                            self.deferred_receiver_is_unknown(range, &resolved_call_outcomes)
                        });
                    let candidate_receiver_type = candidate
                        .diagnostics
                        .as_deref()
                        .and_then(|diagnostics| diagnostics.receiver_type.as_deref())
                        .cloned();
                    let effective_receiver_type = solved_receiver_type.or_else(|| {
                        (!receiver_is_explicitly_unknown)
                            .then_some(candidate_receiver_type)
                            .flatten()
                    });
                    if deferred_receiver_range.is_some() {
                        stats.deferred_receiver_candidates = stats
                            .deferred_receiver_candidates
                            .checked_add(1)
                            .expect(
                                "INVARIANT VIOLATED: deferred-receiver candidate counter overflowed usize. This is a bug because one resolve pass cannot contain more candidates than addressable memory. Fix: inspect corrupt reference-candidate storage.",
                            );
                        if effective_receiver_type.is_some() {
                            stats.deferred_receiver_proven = stats
                                .deferred_receiver_proven
                                .checked_add(1)
                                .expect(
                                    "INVARIANT VIOLATED: proven deferred-receiver counter overflowed usize. This is a bug because proven receivers are a subset of addressable candidates. Fix: inspect corrupt resolve instrumentation.",
                                );
                        } else {
                            stats.deferred_receiver_unknown = stats
                                .deferred_receiver_unknown
                                .checked_add(1)
                                .expect(
                                    "INVARIANT VIOLATED: Unknown deferred-receiver counter overflowed usize. This is a bug because Unknown receivers are a subset of addressable candidates. Fix: inspect corrupt resolve instrumentation.",
                                );
                        }
                    }
                    if deferred_receiver_range.is_some() && effective_receiver_type.is_none() {
                        if let Some(expression_range) = candidate.call_expression_range {
                            Self::insert_resolved_call_outcome(
                                &mut resolved_call_outcomes,
                                expression_range,
                                TypeInferenceOutcome::unknown(UnknownReason::UnknownReceiver),
                            );
                        }
                        continue;
                    }
                    let grouped_receiver_type = effective_receiver_type
                        .as_ref()
                        .filter(|ruby_type| matches!(ruby_type, RubyType::Union(_)))
                        .cloned();
                    if let Some(receiver_type) = grouped_receiver_type.as_ref() {
                        if let Some(callees) = self.resolve_grouped_method_callees(
                            receiver_type,
                            candidate.method,
                            candidate.access,
                            candidate.caller,
                        ) {
                            let targets = grouped_method_targets(&callees, candidate.method);
                            for target in targets {
                                let target = self.names.intern_fqn(target);
                                self.facts.references.resolved.add(
                                    target,
                                    ReferenceFact::method(
                                        candidate.range,
                                        candidate.caller,
                                        candidate.access,
                                    ),
                                );
                            }
                            if let Some(diagnostics) = candidate.diagnostics.as_deref() {
                                self.push_grouped_method_fact_diagnostics(
                                    &callees,
                                    candidate.method,
                                    diagnostics,
                                    &mut unresolved_constants,
                                );
                            }
                            if let Some(expression_range) = candidate.call_expression_range {
                                Self::insert_resolved_call_outcome(
                                    &mut resolved_call_outcomes,
                                    expression_range,
                                    self.call_expression_outcome_from_grouped_resolution(
                                        &callees,
                                        candidate.method,
                                        &mut call_outcome_caches,
                                    ),
                                );
                            }
                        } else if let Some(diagnostics) = candidate.diagnostics.as_deref() {
                            self.push_grouped_unresolved_method_diagnostic(
                                receiver_type,
                                candidate.method,
                                diagnostics,
                                &unresolved_method_edge_sources,
                                &mut method_chain_completeness_cache,
                                &mut unresolved_constants,
                            );
                            if let Some(expression_range) = candidate.call_expression_range {
                                Self::insert_resolved_call_outcome(
                                    &mut resolved_call_outcomes,
                                    expression_range,
                                    TypeInferenceOutcome::unknown(
                                        UnknownReason::UnresolvedMethodReturn,
                                    ),
                                );
                            }
                        }
                        continue;
                    }
                    let (owner, owner_kind) = if let Some(receiver_type) =
                        effective_receiver_type.as_ref()
                    {
                        let allow_unindexed_owner = candidate
                            .diagnostics
                            .as_deref()
                            .is_some_and(|diagnostics| diagnostics.allow_unindexed_owner);
                        let Some(owner_fqn) =
                            self.proven_receiver_namespace(receiver_type, allow_unindexed_owner)
                        else {
                            if let Some(expression_range) = candidate.call_expression_range {
                                Self::insert_resolved_call_outcome(
                                    &mut resolved_call_outcomes,
                                    expression_range,
                                    TypeInferenceOutcome::unknown(UnknownReason::UnknownReceiver),
                                );
                            }
                            continue;
                        };
                        let owner_kind = owner_fqn.namespace_kind().expect(
                            "INVARIANT VIOLATED: a proven receiver namespace has no namespace kind. This is a bug because type-to-namespace conversion must return a Namespace FQN. Fix: keep receiver proof conversion in AnalysisQuery::type_to_namespace.",
                        );
                        let root = self
                            .names
                            .intern_fqn(FullyQualifiedName::namespace(Vec::new()));
                        let owner = self.names.intern_const_lookup(ConstLookup::new(
                            ConstantPath::from_vec(owner_fqn.namespace_parts()),
                            true,
                            root,
                        ));
                        (owner, owner_kind)
                    } else {
                        (candidate.owner, candidate.owner_kind)
                    };
                    let owner_fqn = method_reference_owner_fqn(self, owner, owner_kind);
                    let method_cache_key =
                        (owner, owner_kind, candidate.method, candidate.is_super);
                    let cached = method_fact_cache.contains_key(&method_cache_key);
                    let fact = method_fact_cache
                        .entry(method_cache_key)
                        .or_insert_with(|| {
                            let query = AnalysisQuery::new(self);
                            if candidate.is_super {
                                query.resolve_super_method_reference(&owner_fqn, &candidate.method)
                            } else {
                                query.resolve_method_reference_with_chain_cache(
                                    &owner_fqn,
                                    &candidate.method,
                                    &mut method_lookup_chain_cache,
                                )
                            }
                        });
                    if cached {
                        stats.method_cache_hits = stats.method_cache_hits.checked_add(1).expect(
                            "INVARIANT VIOLATED: method resolve-cache hit counter overflowed usize. \
                             This is a bug because one resolve pass cannot exceed addressable memory operations. \
                             Fix: inspect corrupt resolve instrumentation.",
                        );
                    } else {
                        stats.method_cache_misses = stats.method_cache_misses.checked_add(1).expect(
                            "INVARIANT VIOLATED: method resolve-cache miss counter overflowed usize. \
                             This is a bug because one resolve pass cannot exceed addressable memory operations. \
                             Fix: inspect corrupt resolve instrumentation.",
                        );
                    }
                    let mut fact = fact.clone();
                    if candidate.access == MethodReferenceAccess::Normal
                        && matches!(fact, MethodLookupResult::Ambiguous { .. })
                    {
                        if let Some(source_ordered) = AnalysisQuery::new(self)
                            .source_ordered_top_level_method_reference(
                                &owner_fqn,
                                &candidate.method,
                                candidate.range,
                            )
                        {
                            fact = MethodLookupResult::Unique(source_ordered);
                        }
                    }
                    if let Some(expression_range) = candidate.call_expression_range {
                        let outcome = self.call_expression_outcome_from_method_resolution(
                            method_cache_key,
                            candidate.access,
                            candidate.caller,
                            &method_lookup_chain_cache,
                            &fact,
                            &mut call_outcome_caches,
                        );
                        Self::insert_resolved_call_outcome(
                            &mut resolved_call_outcomes,
                            expression_range,
                            outcome,
                        );
                    }
                    if let Some((owner, resolved_method, fact)) = fact.reference_parts() {
                        let target =
                            FullyQualifiedName::method(owner.namespace_parts(), resolved_method);
                        let target = self.names.intern_fqn(target);
                        self.facts.references.resolved.add(
                            target,
                            ReferenceFact::method(
                                candidate.range,
                                candidate.caller,
                                candidate.access,
                            ),
                        );
                        if resolved_method == candidate.method {
                            if let Some(diagnostics) = candidate.diagnostics.as_deref() {
                                if let Some(fact) = fact {
                                    self.push_unavailable_method_diagnostic(
                                        fact,
                                        &candidate.method,
                                        diagnostics.diagnostic_range,
                                        &mut unresolved_constants,
                                    );
                                    self.push_signature_diagnostics(
                                        fact,
                                        &owner_fqn,
                                        &candidate.method,
                                        diagnostics.signature.as_ref(),
                                        diagnostics.receiver_label.as_deref(),
                                        diagnostics.diagnostic_range,
                                        &mut unresolved_constants,
                                    );
                                }
                            }
                        }
                    } else if fact.is_missing() {
                        let namespace_exists = *method_namespace_exists_cache
                            .entry(owner_fqn.clone())
                            .or_insert_with_key(|owner_fqn| {
                                self.method_namespace_target_exists(owner_fqn)
                            });
                        let allow_unindexed_owner = candidate
                            .diagnostics
                            .as_deref()
                            .is_some_and(|diagnostics| diagnostics.allow_unindexed_owner);
                        if !namespace_exists && !allow_unindexed_owner {
                            continue;
                        }
                        let target = FullyQualifiedName::method(
                            owner_fqn.namespace_parts(),
                            candidate.method,
                        );
                        let target = self.names.intern_fqn(target);
                        self.facts.references.resolved.add(
                            target,
                            ReferenceFact::method(
                                candidate.range,
                                candidate.caller,
                                candidate.access,
                            ),
                        );

                        if let Some(diagnostics) = candidate.diagnostics.as_deref() {
                            if !diagnostics.diagnose_unresolved {
                                continue;
                            }
                            let explicit_absence = self
                                .method_absence_has_explicit_contract(&owner_fqn, candidate.method);
                            if !explicit_absence
                                && self.method_lookup_chain_is_incomplete_cached(
                                    &owner_fqn,
                                    &unresolved_method_edge_sources,
                                    &mut method_chain_completeness_cache,
                                )
                            {
                                continue;
                            }
                            let suggestion = namespace_exists
                                .then(|| {
                                    method_suggestion_cache
                                        .entry((owner_fqn.clone(), candidate.method))
                                        .or_insert_with(|| {
                                            self.find_method_suggestion(
                                                &owner_fqn,
                                                candidate.method.as_str(),
                                            )
                                        })
                                        .clone()
                                })
                                .flatten();
                            let mut message = match &diagnostics.receiver_label {
                                Some(label) => format!(
                                    "Unresolved method `{}` on `{}`",
                                    candidate.method.as_str(),
                                    label
                                ),
                                None => {
                                    format!("Unresolved method `{}`", candidate.method.as_str())
                                }
                            };
                            if let Some(suggestion) = suggestion {
                                message.push_str(&format!(". Did you mean `{}`?", suggestion));
                            }
                            unresolved_constants
                                .entry(diagnostics.diagnostic_range.file_id)
                                .or_default()
                                .push(DiagnosticFact::new(
                                    diagnostics.diagnostic_range,
                                    crate::core::DiagnosticSeverity::Warning,
                                    "unresolved-method",
                                    message,
                                ));
                        }
                    }
                }
            }
        }
        // method_candidates_ns holds the full candidate-loop duration after the
        // one-shot per-arm Instant split was removed (it inflated production A/B).
        // The detailed constant-vs-method split remains in
        // support/performance/resolve-pass-cache-cardinality-2026-08-01.json.
        stats.method_candidates_ns = elapsed_ns(candidate_loop_started);
        stats.constant_cache_unique_keys = constant_target_cache.len();
        stats.method_cache_unique_keys = method_fact_cache.len();
        stats.method_lookup_chain_cache_entries = method_lookup_chain_cache.len();
        stats.method_namespace_exists_cache_entries = method_namespace_exists_cache.len();
        stats.method_suggestion_cache_entries = method_suggestion_cache.len();
        stats.incomplete_method_chain_cache_entries = method_chain_completeness_cache.results.len();
        stats.method_return_cache_hits = call_outcome_caches.return_hits;
        stats.method_return_cache_misses = call_outcome_caches.return_misses;
        stats.method_return_cache_entries = call_outcome_caches.returns.len();
        stats.method_visibility_cache_hits = call_outcome_caches.visibility_hits;
        stats.method_visibility_cache_misses = call_outcome_caches.visibility_misses;
        stats.method_visibility_cache_entries = call_outcome_caches.visibilities.len();
        stats.ambiguous_method_return_cache_hits = call_outcome_caches.ambiguous_return_hits;
        stats.ambiguous_method_return_cache_misses = call_outcome_caches.ambiguous_return_misses;
        stats.ambiguous_method_return_cache_entries = call_outcome_caches.ambiguous_returns.len();

        // These caches are complete once the candidate loop ends. Release
        // them before merging call outcomes into retained inference evidence;
        // keeping both phases alive caused a large, unnecessary resolve peak.
        drop(method_fact_cache);
        drop(method_namespace_exists_cache);
        drop(method_suggestion_cache);
        drop(constant_target_cache);
        drop(method_lookup_chain_cache);
        drop(unresolved_method_edge_sources);
        drop(method_chain_completeness_cache);
        drop(call_outcome_caches);

        self.facts.references.candidates = reference_candidate_store;
        self.replace_resolved_call_expression_outcomes(resolved_call_outcomes);
        let sort_started = Instant::now();
        self.facts.references.resolved.sort_all();
        stats.sort_all_ns = elapsed_ns(sort_started);

        let diagnostic_rebuild_started = Instant::now();
        for file_id in candidate_file_ids {
            let mut diagnostics = self
                .facts
                .diagnostics
                .resolved
                .facts_in_file(file_id)
                .into_iter()
                .filter(|fact| fact.code != "unresolved-constant")
                .filter(|fact| fact.code != "unresolved-method")
                .filter(|fact| fact.code != "unsupported-runtime-api")
                .filter(|fact| fact.code != "wrong-arity")
                .filter(|fact| fact.code != "unknown-kwarg")
                .filter(|fact| fact.code != "missing-kwarg")
                .filter(|fact| fact.code != "raise-non-exception")
                .filter(|fact| fact.code != "bad-splat")
                .collect::<Vec<_>>();
            diagnostics.extend(unresolved_constants.remove(&file_id).unwrap_or_default());
            self.facts
                .diagnostics
                .resolved
                .replace_file(file_id, diagnostics);
        }
        stats.diagnostic_rebuild_ns = elapsed_ns(diagnostic_rebuild_started);
    }

    pub(super) fn resolve_reference_candidates_in_file(&mut self, file_id: SourceFileId) {
        let reference_candidates = self.facts.references.candidates.candidates_in_file(file_id);
        let mut unresolved =
            HashMap::from([(file_id, self.resolve_diagnostic_candidates_in_file(file_id))]);
        let mut method_fact_cache: HashMap<
            (FullyQualifiedName, RubyMethod, bool),
            MethodLookupResult,
        > = HashMap::new();
        let mut method_namespace_exists_cache: HashMap<FullyQualifiedName, bool> = HashMap::new();
        let mut method_suggestion_cache: HashMap<(FullyQualifiedName, RubyMethod), Option<String>> =
            HashMap::new();
        let mut method_lookup_chain_cache = MethodLookupChainCache::new();
        let unresolved_method_edge_sources = self.unresolved_method_edge_sources();
        let mut method_chain_completeness_cache = MethodChainCompletenessCache::default();
        let mut resolved_refs = Vec::new();
        let mut resolved_call_outcomes = HashMap::new();
        let mut call_outcome_caches = MethodCallOutcomeCaches::default();

        for candidate in reference_candidates {
            match candidate.kind {
                StoredReferenceCandidateKind::Resolved { target, caller } => {
                    resolved_refs.push((target, ReferenceFact::new(candidate.range, caller)));
                }
                StoredReferenceCandidateKind::Constant { lookup } => {
                    let lookup = self.names.const_lookup(lookup).expect(
                        "INVARIANT VIOLATED: reference candidate points to missing constant lookup. \
                         This is a bug because stored reference candidates must only contain interned lookup ids. \
                         Fix: intern constant lookups before inserting candidates.",
                    );
                    let parts = lookup.path.to_vec();
                    let context = self.names.fqn(lookup.context).expect(
                        "INVARIANT VIOLATED: constant lookup points to missing context FQN id. \
                         This is a bug because constant lookups must only store interned context FQN ids. \
                         Fix: intern lookup contexts before inserting candidates.",
                    );
                    if let Some(target) = self.resolve_constant_reference(
                        &parts,
                        &if lookup.absolute {
                            Vec::new()
                        } else {
                            context.namespace_parts()
                        },
                    ) {
                        let target = self.names.intern_fqn(target);
                        resolved_refs.push((target, ReferenceFact::new(candidate.range, None)));
                    } else {
                        unresolved
                            .entry(file_id)
                            .or_default()
                            .push(DiagnosticFact::new(
                                candidate.range,
                                crate::core::DiagnosticSeverity::Error,
                                "unresolved-constant",
                                format!("Unresolved constant `{}`", constant_name(&parts)),
                            ));
                    }
                }
                StoredReferenceCandidateKind::Method {
                    owner,
                    owner_kind,
                    method,
                    is_super,
                    access,
                    caller,
                    call_expression_range,
                    preferred_definition_range: _,
                    diagnostics,
                } => {
                    let deferred_receiver_range = diagnostics
                        .as_deref()
                        .and_then(|diagnostics| diagnostics.receiver_expression_range);
                    let solved_receiver_type = deferred_receiver_range.and_then(|range| {
                        self.proven_deferred_receiver_type(range, &resolved_call_outcomes)
                    });
                    let receiver_is_explicitly_unknown =
                        deferred_receiver_range.is_some_and(|range| {
                            self.deferred_receiver_is_unknown(range, &resolved_call_outcomes)
                        });
                    let candidate_receiver_type = diagnostics
                        .as_deref()
                        .and_then(|diagnostics| diagnostics.receiver_type.as_deref())
                        .cloned();
                    let effective_receiver_type = solved_receiver_type.or_else(|| {
                        (!receiver_is_explicitly_unknown)
                            .then_some(candidate_receiver_type)
                            .flatten()
                    });
                    if deferred_receiver_range.is_some() && effective_receiver_type.is_none() {
                        if let Some(expression_range) = call_expression_range {
                            Self::insert_resolved_call_outcome(
                                &mut resolved_call_outcomes,
                                expression_range,
                                TypeInferenceOutcome::unknown(UnknownReason::UnknownReceiver),
                            );
                        }
                        continue;
                    }
                    let grouped_receiver_type = effective_receiver_type
                        .as_ref()
                        .filter(|ruby_type| matches!(ruby_type, RubyType::Union(_)))
                        .cloned();
                    if let Some(receiver_type) = grouped_receiver_type.as_ref() {
                        if let Some(callees) = self.resolve_grouped_method_callees(
                            receiver_type,
                            method,
                            access,
                            caller,
                        ) {
                            for target in grouped_method_targets(&callees, method) {
                                let target = self.names.intern_fqn(target);
                                resolved_refs.push((
                                    target,
                                    ReferenceFact::method(candidate.range, caller, access),
                                ));
                            }
                            if let Some(diagnostics) = diagnostics.as_deref() {
                                self.push_grouped_method_fact_diagnostics(
                                    &callees,
                                    method,
                                    diagnostics,
                                    &mut unresolved,
                                );
                            }
                            if let Some(expression_range) = call_expression_range {
                                Self::insert_resolved_call_outcome(
                                    &mut resolved_call_outcomes,
                                    expression_range,
                                    self.call_expression_outcome_from_grouped_resolution(
                                        &callees,
                                        method,
                                        &mut call_outcome_caches,
                                    ),
                                );
                            }
                        } else if let Some(diagnostics) = diagnostics.as_deref() {
                            self.push_grouped_unresolved_method_diagnostic(
                                receiver_type,
                                method,
                                diagnostics,
                                &unresolved_method_edge_sources,
                                &mut method_chain_completeness_cache,
                                &mut unresolved,
                            );
                            if let Some(expression_range) = call_expression_range {
                                Self::insert_resolved_call_outcome(
                                    &mut resolved_call_outcomes,
                                    expression_range,
                                    TypeInferenceOutcome::unknown(
                                        UnknownReason::UnresolvedMethodReturn,
                                    ),
                                );
                            }
                        }
                        continue;
                    }
                    let (owner_lookup_id, owner_kind) = if let Some(receiver_type) =
                        effective_receiver_type.as_ref()
                    {
                        let allow_unindexed_owner = diagnostics
                            .as_deref()
                            .is_some_and(|diagnostics| diagnostics.allow_unindexed_owner);
                        let Some(owner_fqn) =
                            self.proven_receiver_namespace(receiver_type, allow_unindexed_owner)
                        else {
                            if let Some(expression_range) = call_expression_range {
                                Self::insert_resolved_call_outcome(
                                    &mut resolved_call_outcomes,
                                    expression_range,
                                    TypeInferenceOutcome::unknown(UnknownReason::UnknownReceiver),
                                );
                            }
                            continue;
                        };
                        let owner_kind = owner_fqn.namespace_kind().expect(
                            "INVARIANT VIOLATED: a proven receiver namespace has no namespace kind. This is a bug because type-to-namespace conversion must return a Namespace FQN. Fix: keep receiver proof conversion in AnalysisQuery::type_to_namespace.",
                        );
                        let root = self
                            .names
                            .intern_fqn(FullyQualifiedName::namespace(Vec::new()));
                        let owner_lookup_id = self.names.intern_const_lookup(ConstLookup::new(
                            ConstantPath::from_vec(owner_fqn.namespace_parts()),
                            true,
                            root,
                        ));
                        (owner_lookup_id, owner_kind)
                    } else {
                        (owner, owner_kind)
                    };
                    let owner_lookup = self.names.const_lookup(owner_lookup_id).expect(
                        "INVARIANT VIOLATED: method reference candidate points to missing owner lookup. \
                         This is a bug because stored reference candidates must only contain interned lookup ids. \
                         Fix: intern constant lookups before inserting candidates.",
                    );
                    let owner = owner_lookup.path.to_vec();
                    let owner_fqn = FullyQualifiedName::namespace_with_kind(owner, owner_kind);
                    let mut fact = method_fact_cache
                        .entry((owner_fqn.clone(), method, is_super))
                        .or_insert_with(|| {
                            let query = AnalysisQuery::new(self);
                            if is_super {
                                query.resolve_super_method_reference(&owner_fqn, &method)
                            } else {
                                query.resolve_method_reference_with_chain_cache(
                                    &owner_fqn,
                                    &method,
                                    &mut method_lookup_chain_cache,
                                )
                            }
                        })
                        .clone();
                    if access == MethodReferenceAccess::Normal
                        && matches!(fact, MethodLookupResult::Ambiguous { .. })
                    {
                        if let Some(source_ordered) = AnalysisQuery::new(self)
                            .source_ordered_top_level_method_reference(
                                &owner_fqn,
                                &method,
                                candidate.range,
                            )
                        {
                            fact = MethodLookupResult::Unique(source_ordered);
                        }
                    }
                    if let Some(expression_range) = call_expression_range {
                        let outcome = self.call_expression_outcome_from_method_resolution(
                            (owner_lookup_id, owner_kind, method, is_super),
                            access,
                            caller,
                            &method_lookup_chain_cache,
                            &fact,
                            &mut call_outcome_caches,
                        );
                        Self::insert_resolved_call_outcome(
                            &mut resolved_call_outcomes,
                            expression_range,
                            outcome,
                        );
                    }
                    if let Some((owner, resolved_method, fact)) = fact.reference_parts() {
                        let target =
                            FullyQualifiedName::method(owner.namespace_parts(), resolved_method);
                        let target = self.names.intern_fqn(target);
                        resolved_refs.push((
                            target,
                            ReferenceFact::method(candidate.range, caller, access),
                        ));
                        if resolved_method == method {
                            if let Some(diagnostics) = diagnostics.as_deref() {
                                if let Some(fact) = fact {
                                    self.push_unavailable_method_diagnostic(
                                        fact,
                                        &method,
                                        diagnostics.diagnostic_range,
                                        &mut unresolved,
                                    );
                                    self.push_signature_diagnostics(
                                        fact,
                                        &owner_fqn,
                                        &method,
                                        diagnostics.signature.as_ref(),
                                        diagnostics.receiver_label.as_deref(),
                                        diagnostics.diagnostic_range,
                                        &mut unresolved,
                                    );
                                }
                            }
                        }
                    } else if fact.is_missing() {
                        let namespace_exists = *method_namespace_exists_cache
                            .entry(owner_fqn.clone())
                            .or_insert_with(|| self.method_namespace_target_exists(&owner_fqn));
                        let allow_unindexed_owner = diagnostics
                            .as_deref()
                            .is_some_and(|diagnostics| diagnostics.allow_unindexed_owner);
                        if !namespace_exists && !allow_unindexed_owner {
                            continue;
                        }
                        let target =
                            FullyQualifiedName::method(owner_fqn.namespace_parts(), method);
                        let target = self.names.intern_fqn(target);
                        resolved_refs.push((
                            target,
                            ReferenceFact::method(candidate.range, caller, access),
                        ));

                        if let Some(diagnostics) = diagnostics.as_deref() {
                            if !diagnostics.diagnose_unresolved {
                                continue;
                            }
                            let explicit_absence =
                                self.method_absence_has_explicit_contract(&owner_fqn, method);
                            if !explicit_absence
                                && self.method_lookup_chain_is_incomplete_cached(
                                    &owner_fqn,
                                    &unresolved_method_edge_sources,
                                    &mut method_chain_completeness_cache,
                                )
                            {
                                continue;
                            }
                            let suggestion = namespace_exists
                                .then(|| {
                                    method_suggestion_cache
                                        .entry((owner_fqn.clone(), method))
                                        .or_insert_with(|| {
                                            self.find_method_suggestion(&owner_fqn, method.as_str())
                                        })
                                        .clone()
                                })
                                .flatten();
                            let mut message = match &diagnostics.receiver_label {
                                Some(label) => {
                                    format!(
                                        "Unresolved method `{}` on `{}`",
                                        method.as_str(),
                                        label
                                    )
                                }
                                None => format!("Unresolved method `{}`", method.as_str()),
                            };
                            if let Some(suggestion) = suggestion {
                                message.push_str(&format!(". Did you mean `{}`?", suggestion));
                            }
                            unresolved
                                .entry(file_id)
                                .or_default()
                                .push(DiagnosticFact::new(
                                    diagnostics.diagnostic_range,
                                    crate::core::DiagnosticSeverity::Warning,
                                    "unresolved-method",
                                    message,
                                ));
                        }
                    }
                }
            }
        }

        self.facts
            .references
            .resolved
            .replace_file(file_id, resolved_refs);
        self.replace_resolved_call_expression_outcomes(resolved_call_outcomes);
        let mut diagnostics = self
            .facts
            .diagnostics
            .resolved
            .facts_in_file(file_id)
            .into_iter()
            .filter(|fact| fact.code != "unresolved-constant")
            .filter(|fact| fact.code != "unresolved-method")
            .filter(|fact| fact.code != "unsupported-runtime-api")
            .filter(|fact| fact.code != "wrong-arity")
            .filter(|fact| fact.code != "unknown-kwarg")
            .filter(|fact| fact.code != "missing-kwarg")
            .filter(|fact| fact.code != "raise-non-exception")
            .filter(|fact| fact.code != "bad-splat")
            .collect::<Vec<_>>();
        diagnostics.extend(unresolved.remove(&file_id).unwrap_or_default());
        self.facts
            .diagnostics
            .resolved
            .replace_file(file_id, diagnostics);
    }

    fn proven_deferred_receiver_type(
        &self,
        range: TextRange,
        resolved_call_outcomes: &HashMap<TextRange, TypeInferenceOutcome>,
    ) -> Option<RubyType> {
        if let Some(outcome) = resolved_call_outcomes.get(&range) {
            return outcome.proven_type().cloned();
        }
        if let Some(outcome) = self.call_expression_outcome_at(range) {
            return match outcome {
                TypeInferenceOutcomeRef::Proven(ruby_type) => Some(ruby_type.clone()),
                TypeInferenceOutcomeRef::Unknown(_) => None,
            };
        }
        let query = AnalysisQuery::new(self);
        if let Some(local_type) = query.local_read_type_at(range.file_id, range.start_byte) {
            return (local_type != RubyType::Unknown).then_some(local_type);
        }
        query
            .exact_expression_type(range)
            .filter(|ruby_type| *ruby_type != RubyType::Unknown)
    }

    fn deferred_receiver_is_unknown(
        &self,
        range: TextRange,
        resolved_call_outcomes: &HashMap<TextRange, TypeInferenceOutcome>,
    ) -> bool {
        if let Some(outcome) = resolved_call_outcomes.get(&range) {
            return outcome.unknown_reason().is_some();
        }
        if let Some(outcome) = self.call_expression_outcome_at(range) {
            return matches!(outcome, TypeInferenceOutcomeRef::Unknown(_));
        }
        let query = AnalysisQuery::new(self);
        query.local_read_type_at(range.file_id, range.start_byte) == Some(RubyType::Unknown)
            || query.exact_expression_unknown_reason(range).is_some()
    }

    fn proven_receiver_namespace(
        &self,
        receiver_type: &RubyType,
        allow_unindexed_owner: bool,
    ) -> Option<FullyQualifiedName> {
        let query = AnalysisQuery::new(self);
        let namespace = query.type_to_namespace(receiver_type)?;
        let expected_kind = match receiver_type {
            RubyType::Class(_)
            | RubyType::ClassReference(_)
            | RubyType::Array(_)
            | RubyType::Hash(_, _) => GraphNodeKind::Class,
            RubyType::Module(_) | RubyType::ModuleReference(_) => GraphNodeKind::Module,
            RubyType::Union(_) | RubyType::Unknown => return None,
        };
        match query.namespace_node_kind(&namespace) {
            Some(declaration_kind) => (declaration_kind == expected_kind).then_some(namespace),
            None if allow_unindexed_owner || self.method_namespace_target_exists(&namespace) => {
                Some(namespace)
            }
            None => None,
        }
    }

    fn insert_resolved_call_outcome(
        outcomes: &mut HashMap<TextRange, TypeInferenceOutcome>,
        range: TextRange,
        outcome: TypeInferenceOutcome,
    ) {
        assert!(
            outcomes.insert(range, outcome).is_none(),
            "INVARIANT VIOLATED: one call expression resolved through multiple method candidates. This is a bug because one runtime dispatch must have one proof outcome. Fix: attach the call range only to the candidate representing the invoked method."
        );
    }

    fn call_expression_outcome_from_grouped_resolution(
        &self,
        callees: &[ResolvedMethodCallee],
        method: RubyMethod,
        caches: &mut MethodCallOutcomeCaches,
    ) -> TypeInferenceOutcome {
        let mut return_types = Vec::new();
        for callee in callees {
            let mut matching = self
                .method_facts_matching_owner_name(&callee.owner, &method)
                .into_iter()
                .filter(|fact| callee.definition_ranges.contains(&fact.range))
                .collect::<Vec<_>>();
            if matching.len() != 1 {
                return TypeInferenceOutcome::unknown(UnknownReason::UnresolvedMethodReturn);
            }
            let fact = matching.pop().expect(
                "INVARIANT VIOLATED: one grouped method fact disappeared after length validation. This is a bug because the local fact vector is not mutated between the check and pop. Fix: keep grouped return selection atomic.",
            );
            let Some(return_type) = self.cached_method_return_type(&fact, caches) else {
                return TypeInferenceOutcome::unknown(UnknownReason::UnresolvedMethodReturn);
            };
            return_types.push(return_type);
        }
        TypeInferenceOutcome::from_optional(
            (!return_types.is_empty()).then(|| RubyType::union(return_types)),
            UnknownReason::UnresolvedMethodReturn,
        )
    }

    fn resolution_uses_builtin_constructor(&self, resolution: &MethodLookupResult) -> bool {
        match resolution {
            MethodLookupResult::Missing => true,
            MethodLookupResult::Ambiguous { .. } => false,
            MethodLookupResult::Unique(fact) => {
                let FullyQualifiedName::Method(_, resolved_method) = &fact.fqn else {
                    panic!(
                        "INVARIANT VIOLATED: method lookup returned a fact whose FQN is not a method. This is a bug because constructor proof can inspect only resolved method declarations. Fix: keep MethodStore restricted to Method FQNs."
                    );
                };
                if resolved_method.as_str() != "new" {
                    return true;
                }
                let owner_parts = fact.owner.namespace_parts();
                let is_builtin_class_owner = owner_parts.len() == 1
                    && owner_parts[0].as_str() == "Class"
                    && self.file(fact.range.file_id).is_some_and(|file| {
                        matches!(
                            file.kind,
                            crate::core::SourceKind::Stub
                                | crate::core::SourceKind::Signature
                                | crate::core::SourceKind::Stdlib
                        )
                    });
                is_builtin_class_owner
            }
        }
    }

    fn call_expression_outcome_from_method_resolution(
        &self,
        method_cache_key: MethodReferenceCacheKey,
        access: MethodReferenceAccess,
        caller: Option<FqnId>,
        method_lookup_chain_cache: &MethodLookupChainCache,
        resolution: &MethodLookupResult,
        caches: &mut MethodCallOutcomeCaches,
    ) -> crate::core::TypeInferenceOutcome {
        let (owner, owner_kind, method, _is_super) = method_cache_key;
        let return_type = match (access, resolution) {
            (
                MethodReferenceAccess::Normal | MethodReferenceAccess::VisibilityBypass,
                MethodLookupResult::Unique(fact),
            ) => self.cached_method_return_type(fact, caches),
            (MethodReferenceAccess::ExplicitReceiver, MethodLookupResult::Unique(fact)) => {
                match self.cached_method_visibility(
                    method_cache_key,
                    fact,
                    method_lookup_chain_cache,
                    caches,
                ) {
                    CachedMethodVisibility::Public => {
                        self.cached_method_return_type(fact, caches)
                    }
                    CachedMethodVisibility::Protected(visibility_owner) => caller
                        .and_then(|caller| self.call_expression_caller_namespace(caller))
                        .and_then(|caller| {
                            let visibility_owner = self.fqn_for_id(visibility_owner).expect(
                                "INVARIANT VIOLATED: cached protected visibility owner disappeared from the name registry. This is a bug because resolve-local cache entries reference the immutable engine name registry. Fix: discard visibility caches before mutating engine names.",
                            );
                            protected_method_visible_from(self, visibility_owner, &caller)
                                .then(|| self.cached_method_return_type(fact, caches))
                                .flatten()
                        }),
                    CachedMethodVisibility::Private => None,
                }
            }
            (
                MethodReferenceAccess::Normal | MethodReferenceAccess::VisibilityBypass,
                MethodLookupResult::Ambiguous { .. },
            ) => self.cached_ambiguous_method_return_type(
                method_cache_key,
                AmbiguousMethodReturnAccess::Private,
                caches,
            ),
            (MethodReferenceAccess::ExplicitReceiver, MethodLookupResult::Ambiguous { .. }) => {
                let access = caller.map_or(
                    AmbiguousMethodReturnAccess::Public,
                    AmbiguousMethodReturnAccess::Protected,
                );
                self.cached_ambiguous_method_return_type(method_cache_key, access, caches)
            }
            (
                MethodReferenceAccess::Normal
                | MethodReferenceAccess::ExplicitReceiver
                | MethodReferenceAccess::VisibilityBypass,
                MethodLookupResult::Missing,
            ) => None,
        };
        // Core Class#new is intentionally generic/untyped in the bundled
        // signature. A proven class receiver supplies the stronger language
        // constructor result, but only for that exact built-in declaration;
        // a user-defined `self.new` with Unknown return must remain Unknown.
        let return_type = return_type
            .filter(|ruby_type| *ruby_type != RubyType::Unknown)
            .or_else(|| {
            if method.as_str() != "new"
                || owner_kind != NamespaceKind::Singleton
                || !self.resolution_uses_builtin_constructor(resolution)
            {
                return None;
            }
            let owner_lookup = self.names.const_lookup(owner).expect(
                "INVARIANT VIOLATED: constructor call candidate points to a missing owner lookup. This is a bug because reference candidates contain only interned lookup IDs. Fix: retain the owner lookup for the candidate lifetime.",
            );
            let instance_namespace =
                FullyQualifiedName::namespace(owner_lookup.path.to_vec());
            (AnalysisQuery::new(self).namespace_node_kind(&instance_namespace)
                == Some(GraphNodeKind::Class))
            .then(|| {
                RubyType::Class(FullyQualifiedName::constant(owner_lookup.path.to_vec()))
            })
        });
        TypeInferenceOutcome::from_optional(return_type, UnknownReason::UnresolvedMethodReturn)
    }

    fn cached_method_return_type(
        &self,
        fact: &MethodFact,
        caches: &mut MethodCallOutcomeCaches,
    ) -> Option<RubyType> {
        let fqn = self.names.fqn_id(&fact.fqn).expect(
            "INVARIANT VIOLATED: resolved method fact has no interned FQN. This is a bug because resolve-local return caching can reference only facts stored in this engine. Fix: intern method FQNs before reference resolution.",
        );
        let key = (fqn, fact.range);
        if let Some(cached) = caches.returns.get(&key) {
            caches.return_hits = caches.return_hits.checked_add(1).expect(
                "INVARIANT VIOLATED: method-return cache hit counter overflowed usize. This is a bug because one resolve pass cannot exceed addressable operations. Fix: inspect corrupt resolve instrumentation.",
            );
            return cached.clone();
        }
        caches.return_misses = caches.return_misses.checked_add(1).expect(
            "INVARIANT VIOLATED: method-return cache miss counter overflowed usize. This is a bug because one resolve pass cannot exceed addressable operations. Fix: inspect corrupt resolve instrumentation.",
        );
        let result = AnalysisQuery::new(self).method_return_type(fact);
        caches.returns.insert(key, result.clone());
        result
    }

    fn cached_ambiguous_method_return_type(
        &self,
        key: MethodReferenceCacheKey,
        access: AmbiguousMethodReturnAccess,
        caches: &mut MethodCallOutcomeCaches,
    ) -> Option<RubyType> {
        let cache_key = (key, access);
        if let Some(cached) = caches.ambiguous_returns.get(&cache_key) {
            caches.ambiguous_return_hits = caches
                .ambiguous_return_hits
                .checked_add(1)
                .expect(
                    "INVARIANT VIOLATED: ambiguous method-return cache hit counter overflowed usize. This is a bug because one resolve pass cannot exceed addressable operations. Fix: inspect corrupt resolve instrumentation.",
                );
            return cached.clone();
        }
        caches.ambiguous_return_misses = caches
            .ambiguous_return_misses
            .checked_add(1)
            .expect(
                "INVARIANT VIOLATED: ambiguous method-return cache miss counter overflowed usize. This is a bug because one resolve pass cannot exceed addressable operations. Fix: inspect corrupt resolve instrumentation.",
            );

        let (owner, owner_kind, method, _is_super) = key;
        let owner_lookup = self.names.const_lookup(owner).expect(
            "INVARIANT VIOLATED: ambiguous call-expression candidate points to a missing owner lookup. This is a bug because reference candidates contain only interned lookup IDs. Fix: retain the owner lookup for the candidate lifetime.",
        );
        let owner = FullyQualifiedName::namespace_with_kind(owner_lookup.path.to_vec(), owner_kind);
        let query = AnalysisQuery::new(self);
        let result = match access {
            AmbiguousMethodReturnAccess::Private => {
                query.method_return_type_for_receiver(&owner, &method)
            }
            AmbiguousMethodReturnAccess::Public => {
                let all_callees = query.resolve_method_callees(&owner, &method);
                let visible_callees = query.resolve_public_method_callees(&owner, &method);
                (all_callees == visible_callees)
                    .then(|| query.method_return_type_for_public_receiver(&owner, &method))
                    .flatten()
            }
            AmbiguousMethodReturnAccess::Protected(caller) => self
                .call_expression_caller_namespace(caller)
                .and_then(|caller| {
                    let all_callees = query.resolve_method_callees(&owner, &method);
                    let visible_callees =
                        query.resolve_protected_method_callees(&owner, &method, &caller);
                    (all_callees == visible_callees)
                        .then(|| {
                            query
                                .method_return_type_for_protected_receiver(&owner, &method, &caller)
                        })
                        .flatten()
                }),
        };
        assert!(
            caches
                .ambiguous_returns
                .insert(cache_key, result.clone())
                .is_none(),
            "INVARIANT VIOLATED: ambiguous method-return cache replaced an existing key after a confirmed miss. This is a bug because resolve-local lookup identity, access, and caller are immutable. Fix: keep ambiguous return lookup and insertion in one candidate step."
        );
        result
    }

    fn cached_method_visibility(
        &self,
        key: MethodReferenceCacheKey,
        fact: &MethodFact,
        method_lookup_chain_cache: &MethodLookupChainCache,
        caches: &mut MethodCallOutcomeCaches,
    ) -> CachedMethodVisibility {
        if let Some(cached) = caches.visibilities.get(&key) {
            caches.visibility_hits = caches.visibility_hits.checked_add(1).expect(
                "INVARIANT VIOLATED: method-visibility cache hit counter overflowed usize. This is a bug because one resolve pass cannot exceed addressable operations. Fix: inspect corrupt resolve instrumentation.",
            );
            return *cached;
        }
        caches.visibility_misses = caches.visibility_misses.checked_add(1).expect(
            "INVARIANT VIOLATED: method-visibility cache miss counter overflowed usize. This is a bug because one resolve pass cannot exceed addressable operations. Fix: inspect corrupt resolve instrumentation.",
        );
        let (owner, owner_kind, method, _is_super) = key;
        let owner_lookup = self.names.const_lookup(owner).expect(
            "INVARIANT VIOLATED: call-expression candidate points to a missing owner lookup. This is a bug because reference candidates contain only interned lookup IDs. Fix: retain the owner lookup for the candidate lifetime.",
        );
        let owner = FullyQualifiedName::namespace_with_kind(owner_lookup.path.to_vec(), owner_kind);
        let ancestor_chain = method_lookup_chain_cache
            .get(&owner)
            .map(|owner_ids| {
                owner_ids
                    .iter()
                    .map(|owner_id| {
                        self.fqn_for_id(*owner_id)
                            .expect(
                                "INVARIANT VIOLATED: call-expression lookup-chain owner ID is absent from the name registry. This is a bug because the resolution-local cache contains only IDs from that registry. Fix: invalidate lookup-chain caches when names change.",
                            )
                            .clone()
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_else(|| method_lookup_chain(self, &owner));
        let (visibility, visibility_owner) =
            effective_method_visibility_for_chain(self, &ancestor_chain, fact, &method);
        let cached = match visibility {
            MethodVisibility::Public => CachedMethodVisibility::Public,
            MethodVisibility::Protected => self
                .names
                .fqn_id(&visibility_owner)
                .map(CachedMethodVisibility::Protected)
                .expect(
                    "INVARIANT VIOLATED: effective protected visibility owner has no interned FQN. This is a bug because lookup-chain owners come from this engine's name registry. Fix: intern graph and method owners before reference resolution.",
                ),
            MethodVisibility::Private => CachedMethodVisibility::Private,
        };
        assert!(
            caches.visibilities.insert(key, cached).is_none(),
            "INVARIANT VIOLATED: method visibility cache replaced an existing key after a confirmed miss. This is a bug because resolve-local lookup identity is immutable. Fix: keep visibility lookup and insertion in one candidate step."
        );
        cached
    }

    fn call_expression_caller_namespace(&self, caller: FqnId) -> Option<FullyQualifiedName> {
        let caller = self.fqn_for_id(caller)?;
        let mut owners = self
            .method_facts_for(caller)
            .into_iter()
            .map(|fact| fact.owner)
            .collect::<Vec<_>>();
        owners.sort_by_key(ToString::to_string);
        owners.dedup();
        Some(if owners.len() == 1 {
            owners.pop().expect(
                "INVARIANT VIOLATED: one call-expression caller owner disappeared after length validation. This is a bug because protected return-type lookup needs a stable caller namespace. Fix: keep caller-owner selection atomic.",
            )
        } else {
            FullyQualifiedName::namespace(caller.namespace_parts())
        })
    }

    fn resolve_grouped_method_callees(
        &self,
        receiver_type: &RubyType,
        method: RubyMethod,
        access: MethodReferenceAccess,
        caller: Option<FqnId>,
    ) -> Option<Vec<ResolvedMethodCallee>> {
        assert!(
            matches!(receiver_type, RubyType::Union(_)),
            "INVARIANT VIOLATED: grouped method dispatch received a non-union receiver. This is a bug because scalar receivers must use the compact single-owner resolution path. Fix: enter grouped resolution only after validating a canonical RubyType::Union."
        );
        let query = AnalysisQuery::new(self);
        match access {
            MethodReferenceAccess::Normal | MethodReferenceAccess::VisibilityBypass => {
                query.resolve_method_callees_for_type(receiver_type, &method)
            }
            MethodReferenceAccess::ExplicitReceiver => caller
                .and_then(|caller| self.call_expression_caller_namespace(caller))
                .and_then(|caller| {
                    query.resolve_protected_method_callees_for_type(receiver_type, &method, &caller)
                })
                .or_else(|| query.resolve_public_method_callees_for_type(receiver_type, &method)),
        }
    }

    fn unambiguous_grouped_method_facts(
        &self,
        callees: &[ResolvedMethodCallee],
        method: RubyMethod,
    ) -> Vec<MethodFact> {
        let mut facts = Vec::new();
        for callee in callees {
            let mut matching = self
                .method_facts_matching_owner_name(&callee.owner, &method)
                .into_iter()
                .filter(|fact| callee.definition_ranges.contains(&fact.range))
                .collect::<Vec<_>>();
            if matching.len() == 1 {
                facts.push(matching.pop().expect(
                    "INVARIANT VIOLATED: one grouped method fact disappeared after length validation. This is a bug because the local fact vector is not mutated between the check and pop. Fix: keep grouped fact selection atomic.",
                ));
            }
        }
        facts.sort_by_key(|fact| {
            (
                fact.range.file_id,
                fact.range.start_byte,
                fact.range.end_byte,
                fact.fqn.to_string(),
            )
        });
        facts.dedup();
        facts
    }

    fn push_grouped_method_fact_diagnostics(
        &self,
        callees: &[ResolvedMethodCallee],
        method: RubyMethod,
        diagnostics: &crate::core::MethodReferenceDiagnostics,
        diagnostics_by_file: &mut HashMap<SourceFileId, Vec<DiagnosticFact>>,
    ) {
        let mut grouped = HashMap::new();
        for fact in self.unambiguous_grouped_method_facts(callees, method) {
            self.push_unavailable_method_diagnostic(
                &fact,
                &method,
                diagnostics.diagnostic_range,
                &mut grouped,
            );
            self.push_signature_diagnostics(
                &fact,
                &fact.owner,
                &method,
                diagnostics.signature.as_ref(),
                diagnostics.receiver_label.as_deref(),
                diagnostics.diagnostic_range,
                &mut grouped,
            );
        }
        let mut grouped = grouped.into_iter().collect::<Vec<_>>();
        grouped.sort_by_key(|(file_id, _facts)| file_id.0);
        for (file_id, mut facts) in grouped {
            facts.sort_by(|left, right| {
                (
                    left.range.start_byte,
                    left.range.end_byte,
                    left.code.as_str(),
                    left.message.as_str(),
                )
                    .cmp(&(
                        right.range.start_byte,
                        right.range.end_byte,
                        right.code.as_str(),
                        right.message.as_str(),
                    ))
            });
            facts.dedup();
            diagnostics_by_file
                .entry(file_id)
                .or_default()
                .extend(facts);
        }
    }

    fn push_grouped_unresolved_method_diagnostic(
        &self,
        receiver_type: &RubyType,
        method: RubyMethod,
        diagnostics: &crate::core::MethodReferenceDiagnostics,
        unresolved_method_edge_sources: &HashSet<Vec<RubyConstant>>,
        completeness_cache: &mut MethodChainCompletenessCache,
        diagnostics_by_file: &mut HashMap<SourceFileId, Vec<DiagnosticFact>>,
    ) {
        if !diagnostics.diagnose_unresolved {
            return;
        }
        let namespaces = AnalysisQuery::receiver_type_to_method_namespaces(receiver_type);
        if namespaces.is_empty()
            || namespaces.iter().any(|owner| {
                !self.method_namespace_target_exists(owner)
                    || (!self.method_absence_has_explicit_contract(owner, method)
                        && self.method_lookup_chain_is_incomplete_cached(
                            owner,
                            unresolved_method_edge_sources,
                            completeness_cache,
                        ))
            })
        {
            return;
        }
        let query = AnalysisQuery::new(self);
        if namespaces.iter().any(|owner| {
            query
                .resolve_method_callees(owner, &method)
                .is_some_and(|callees| {
                    callees
                        .iter()
                        .any(|callee| callee.resolution == MethodCalleeResolution::MethodMissing)
                })
        }) {
            return;
        }
        let message = match &diagnostics.receiver_label {
            Some(label) => format!("Unresolved method `{}` on `{}`", method.as_str(), label),
            None => format!("Unresolved method `{}`", method.as_str()),
        };
        diagnostics_by_file
            .entry(diagnostics.diagnostic_range.file_id)
            .or_default()
            .push(DiagnosticFact::new(
                diagnostics.diagnostic_range,
                crate::core::DiagnosticSeverity::Warning,
                "unresolved-method",
                message,
            ));
    }

    fn unresolved_method_edge_sources(&self) -> HashSet<Vec<RubyConstant>> {
        self.graph
            .unresolved_edges()
            .into_iter()
            .filter(|edge| {
                let lookup = self.names.const_lookup(edge.target).expect(
                    "INVARIANT VIOLATED: unresolved graph edge points to a missing constant lookup. This is a bug because graph edges must retain valid interned targets. Fix: intern and retain every unresolved graph target for the edge lifetime.",
                );
                !(edge.kind == GraphEdgeKind::Superclass
                    && lookup.absolute
                    && lookup.path.len() == 1
                    && lookup.path[0].as_str() == "Object")
            })
            .filter_map(|edge| {
                self.names
                    .fqn(edge.source)
                    .map(FullyQualifiedName::namespace_parts)
            })
            .collect()
    }

    fn method_lookup_chain_is_incomplete_cached(
        &self,
        owner: &FullyQualifiedName,
        unresolved_sources: &HashSet<Vec<RubyConstant>>,
        cache: &mut MethodChainCompletenessCache,
    ) -> bool {
        if let Some(incomplete) = cache.results.get(owner) {
            return *incomplete;
        }
        let incomplete = self.method_lookup_chain_is_incomplete(owner, unresolved_sources, cache);
        cache.results.insert(owner.clone(), incomplete);
        incomplete
    }

    fn method_lookup_chain_is_incomplete(
        &self,
        owner: &FullyQualifiedName,
        unresolved_sources: &HashSet<Vec<RubyConstant>>,
        cache: &mut MethodChainCompletenessCache,
    ) -> bool {
        // Top-level Ruby is an open execution environment. Test/framework DSLs
        // install methods on Object/Kernel at runtime, and an empty namespace
        // has no closed declaration whose absent method set can be proven.
        if owner.namespace_parts().is_empty() {
            return true;
        }
        if owner.namespace_kind() == Some(NamespaceKind::Singleton) {
            let metaclass = match self.graph_nodes_for(owner).first().map(|fact| fact.kind) {
                Some(GraphNodeKind::Class) => Some("Class"),
                Some(GraphNodeKind::Module) => Some("Module"),
                None => None,
            };
            if metaclass.is_some_and(|name| {
                let constant = RubyConstant::new(name).unwrap_or_else(|error| {
                    panic!(
                        "INVARIANT VIOLATED: Ruby metaclass name `{name}` is invalid: {error}. This is a bug because Class and Module are universal Ruby constants. Fix: preserve RubyConstant support for language-defined class names."
                    )
                });
                self.graph_nodes_for(&FullyQualifiedName::namespace(vec![constant]))
                    .is_empty()
            }) {
                return true;
            }
        }
        let mut pending = vec![owner.clone()];
        let mut visited = std::collections::HashSet::new();
        while let Some(current) = pending.pop() {
            if !visited.insert(current.clone()) {
                continue;
            }
            if *cache
                .ambiguous_superclasses
                .entry(current.clone())
                .or_insert_with(|| self.superclass_is_ambiguous(&current))
            {
                return true;
            }
            if unresolved_sources.contains(&current.namespace_parts()) {
                return true;
            }
            if *cache
                .dynamic_mixin_hooks
                .entry(current.clone())
                .or_insert_with(|| self.namespace_has_dynamic_mixin_hook(&current))
            {
                return true;
            }
            pending.extend(
                self.graph_edges_from(&current)
                    .into_iter()
                    .filter(|edge| {
                        matches!(
                            edge.kind,
                            GraphEdgeKind::Superclass
                                | GraphEdgeKind::Include
                                | GraphEdgeKind::Prepend
                                | GraphEdgeKind::Extend
                        )
                    })
                    .map(|edge| edge.target),
            );
        }
        false
    }

    fn method_absence_has_explicit_contract(
        &self,
        owner: &FullyQualifiedName,
        method: RubyMethod,
    ) -> bool {
        self.method_absence_contract_matches_owner_name(owner, &method)
    }

    /// An include/prepend/extend callback can install methods through arbitrary
    /// Ruby code. Static edges model the common `base.extend(ClassMethods)`
    /// shape, but a custom callback remains an incomplete negative-proof
    /// surface unless every effect is represented. Concrete lookup may still
    /// resolve known methods; only "method is absent" diagnostics fail closed.
    fn namespace_has_dynamic_mixin_hook(&self, namespace: &FullyQualifiedName) -> bool {
        let instance_namespace = match namespace.namespace_kind() {
            Some(NamespaceKind::Instance) => namespace.clone(),
            Some(NamespaceKind::Singleton) => namespace.to_instance_namespace().expect(
                "INVARIANT VIOLATED: singleton namespace cannot produce its instance counterpart. This is a bug because method lookup chains contain only Namespace FQNs. Fix: preserve Namespace identity while traversing mixin hooks.",
            ),
            None => panic!(
                "INVARIANT VIOLATED: method lookup completeness received a non-namespace FQN `{namespace}`. This is a bug because only namespaces own method lookup chains. Fix: convert receiver types to Namespace FQNs before diagnostics."
            ),
        };

        for edge in self.graph_edges_from(&instance_namespace) {
            let callbacks: &[&str] = match edge.kind {
                GraphEdgeKind::Include => &["included", "append_features"],
                GraphEdgeKind::Prepend => &["prepended", "prepend_features"],
                GraphEdgeKind::Superclass
                | GraphEdgeKind::Extend
                | GraphEdgeKind::ExecutionContextApplication => continue,
            };
            if self.namespace_defines_any_singleton_method(&edge.target, callbacks) {
                return true;
            }
        }

        for edge in self.graph_edges_from(namespace) {
            if edge.kind == GraphEdgeKind::Extend
                && self.namespace_defines_any_singleton_method(
                    &edge.target,
                    &["extended", "extend_object"],
                )
            {
                return true;
            }
        }
        false
    }

    fn namespace_defines_any_singleton_method(
        &self,
        namespace: &FullyQualifiedName,
        names: &[&str],
    ) -> bool {
        let Some(singleton) = namespace.to_singleton_namespace() else {
            return false;
        };
        names.iter().any(|name| {
            let method = RubyMethod::new(name).unwrap_or_else(|error| {
                panic!(
                    "INVARIANT VIOLATED: Ruby lifecycle method name `{name}` is invalid: {error}. This is a bug because lifecycle names are fixed Ruby identifiers. Fix: preserve RubyMethod support for language-defined callback names."
                )
            });
            !self
                .method_facts_matching_owner_name(&singleton, &method)
                .is_empty()
        })
    }

    fn push_unavailable_method_diagnostic(
        &self,
        fact: &MethodFact,
        method: &crate::core::RubyMethod,
        diagnostic_range: TextRange,
        diagnostics_by_file: &mut HashMap<SourceFileId, Vec<DiagnosticFact>>,
    ) {
        let MethodAvailability::Unavailable { reason } = &fact.availability else {
            return;
        };
        diagnostics_by_file
            .entry(diagnostic_range.file_id)
            .or_default()
            .push(DiagnosticFact::new(
                diagnostic_range,
                crate::core::DiagnosticSeverity::Warning,
                "unsupported-runtime-api",
                format!(
                    "Runtime API `{}` is unavailable: {}",
                    method.as_str(),
                    reason
                ),
            ));
    }

    fn push_signature_diagnostics(
        &self,
        fact: &MethodFact,
        requested_owner: &FullyQualifiedName,
        method: &crate::core::RubyMethod,
        signature: Option<&MethodCallSignatureCandidate>,
        receiver_label: Option<&str>,
        diagnostic_range: TextRange,
        diagnostics_by_file: &mut HashMap<SourceFileId, Vec<DiagnosticFact>>,
    ) {
        let Some(signature) = signature else {
            return;
        };
        if !fact.has_complete_parameter_shape() {
            return;
        }

        let arity = MethodArity::from_params(&fact.param_facts);
        let declares_keywords = arity.has_kwrest
            || !arity.required_keywords.is_empty()
            || !arity.optional_keywords.is_empty();
        let keywords_form_options_hash = !declares_keywords && signature.has_nonempty_keyword_hash;
        let mut effective_signature = signature.clone();
        if keywords_form_options_hash {
            effective_signature.positional_count += 1;
        }
        if !declares_keywords && signature.has_keyword_splat {
            // The splatted hash may be empty or non-empty. Under options-hash
            // calling semantics that makes the positional count a range, not
            // one additional proven argument.
            effective_signature.has_positional_splat = true;
        }
        let mismatch = if declares_keywords
            && effective_signature.trailing_positional_may_be_options_hash
        {
            assert!(
                effective_signature.positional_count > 0,
                "INVARIANT VIOLATED: a trailing positional options-hash marker exists without a positional argument. This is a bug because the marker can only be set while counting the final positional argument. Fix: update positional_count and trailing_positional_may_be_options_hash atomically."
            );
            let direct_mismatch = arity_mismatch(&effective_signature, &arity);
            let mut converted_signature = effective_signature.clone();
            converted_signature.positional_count -= 1;
            let converted_mismatch = arity_mismatch(&converted_signature, &arity);
            match (direct_mismatch, converted_mismatch) {
                (Some(direct), Some(_converted)) => Some(direct),
                (Some(_direct), None) => None,
                (None, Some(_converted)) => None,
                (None, None) => None,
            }
        } else {
            arity_mismatch(&effective_signature, &arity)
        };
        if let Some((min, max, actual)) = mismatch {
            if log::log_enabled!(
                target: "ruby_analysis::engine::diagnostics::wrong_arity",
                log::Level::Debug
            ) {
                let lookup_chain = method_lookup_chain(self, requested_owner);
                debug!(
                    target: "ruby_analysis::engine::diagnostics::wrong_arity",
                    "wrong-arity proof: method={} receiver={} requested_owner={} resolved_fqn={} owner={} definition_file_id={} definition_range={}..{} call_file_id={} call_range={}..{} positional={} params={:?} lookup_chain={:?}",
                    method.as_str(),
                    receiver_label.unwrap_or("<implicit-self>"),
                    requested_owner,
                    fact.fqn,
                    fact.owner,
                    fact.range.file_id.0,
                    fact.range.start_byte,
                    fact.range.end_byte,
                    diagnostic_range.file_id.0,
                    diagnostic_range.start_byte,
                    diagnostic_range.end_byte,
                    actual,
                    fact.param_facts,
                    lookup_chain,
                );
            }
            let expected = match max {
                Some(max) if max == min => format!("{}", min),
                Some(max) => format!("{}..{}", min, max),
                None => format!("{}+", min),
            };
            diagnostics_by_file
                .entry(diagnostic_range.file_id)
                .or_default()
                .push(DiagnosticFact::new(
                    diagnostic_range,
                    crate::core::DiagnosticSeverity::Warning,
                    "wrong-arity",
                    format!(
                        "Wrong number of arguments for `{}` (expected {}, got {})",
                        method.as_str(),
                        expected,
                        actual
                    ),
                ));
        }

        if declares_keywords && !arity.has_kwrest && !signature.has_keyword_splat {
            let declared = arity
                .required_keywords
                .iter()
                .chain(arity.optional_keywords.iter())
                .cloned()
                .collect::<Vec<_>>();
            for kwarg in &signature.keyword_args {
                if declared.contains(&kwarg.name) {
                    continue;
                }
                let suggestion = closest_keyword(&kwarg.name, &declared);
                let mut message = format!(
                    "Unknown keyword argument `{}:` for `{}`",
                    kwarg.name,
                    method.as_str()
                );
                if let Some(suggestion) = suggestion {
                    message.push_str(&format!(". Did you mean `{}:`?", suggestion));
                }
                diagnostics_by_file
                    .entry(kwarg.range.file_id)
                    .or_default()
                    .push(DiagnosticFact::new(
                        kwarg.range,
                        crate::core::DiagnosticSeverity::Warning,
                        "unknown-kwarg",
                        message,
                    ));
            }
        }

        if !arity.required_keywords.is_empty()
            && !signature.has_keyword_splat
            && !signature.has_positional_splat
            && !signature.trailing_positional_may_be_options_hash
        {
            let supplied = signature
                .keyword_args
                .iter()
                .map(|kwarg| kwarg.name.as_str())
                .collect::<Vec<_>>();
            let mut missing = arity
                .required_keywords
                .iter()
                .filter(|kwarg| !supplied.contains(&kwarg.as_str()))
                .cloned()
                .collect::<Vec<_>>();
            missing.sort();
            if !missing.is_empty() {
                let kw_list = missing
                    .iter()
                    .map(|kwarg| format!("`{}:`", kwarg))
                    .collect::<Vec<_>>()
                    .join(", ");
                diagnostics_by_file
                    .entry(diagnostic_range.file_id)
                    .or_default()
                    .push(DiagnosticFact::new(
                        diagnostic_range,
                        crate::core::DiagnosticSeverity::Warning,
                        "missing-kwarg",
                        format!(
                            "Missing required keyword argument(s) for `{}`: {}",
                            method.as_str(),
                            kw_list
                        ),
                    ));
            }
        }
    }

    fn resolve_diagnostic_candidates(&self) -> HashMap<SourceFileId, Vec<DiagnosticFact>> {
        let mut diagnostics = HashMap::new();
        for candidate in self.facts.diagnostics.candidates.iter_candidates() {
            if let Some(diagnostic) = self.resolve_diagnostic_candidate(candidate) {
                diagnostics
                    .entry(diagnostic.range.file_id)
                    .or_insert_with(Vec::new)
                    .push(diagnostic);
            }
        }
        diagnostics
    }

    fn resolve_diagnostic_candidates_in_file(&self, file_id: SourceFileId) -> Vec<DiagnosticFact> {
        self.facts
            .diagnostics
            .candidates
            .candidates_in_file(file_id)
            .iter()
            .filter_map(|candidate| self.resolve_diagnostic_candidate(candidate))
            .collect()
    }

    fn resolve_diagnostic_candidate(
        &self,
        candidate: &DiagnosticCandidate,
    ) -> Option<DiagnosticFact> {
        match &candidate.kind {
            DiagnosticCandidateKind::BadSplat {
                operator,
                arg_repr,
                expected,
            } => Some(DiagnosticFact::new(
                candidate.range,
                crate::core::DiagnosticSeverity::Warning,
                "bad-splat",
                format!(
                    "`{}{}` expected {} but got non-{} value",
                    operator, arg_repr, expected, expected
                ),
            )),
            DiagnosticCandidateKind::RaiseNonException { arg_repr, arg } => {
                if self.raise_arg_is_exception(arg.clone()) {
                    None
                } else {
                    Some(DiagnosticFact::new(
                        candidate.range,
                        crate::core::DiagnosticSeverity::Warning,
                        "raise-non-exception",
                        format!(
                            "`raise` argument `{}` is not an Exception subclass",
                            arg_repr
                        ),
                    ))
                }
            }
        }
    }

    fn raise_arg_is_exception(&self, arg: RaiseArgCandidate) -> bool {
        match arg {
            RaiseArgCandidate::StringLiteral | RaiseArgCandidate::Unknown => true,
            RaiseArgCandidate::NonExceptionLiteral => false,
            RaiseArgCandidate::Constant(name) => self.is_exception_class_name(&name),
            RaiseArgCandidate::Type(ruby_type) => self.ruby_type_is_exception(ruby_type),
            RaiseArgCandidate::LocalRead(range) => self
                .local_read_type_at(range.file_id, range.start_byte)
                .map(|ruby_type| self.ruby_type_is_exception(ruby_type.clone()))
                .unwrap_or(true),
            RaiseArgCandidate::BareMethodReturn {
                current_namespace,
                method,
            } => self
                .bare_method_return_type(&current_namespace, &method)
                .map(|ruby_type| self.ruby_type_is_exception(ruby_type))
                .unwrap_or(true),
        }
    }

    fn bare_method_return_type(
        &self,
        current_namespace: &[RubyConstant],
        method: &RubyMethod,
    ) -> Option<RubyType> {
        let query = AnalysisQuery::new(self);
        let mut namespace = current_namespace.to_vec();
        loop {
            let namespace_fqn = FullyQualifiedName::namespace_with_kind(
                namespace.clone(),
                crate::core::NamespaceKind::Instance,
            );
            let lookup = query.resolve_method_reference(&namespace_fqn, method);
            if let Some((_owner, _resolved_method, fact)) = lookup.reference_parts() {
                return fact
                    .and_then(|fact| query.method_return_type(fact))
                    .or(Some(RubyType::Unknown));
            }
            if namespace.is_empty() {
                break;
            }
            namespace.pop();
        }
        None
    }

    fn ruby_type_is_exception(&self, ruby_type: RubyType) -> bool {
        match ruby_type {
            RubyType::Class(fqn) | RubyType::ClassReference(fqn) => {
                let name = fqn
                    .namespace_parts()
                    .last()
                    .map(|constant| constant.to_string())
                    .unwrap_or_default();
                if name == "String" {
                    return true;
                }
                if NON_EXCEPTION_TYPES.contains(&name.as_str()) {
                    return false;
                }
                self.is_exception_class_name(&name)
            }
            RubyType::Module(_) | RubyType::ModuleReference(_) => false,
            RubyType::Union(_) | RubyType::Unknown => true,
            RubyType::Array(_) | RubyType::Hash(_, _) => false,
        }
    }

    fn is_exception_class_name(&self, name: &str) -> bool {
        if EXCEPTION_WHITELIST.contains(&name) {
            return true;
        }
        if name.ends_with("Error") || name.ends_with("Exception") {
            return true;
        }
        let Ok(ruby_const) = RubyConstant::new(name) else {
            return true;
        };
        let ns_fqn = FullyQualifiedName::namespace_with_kind(
            vec![ruby_const],
            crate::core::NamespaceKind::Instance,
        );
        if self.graph_nodes_for(&ns_fqn).is_empty() && self.symbol_facts_for(&ns_fqn).is_empty() {
            return true;
        }

        let mut current = ns_fqn;
        let mut visited = std::collections::HashSet::new();
        while visited.insert(current.clone()) {
            if self.superclass_is_ambiguous(&current) {
                return true;
            }
            let Some(edge) = self.proven_superclass_edge(&current) else {
                break;
            };
            let last = edge.target.namespace_parts().last().map(|c| c.to_string());
            if let Some(target_name) = last {
                if EXCEPTION_WHITELIST.contains(&target_name.as_str()) {
                    return true;
                }
            }
            current = edge.target;
        }

        false
    }

    fn find_method_suggestion(
        &self,
        owner_fqn: &FullyQualifiedName,
        target: &str,
    ) -> Option<String> {
        let threshold = suggestion_threshold(target.len());
        if threshold == 0 {
            return None;
        }

        let target_len = target.len();
        let mut best: Option<(String, usize)> = None;
        for candidate in self.method_names_for_owner(owner_fqn) {
            if candidate == target {
                continue;
            }
            if candidate.len().abs_diff(target_len) > threshold {
                continue;
            }
            let dist = levenshtein(candidate, target);
            if dist > threshold {
                continue;
            }
            match &best {
                Some((_, d)) if *d <= dist => {}
                Some(_) | None => best = Some((candidate.to_string(), dist)),
            }
        }
        best.map(|(name, _)| name)
    }

    fn method_namespace_target_exists(&self, fqn: &FullyQualifiedName) -> bool {
        let parts = fqn.namespace_parts();
        if parts.is_empty() {
            return true;
        }
        let instance_fqn = FullyQualifiedName::namespace_with_kind(
            parts.clone(),
            crate::core::NamespaceKind::Instance,
        );
        let singleton_fqn = FullyQualifiedName::namespace_with_kind(
            parts.clone(),
            crate::core::NamespaceKind::Singleton,
        );
        !self.graph_nodes_for(&instance_fqn).is_empty()
            || !self.graph_nodes_for(&singleton_fqn).is_empty()
            || !self
                .symbol_facts_for(&FullyQualifiedName::constant(parts))
                .is_empty()
            || !self.method_facts_matching_owner(fqn, "").is_empty()
    }

    fn resolve_constant_reference(
        &self,
        parts: &[crate::core::RubyConstant],
        current_namespace: &[crate::core::RubyConstant],
    ) -> Option<FullyQualifiedName> {
        let mut search = current_namespace.to_vec();

        loop {
            let mut probe = search.clone();
            probe.extend(parts.iter().cloned());

            let namespace_fqn = FullyQualifiedName::namespace(probe.clone());
            if !self.graph_nodes_for(&namespace_fqn).is_empty()
                || !self.symbol_facts_for(&namespace_fqn).is_empty()
            {
                return Some(namespace_fqn);
            }

            let constant_fqn = FullyQualifiedName::constant(probe);
            if !self.symbol_facts_for(&constant_fqn).is_empty() {
                return Some(constant_fqn);
            }

            if search.is_empty() {
                break;
            }
            search.pop();
        }

        None
    }
}

fn grouped_method_targets(
    callees: &[ResolvedMethodCallee],
    method: RubyMethod,
) -> Vec<FullyQualifiedName> {
    let mut targets = callees
        .iter()
        .map(|callee| {
            assert!(
                callee.method == method && !callee.definition_ranges.is_empty(),
                "INVARIANT VIOLATED: complete grouped dispatch contains a non-exact method callee. This is a bug because resolve_method_callees_for_type must return Some only after every receiver member resolves to an exact declaration. Fix: retain exact-callee filtering in the shared type resolver."
            );
            FullyQualifiedName::method(callee.owner.namespace_parts(), callee.method)
        })
        .collect::<Vec<_>>();
    targets.sort_by_key(ToString::to_string);
    targets.dedup();
    targets
}

fn constant_name(parts: &[RubyConstant]) -> String {
    parts
        .iter()
        .map(RubyConstant::as_str)
        .collect::<Vec<_>>()
        .join("::")
}

fn method_reference_owner_fqn(
    engine: &AnalysisEngine,
    owner: ConstLookupId,
    owner_kind: NamespaceKind,
) -> FullyQualifiedName {
    let owner_lookup = engine.names.const_lookup(owner).expect(
        "INVARIANT VIOLATED: method reference candidate points to missing owner lookup. \
         This is a bug because stored reference candidates must only contain interned lookup ids. \
         Fix: intern constant lookups before inserting candidates.",
    );
    FullyQualifiedName::namespace_with_kind(owner_lookup.path.to_vec(), owner_kind)
}
