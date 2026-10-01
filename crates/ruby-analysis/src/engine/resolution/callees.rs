//! Method callee resolution through lookup chains, including candidate callees.

use super::chain_methods::{
    execution_context_application_targets, method_callee_after_owner, method_callee_in_chain,
    method_missing_callee_in_chain, private_method_in_chain, receiver_only_callee,
};
use super::lookup_chain::{
    method_lookup_chain, method_lookup_chain_has_unresolved_dependency_from_graph,
};
use super::{module_instance_receivers, namespace_target_exists, receiver_type_members};
use crate::core::storage::reference_store::StoredMethodReferenceCandidate;
use crate::core::{
    FullyQualifiedName, MethodCalleeResolution, MethodReferenceAccess, ResolvedMethodCallee,
    RubyMethod, RubyType,
};
use crate::engine::queries::cache::{AnalysisQueryCache, MethodReturnQueryAccess};
use crate::engine::queries::definitions::DefinitionLookupChains;
use crate::engine::queries::AnalysisQuery;

impl<'a> AnalysisQuery<'a> {
    pub(in crate::engine) fn method_candidate_callees(
        &self,
        candidate: &StoredMethodReferenceCandidate,
    ) -> Vec<ResolvedMethodCallee> {
        self.method_candidate_callees_with_navigation(candidate, None)
    }

    pub(in crate::engine) fn method_candidate_callees_with_navigation(
        &self,
        candidate: &StoredMethodReferenceCandidate,
        mut lookup_chains: Option<&mut DefinitionLookupChains>,
    ) -> Vec<ResolvedMethodCallee> {
        let owner_lookup = self.engine.names.const_lookup(candidate.owner).expect(
            "INVARIANT VIOLATED: method rename candidate points to a missing owner lookup. This is a bug because candidates contain only interned lookup ids. Fix: intern method owners before storing candidates.",
        );
        let owner = FullyQualifiedName::namespace_with_kind(
            owner_lookup.path.to_vec(),
            candidate.owner_kind,
        );
        let grouped_receiver_type = candidate
            .diagnostics
            .as_deref()
            .and_then(|diagnostics| diagnostics.receiver_type.as_deref())
            .filter(|receiver_type| matches!(receiver_type, RubyType::Union(_)));
        assert!(
            grouped_receiver_type.is_none() || !candidate.is_super,
            "INVARIANT VIOLATED: a super reference carries grouped receiver metadata. This is a bug because super has one lexical owner chain rather than a value receiver union. Fix: attach receiver_type only to explicit call-node receiver inference."
        );
        let callees = if let Some(receiver_type) = grouped_receiver_type {
            match candidate.access {
                MethodReferenceAccess::InstanceMethodReflection => panic!(
                    "INVARIANT VIOLATED: instance-method reflection has a grouped value receiver. This is a bug because reflection names one namespace. Fix: collect reflection separately from value dispatch."
                ),
                MethodReferenceAccess::Normal | MethodReferenceAccess::VisibilityBypass => self
                    .resolve_method_callees_for_type_inner(receiver_type, &candidate.method, true, None, lookup_chains.as_deref_mut())
                    .unwrap_or_default(),
                MethodReferenceAccess::ExplicitReceiver => {
                    let protected = candidate
                        .caller
                        .and_then(|caller| self.engine.fqn_for_id(caller))
                        .and_then(|caller| {
                            let mut owners = self
                                .engine
                                .method_facts_for(caller)
                                .into_iter()
                                .map(|fact| fact.owner)
                                .collect::<Vec<_>>();
                            owners.sort_by_key(ToString::to_string);
                            owners.dedup();
                            let caller = if owners.len() == 1 {
                                owners.pop().expect(
                                    "INVARIANT VIOLATED: one grouped rename caller owner disappeared after length validation. This is a bug because caller selection must be atomic. Fix: keep the local owner vector unchanged before pop.",
                                )
                            } else {
                                FullyQualifiedName::namespace(caller.namespace_parts())
                            };
                            self.resolve_method_callees_for_type_inner(receiver_type, &candidate.method, false, Some(&caller), lookup_chains.as_deref_mut())
                        });
                    protected
                        .or_else(|| {
                            self.resolve_method_callees_for_type_inner(receiver_type, &candidate.method, false, None, lookup_chains.as_deref_mut())
                        })
                        .unwrap_or_default()
                }
            }
        } else if candidate.is_super {
            self.resolve_super_method_callee(&owner, &candidate.method)
                .into_iter()
                .collect::<Vec<_>>()
        } else {
            match candidate.access {
                MethodReferenceAccess::InstanceMethodReflection => {
                    let chain = method_lookup_chain(self.engine, &owner);
                    method_callee_in_chain(
                        self.engine,
                        &chain,
                        &candidate.method,
                        MethodCalleeResolution::Exact,
                        true,
                        None,
                    )
                    .into_iter()
                    .collect()
                }
                MethodReferenceAccess::Normal | MethodReferenceAccess::VisibilityBypass => self
                    .resolve_method_callees_inner(
                        &owner,
                        &candidate.method,
                        true,
                        None,
                        lookup_chains.as_deref_mut(),
                    )
                    .unwrap_or_default(),
                MethodReferenceAccess::ExplicitReceiver => {
                    let protected = candidate
                        .caller
                        .and_then(|caller| self.engine.fqn_for_id(caller))
                        .and_then(|caller| {
                            let mut owners = self
                                .engine
                                .method_facts_for(caller)
                                .into_iter()
                                .map(|fact| fact.owner)
                                .collect::<Vec<_>>();
                            owners.sort_by_key(ToString::to_string);
                            owners.dedup();
                            let caller = if owners.len() == 1 {
                                owners.pop().expect(
                                    "INVARIANT VIOLATED: one method rename caller owner disappeared after length validation. This is a bug because caller selection must be atomic. Fix: keep the local owner vector unchanged before pop.",
                                )
                            } else {
                                FullyQualifiedName::namespace(caller.namespace_parts())
                            };
                            self.resolve_method_callees_inner(&owner, &candidate.method, false, Some(&caller), lookup_chains.as_deref_mut())
                        });
                    protected
                        .or_else(|| {
                            self.resolve_method_callees_inner(
                                &owner,
                                &candidate.method,
                                false,
                                None,
                                lookup_chains.as_deref_mut(),
                            )
                        })
                        .unwrap_or_default()
                }
            }
        };
        if candidate.access == MethodReferenceAccess::Normal
            && !callees.iter().any(|callee| {
                callee.resolution == MethodCalleeResolution::Exact
                    && !callee.definition_ranges.is_empty()
            })
        {
            if let Some(fact) = self.source_ordered_top_level_method_reference(
                &owner,
                &candidate.method,
                candidate.range,
            ) {
                return vec![ResolvedMethodCallee {
                    owner: fact.owner.clone(),
                    method: candidate.method,
                    resolution: MethodCalleeResolution::Exact,
                    definition_ranges: vec![fact.range],
                }];
            }
        }
        callees
    }
}

impl<'a> AnalysisQuery<'a> {
    pub fn resolve_method_callees(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Option<Vec<ResolvedMethodCallee>> {
        self.resolve_method_callees_inner(namespace_fqn, method, true, None, None)
    }

    pub fn resolve_method_callees_cached(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
        cache: &AnalysisQueryCache,
    ) -> Option<Vec<ResolvedMethodCallee>> {
        cache.method_callees(
            self.engine.query_cache_identity(),
            namespace_fqn,
            *method,
            MethodReturnQueryAccess::Private,
            || self.resolve_method_callees(namespace_fqn, method),
        )
    }

    pub fn resolve_public_method_callees(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Option<Vec<ResolvedMethodCallee>> {
        self.resolve_method_callees_inner(namespace_fqn, method, false, None, None)
    }

    pub fn resolve_protected_method_callees(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
        caller_namespace_fqn: &FullyQualifiedName,
    ) -> Option<Vec<ResolvedMethodCallee>> {
        self.resolve_method_callees_inner(
            namespace_fqn,
            method,
            false,
            Some(caller_namespace_fqn),
            None,
        )
    }

    pub fn resolve_method_callees_for_type(
        &self,
        receiver_type: &RubyType,
        method: &RubyMethod,
    ) -> Option<Vec<ResolvedMethodCallee>> {
        self.resolve_method_callees_for_type_inner(receiver_type, method, true, None, None)
    }

    pub fn resolve_public_method_callees_for_type(
        &self,
        receiver_type: &RubyType,
        method: &RubyMethod,
    ) -> Option<Vec<ResolvedMethodCallee>> {
        self.resolve_method_callees_for_type_inner(receiver_type, method, false, None, None)
    }

    pub fn resolve_protected_method_callees_for_type(
        &self,
        receiver_type: &RubyType,
        method: &RubyMethod,
        caller_namespace_fqn: &FullyQualifiedName,
    ) -> Option<Vec<ResolvedMethodCallee>> {
        self.resolve_method_callees_for_type_inner(
            receiver_type,
            method,
            false,
            Some(caller_namespace_fqn),
            None,
        )
    }

    pub(in crate::engine) fn resolve_method_callees_for_type_inner(
        &self,
        receiver_type: &RubyType,
        method: &RubyMethod,
        allow_private: bool,
        protected_caller: Option<&FullyQualifiedName>,
        mut lookup_chains: Option<&mut DefinitionLookupChains>,
    ) -> Option<Vec<ResolvedMethodCallee>> {
        let members = receiver_type_members(receiver_type);
        let mut all_callees = Vec::new();
        for member in members {
            let namespaces = Self::receiver_type_to_method_namespaces(member);
            if namespaces.is_empty() {
                return None;
            }

            let mut member_callees = Vec::new();
            for namespace in namespaces {
                let callees = self.resolve_method_callees_inner(
                    &namespace,
                    method,
                    allow_private,
                    protected_caller,
                    lookup_chains.as_deref_mut(),
                )?;
                member_callees.extend(
                    callees
                        .into_iter()
                        .filter(|callee| callee.resolution == MethodCalleeResolution::Exact),
                );
            }
            if member_callees.is_empty() {
                return None;
            }
            all_callees.extend(member_callees);
        }

        all_callees.sort_by_key(|callee| {
            (
                callee.owner.to_string(),
                callee.method.to_string(),
                callee.definition_ranges.clone(),
            )
        });
        all_callees.dedup();
        Some(all_callees)
    }

    pub(in crate::engine) fn resolve_method_callees_inner(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
        allow_private: bool,
        protected_caller: Option<&FullyQualifiedName>,
        mut lookup_chains: Option<&mut DefinitionLookupChains>,
    ) -> Option<Vec<ResolvedMethodCallee>> {
        if !namespace_target_exists(self.engine, namespace_fqn) {
            return None;
        }

        // Known includers may override even methods in the module's own
        // ancestry. Resolve each receiver's full MRO before taking a winner.
        let includers = module_instance_receivers(self.engine, namespace_fqn);
        let fqns_to_search = if includers.is_empty() {
            vec![namespace_fqn.clone()]
        } else {
            includers
        };

        let mut callees = Vec::new();
        let mut method_missing_fallbacks = Vec::new();
        for fqn in &fqns_to_search {
            let ancestor_chain = method_lookup_chain(self.engine, fqn);
            if let Some(callee) = method_callee_in_chain(
                self.engine,
                &ancestor_chain,
                method,
                MethodCalleeResolution::Exact,
                allow_private,
                protected_caller,
            ) {
                retain_definition_lookup_chain(
                    self.engine,
                    lookup_chains.as_deref_mut(),
                    fqn,
                    &callee.owner,
                    ancestor_chain,
                );
                callees.push(callee);
            } else if !allow_private
                && private_method_in_chain(self.engine, &ancestor_chain, method)
            {
                callees.push(receiver_only_callee(fqn.clone(), method));
            } else if let Some(callee) =
                method_missing_callee_in_chain(self.engine, &ancestor_chain)
            {
                method_missing_fallbacks.push(callee);
            }
        }

        if callees.is_empty() {
            for application in execution_context_application_targets(self.engine, namespace_fqn) {
                let ancestor_chain = method_lookup_chain(self.engine, &application);
                if let Some(callee) = method_callee_in_chain(
                    self.engine,
                    &ancestor_chain,
                    method,
                    MethodCalleeResolution::Exact,
                    allow_private,
                    protected_caller,
                ) {
                    retain_definition_lookup_chain(
                        self.engine,
                        lookup_chains.as_deref_mut(),
                        &application,
                        &callee.owner,
                        ancestor_chain,
                    );
                    callees.push(callee);
                } else if !allow_private
                    && private_method_in_chain(self.engine, &ancestor_chain, method)
                {
                    callees.push(receiver_only_callee(application, method));
                } else if let Some(callee) =
                    method_missing_callee_in_chain(self.engine, &ancestor_chain)
                {
                    method_missing_fallbacks.push(callee);
                }
            }
            callees.sort_by_key(|callee| {
                (
                    callee.owner.to_string(),
                    callee
                        .definition_ranges
                        .first()
                        .map(|range| (range.file_id, range.start_byte, range.end_byte)),
                )
            });
            callees.dedup();
        }

        if callees.is_empty() {
            if !method_missing_fallbacks.is_empty() {
                return Some(method_missing_fallbacks);
            }

            return Some(
                fqns_to_search
                    .into_iter()
                    .map(|fqn| receiver_only_callee(fqn, method))
                    .collect(),
            );
        }

        Some(callees)
    }

    pub fn resolve_super_method_callee(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Option<ResolvedMethodCallee> {
        if !namespace_target_exists(self.engine, namespace_fqn) {
            return None;
        }

        let ancestor_chain = method_lookup_chain(self.engine, namespace_fqn);
        method_callee_after_owner(self.engine, &ancestor_chain, namespace_fqn, method)
    }
}

/// Retain the proven chain starting at the callable winner. Earlier namespaces
/// rejected by visibility/availability cannot claim navigation priority.
fn retain_definition_lookup_chain(
    engine: &crate::engine::AnalysisEngine,
    lookup_chains: Option<&mut DefinitionLookupChains>,
    receiver: &FullyQualifiedName,
    winner: &FullyQualifiedName,
    mut chain: Vec<FullyQualifiedName>,
) {
    let Some(chains) = lookup_chains else {
        return;
    };
    if method_lookup_chain_has_unresolved_dependency_from_graph(engine, receiver) {
        return;
    }
    let start = chain.iter().position(|owner| owner == winner).expect(
        "INVARIANT VIOLATED: a method winner is absent from its lookup chain. This is a bug because ranking must use the chain that selected the callee. Fix: retain the owning namespace returned by method lookup.",
    );
    chain.drain(..start);
    chains.push(chain);
}
