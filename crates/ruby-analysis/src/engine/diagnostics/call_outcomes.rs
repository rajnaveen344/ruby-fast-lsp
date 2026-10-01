//! Call-expression outcomes: deferred receiver proof and cached return-type and
//! visibility derivation for resolved method calls.

use std::collections::HashMap;

use super::{
    AmbiguousMethodReturnAccess, CachedMethodVisibility, MethodCallOutcomeCaches,
    MethodReferenceCacheKey,
};
use crate::core::names::fqn_id::FqnId;
use crate::core::MethodVisibility;
use crate::core::{
    FullyQualifiedName, GraphNodeKind, MethodFact, MethodReferenceAccess, NamespaceKind,
    ResolvedMethodCallee, RubyMethod, RubyType, TextRange, TypeInferenceOutcome, UnknownReason,
};
use crate::engine::resolution::{
    effective_method_visibility_for_chain, method_lookup_chain, protected_method_visible_from,
    MethodLookupChainCache, MethodLookupResult,
};
use crate::engine::state::TypeInferenceOutcomeRef;
use crate::engine::{AnalysisEngine, AnalysisQuery};

impl AnalysisEngine {
    pub(super) fn proven_deferred_receiver_type(
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

    pub(super) fn deferred_receiver_is_unknown(
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

    pub(super) fn proven_receiver_namespace(
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
            | RubyType::Hash(_, _)
            | RubyType::Literal(_)
            | RubyType::Shape(_) => GraphNodeKind::Class,
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

    pub(super) fn insert_resolved_call_outcome(
        outcomes: &mut HashMap<TextRange, TypeInferenceOutcome>,
        range: TextRange,
        outcome: TypeInferenceOutcome,
    ) {
        assert!(
            outcomes.insert(range, outcome).is_none(),
            "INVARIANT VIOLATED: one call expression resolved through multiple method candidates. This is a bug because one runtime dispatch must have one proof outcome. Fix: attach the call range only to the candidate representing the invoked method."
        );
    }

    /// Split the receiver of a `&.` dispatch before resolution.
    ///
    /// Returns `None` when the receiver is only nil: no message is sent, so
    /// the call is recorded as proven nil and must be neither resolved nor
    /// diagnosed. Otherwise returns the receiver to dispatch on and whether a
    /// nil branch skips dispatch, which `insert_dispatched_call_outcome` adds
    /// to the call result.
    pub(super) fn safe_navigation_receiver(
        outcomes: &mut HashMap<TextRange, TypeInferenceOutcome>,
        call_expression_range: Option<TextRange>,
        safe_navigation: bool,
        receiver_type: Option<RubyType>,
    ) -> Option<(Option<RubyType>, bool)> {
        match receiver_type {
            Some(ruby_type) if safe_navigation => match ruby_type.safe_navigation_dispatch() {
                Some((receiver, nil_skips_dispatch)) => Some((Some(receiver), nil_skips_dispatch)),
                None => {
                    if let Some(range) = call_expression_range {
                        Self::insert_resolved_call_outcome(
                            outcomes,
                            range,
                            TypeInferenceOutcome::proven(RubyType::nil_class()),
                        );
                    }
                    None
                }
            },
            receiver_type => Some((receiver_type, false)),
        }
    }

    /// Record a dispatched call's outcome. A safe-navigation call whose
    /// receiver may be nil also yields nil, so `a&.b.c` dispatches `c` on that
    /// nil alternative exactly as Ruby does.
    pub(super) fn insert_dispatched_call_outcome(
        outcomes: &mut HashMap<TextRange, TypeInferenceOutcome>,
        range: TextRange,
        outcome: TypeInferenceOutcome,
        nil_skips_dispatch: bool,
    ) {
        let outcome = if nil_skips_dispatch {
            outcome.with_nil_alternative()
        } else {
            outcome
        };
        Self::insert_resolved_call_outcome(outcomes, range, outcome);
    }

    pub(super) fn call_expression_outcome_from_grouped_resolution(
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

    pub(super) fn call_expression_outcome_from_method_resolution(
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
                MethodReferenceAccess::Normal | MethodReferenceAccess::VisibilityBypass | MethodReferenceAccess::InstanceMethodReflection,
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
                MethodReferenceAccess::Normal | MethodReferenceAccess::VisibilityBypass | MethodReferenceAccess::InstanceMethodReflection,
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
                | MethodReferenceAccess::VisibilityBypass
                | MethodReferenceAccess::InstanceMethodReflection,
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

    pub(super) fn call_expression_caller_namespace(
        &self,
        caller: FqnId,
    ) -> Option<FullyQualifiedName> {
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
}
