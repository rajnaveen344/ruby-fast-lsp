//! Resolution of method references and their navigation targets.

use crate::invariant::ExpectInvariant;
use std::sync::Arc;

use super::chain_methods::{
    default_basic_object_method_missing_fact, execution_context_application_targets,
    method_callee_in_chain, method_facts_in_chain, method_missing_callee_in_chain,
    method_missing_method,
};
use super::lookup_chain::{
    interned_universal_open_root_ids, metaclass_namespace_for_object, method_lookup_chain,
    method_lookup_chain_for_reference_cached, method_lookup_chain_has_unresolved_dependency_cached,
    method_lookup_chain_has_unresolved_dependency_from_graph,
    method_lookup_chain_without_metaclass, unproven_universal_method_exists,
};
use super::{
    module_instance_receivers, namespace_target_exists, MethodLookupChainCache, MethodLookupResult,
};
use crate::core::{
    FullyQualifiedName, MethodCalleeResolution, MethodFact, RubyMethod, SourceKind, TextRange,
};
use crate::engine::lookup::{LookupUnknown, MethodAnswer};
use crate::engine::queries::View;
use crate::engine::state::EffectiveMethodFactMatch;

impl<'a> View<'a> {
    pub fn resolve_method_reference(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> MethodLookupResult {
        let mut cache = MethodLookupChainCache::new();
        self.resolve_method_reference_with_chain_cache(namespace_fqn, method, &mut cache)
    }

    /// Resolve a top-level method only when this exact source location proves
    /// it has already executed in the same file and no receiver-specific,
    /// metaclass, execution-context, or unresolved-ancestry candidate can
    /// precede it. This is the narrow proof needed for Ruby class-body forms
    /// such as `attr_accessor helper_call`; arbitrary workspace indexing is
    /// never treated as runtime load evidence.
    pub(crate) fn source_ordered_top_level_method_reference(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
        call_range: TextRange,
    ) -> Option<Arc<MethodFact>> {
        if namespace_fqn.namespace_parts().is_empty()
            || method_lookup_chain_has_unresolved_dependency_from_graph(self.engine, namespace_fqn)
        {
            return None;
        }

        let receiver_chain = method_lookup_chain_without_metaclass(self.engine, namespace_fqn);
        if method_facts_in_chain(self.engine, &receiver_chain, method, true, None).is_some() {
            return None;
        }
        if let Some(metaclass) = metaclass_namespace_for_object(self.engine, namespace_fqn) {
            let metaclass_chain = method_lookup_chain_without_metaclass(self.engine, &metaclass);
            if method_facts_in_chain(self.engine, &metaclass_chain, method, true, None).is_some() {
                return None;
            }
        }
        if execution_context_application_targets(self.engine, namespace_fqn)
            .into_iter()
            .any(|application| {
                let chain = method_lookup_chain_without_metaclass(self.engine, &application);
                method_facts_in_chain(self.engine, &chain, method, true, None).is_some()
            })
        {
            return None;
        }

        let root = FullyQualifiedName::namespace_with_kind(
            Vec::new(),
            crate::core::NamespaceKind::Instance,
        );
        let EffectiveMethodFactMatch::Found(fact) = self
            .engine
            .effective_method_fact_matching_owner_name(&root, method)
        else {
            return None;
        };
        (fact.range.file_id == call_range.file_id && fact.range.end_byte <= call_range.start_byte)
            .then(|| Arc::new(fact))
    }

    pub(crate) fn resolve_method_reference_with_chain_cache(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
        chain_cache: &mut MethodLookupChainCache,
    ) -> MethodLookupResult {
        self.resolve_method_reference_in_context(namespace_fqn, method, chain_cache, true)
    }

    pub(crate) fn resolve_instance_method_reference_with_chain_cache(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
        chain_cache: &mut MethodLookupChainCache,
    ) -> MethodLookupResult {
        self.resolve_method_reference_in_context(namespace_fqn, method, chain_cache, false)
    }

    fn resolve_method_reference_in_context(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
        chain_cache: &mut MethodLookupChainCache,
        dispatch_to_includers: bool,
    ) -> MethodLookupResult {
        if !namespace_target_exists(self.engine, namespace_fqn) {
            return MethodLookupResult::Unknown(LookupUnknown::Receiver);
        }

        if dispatch_to_includers {
            let includers = chain_cache
                .module_receivers
                .entry(namespace_fqn.clone())
                .or_insert_with(|| module_instance_receivers(self.engine, namespace_fqn).into())
                .clone();
            if !includers.is_empty() {
                let mut facts = Vec::new();
                let mut incomplete = false;
                let mut unknown = None;
                for includer in includers.iter() {
                    match self.resolve_method_reference_with_chain_cache(
                        includer,
                        method,
                        chain_cache,
                    ) {
                        MethodLookupResult::Found(fact) => facts.push(fact),
                        MethodLookupResult::Missing => incomplete = true,
                        MethodLookupResult::Unknown(reason) => {
                            incomplete = true;
                            unknown.get_or_insert(reason);
                        }
                        MethodLookupResult::Ambiguous { .. } => {
                            return MethodLookupResult::Ambiguous {
                                owner: namespace_fqn.clone(),
                                method: *method,
                            };
                        }
                    }
                }
                facts.sort_by_key(|fact| (fact.owner.clone(), fact.range));
                facts.dedup();
                return match facts.as_slice() {
                    [] => absent(unknown),
                    [fact] if !incomplete => MethodLookupResult::Found(fact.clone()),
                    [_] | [_, _, ..] => MethodLookupResult::Ambiguous {
                        owner: namespace_fqn.clone(),
                        method: *method,
                    },
                };
            }
        }

        let universal_root_ids = interned_universal_open_root_ids(self.engine, chain_cache);
        let ancestor_chain =
            method_lookup_chain_for_reference_cached(self.engine, namespace_fqn, chain_cache);
        let universal_roots = ancestor_chain
            .iter()
            .copied()
            .filter(|owner| universal_root_ids.contains(owner))
            .collect::<Vec<_>>();
        let mut crossed_universal_root = false;

        for owner_id in ancestor_chain.iter().copied() {
            crossed_universal_root |= universal_root_ids.contains(&owner_id);
            match self
                .engine
                .effective_method_fact_matching_owner_id(owner_id, method)
            {
                EffectiveMethodFactMatch::Missing => continue,
                EffectiveMethodFactMatch::Found(fact) => {
                    if non_core_fact_requires_ancestry_proof(
                        self.engine,
                        namespace_fqn,
                        &fact,
                        crossed_universal_root,
                    ) {
                        return MethodLookupResult::Ambiguous {
                            owner: namespace_fqn.clone(),
                            method: *method,
                        };
                    }
                    return MethodLookupResult::Found(Arc::new(fact));
                }
                EffectiveMethodFactMatch::Ambiguous { owner, method } => {
                    return MethodLookupResult::Ambiguous { owner, method };
                }
            }
        }

        // An unresolved ancestry edge is a proof barrier. A method found on
        // the receiver or on a fully traversed known prepend/include remains
        // usable because it returned above, but lookup must not continue into
        // Class/Module or top-level fallbacks: the missing ancestor can define
        // the same method earlier in Ruby's lookup order.
        if method_lookup_chain_has_unresolved_dependency_cached(
            self.engine,
            namespace_fqn,
            chain_cache,
        ) {
            return MethodLookupResult::Unknown(LookupUnknown::IncompleteChain);
        }

        // Class and module objects share one language-defined fallback through
        // Class/Module. Keep that continuation lazy for reference resolution:
        // most singleton calls resolve in their own ancestry, and appending the
        // same metaclass chain to every cached owner multiplied transient work
        // on large projects without adding proof.
        if let Some(metaclass) = metaclass_namespace_for_object(self.engine, namespace_fqn) {
            let metaclass_id = self.engine.names.fqn_id(&metaclass).expect_invariant(
                "proven metaclass namespace is absent from the name registry",
                "metaclass fallback requires an indexed graph node",
                "intern graph node FQNs before method resolution",
            );
            let cache_key = (metaclass_id, *method);
            let fallback = if let Some(cached) = chain_cache.metaclass_methods.get(&cache_key) {
                cached.clone()
            } else {
                let metaclass_chain =
                    method_lookup_chain_for_reference_cached(self.engine, &metaclass, chain_cache);
                let mut result = MethodLookupResult::Missing;
                for owner_id in metaclass_chain.iter().copied() {
                    match self
                        .engine
                        .effective_method_fact_matching_owner_id(owner_id, method)
                    {
                        EffectiveMethodFactMatch::Missing => continue,
                        EffectiveMethodFactMatch::Found(fact) => {
                            if non_core_fact_requires_ancestry_proof(
                                self.engine,
                                namespace_fqn,
                                &fact,
                                true,
                            ) {
                                result = MethodLookupResult::Ambiguous {
                                    owner: metaclass.clone(),
                                    method: *method,
                                };
                                break;
                            }
                            result = MethodLookupResult::Found(Arc::new(fact));
                            break;
                        }
                        EffectiveMethodFactMatch::Ambiguous { owner, method } => {
                            result = MethodLookupResult::Ambiguous { owner, method };
                            break;
                        }
                    }
                }
                invariant!(
                    chain_cache
                        .metaclass_methods
                        .insert(cache_key, result.clone())
                        .is_none(),
                    what = "metaclass fallback cache replaced an entry after a confirmed miss",
                    why =
                        "metaclass identity and method names are immutable during one resolve pass",
                    fix = "keep fallback lookup and insertion in one resolution step",
                );
                result
            };
            match fallback {
                MethodLookupResult::Missing | MethodLookupResult::Unknown(_) => {}
                MethodLookupResult::Found(_) | MethodLookupResult::Ambiguous { .. } => {
                    return fallback;
                }
            }
        }

        let mut application_facts = Vec::new();
        let mut application_ambiguous = false;
        let mut application_unknown = None;
        for application in execution_context_application_targets(self.engine, namespace_fqn) {
            match self.resolve_method_reference_with_chain_cache(&application, method, chain_cache)
            {
                MethodLookupResult::Found(fact) => application_facts.push(fact),
                MethodLookupResult::Ambiguous { .. } => application_ambiguous = true,
                MethodLookupResult::Missing => {}
                MethodLookupResult::Unknown(reason) => {
                    application_unknown.get_or_insert(reason);
                }
            }
        }
        application_facts.sort_by_key(|fact| {
            (
                fact.range.file_id,
                fact.range.start_byte,
                fact.range.end_byte,
                fact.fqn.to_string(),
            )
        });
        application_facts.dedup();
        if application_ambiguous || application_facts.len() > 1 {
            return MethodLookupResult::Ambiguous {
                owner: namespace_fqn.clone(),
                method: *method,
            };
        }
        if let Some(fact) = application_facts.pop() {
            return MethodLookupResult::Found(fact);
        }

        if unproven_universal_method_exists(self.engine, &universal_roots, method, chain_cache) {
            return MethodLookupResult::Ambiguous {
                owner: namespace_fqn.clone(),
                method: *method,
            };
        }

        if *method != method_missing_method() {
            let fallback = self.resolve_method_reference_with_chain_cache(
                namespace_fqn,
                &method_missing_method(),
                chain_cache,
            );
            if matches!(
                &fallback,
                MethodLookupResult::Found(fact)
                    if default_basic_object_method_missing_fact(self.engine, fact)
            ) {
                return absent(application_unknown);
            }
            return match fallback {
                MethodLookupResult::Missing => absent(application_unknown),
                MethodLookupResult::Found(_)
                | MethodLookupResult::Ambiguous { .. }
                | MethodLookupResult::Unknown(_) => fallback,
            };
        }

        absent(application_unknown)
    }

    pub(crate) fn resolve_super_method_reference(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> MethodLookupResult {
        let Some(callee) = self.resolve_super_method_callee(namespace_fqn, method) else {
            return MethodLookupResult::Missing;
        };
        match self
            .engine
            .effective_method_fact_matching_owner_name(&callee.owner, method)
        {
            EffectiveMethodFactMatch::Missing => MethodLookupResult::Missing,
            EffectiveMethodFactMatch::Found(fact) => MethodLookupResult::Found(Arc::new(fact)),
            EffectiveMethodFactMatch::Ambiguous { owner, method } => {
                MethodLookupResult::Ambiguous { owner, method }
            }
        }
    }

    pub fn method_reference_targets(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Vec<FullyQualifiedName> {
        if !namespace_target_exists(self.engine, namespace_fqn) {
            return Vec::new();
        }

        let mut targets = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let ancestor_chain = method_lookup_chain(self.engine, namespace_fqn);
        let base_has_exact = method_callee_in_chain(
            self.engine,
            &ancestor_chain,
            method,
            MethodCalleeResolution::Exact,
            true,
            None,
        )
        .is_some();
        let mut lookup_chains = vec![ancestor_chain];
        if !base_has_exact {
            lookup_chains.extend(
                execution_context_application_targets(self.engine, namespace_fqn)
                    .into_iter()
                    .map(|application| method_lookup_chain(self.engine, &application)),
            );
        }
        let has_exact = lookup_chains.iter().any(|chain| {
            method_callee_in_chain(
                self.engine,
                chain,
                method,
                MethodCalleeResolution::Exact,
                true,
                None,
            )
            .is_some()
        });

        if !has_exact {
            for chain in &lookup_chains {
                if let Some(callee) = method_missing_callee_in_chain(self.engine, chain) {
                    let method_fqn =
                        FullyQualifiedName::method(callee.owner.namespace_parts(), callee.method);
                    if seen.insert(method_fqn.clone()) {
                        targets.push(method_fqn);
                    }
                }
            }
            if !targets.is_empty() {
                return targets;
            }
        }

        for ancestor_chain in &lookup_chains {
            for ancestor in ancestor_chain {
                let has_method_fact = !self
                    .method_facts_matching_owner_name(ancestor, method)
                    .is_empty();
                if ancestor != namespace_fqn
                    && ancestor.namespace_parts().is_empty()
                    && !has_method_fact
                {
                    continue;
                }

                let method_fqn = FullyQualifiedName::method(ancestor.namespace_parts(), *method);
                if seen.insert(method_fqn.clone()) {
                    targets.push(method_fqn);
                }
            }
        }
        for override_fact in self.method_visibility_overrides_named(*method) {
            if !lookup_chains.iter().any(|chain| {
                chain.iter().any(|ancestor| {
                    ancestor.namespace_parts() == override_fact.owner.namespace_parts()
                        && ancestor.namespace_kind() == override_fact.owner.namespace_kind()
                })
            }) {
                continue;
            }
            let method_fqn =
                FullyQualifiedName::method(override_fact.owner.namespace_parts(), *method);
            if seen.insert(method_fqn.clone()) {
                targets.push(method_fqn);
            }
        }
        targets
    }

    pub fn super_method_reference_target(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Option<FullyQualifiedName> {
        self.resolve_super_method_callee(namespace_fqn, method)
            .map(|callee| FullyQualifiedName::method(callee.owner.namespace_parts(), *method))
    }
}

/// No reference was selected: proven missing unless an edge consulted on the
/// way was unknown.
fn absent(unknown: Option<LookupUnknown>) -> MethodLookupResult {
    match unknown {
        Some(reason) => MethodAnswer::Unknown(reason),
        None => MethodAnswer::Missing,
    }
}

fn non_core_fact_requires_ancestry_proof(
    engine: &crate::engine::Project,
    requested_owner: &FullyQualifiedName,
    fact: &MethodFact,
    crossed_universal_root: bool,
) -> bool {
    if requested_owner == &fact.owner {
        return false;
    }

    let source_kind = engine
        .view()
        .file(fact.range.file_id)
        .unwrap_or_else(|| {
            unreachable_invariant!(
                what = "method fact `{}` references missing source file {}",
                why = "every fact must remain owned by a registered file",
                fix = "remove facts before unregistering their source",
                fact.fqn,
                fact.range.file_id.0,
            )
        })
        .kind;
    if matches!(source_kind, SourceKind::Stub | SourceKind::Signature) {
        return false;
    }

    crossed_universal_root
}
