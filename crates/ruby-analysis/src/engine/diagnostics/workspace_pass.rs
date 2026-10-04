//! Whole-workspace resolve pass: turns every stored reference candidate into
//! resolved references, call outcomes, and rebuilt diagnostics.

use crate::invariant::ExpectInvariant;
use std::collections::HashMap;
use std::time::Instant;

use super::grouped_methods::grouped_method_targets;
use super::policy::{MethodAbsenceClaims, UNRESOLVED_CONSTANT, UNRESOLVED_METHOD};
use super::{constant_name, MethodCallOutcomeCaches, MethodReferenceCacheKey};
use crate::core::names::fqn_id::ConstLookupId;
use crate::core::names::fqn_id::FqnId;
use crate::core::storage::reference_store::ConstLookup;
use crate::core::storage::reference_store::StoredReferenceCandidateRef;
use crate::core::{
    ConstantPath, FullyQualifiedName, MethodReferenceAccess, NamespaceKind, ReferenceFact,
    RubyMethod, RubyType, TypeInferenceOutcome, UnknownReason,
};
use crate::engine::resolution::{MethodLookupChainCache, MethodLookupResult};
use crate::engine::state::ResolveStat;
use crate::engine::{Project, View};
use crate::stats::{self, StatsSnapshot};

impl Project {
    pub(in crate::engine) fn resolve_reference_candidates(
        &mut self,
        stats: &mut StatsSnapshot<ResolveStat>,
    ) {
        let mut candidate_file_ids = self.uses.candidate_file_ids();
        for file_id in self.diagnostics.candidate_file_ids() {
            if !candidate_file_ids.contains(&file_id) {
                candidate_file_ids.push(file_id);
            }
        }

        let diagnostic_seed_started = Instant::now();
        let mut unresolved_constants = self.resolve_diagnostic_candidates();
        stats.record_duration(
            ResolveStat::DiagnosticSeedNs,
            diagnostic_seed_started.elapsed(),
        );
        let reference_candidate_store = self.uses.take_candidates();
        let mut method_fact_cache: HashMap<(MethodReferenceCacheKey, bool), MethodLookupResult> =
            HashMap::new();
        let mut method_namespace_exists_cache: HashMap<FullyQualifiedName, bool> = HashMap::new();
        let mut method_suggestion_cache: HashMap<(FullyQualifiedName, RubyMethod), Option<String>> =
            HashMap::new();
        let mut constant_target_cache: HashMap<ConstLookupId, Option<FqnId>> = HashMap::new();
        let mut method_lookup_chain_cache = MethodLookupChainCache::new();
        let mut method_absence_claims = MethodAbsenceClaims::new(self);
        let mut resolved_call_outcomes = HashMap::new();
        let mut call_outcome_caches = MethodCallOutcomeCaches::default();
        self.uses.clear_resolved();
        let candidate_loop_started = Instant::now();
        for candidate in reference_candidate_store.iter_candidates() {
            match candidate {
                StoredReferenceCandidateRef::Resolved(candidate) => {
                    self.uses.add_resolved(
                        candidate.target,
                        ReferenceFact::new(candidate.range, candidate.caller()),
                    );
                }
                StoredReferenceCandidateRef::Constant(candidate) => {
                    let lookup = self.names.const_lookup(candidate.lookup).expect_invariant(
                        "reference candidate points to missing constant lookup",
                        "stored reference candidates must only contain interned lookup ids",
                        "intern constant lookups before inserting candidates",
                    );
                    let parts = lookup.path.to_vec();
                    let context = self.names.fqn(lookup.context).expect_invariant(
                        "constant lookup points to missing context FQN id",
                        "constant lookups must only store interned context FQN ids",
                        "intern lookup contexts before inserting candidates",
                    );
                    let target = if let Some(target) = constant_target_cache.get(&candidate.lookup)
                    {
                        stats.increment(ResolveStat::ConstantCacheHits);
                        *target
                    } else {
                        stats.increment(ResolveStat::ConstantCacheMisses);
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
                        self.uses
                            .add_resolved(target, ReferenceFact::new(candidate.range, None));
                    } else {
                        unresolved_constants
                            .entry(candidate.range.file_id)
                            .or_default()
                            .push(UNRESOLVED_CONSTANT.fact(
                                candidate.range,
                                format!("Unresolved constant `{}`", constant_name(&parts)),
                            ));
                    }
                }
                StoredReferenceCandidateRef::Method(candidate) => {
                    let (method, access, caller) =
                        (candidate.method(), candidate.access(), candidate.caller());
                    let call_expression_range = candidate.call_expression_range();
                    let diagnostics = candidate.diagnostics();
                    let deferred_receiver_range =
                        diagnostics.and_then(|diagnostics| diagnostics.receiver_expression_range());
                    let solved_receiver_type = deferred_receiver_range.and_then(|range| {
                        self.proven_deferred_receiver_type(range, &resolved_call_outcomes)
                    });
                    let receiver_is_explicitly_unknown =
                        deferred_receiver_range.is_some_and(|range| {
                            self.deferred_receiver_is_unknown(range, &resolved_call_outcomes)
                        });
                    let candidate_receiver_type = diagnostics
                        .and_then(|diagnostics| self.names.method_receiver_type(diagnostics));
                    let effective_receiver_type = solved_receiver_type.or_else(|| {
                        (!receiver_is_explicitly_unknown)
                            .then_some(candidate_receiver_type)
                            .flatten()
                    });
                    if deferred_receiver_range.is_some() {
                        stats.increment(ResolveStat::DeferredReceiverCandidates);
                        if effective_receiver_type.is_some() {
                            stats.increment(ResolveStat::DeferredReceiverProven);
                        } else {
                            stats.increment(ResolveStat::DeferredReceiverUnknown);
                        }
                    }
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
                    let safe_navigation =
                        diagnostics.is_some_and(|diagnostics| diagnostics.safe_navigation());
                    let Some((effective_receiver_type, nil_skips_dispatch)) =
                        Self::safe_navigation_receiver(
                            &mut resolved_call_outcomes,
                            call_expression_range,
                            safe_navigation,
                            effective_receiver_type,
                        )
                    else {
                        continue;
                    };
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
                            let targets = grouped_method_targets(&callees, method);
                            for target in targets {
                                let target = self.names.intern_fqn(target);
                                self.uses.add_resolved(
                                    target,
                                    ReferenceFact::method(candidate.range, caller, access),
                                );
                            }
                            if let Some(diagnostics) = diagnostics {
                                self.push_grouped_method_fact_diagnostics(
                                    &callees,
                                    method,
                                    diagnostics,
                                    &mut unresolved_constants,
                                );
                            }
                            if let Some(expression_range) = call_expression_range {
                                Self::insert_dispatched_call_outcome(
                                    &mut resolved_call_outcomes,
                                    expression_range,
                                    self.call_expression_outcome_from_grouped_resolution(
                                        &callees,
                                        method,
                                        &mut call_outcome_caches,
                                    ),
                                    nil_skips_dispatch,
                                );
                            }
                        } else if let Some(diagnostics) = diagnostics {
                            self.push_grouped_unresolved_method_diagnostic(
                                receiver_type,
                                method,
                                diagnostics,
                                &mut method_absence_claims,
                                &mut unresolved_constants,
                            );
                            if let Some(expression_range) = call_expression_range {
                                Self::insert_dispatched_call_outcome(
                                    &mut resolved_call_outcomes,
                                    expression_range,
                                    TypeInferenceOutcome::unknown(
                                        UnknownReason::UnresolvedMethodReturn,
                                    ),
                                    nil_skips_dispatch,
                                );
                            }
                        }
                        continue;
                    }
                    let (owner, owner_kind) = if let Some(receiver_type) =
                        effective_receiver_type.as_ref()
                    {
                        let allow_unindexed_owner = diagnostics
                            .is_some_and(|diagnostics| diagnostics.allow_unindexed_owner());
                        let Some(owner_fqn) =
                            self.proven_receiver_namespace(receiver_type, allow_unindexed_owner)
                        else {
                            if let Some(expression_range) = call_expression_range {
                                Self::insert_dispatched_call_outcome(
                                    &mut resolved_call_outcomes,
                                    expression_range,
                                    TypeInferenceOutcome::unknown(UnknownReason::UnknownReceiver),
                                    nil_skips_dispatch,
                                );
                            }
                            continue;
                        };
                        let owner_kind = owner_fqn.namespace_kind().expect_invariant(
                            "a proven receiver namespace has no namespace kind",
                            "type-to-namespace conversion must return a Namespace FQN",
                            "keep receiver proof conversion in View::type_to_namespace",
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
                        (candidate.owner(), candidate.owner_kind())
                    };
                    let owner_fqn = method_reference_owner_fqn(self, owner, owner_kind);
                    let method_cache_key = (owner, owner_kind, method, candidate.is_super());
                    let reflects_instance =
                        access == MethodReferenceAccess::InstanceMethodReflection;
                    let fact_cache_key = (method_cache_key, reflects_instance);
                    let cached = method_fact_cache.contains_key(&fact_cache_key);
                    let fact = method_fact_cache.entry(fact_cache_key).or_insert_with(|| {
                        let query = View::new(self);
                        if candidate.is_super() {
                            query.resolve_super_method_reference(&owner_fqn, &method)
                        } else if reflects_instance {
                            query.resolve_instance_method_reference_with_chain_cache(
                                &owner_fqn,
                                &method,
                                &mut method_lookup_chain_cache,
                            )
                        } else {
                            query.resolve_method_reference_with_chain_cache(
                                &owner_fqn,
                                &method,
                                &mut method_lookup_chain_cache,
                            )
                        }
                    });
                    if cached {
                        stats.increment(ResolveStat::MethodCacheHits);
                    } else {
                        stats.increment(ResolveStat::MethodCacheMisses);
                    }
                    let mut fact = fact.clone();
                    if access == MethodReferenceAccess::Normal
                        && matches!(fact, MethodLookupResult::Ambiguous { .. })
                    {
                        if let Some(source_ordered) = View::new(self)
                            .source_ordered_top_level_method_reference(
                                &owner_fqn,
                                &method,
                                candidate.range,
                            )
                        {
                            fact = MethodLookupResult::Found(source_ordered);
                        }
                    }
                    if let Some(expression_range) = call_expression_range {
                        let outcome = self.call_expression_outcome_from_method_resolution(
                            method_cache_key,
                            access,
                            caller,
                            &method_lookup_chain_cache,
                            &fact,
                            &mut call_outcome_caches,
                        );
                        Self::insert_dispatched_call_outcome(
                            &mut resolved_call_outcomes,
                            expression_range,
                            outcome,
                            nil_skips_dispatch,
                        );
                    }
                    if let Some((owner, resolved_method, fact)) = fact.reference_parts() {
                        let target =
                            FullyQualifiedName::method(owner.namespace_parts(), resolved_method);
                        let target = self.names.intern_fqn(target);
                        self.uses.add_resolved(
                            target,
                            ReferenceFact::method(candidate.range, caller, access),
                        );
                        if resolved_method == method {
                            if let Some(diagnostics) = diagnostics {
                                if let Some(fact) = fact {
                                    self.push_unavailable_method_diagnostic(
                                        fact,
                                        &method,
                                        diagnostics.diagnostic_range(),
                                        &mut unresolved_constants,
                                    );
                                    self.push_signature_diagnostics(
                                        fact,
                                        &owner_fqn,
                                        &method,
                                        diagnostics,
                                        &mut unresolved_constants,
                                    );
                                }
                            }
                        }
                    } else if fact.has_no_target() {
                        let namespace_exists = *method_namespace_exists_cache
                            .entry(owner_fqn.clone())
                            .or_insert_with_key(|owner_fqn| {
                                self.method_namespace_target_exists(owner_fqn)
                            });
                        let allow_unindexed_owner = diagnostics
                            .is_some_and(|diagnostics| diagnostics.allow_unindexed_owner());
                        if !namespace_exists && !allow_unindexed_owner {
                            continue;
                        }
                        let target =
                            FullyQualifiedName::method(owner_fqn.namespace_parts(), method);
                        let target = self.names.intern_fqn(target);
                        self.uses.add_resolved(
                            target,
                            ReferenceFact::method(candidate.range, caller, access),
                        );

                        if let Some(diagnostics) = diagnostics {
                            if !diagnostics.diagnose_unresolved() {
                                continue;
                            }
                            if method_absence_claims.suppresses(self, &owner_fqn, method) {
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
                            let mut message = match self.names.method_receiver_label(diagnostics) {
                                Some(label) => format!(
                                    "Unresolved method `{}` on `{}`",
                                    method.as_str(),
                                    label
                                ),
                                None => {
                                    format!("Unresolved method `{}`", method.as_str())
                                }
                            };
                            if let Some(suggestion) = suggestion {
                                message.push_str(&format!(". Did you mean `{}`?", suggestion));
                            }
                            unresolved_constants
                                .entry(diagnostics.diagnostic_range().file_id)
                                .or_default()
                                .push(
                                    UNRESOLVED_METHOD.fact(diagnostics.diagnostic_range(), message),
                                );
                        }
                    }
                }
            }
        }
        // method_candidates_ns holds the full candidate-loop duration after the
        // one-shot per-arm Instant split was removed (it inflated production A/B).
        // The detailed constant-vs-method split remains in
        // support/performance/resolve-pass-cache-cardinality-2026-08-01.json.
        stats.record_duration(
            ResolveStat::MethodCandidatesNs,
            candidate_loop_started.elapsed(),
        );
        stats.set(
            ResolveStat::ConstantCacheUniqueKeys,
            stats::count(constant_target_cache.len()),
        );
        stats.set(
            ResolveStat::MethodCacheUniqueKeys,
            stats::count(method_fact_cache.len()),
        );
        stats.set(
            ResolveStat::MethodLookupChainCacheEntries,
            stats::count(method_lookup_chain_cache.len()),
        );
        stats.set(
            ResolveStat::MethodNamespaceExistsCacheEntries,
            stats::count(method_namespace_exists_cache.len()),
        );
        stats.set(
            ResolveStat::MethodSuggestionCacheEntries,
            stats::count(method_suggestion_cache.len()),
        );
        stats.set(
            ResolveStat::IncompleteMethodChainCacheEntries,
            stats::count(method_absence_claims.decided_owner_count()),
        );
        stats.set(
            ResolveStat::MethodReturnCacheHits,
            stats::count(call_outcome_caches.return_hits),
        );
        stats.set(
            ResolveStat::MethodReturnCacheMisses,
            stats::count(call_outcome_caches.return_misses),
        );
        stats.set(
            ResolveStat::MethodReturnCacheEntries,
            stats::count(call_outcome_caches.returns.len()),
        );
        stats.set(
            ResolveStat::MethodVisibilityCacheHits,
            stats::count(call_outcome_caches.visibility_hits),
        );
        stats.set(
            ResolveStat::MethodVisibilityCacheMisses,
            stats::count(call_outcome_caches.visibility_misses),
        );
        stats.set(
            ResolveStat::MethodVisibilityCacheEntries,
            stats::count(call_outcome_caches.visibilities.len()),
        );
        stats.set(
            ResolveStat::AmbiguousMethodReturnCacheHits,
            stats::count(call_outcome_caches.ambiguous_return_hits),
        );
        stats.set(
            ResolveStat::AmbiguousMethodReturnCacheMisses,
            stats::count(call_outcome_caches.ambiguous_return_misses),
        );
        stats.set(
            ResolveStat::AmbiguousMethodReturnCacheEntries,
            stats::count(call_outcome_caches.ambiguous_returns.len()),
        );

        // These caches are complete once the candidate loop ends. Release
        // them before merging call outcomes into retained inference evidence;
        // keeping both phases alive caused a large, unnecessary resolve peak.
        drop(method_fact_cache);
        drop(method_namespace_exists_cache);
        drop(method_suggestion_cache);
        drop(constant_target_cache);
        drop(method_lookup_chain_cache);
        drop(method_absence_claims);
        drop(call_outcome_caches);

        self.uses.restore_candidates(reference_candidate_store);
        self.replace_resolved_call_expression_outcomes(resolved_call_outcomes);
        let sort_started = Instant::now();
        self.uses.sort_resolved();
        stats.record_duration(ResolveStat::SortAllNs, sort_started.elapsed());

        let diagnostic_rebuild_started = Instant::now();
        for file_id in candidate_file_ids {
            self.diagnostics.rebuild_resolved(
                file_id,
                unresolved_constants.remove(&file_id).unwrap_or_default(),
            );
        }
        stats.record_duration(
            ResolveStat::DiagnosticRebuildNs,
            diagnostic_rebuild_started.elapsed(),
        );
    }
}

fn method_reference_owner_fqn(
    engine: &Project,
    owner: ConstLookupId,
    owner_kind: NamespaceKind,
) -> FullyQualifiedName {
    let owner_lookup = engine.names.const_lookup(owner).expect_invariant(
        "method reference candidate points to missing owner lookup",
        "stored reference candidates must only contain interned lookup ids",
        "intern constant lookups before inserting candidates",
    );
    FullyQualifiedName::namespace_with_kind(owner_lookup.path.to_vec(), owner_kind)
}
