use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::core::method_store::MethodVisibility;
use crate::core::{
    FqnId, FullyQualifiedName, GraphEdgeFact, GraphEdgeKind, GraphEdgeProvenance, GraphNodeKind,
    MethodCalleeResolution, MethodFact, MethodReferenceAccess, ResolvedMethodCallee, RubyConstant,
    RubyMethod, RubyType, SourceFileId, SourceKind, StoredMethodReferenceCandidate,
    StoredReferenceCandidateRef, SymbolKind, TextRange, TypeSubject,
};
use crate::engine::query::AnalysisQuery;
use crate::engine::state::EffectiveMethodFactMatch;
use crate::engine::types::{AnalysisQueryCache, MethodReturnQueryAccess};

#[derive(Default)]
pub(crate) struct MethodLookupChainCache {
    chains: HashMap<FullyQualifiedName, Vec<FqnId>>,
    metaclass_methods: HashMap<(FqnId, RubyMethod), MethodLookupResult>,
    unresolved_edge_sources: Option<HashSet<FqnId>>,
    unresolved_dependencies: HashMap<FullyQualifiedName, bool>,
    unproven_universal_methods: HashMap<FqnId, HashSet<RubyMethod>>,
}

impl MethodLookupChainCache {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn len(&self) -> usize {
        self.chains.len()
    }

    pub(crate) fn get(&self, owner: &FullyQualifiedName) -> Option<&Vec<FqnId>> {
        self.chains.get(owner)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstantRenameTarget {
    pub fqn: FullyQualifiedName,
    pub current_name: RubyConstant,
    pub ranges: Vec<TextRange>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct MethodRenameIdentity {
    owner: FullyQualifiedName,
    method: RubyMethod,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodRenameTarget {
    pub owner: FullyQualifiedName,
    pub current_name: RubyMethod,
    pub ranges: Vec<TextRange>,
}

#[derive(Clone)]
pub enum MethodLookupResult {
    Unique(Arc<MethodFact>),
    Ambiguous {
        owner: FullyQualifiedName,
        method: RubyMethod,
    },
    Missing,
}

impl MethodLookupResult {
    pub fn reference_parts(
        &self,
    ) -> Option<(&FullyQualifiedName, RubyMethod, Option<&MethodFact>)> {
        match self {
            MethodLookupResult::Unique(fact) => {
                Some((&fact.owner, method_name_from_fact(fact), Some(fact)))
            }
            MethodLookupResult::Ambiguous { owner, method } => Some((owner, *method, None)),
            MethodLookupResult::Missing => None,
        }
    }

    pub fn is_missing(&self) -> bool {
        matches!(self, MethodLookupResult::Missing)
    }
}

fn receiver_type_members(receiver_type: &RubyType) -> &[RubyType] {
    match receiver_type {
        RubyType::Union(members) => {
            assert!(
                members.len() >= 2,
                "INVARIANT VIOLATED: method resolution received a RubyType::Union with fewer than two members. This is a bug because canonical union construction must collapse empty and singleton inputs. Fix: construct receiver unions only through RubyType::union helpers."
            );
            members
        }
        RubyType::Class(_)
        | RubyType::Module(_)
        | RubyType::ClassReference(_)
        | RubyType::ModuleReference(_)
        | RubyType::Literal(_)
        | RubyType::Array(_)
        | RubyType::Hash(_, _)
        | RubyType::Shape(_)
        | RubyType::Unknown => std::slice::from_ref(receiver_type),
    }
}

impl<'a> AnalysisQuery<'a> {
    /// Resolve a method declaration or call at a byte offset into a safe,
    /// project-editable rename target.
    ///
    /// Method identity includes the namespace kind, so `User#name` and
    /// `User.name` never share an edit set. Declarations without an exact name
    /// token (aliases, delegates, generated macros, signatures, and external
    /// sources) are deliberately rejected.
    pub fn method_rename_target_at(
        &self,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> Option<MethodRenameTarget> {
        let identity = self.method_rename_identity_at(file_id, byte_offset)?;
        self.method_rename_target(identity, None)
    }

    pub fn method_rename_target_for_name_at(
        &self,
        file_id: SourceFileId,
        byte_offset: u32,
        new_name: RubyMethod,
    ) -> Option<MethodRenameTarget> {
        let identity = self.method_rename_identity_at(file_id, byte_offset)?;
        self.method_rename_target(identity, Some(new_name))
    }

    fn method_rename_identity_at(
        &self,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> Option<MethodRenameIdentity> {
        let mut identities = self
            .engine
            .method_facts_in_file(file_id)
            .into_iter()
            .filter(|fact| fact.name_range.contains_offset(file_id, byte_offset))
            .filter_map(|fact| {
                let FullyQualifiedName::Method(_, method) = fact.fqn else {
                    return None;
                };
                Some(MethodRenameIdentity {
                    owner: fact.owner,
                    method,
                })
            })
            .collect::<Vec<_>>();

        for candidate in self
            .engine
            .reference_candidate_store()
            .candidates_in_file(file_id)
        {
            if !candidate.range.contains_offset(file_id, byte_offset) {
                continue;
            }
            let crate::core::StoredReferenceCandidateKind::Method {
                owner,
                owner_kind,
                method,
                is_super,
                access,
                caller,
                call_expression_range,
                preferred_definition_range,
                diagnostics,
            } = candidate.kind
            else {
                continue;
            };
            let candidate = StoredMethodReferenceCandidate {
                range: candidate.range,
                owner,
                owner_kind,
                method,
                is_super,
                access,
                caller,
                call_expression_range,
                preferred_definition_range,
                diagnostics,
            };
            identities.extend(self.method_candidate_rename_identities(&candidate));
        }

        identities.sort();
        identities.dedup();
        (identities.len() == 1).then(|| {
            identities.pop().expect(
                "INVARIANT VIOLATED: method rename identity disappeared after length validation. This is a bug because the local identity vector is not mutated between the check and pop. Fix: keep identity selection atomic.",
            )
        })
    }

    fn method_rename_target(
        &self,
        identity: MethodRenameIdentity,
        new_name: Option<RubyMethod>,
    ) -> Option<MethodRenameTarget> {
        if identity.owner.has_generated_owner() {
            return None;
        }
        if !method_name_is_refactorable(identity.method) {
            return None;
        }
        if new_name.is_some_and(|new_name| new_name == identity.method) {
            return None;
        }
        if new_name.is_some_and(|new_name| {
            !method_name_is_refactorable(new_name)
                || identity.method.as_str().ends_with('=') != new_name.as_str().ends_with('=')
        }) {
            return None;
        }
        if self.method_lookup_chain_is_incomplete_for_rename(&identity.owner) {
            return None;
        }
        if let Some(new_name) = new_name {
            let target_chain = method_lookup_chain(self.engine, &identity.owner);
            let collision = target_chain.iter().any(|owner| {
                self.engine
                    .method_facts_matching_owner_name(&owner, &new_name)
                    .into_iter()
                    .any(|fact| {
                        self.engine
                            .file(fact.range.file_id)
                            .is_some_and(|file| file.kind != crate::core::SourceKind::Signature)
                    })
            }) || self.engine.all_method_facts().into_iter().any(|fact| {
                let FullyQualifiedName::Method(_, fact_method) = fact.fqn else {
                    return false;
                };
                fact_method == new_name
                    && self
                        .engine
                        .file(fact.range.file_id)
                        .is_some_and(|file| file.kind != crate::core::SourceKind::Signature)
                    && (target_chain.contains(&fact.owner)
                        || method_lookup_chain(self.engine, &fact.owner).contains(&identity.owner))
            });
            if collision {
                return None;
            }
        }

        let method_fqn =
            FullyQualifiedName::method(identity.owner.namespace_parts(), identity.method);
        let all_method_facts = self.engine.method_facts_for(&method_fqn);
        let declaration_facts = all_method_facts
            .iter()
            .filter(|fact| fact.owner == identity.owner)
            .filter(|fact| {
                self.engine
                    .file(fact.range.file_id)
                    .is_some_and(|file| file.kind != crate::core::SourceKind::Signature)
            })
            .collect::<Vec<_>>();
        if declaration_facts.is_empty()
            || declaration_facts.iter().any(|fact| {
                !self
                    .engine
                    .file(fact.range.file_id)
                    .is_some_and(|file| file.kind.is_editable())
                    || fact
                        .name_range
                        .end_byte
                        .checked_sub(fact.name_range.start_byte)
                        != u32::try_from(identity.method.as_str().len()).ok()
                    || fact.range == fact.name_range
            })
        {
            return None;
        }

        // One Ruby declaration can materialize multiple semantic owners (for
        // example `module_function`). Renaming only one of those identities
        // would lie about the resulting program, so reject the coupled token.
        let every_method_fact = self.engine.all_method_facts();
        if declaration_facts.iter().any(|declaration| {
            every_method_fact.iter().any(|other| {
                other.owner != identity.owner
                    && (other.name_range == declaration.name_range
                        || other.range == declaration.range)
            })
        }) {
            return None;
        }

        let mut ranges = declaration_facts
            .into_iter()
            .map(|fact| fact.name_range)
            .collect::<Vec<_>>();
        ranges.extend(
            self.engine
                .method_visibility_overrides_matching_owner_name(&identity.owner, &identity.method)
                .into_iter()
                .filter(|fact| {
                    self.engine
                        .file(fact.range.file_id)
                        .is_some_and(|file| file.kind.is_editable())
                })
                .map(|fact| fact.range),
        );

        for candidate in self.engine.reference_candidate_store().iter_candidates() {
            match candidate {
                StoredReferenceCandidateRef::Method(candidate)
                    if candidate.method == identity.method =>
                {
                    let targets = self.method_candidate_rename_identities(candidate);
                    let caller_is_target = candidate.caller.is_some_and(|caller| {
                        self.engine.fqn_for_id(caller).is_some_and(|caller| {
                            matches!(
                                caller,
                                FullyQualifiedName::Method(parts, method)
                                    if *method == identity.method
                                        && parts.as_slice()
                                            == identity.owner.namespace_parts_slice()
                            )
                        })
                    });
                    if candidate.is_super && (targets.contains(&identity) || caller_is_target) {
                        return None;
                    }
                    if targets.contains(&identity) {
                        if targets.len() != 1 {
                            return None;
                        }
                        if let Some(new_name) = new_name {
                            let mut collision_candidate = candidate.clone();
                            collision_candidate.method = new_name;
                            if !self
                                .method_candidate_rename_identities(&collision_candidate)
                                .is_empty()
                            {
                                return None;
                            }
                        }
                        if self
                            .engine
                            .file(candidate.range.file_id)
                            .is_some_and(|file| file.kind.is_editable())
                        {
                            ranges.push(candidate.range);
                        }
                    }
                }
                StoredReferenceCandidateRef::Resolved(candidate) => {
                    let Some(target) = self.engine.fqn_for_id(candidate.target) else {
                        panic!(
                            "INVARIANT VIOLATED: resolved rename candidate points to a missing FQN. This is a bug because resolved candidates retain interned targets. Fix: retain interned names for the candidate lifetime."
                        );
                    };
                    if target != &method_fqn {
                        continue;
                    }
                    let mut owners = all_method_facts
                        .iter()
                        .map(|fact| fact.owner.clone())
                        .collect::<Vec<_>>();
                    owners.sort_by_key(ToString::to_string);
                    owners.dedup();
                    if owners != vec![identity.owner.clone()] {
                        return None;
                    }
                    if self
                        .engine
                        .file(candidate.range.file_id)
                        .is_some_and(|file| file.kind.is_editable())
                    {
                        ranges.push(candidate.range);
                    }
                }
                StoredReferenceCandidateRef::Constant(_)
                | StoredReferenceCandidateRef::Method(_) => {}
            }
        }

        ranges.sort_by_key(|range| (range.file_id, range.start_byte, range.end_byte));
        ranges.dedup();
        Some(MethodRenameTarget {
            owner: identity.owner,
            current_name: identity.method,
            ranges,
        })
    }

    pub(super) fn method_candidate_callees(
        &self,
        candidate: &StoredMethodReferenceCandidate,
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
                MethodReferenceAccess::Normal | MethodReferenceAccess::VisibilityBypass => self
                    .resolve_method_callees_for_type(receiver_type, &candidate.method)
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
                            self.resolve_protected_method_callees_for_type(
                                receiver_type,
                                &candidate.method,
                                &caller,
                            )
                        });
                    protected
                        .or_else(|| {
                            self.resolve_public_method_callees_for_type(
                                receiver_type,
                                &candidate.method,
                            )
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
                MethodReferenceAccess::Normal | MethodReferenceAccess::VisibilityBypass => self
                    .resolve_method_callees(&owner, &candidate.method)
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
                            self.resolve_protected_method_callees(
                                &owner,
                                &candidate.method,
                                &caller,
                            )
                        });
                    protected
                        .or_else(|| self.resolve_public_method_callees(&owner, &candidate.method))
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

    fn method_candidate_rename_identities(
        &self,
        candidate: &StoredMethodReferenceCandidate,
    ) -> Vec<MethodRenameIdentity> {
        let callees = self.method_candidate_callees(candidate);
        let mut identities = callees
            .into_iter()
            .filter(|callee| {
                callee.resolution == crate::core::MethodCalleeResolution::Exact
                    && callee.method == candidate.method
                    && !callee.definition_ranges.is_empty()
            })
            .map(|callee| MethodRenameIdentity {
                owner: callee.owner,
                method: callee.method,
            })
            .collect::<Vec<_>>();
        identities.sort();
        identities.dedup();
        identities
    }

    fn method_lookup_chain_is_incomplete_for_rename(&self, owner: &FullyQualifiedName) -> bool {
        let unresolved_sources = self
            .engine
            .graph
            .unresolved_edges()
            .into_iter()
            .filter_map(|edge| {
                let lookup = self.engine.names.const_lookup(edge.target).expect(
                    "INVARIANT VIOLATED: unresolved rename graph edge points to a missing lookup. This is a bug because graph edges retain interned targets. Fix: retain target lookups for the graph edge lifetime.",
                );
                if edge.kind == GraphEdgeKind::Superclass
                    && lookup.absolute
                    && lookup.path.len() == 1
                    && lookup.path[0].as_str() == "Object"
                {
                    return None;
                }
                self.engine
                    .names
                    .fqn(edge.source)
                    .map(FullyQualifiedName::namespace_parts)
            })
            .collect::<HashSet<_>>();
        let mut pending = vec![owner.clone()];
        let mut visited = HashSet::new();
        while let Some(current) = pending.pop() {
            if !visited.insert(current.clone()) {
                continue;
            }
            if self.engine.superclass_is_ambiguous(&current) {
                return true;
            }
            if unresolved_sources.contains(&current.namespace_parts()) {
                return true;
            }
            pending.extend(
                self.engine
                    .graph_edges_from(&current)
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

    pub fn resolve_method_signature_facts(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Vec<MethodFact> {
        self.resolve_method_signature_facts_inner(namespace_fqn, method, true, None)
    }

    pub fn resolve_public_method_signature_facts(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Vec<MethodFact> {
        self.resolve_method_signature_facts_inner(namespace_fqn, method, false, None)
    }

    pub fn resolve_protected_method_signature_facts(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
        caller_namespace_fqn: &FullyQualifiedName,
    ) -> Vec<MethodFact> {
        self.resolve_method_signature_facts_inner(
            namespace_fqn,
            method,
            false,
            Some(caller_namespace_fqn),
        )
    }

    pub fn resolve_protected_method_signature_facts_for_type(
        &self,
        receiver_type: &RubyType,
        method: &RubyMethod,
        caller_namespace_fqn: &FullyQualifiedName,
    ) -> Vec<MethodFact> {
        self.resolve_method_signature_facts_for_type_inner(
            receiver_type,
            method,
            false,
            Some(caller_namespace_fqn),
        )
    }

    pub fn resolve_method_signature_facts_for_type(
        &self,
        receiver_type: &RubyType,
        method: &RubyMethod,
    ) -> Vec<MethodFact> {
        self.resolve_method_signature_facts_for_type_inner(receiver_type, method, true, None)
    }

    pub fn resolve_public_method_signature_facts_for_type(
        &self,
        receiver_type: &RubyType,
        method: &RubyMethod,
    ) -> Vec<MethodFact> {
        self.resolve_method_signature_facts_for_type_inner(receiver_type, method, false, None)
    }

    fn resolve_method_signature_facts_for_type_inner(
        &self,
        receiver_type: &RubyType,
        method: &RubyMethod,
        allow_private: bool,
        protected_caller: Option<&FullyQualifiedName>,
    ) -> Vec<MethodFact> {
        let members = receiver_type_members(receiver_type);
        let mut all_facts = Vec::new();
        for member in members {
            let namespaces = Self::receiver_type_to_method_namespaces(member);
            if namespaces.is_empty() {
                return Vec::new();
            }

            let mut member_facts = Vec::new();
            for namespace in namespaces {
                member_facts.extend(self.resolve_method_signature_facts_inner(
                    &namespace,
                    method,
                    allow_private,
                    protected_caller,
                ));
            }
            if member_facts.is_empty() {
                return Vec::new();
            }
            all_facts.extend(member_facts);
        }

        all_facts.sort_by_key(|fact| {
            (
                fact.range.file_id,
                fact.range.start_byte,
                fact.range.end_byte,
                fact.fqn.to_string(),
            )
        });
        all_facts.dedup();
        all_facts
    }

    fn resolve_method_signature_facts_inner(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
        allow_private: bool,
        protected_caller: Option<&FullyQualifiedName>,
    ) -> Vec<MethodFact> {
        let Some(callees) = self.resolve_method_callees_inner(
            namespace_fqn,
            method,
            allow_private,
            protected_caller,
        ) else {
            return Vec::new();
        };

        let mut facts = callees
            .into_iter()
            .filter(|callee| callee.resolution == MethodCalleeResolution::Exact)
            .flat_map(|callee| {
                let matching = self
                    .engine
                    .method_facts_matching_owner_name(&callee.owner, method)
                    .into_iter()
                    .collect::<Vec<_>>();
                let signatures = matching
                    .iter()
                    .filter(|fact| {
                        self.engine
                            .file(fact.range.file_id)
                            .expect(
                                "INVARIANT VIOLATED: signature fact references an unregistered file. \
                                 This is a bug because signature selection requires source metadata. \
                                 Fix: replace signature facts only after registering their source file.",
                            )
                            .kind
                            == crate::core::SourceKind::Signature
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                if signatures.is_empty() {
                    matching
                        .into_iter()
                        .filter(|fact| callee.definition_ranges.contains(&fact.range))
                        .collect::<Vec<_>>()
                } else {
                    signatures
                }
            })
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
        facts
    }

    pub fn resolve_method_callees(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Option<Vec<ResolvedMethodCallee>> {
        self.resolve_method_callees_inner(namespace_fqn, method, true, None)
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
        self.resolve_method_callees_inner(namespace_fqn, method, false, None)
    }

    pub fn resolve_public_method_callees_cached(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
        cache: &AnalysisQueryCache,
    ) -> Option<Vec<ResolvedMethodCallee>> {
        cache.method_callees(
            self.engine.query_cache_identity(),
            namespace_fqn,
            *method,
            MethodReturnQueryAccess::Public,
            || self.resolve_public_method_callees(namespace_fqn, method),
        )
    }

    pub fn resolve_protected_method_callees(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
        caller_namespace_fqn: &FullyQualifiedName,
    ) -> Option<Vec<ResolvedMethodCallee>> {
        self.resolve_method_callees_inner(namespace_fqn, method, false, Some(caller_namespace_fqn))
    }

    pub fn resolve_method_callees_for_type(
        &self,
        receiver_type: &RubyType,
        method: &RubyMethod,
    ) -> Option<Vec<ResolvedMethodCallee>> {
        self.resolve_method_callees_for_type_inner(receiver_type, method, true, None)
    }

    pub fn resolve_public_method_callees_for_type(
        &self,
        receiver_type: &RubyType,
        method: &RubyMethod,
    ) -> Option<Vec<ResolvedMethodCallee>> {
        self.resolve_method_callees_for_type_inner(receiver_type, method, false, None)
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
        )
    }

    fn resolve_method_callees_for_type_inner(
        &self,
        receiver_type: &RubyType,
        method: &RubyMethod,
        allow_private: bool,
        protected_caller: Option<&FullyQualifiedName>,
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

    pub fn resolve_protected_method_callees_cached(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
        caller_namespace_fqn: &FullyQualifiedName,
        cache: &AnalysisQueryCache,
    ) -> Option<Vec<ResolvedMethodCallee>> {
        cache.method_callees(
            self.engine.query_cache_identity(),
            namespace_fqn,
            *method,
            MethodReturnQueryAccess::Protected(caller_namespace_fqn.clone()),
            || self.resolve_protected_method_callees(namespace_fqn, method, caller_namespace_fqn),
        )
    }

    fn resolve_method_callees_inner(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
        allow_private: bool,
        protected_caller: Option<&FullyQualifiedName>,
    ) -> Option<Vec<ResolvedMethodCallee>> {
        if !namespace_target_exists(self.engine, namespace_fqn) {
            return None;
        }

        let fqns_to_search = if is_module_instance_namespace(self.engine, namespace_fqn) {
            let ancestor_chain = method_lookup_chain(self.engine, namespace_fqn);
            if let Some(callee) = method_callee_in_chain(
                self.engine,
                &ancestor_chain,
                method,
                MethodCalleeResolution::Exact,
                allow_private,
                protected_caller,
            ) {
                return Some(vec![callee]);
            }
            if !allow_private && private_method_in_chain(self.engine, &ancestor_chain, method) {
                return Some(vec![receiver_only_callee(namespace_fqn.clone(), method)]);
            }
            let includers = module_includers(self.engine, namespace_fqn);
            if includers.is_empty() {
                vec![namespace_fqn.clone()]
            } else {
                includers
            }
        } else {
            vec![namespace_fqn.clone()]
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
            || method_lookup_chain_has_unresolved_dependency(self.engine, namespace_fqn)
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
        let EffectiveMethodFactMatch::Unique(fact) = self
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
        if !namespace_target_exists(self.engine, namespace_fqn) {
            return MethodLookupResult::Missing;
        }

        let ancestor_chain =
            method_lookup_chain_for_reference_cached(self.engine, namespace_fqn, chain_cache);
        let universal_roots = ancestor_chain
            .iter()
            .copied()
            .filter(|owner| {
                self.engine
                    .names
                    .fqn(*owner)
                    .is_some_and(is_universal_open_root)
            })
            .collect::<Vec<_>>();
        let mut crossed_universal_root = false;

        for owner_id in ancestor_chain.iter().copied() {
            let owner = self.engine.names.fqn(owner_id).expect(
                "INVARIANT VIOLATED: cached method-chain owner ID is absent from the name registry. This is a bug because resolution-local chain IDs originate from that same immutable registry. Fix: invalidate all resolution-local chain caches whenever names can change.",
            );
            crossed_universal_root |= is_universal_open_root(owner);
            match self
                .engine
                .effective_method_fact_matching_owner_id(owner_id, method)
            {
                EffectiveMethodFactMatch::Missing => continue,
                EffectiveMethodFactMatch::Unique(fact) => {
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
                    return MethodLookupResult::Unique(Arc::new(fact));
                }
                EffectiveMethodFactMatch::Ambiguous => {
                    return MethodLookupResult::Ambiguous {
                        owner: self
                            .engine
                            .names
                            .fqn(owner_id)
                            .expect(
                                "INVARIANT VIOLATED: cached method-chain owner ID is absent from the name registry. This is a bug because resolution-local chain IDs originate from that same immutable registry. Fix: invalidate all resolution-local chain caches whenever names can change.",
                            )
                            .clone(),
                        method: *method,
                    };
                }
            }
        }

        if is_module_instance_namespace(self.engine, namespace_fqn) {
            let mut includer_callees = self
                .resolve_method_callees_inner(namespace_fqn, method, true, None)
                .unwrap_or_default()
                .into_iter()
                .filter(|callee| {
                    callee.resolution == MethodCalleeResolution::Exact
                        && !callee.definition_ranges.is_empty()
                })
                .collect::<Vec<_>>();
            includer_callees
                .sort_by_key(|callee| (callee.owner.to_string(), callee.definition_ranges.clone()));
            includer_callees.dedup();
            let mut includer_owners = includer_callees
                .iter()
                .map(|callee| callee.owner.clone())
                .collect::<Vec<_>>();
            includer_owners.sort_by_key(ToString::to_string);
            includer_owners.dedup();
            if includer_owners.len() > 1 {
                return MethodLookupResult::Ambiguous {
                    owner: namespace_fqn.clone(),
                    method: *method,
                };
            }
            if let Some(owner) = includer_owners.pop() {
                let definition_ranges = includer_callees
                    .into_iter()
                    .flat_map(|callee| callee.definition_ranges)
                    .collect::<HashSet<_>>();
                let target = FullyQualifiedName::method(owner.namespace_parts(), *method);
                let mut facts = self
                    .engine
                    .method_facts_for(&target)
                    .into_iter()
                    .filter(|fact| fact.owner == owner && definition_ranges.contains(&fact.range))
                    .collect::<Vec<_>>();
                facts.sort_by_key(|fact| {
                    (
                        fact.range.file_id,
                        fact.range.start_byte,
                        fact.range.end_byte,
                    )
                });
                if let Some(fact) = facts.into_iter().next() {
                    return MethodLookupResult::Unique(Arc::new(fact));
                }
                panic!(
                    "INVARIANT VIOLATED: exact module-includer method resolution has no matching method fact. This is a bug because callee definition ranges originate from method facts in the same immutable engine. Fix: keep includer callee construction and reference materialization on the same fact snapshot."
                );
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
            return MethodLookupResult::Missing;
        }

        // Class and module objects share one language-defined fallback through
        // Class/Module. Keep that continuation lazy for reference resolution:
        // most singleton calls resolve in their own ancestry, and appending the
        // same metaclass chain to every cached owner multiplied transient work
        // on large projects without adding proof.
        if let Some(metaclass) = metaclass_namespace_for_object(self.engine, namespace_fqn) {
            let metaclass_id = self.engine.names.fqn_id(&metaclass).expect(
                "INVARIANT VIOLATED: proven metaclass namespace is absent from the name registry. This is a bug because metaclass fallback requires an indexed graph node. Fix: intern graph node FQNs before method resolution.",
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
                        EffectiveMethodFactMatch::Unique(fact) => {
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
                            result = MethodLookupResult::Unique(Arc::new(fact));
                            break;
                        }
                        EffectiveMethodFactMatch::Ambiguous => {
                            result = MethodLookupResult::Ambiguous {
                                owner: self
                                    .engine
                                    .names
                                    .fqn(owner_id)
                                    .expect(
                                        "INVARIANT VIOLATED: cached metaclass-chain owner ID is absent from the name registry. This is a bug because resolution-local chain IDs originate from that same immutable registry. Fix: invalidate all resolution-local chain caches whenever names can change.",
                                    )
                                    .clone(),
                                method: *method,
                            };
                            break;
                        }
                    }
                }
                assert!(
                    chain_cache
                        .metaclass_methods
                        .insert(cache_key, result.clone())
                        .is_none(),
                    "INVARIANT VIOLATED: metaclass fallback cache replaced an entry after a confirmed miss. This is a bug because metaclass identity and method names are immutable during one resolve pass. Fix: keep fallback lookup and insertion in one resolution step."
                );
                result
            };
            match fallback {
                MethodLookupResult::Missing => {}
                MethodLookupResult::Unique(_) | MethodLookupResult::Ambiguous { .. } => {
                    return fallback;
                }
            }
        }

        let mut application_facts = Vec::new();
        let mut application_ambiguous = false;
        for application in execution_context_application_targets(self.engine, namespace_fqn) {
            match self.resolve_method_reference_with_chain_cache(&application, method, chain_cache)
            {
                MethodLookupResult::Unique(fact) => application_facts.push(fact),
                MethodLookupResult::Ambiguous { .. } => application_ambiguous = true,
                MethodLookupResult::Missing => {}
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
            return MethodLookupResult::Unique(fact);
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
                MethodLookupResult::Unique(fact)
                    if default_basic_object_method_missing_fact(self.engine, fact)
            ) {
                return MethodLookupResult::Missing;
            }
            return fallback;
        }

        MethodLookupResult::Missing
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
            EffectiveMethodFactMatch::Unique(fact) => MethodLookupResult::Unique(Arc::new(fact)),
            EffectiveMethodFactMatch::Ambiguous => MethodLookupResult::Ambiguous {
                owner: callee.owner,
                method: *method,
            },
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
                    .engine
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
        for override_fact in self.engine.all_method_visibility_overrides() {
            if override_fact.method != *method {
                continue;
            }
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

    pub fn resolve_constant_receiver(
        &self,
        path: &[RubyConstant],
        current_namespace: &[RubyConstant],
    ) -> FullyQualifiedName {
        let current_fqn = FullyQualifiedName::namespace_with_kind(
            current_namespace.to_vec(),
            crate::core::NamespaceKind::Instance,
        );
        let resolved = resolve_constant_fqn(self.engine, path, false, &current_fqn)
            .unwrap_or_else(|| FullyQualifiedName::constant(path.to_vec()));
        let resolved_constant = FullyQualifiedName::constant(resolved.namespace_parts().to_vec());
        if let Some(receiver_type) = self.constant_value_type(&resolved_constant) {
            if let Some(namespace) = self.type_to_namespace(&receiver_type) {
                return namespace;
            }
        }

        FullyQualifiedName::namespace_with_kind(
            resolved.namespace_parts(),
            crate::core::NamespaceKind::Singleton,
        )
    }

    pub fn resolve_constant_in_context(
        &self,
        parts: &[RubyConstant],
        context: &[RubyConstant],
    ) -> Option<FullyQualifiedName> {
        let context_fqn = FullyQualifiedName::namespace(context.to_vec());
        resolve_constant_fqn(self.engine, parts, false, &context_fqn)
    }

    /// Resolve a constant-like symbol and return every editable project range.
    ///
    /// Definition token boundaries come from indexer facts; references come
    /// from the engine's centralized constant resolution. External sources are
    /// intentionally excluded because an editor rename must never edit gems,
    /// stdlib, or generated stubs.
    pub fn constant_rename_target(
        &self,
        parts: &[RubyConstant],
        context: &[RubyConstant],
    ) -> Option<ConstantRenameTarget> {
        let fqn = self.resolve_constant_in_context(parts, context)?;
        if fqn.has_generated_owner() {
            return None;
        }
        let current_name = *fqn.namespace_parts_slice().last()?;
        let symbol_facts = self
            .engine
            .symbol_facts_for(&fqn)
            .into_iter()
            .filter(|fact| {
                matches!(
                    fact.kind,
                    SymbolKind::Class | SymbolKind::Module | SymbolKind::Constant
                ) && fact
                    .name_range
                    .end_byte
                    .checked_sub(fact.name_range.start_byte)
                    == u32::try_from(current_name.as_str().len()).ok()
                    && self
                        .engine
                        .file(fact.range.file_id)
                        .is_some_and(|file| file.kind.is_editable())
            })
            .collect::<Vec<_>>();
        if symbol_facts.is_empty() {
            return None;
        }
        let mut ranges = symbol_facts
            .into_iter()
            .map(|fact| fact.name_range)
            .chain(
                self.engine
                    .reference_facts_for(&fqn)
                    .iter()
                    .filter(|fact| {
                        self.engine
                            .file(fact.range.file_id)
                            .is_some_and(|file| file.kind.is_editable())
                    })
                    .filter_map(|fact| {
                        constant_reference_name_range(self.engine, fact.range, current_name)
                    }),
            )
            .collect::<Vec<_>>();
        ranges.sort_by_key(|range| (range.file_id, range.start_byte, range.end_byte));
        ranges.dedup();

        Some(ConstantRenameTarget {
            fqn,
            current_name,
            ranges,
        })
    }

    pub fn constant_rename_target_for_name(
        &self,
        parts: &[RubyConstant],
        context: &[RubyConstant],
        new_name: RubyConstant,
    ) -> Option<ConstantRenameTarget> {
        let target = self.constant_rename_target(parts, context)?;
        if target.current_name == new_name
            || constant_name_collides(self.engine, &target.fqn, new_name)
        {
            return None;
        }
        Some(target)
    }

    pub fn constant_definition_ranges(
        &self,
        parts: &[RubyConstant],
        context: &[RubyConstant],
    ) -> Vec<TextRange> {
        let fqn = self
            .resolve_constant_in_context(parts, context)
            .unwrap_or_else(|| FullyQualifiedName::constant(parts.to_vec()));
        let mut runtime_targets = self
            .engine
            .type_facts_for(&TypeSubject::Constant(fqn.clone()))
            .into_iter()
            .filter(|fact| fact.provenance == crate::core::TypeProvenance::Runtime)
            .filter_map(|fact| match fact.ruby_type {
                crate::core::RubyType::ClassReference(target)
                | crate::core::RubyType::ModuleReference(target)
                    if target != fqn =>
                {
                    Some(target)
                }
                crate::core::RubyType::ClassReference(_)
                | crate::core::RubyType::ModuleReference(_)
                | crate::core::RubyType::Class(_)
                | crate::core::RubyType::Module(_)
                | crate::core::RubyType::Array(_)
                | crate::core::RubyType::Hash(_, _)
                | crate::core::RubyType::Literal(_)
                | crate::core::RubyType::Shape(_)
                | crate::core::RubyType::Union(_)
                | crate::core::RubyType::Unknown => None,
            })
            .collect::<Vec<_>>();
        runtime_targets.sort_by_key(ToString::to_string);
        runtime_targets.dedup();
        if runtime_targets.len() == 1 {
            let implementation_ranges = self.symbol_definition_ranges(
                &runtime_targets[0],
                &[SymbolKind::Class, SymbolKind::Module, SymbolKind::Constant],
            );
            if !implementation_ranges.is_empty() {
                return implementation_ranges;
            }
        }
        self.symbol_definition_ranges(
            &fqn,
            &[SymbolKind::Class, SymbolKind::Module, SymbolKind::Constant],
        )
    }

    pub fn yard_type_definition_ranges(
        &self,
        type_name: &str,
        context: &[RubyConstant],
    ) -> Vec<TextRange> {
        let builtins = ["nil", "true", "false", "void", "Boolean", "bool"];
        if builtins
            .iter()
            .any(|builtin| builtin.eq_ignore_ascii_case(type_name))
        {
            return Vec::new();
        }

        let is_root_constant = type_name.starts_with("::");
        let type_to_parse = if is_root_constant {
            &type_name[2..]
        } else {
            type_name
        };

        let mut parts = Vec::new();
        for part in type_to_parse.split("::") {
            let trimmed = part.trim();
            if trimmed.is_empty() {
                continue;
            }
            let Ok(constant) = RubyConstant::try_from(trimmed) else {
                return Vec::new();
            };
            parts.push(constant);
        }

        if parts.is_empty() {
            return Vec::new();
        }

        let context = if is_root_constant { &[][..] } else { context };
        self.constant_definition_ranges(&parts, context)
    }

    pub fn variable_definition_ranges(&self, fqn: &FullyQualifiedName) -> Vec<TextRange> {
        self.symbol_definition_ranges(
            fqn,
            &[
                SymbolKind::LocalVariable,
                SymbolKind::InstanceVariable,
                SymbolKind::ClassVariable,
                SymbolKind::GlobalVariable,
            ],
        )
    }

    pub fn instance_variable_definition_ranges(&self, name: &str) -> Vec<TextRange> {
        match FullyQualifiedName::instance_variable(name.to_string()) {
            Ok(fqn) => self.variable_definition_ranges(&fqn),
            Err(_) => Vec::new(),
        }
    }

    pub fn class_variable_definition_ranges(&self, name: &str) -> Vec<TextRange> {
        match FullyQualifiedName::class_variable(name.to_string()) {
            Ok(fqn) => self.variable_definition_ranges(&fqn),
            Err(_) => Vec::new(),
        }
    }

    pub fn global_variable_definition_ranges(&self, name: &str) -> Vec<TextRange> {
        match FullyQualifiedName::global_variable(name.to_string()) {
            Ok(fqn) => self.variable_definition_ranges(&fqn),
            Err(_) => Vec::new(),
        }
    }

    pub fn reference_ranges_for_fqn(&self, fqn: &FullyQualifiedName) -> Vec<TextRange> {
        self.engine
            .reference_facts_for(fqn)
            .iter()
            .map(|fact| fact.range)
            .collect()
    }

    pub fn constant_reference_ranges(
        &self,
        parts: &[RubyConstant],
        context: &[RubyConstant],
    ) -> Vec<TextRange> {
        if let Some(target) = self.resolve_constant_reference_target(parts, context) {
            let ranges = self.reference_ranges_for_fqn(&target);
            if !ranges.is_empty() {
                return ranges;
            }
        }

        let mut fallback = context.to_vec();
        fallback.extend(parts.iter().cloned());

        let namespace_fqn = FullyQualifiedName::namespace(fallback.clone());
        let namespace_ranges = self.reference_ranges_for_fqn(&namespace_fqn);
        if !namespace_ranges.is_empty() {
            return namespace_ranges;
        }

        self.reference_ranges_for_fqn(&FullyQualifiedName::constant(fallback))
    }

    pub fn variable_reference_ranges(&self, fqn: &FullyQualifiedName) -> Vec<TextRange> {
        self.reference_ranges_for_fqn(fqn)
    }

    pub fn method_reference_ranges(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Vec<TextRange> {
        self.method_reference_ranges_with_private(namespace_fqn, method, true, None)
    }

    pub fn method_reference_ranges_public_receiver(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Vec<TextRange> {
        self.method_reference_ranges_with_private(namespace_fqn, method, false, None)
    }

    pub fn method_reference_ranges_protected_receiver(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
        caller_namespace_fqn: &FullyQualifiedName,
    ) -> Vec<TextRange> {
        self.method_reference_ranges_with_private(
            namespace_fqn,
            method,
            false,
            Some(caller_namespace_fqn),
        )
    }

    fn method_reference_ranges_with_private(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
        allow_private: bool,
        protected_caller: Option<&FullyQualifiedName>,
    ) -> Vec<TextRange> {
        let mut ranges = Vec::new();
        let receiver_non_public =
            self.method_lookup_has_visibility(namespace_fqn, method, MethodVisibility::Private)
                || self.method_lookup_has_visibility(
                    namespace_fqn,
                    method,
                    MethodVisibility::Protected,
                );
        let same_name_non_public = self
            .method_name_has_visibility(method, MethodVisibility::Private)
            || self.method_name_has_visibility(method, MethodVisibility::Protected);
        let targets = self.method_reference_targets(namespace_fqn, method);
        let target_set = targets.iter().cloned().collect::<HashSet<_>>();
        for target in targets {
            let ancestor_chain = method_lookup_chain(self.engine, namespace_fqn);
            let target_visibility_owner =
                self.method_target_visibility_owner(&target, &ancestor_chain);
            let target_non_public = target_visibility_owner
                .as_ref()
                .is_some_and(|(visibility, _owner)| *visibility != MethodVisibility::Public);
            let non_public_target = receiver_non_public
                || target_non_public
                || (same_name_non_public && target_visibility_owner.is_none());
            let protected_query_allowed =
                target_visibility_owner
                    .as_ref()
                    .is_some_and(|(visibility, owner)| {
                        *visibility == MethodVisibility::Protected
                            && protected_caller.is_some_and(|caller| {
                                protected_method_visible_from(self.engine, owner, caller)
                            })
                    });
            if non_public_target && !allow_private && !protected_query_allowed {
                continue;
            }
            ranges.extend(
                self.engine
                    .reference_facts_for(&target)
                    .iter()
                    .filter_map(|fact| {
                        if target_non_public
                            && fact.access == MethodReferenceAccess::ExplicitReceiver
                        {
                            if target_visibility_owner.as_ref().is_some_and(
                                |(visibility, owner)| {
                                    *visibility == MethodVisibility::Protected
                                        && self.reference_caller_can_see_protected(fact, owner)
                                },
                            ) {
                                Some(fact.range)
                            } else {
                                None
                            }
                        } else {
                            Some(fact.range)
                        }
                    }),
            );
        }
        for candidate in self
            .engine
            .reference_candidate_store()
            .method_candidates_named(*method)
        {
            let resolves_to_target = self
                .method_candidate_callees(candidate)
                .into_iter()
                .filter(|callee| {
                    callee.resolution == MethodCalleeResolution::Exact
                        && callee.method == *method
                        && !callee.definition_ranges.is_empty()
                })
                .map(|callee| {
                    FullyQualifiedName::method(callee.owner.namespace_parts(), callee.method)
                })
                .any(|target| target_set.contains(&target));
            if resolves_to_target {
                ranges.push(candidate.range);
            }
        }
        ranges.sort_by_key(|range| (range.file_id, range.start_byte, range.end_byte));
        ranges.dedup();
        ranges
    }

    fn method_target_visibility_owner(
        &self,
        method_fqn: &FullyQualifiedName,
        ancestor_chain: &[FullyQualifiedName],
    ) -> Option<(MethodVisibility, FullyQualifiedName)> {
        let FullyQualifiedName::Method(parts, method) = method_fqn else {
            return None;
        };
        self.engine.all_method_facts().iter().find_map(|fact| {
            let FullyQualifiedName::Method(_, fact_method) = &fact.fqn else {
                return None;
            };
            if fact_method != method || fact.owner.namespace_parts().as_slice() != parts.as_slice()
            {
                return None;
            }
            let effective =
                effective_method_visibility_for_chain(self.engine, ancestor_chain, fact, method);
            if effective.0 != MethodVisibility::Public {
                if let Some(override_fact) = global_visibility_override_for_method_owner_matching(
                    self.engine,
                    &fact.owner,
                    method,
                    MethodVisibility::Public,
                ) {
                    return Some((override_fact.visibility, override_fact.owner));
                }
            }
            if effective.0 == MethodVisibility::Public {
                if let Some(override_fact) =
                    global_visibility_override_for_method_owner(self.engine, &fact.owner, method)
                {
                    return Some((override_fact.visibility, override_fact.owner));
                }
            }
            Some(effective_method_visibility_for_chain(
                self.engine,
                ancestor_chain,
                fact,
                method,
            ))
        })
    }

    fn reference_caller_can_see_protected(
        &self,
        fact: &crate::core::ReferenceFact,
        protected_owner: &FullyQualifiedName,
    ) -> bool {
        let Some(caller_id) = fact.caller else {
            return false;
        };
        let Some(FullyQualifiedName::Method(parts, _method)) = self.engine.names.fqn(caller_id)
        else {
            return false;
        };
        let caller_namespace = FullyQualifiedName::namespace_with_kind(
            parts.clone(),
            crate::core::NamespaceKind::Instance,
        );
        protected_method_visible_from(self.engine, protected_owner, &caller_namespace)
    }

    fn method_lookup_has_visibility(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
        visibility: MethodVisibility,
    ) -> bool {
        let ancestor_chain = method_lookup_chain(self.engine, namespace_fqn);
        ancestor_chain.iter().any(|owner| {
            self.engine
                .method_facts_matching_owner_name(owner, method)
                .iter()
                .any(|fact| {
                    effective_method_visibility_for_chain(
                        self.engine,
                        &ancestor_chain,
                        fact,
                        method,
                    )
                    .0 == visibility
                })
        })
    }

    fn method_name_has_visibility(
        &self,
        method: &RubyMethod,
        visibility: MethodVisibility,
    ) -> bool {
        self.engine.all_method_facts().iter().any(|fact| {
            let FullyQualifiedName::Method(_, fact_method) = &fact.fqn else {
                return false;
            };
            fact_method == method && fact.visibility == visibility
        })
    }

    pub fn super_method_reference_ranges(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Vec<TextRange> {
        let Some(target) = self.super_method_reference_target(namespace_fqn, method) else {
            return Vec::new();
        };
        self.engine
            .reference_facts_for(&target)
            .iter()
            .map(|fact| fact.range)
            .collect()
    }

    pub fn method_reference_ranges_for_constant_receiver(
        &self,
        receiver_path: &[RubyConstant],
        context: &[RubyConstant],
        method: &RubyMethod,
    ) -> Vec<TextRange> {
        let namespace_fqn = self.resolve_constant_receiver(receiver_path, context);
        self.method_reference_ranges(&namespace_fqn, method)
    }

    pub fn method_reference_ranges_for_constant_receiver_public(
        &self,
        receiver_path: &[RubyConstant],
        context: &[RubyConstant],
        method: &RubyMethod,
    ) -> Vec<TextRange> {
        let namespace_fqn = self.resolve_constant_receiver(receiver_path, context);
        self.method_reference_ranges_public_receiver(&namespace_fqn, method)
    }

    pub fn method_reference_ranges_for_current_scope(
        &self,
        context: &[RubyConstant],
        method: &RubyMethod,
    ) -> Vec<TextRange> {
        let namespace_fqn = FullyQualifiedName::namespace_with_kind(
            context.to_vec(),
            crate::core::NamespaceKind::Instance,
        );
        self.method_reference_ranges(&namespace_fqn, method)
    }

    pub fn symbol_definition_ranges(
        &self,
        fqn: &FullyQualifiedName,
        allowed_kinds: &[SymbolKind],
    ) -> Vec<TextRange> {
        let mut facts = self
            .engine
            .symbol_facts_for(fqn)
            .into_iter()
            .filter(|fact| allowed_kinds.contains(&fact.kind))
            .collect::<Vec<_>>();
        if facts.iter().any(|fact| {
            self.engine
                .file(fact.range.file_id)
                .expect(
                    "INVARIANT VIOLATED: symbol fact references an unregistered source file. \
                     This is a bug because definition precedence requires stable source metadata. \
                     Fix: remove symbol facts through per-file replacement.",
                )
                .kind
                != crate::core::SourceKind::Signature
        }) {
            facts.retain(|fact| {
                self.engine
                    .file(fact.range.file_id)
                    .expect(
                        "INVARIANT VIOLATED: symbol fact references an unregistered source file. \
                         This is a bug because RBS overlay filtering requires valid file metadata. \
                         Fix: register sources before inserting symbol facts.",
                    )
                    .kind
                    != crate::core::SourceKind::Signature
            });
        }
        facts.into_iter().map(|fact| fact.range).collect()
    }

    fn resolve_constant_reference_target(
        &self,
        parts: &[RubyConstant],
        current_namespace: &[RubyConstant],
    ) -> Option<FullyQualifiedName> {
        let mut search = current_namespace.to_vec();

        loop {
            let mut probe = search.clone();
            probe.extend(parts.iter().cloned());

            let namespace_fqn = FullyQualifiedName::namespace(probe.clone());
            if !self.engine.graph_nodes_for(&namespace_fqn).is_empty()
                || !self.engine.symbol_facts_for(&namespace_fqn).is_empty()
            {
                return Some(namespace_fqn);
            }

            let constant_fqn = FullyQualifiedName::constant(probe);
            if !self.engine.symbol_facts_for(&constant_fqn).is_empty() {
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

fn non_core_fact_requires_ancestry_proof(
    engine: &crate::AnalysisEngine,
    requested_owner: &FullyQualifiedName,
    fact: &MethodFact,
    crossed_universal_root: bool,
) -> bool {
    if requested_owner == &fact.owner {
        return false;
    }

    let source_kind = engine
        .file(fact.range.file_id)
        .unwrap_or_else(|| {
            panic!(
                "INVARIANT VIOLATED: method fact `{}` references missing source file {}. This is a bug because every fact must remain owned by a registered file. Fix: remove facts before unregistering their source.",
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

fn is_universal_open_root(owner: &FullyQualifiedName) -> bool {
    if owner.namespace_parts().is_empty() {
        return true;
    }
    matches!(
        owner.namespace_parts().as_slice(),
        [name]
            if matches!(
                name.as_str(),
                "BasicObject" | "Object" | "Kernel" | "Module" | "Class"
            )
    )
}

fn method_name_is_refactorable(method: RubyMethod) -> bool {
    !matches!(
        method.as_str(),
        "+" | "-"
            | "*"
            | "/"
            | "%"
            | "**"
            | "+@"
            | "-@"
            | "<<"
            | ">>"
            | "&"
            | "|"
            | "^"
            | "~"
            | "<=>"
            | "<"
            | "<="
            | ">"
            | ">="
            | "=="
            | "==="
            | "!="
            | "=~"
            | "!~"
            | "[]"
            | "[]="
            | "`"
    )
}

fn constant_reference_name_range(
    engine: &crate::AnalysisEngine,
    range: TextRange,
    name: RubyConstant,
) -> Option<TextRange> {
    let file = engine.file(range.file_id)?;
    let start = usize::try_from(range.start_byte).ok()?;
    let end = usize::try_from(range.end_byte).ok()?;
    if let Some(source) = file.source_text() {
        let text = source.get(start..end)?;
        if !text.as_bytes().ends_with(name.as_str().as_bytes()) {
            return None;
        }
    } else {
        assert!(
            file.line_index.is_ascii(),
            "INVARIANT VIOLATED: source text was discarded for a non-ASCII file. \
             This is a bug because exact rename validation requires retained non-ASCII source. \
             Fix: retain SourceFile::source whenever SourceLineIndex::is_ascii is false."
        );
    }
    let name_start = end.checked_sub(name.as_str().len())?;
    if name_start < start {
        return None;
    }
    Some(TextRange::new(
        range.file_id,
        u32::try_from(name_start).ok()?,
        range.end_byte,
    ))
}

fn constant_name_collides(
    engine: &crate::AnalysisEngine,
    target: &FullyQualifiedName,
    new_name: RubyConstant,
) -> bool {
    let mut parts = target.namespace_parts();
    let last = parts.last_mut().expect(
        "INVARIANT VIOLATED: rename target has no constant path component. \
         This is a bug because constant_rename_target only returns constant-like FQNs. \
         Fix: reject empty constant paths before constructing a rename target.",
    );
    *last = new_name;

    let namespace = FullyQualifiedName::namespace(parts.clone());
    let constant = FullyQualifiedName::constant(parts);
    !engine.symbol_facts_for(&namespace).is_empty()
        || !engine.graph_nodes_for(&namespace).is_empty()
        || !engine.symbol_facts_for(&constant).is_empty()
}

fn method_name_from_fact(fact: &MethodFact) -> RubyMethod {
    let FullyQualifiedName::Method(_, method) = &fact.fqn else {
        panic!(
            "INVARIANT VIOLATED: method fact has non-method FQN `{}`. \
             This is a bug because method facts must be keyed by method FQNs. \
             Fix: only insert MethodFact values built from FullyQualifiedName::Method.",
            fact.fqn
        );
    };
    *method
}

pub(super) fn namespace_target_exists(
    engine: &crate::AnalysisEngine,
    fqn: &FullyQualifiedName,
) -> bool {
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
    let constant_fqn = FullyQualifiedName::constant(parts);

    !engine.graph_nodes_for(&instance_fqn).is_empty()
        || !engine.graph_nodes_for(&singleton_fqn).is_empty()
        || !engine.symbol_facts_for(&constant_fqn).is_empty()
}

fn is_module_instance_namespace(engine: &crate::AnalysisEngine, fqn: &FullyQualifiedName) -> bool {
    if fqn.namespace_kind() != Some(crate::core::NamespaceKind::Instance) {
        return false;
    }
    engine
        .graph_nodes_for(fqn)
        .iter()
        .any(|fact| fact.kind == GraphNodeKind::Module)
}

fn module_includers(
    engine: &crate::AnalysisEngine,
    module_fqn: &FullyQualifiedName,
) -> Vec<FullyQualifiedName> {
    let mut result = Vec::new();
    let mut visited = std::collections::HashSet::new();
    let mut queue = std::collections::VecDeque::new();

    for edge in engine.graph_edges_to(module_fqn) {
        if matches!(edge.kind, GraphEdgeKind::Include | GraphEdgeKind::Prepend)
            && visited.insert(edge.source.clone())
        {
            queue.push_back(edge.source.clone());
        }
    }

    while let Some(current) = queue.pop_front() {
        if node_kind(engine, &current) == Some(GraphNodeKind::Class) {
            result.push(current);
            continue;
        }

        if node_kind(engine, &current) == Some(GraphNodeKind::Module) {
            for edge in engine.graph_edges_to(&current) {
                if matches!(edge.kind, GraphEdgeKind::Include | GraphEdgeKind::Prepend)
                    && visited.insert(edge.source.clone())
                {
                    queue.push_back(edge.source.clone());
                }
            }
        }
    }

    result.sort_by_key(|fqn| fqn.to_string());
    result
}

pub(super) fn method_lookup_chain(
    engine: &crate::AnalysisEngine,
    fqn: &FullyQualifiedName,
) -> Vec<FullyQualifiedName> {
    let mut chain = method_lookup_chain_without_metaclass(engine, fqn);
    let Some(metaclass) = metaclass_namespace_for_object(engine, fqn) else {
        return chain;
    };
    let mut visited = chain
        .iter()
        .cloned()
        .collect::<std::collections::HashSet<_>>();
    build_mro(engine, &metaclass, &mut chain, &mut visited, false);
    chain
}

fn method_lookup_chain_without_metaclass(
    engine: &crate::AnalysisEngine,
    fqn: &FullyQualifiedName,
) -> Vec<FullyQualifiedName> {
    let allow_top_level_fallback = !method_lookup_chain_has_unresolved_dependency(engine, fqn);
    method_lookup_chain_without_metaclass_with_fallback(engine, fqn, allow_top_level_fallback)
}

fn method_lookup_chain_without_metaclass_with_fallback(
    engine: &crate::AnalysisEngine,
    fqn: &FullyQualifiedName,
    allow_top_level_fallback: bool,
) -> Vec<FullyQualifiedName> {
    assert!(
        matches!(fqn, FullyQualifiedName::Namespace(_, _)),
        "INVARIANT VIOLATED: analysis method lookup requested for non-namespace FQN: {fqn}. \
         This is a bug because only namespaces have method lookup chains. \
         Fix: resolve receivers to Namespace FQNs before method lookup."
    );

    if engine.graph_nodes_for(fqn).is_empty() {
        if fqn.namespace_parts().is_empty() {
            let mut chain = Vec::new();
            let mut visited = std::collections::HashSet::new();
            append_top_level_instance_fallback(engine, &mut chain, &mut visited);
            if chain.is_empty() {
                chain.push(fqn.clone());
            }
            chain
        } else {
            // A constant or inferred receiver without an indexed namespace
            // has no proven ancestry. Retain its exact receiver identity for
            // reference candidates, but never guess that it falls through to
            // top-level Object/Kernel methods.
            vec![fqn.clone()]
        }
    } else {
        let mut chain = Vec::new();
        let mut visited = std::collections::HashSet::new();
        build_mro(
            engine,
            fqn,
            &mut chain,
            &mut visited,
            is_universal_open_root(fqn),
        );

        let root = FullyQualifiedName::namespace_with_kind(
            Vec::new(),
            crate::core::NamespaceKind::Instance,
        );
        if !chain.contains(&root)
            && !fqn.namespace_parts().is_empty()
            && (is_module_instance_namespace(engine, fqn)
                || fqn.namespace_kind() == Some(crate::core::NamespaceKind::Singleton))
            && allow_top_level_fallback
        {
            append_universal_object_fallback(engine, &mut chain, &mut visited);
        }

        chain
    }
}

fn method_lookup_chain_has_unresolved_dependency(
    engine: &crate::AnalysisEngine,
    owner: &FullyQualifiedName,
) -> bool {
    let unresolved_sources = engine
        .graph
        .unresolved_edges()
        .into_iter()
        .filter(|edge| edge.provenance == GraphEdgeProvenance::Explicit)
        .map(|edge| edge.source)
        .collect::<HashSet<_>>();
    method_lookup_chain_has_unresolved_dependency_from_sources(engine, owner, &unresolved_sources)
}

fn method_lookup_chain_has_unresolved_dependency_cached(
    engine: &crate::AnalysisEngine,
    owner: &FullyQualifiedName,
    cache: &mut MethodLookupChainCache,
) -> bool {
    if let Some(incomplete) = cache.unresolved_dependencies.get(owner) {
        return *incomplete;
    }
    if cache.unresolved_edge_sources.is_none() {
        cache.unresolved_edge_sources = Some(
            engine
                .graph
                .unresolved_edges()
                .into_iter()
                .filter(|edge| edge.provenance == GraphEdgeProvenance::Explicit)
                .map(|edge| edge.source)
                .collect(),
        );
    }
    let incomplete = method_lookup_chain_has_unresolved_dependency_from_sources(
        engine,
        owner,
        cache.unresolved_edge_sources.as_ref().expect(
            "INVARIANT VIOLATED: unresolved method-edge sources disappeared after initialization. This is a bug because a resolution-local cache is immutable between candidate lookups. Fix: initialize the source set once and retain it for the resolve pass.",
        ),
    );
    assert!(
        cache
            .unresolved_dependencies
            .insert(owner.clone(), incomplete)
            .is_none(),
        "INVARIANT VIOLATED: unresolved method-dependency cache replaced an entry after a confirmed miss. This is a bug because the engine graph is immutable during one resolve pass. Fix: keep lookup and insertion in one candidate step."
    );
    incomplete
}

fn method_lookup_chain_has_unresolved_dependency_from_sources(
    engine: &crate::AnalysisEngine,
    owner: &FullyQualifiedName,
    unresolved_sources: &HashSet<FqnId>,
) -> bool {
    let mut pending = vec![owner.clone()];
    let mut visited = HashSet::new();

    while let Some(current) = pending.pop() {
        if !visited.insert(current.clone()) {
            continue;
        }
        if engine.superclass_is_ambiguous(&current) {
            return true;
        }

        let unresolved_source = match current.namespace_kind() {
            Some(crate::core::NamespaceKind::Singleton) => current
                .to_instance_namespace()
                .expect(
                    "INVARIANT VIOLATED: singleton lookup owner cannot produce an instance namespace. This is a bug because unresolved superclass facts are keyed by class/module instance identity. Fix: preserve Namespace identity while checking method proof barriers.",
                ),
            Some(crate::core::NamespaceKind::Instance) => current.clone(),
            None => panic!(
                "INVARIANT VIOLATED: method lookup proof barrier received non-namespace FQN `{current}`. This is a bug because only namespace receivers have ancestry. Fix: convert receiver types before method resolution."
            ),
        };
        if let Some(source_id) = engine.names.fqn_id(&unresolved_source) {
            if unresolved_sources.contains(&source_id) {
                return true;
            }
        }

        pending.extend(
            engine
                .graph_edges_from(&current)
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

fn metaclass_namespace_for_object(
    engine: &crate::AnalysisEngine,
    fqn: &FullyQualifiedName,
) -> Option<FullyQualifiedName> {
    if fqn.namespace_kind() != Some(crate::core::NamespaceKind::Singleton) {
        return None;
    }
    let metaclass_name = match node_kind(engine, fqn) {
        Some(GraphNodeKind::Class) => "Class",
        Some(GraphNodeKind::Module) => "Module",
        None => return None,
    };
    let metaclass = FullyQualifiedName::namespace_with_kind(
        vec![RubyConstant::new(metaclass_name).unwrap_or_else(|error| {
            panic!(
                "INVARIANT VIOLATED: Ruby metaclass name `{metaclass_name}` is invalid: {error}. This is a bug because Class and Module are universal Ruby constants. Fix: preserve RubyConstant support for language-defined class names."
            )
        })],
        crate::core::NamespaceKind::Instance,
    );
    (!engine.graph_nodes_for(&metaclass).is_empty()).then_some(metaclass)
}

pub(super) fn method_lookup_chain_for_reference_cached<'cache>(
    engine: &crate::AnalysisEngine,
    fqn: &FullyQualifiedName,
    chain_cache: &'cache mut MethodLookupChainCache,
) -> &'cache [FqnId] {
    if !chain_cache.chains.contains_key(fqn) {
        let allow_top_level_fallback =
            !method_lookup_chain_has_unresolved_dependency_cached(engine, fqn, chain_cache);
        let chain = method_lookup_chain_without_metaclass_with_fallback(
            engine,
            fqn,
            allow_top_level_fallback,
        )
        .into_iter()
        // An uninterned fallback namespace cannot own graph or method
        // facts because both stores are keyed exclusively by FqnId.
        // Omitting it is therefore the exact ID-domain equivalent of
        // the previous owner lookup returning Missing.
        .filter_map(|fqn| engine.names.fqn_id(&fqn))
        .collect();
        assert!(
            chain_cache.chains.insert(fqn.clone(), chain).is_none(),
            "INVARIANT VIOLATED: method lookup-chain cache replaced an entry after a confirmed miss. This is a bug because the engine graph is immutable during one resolve pass. Fix: keep chain construction and insertion in one candidate step."
        );
    }
    chain_cache
        .chains
        .get(fqn)
        .expect(
            "INVARIANT VIOLATED: method lookup-chain cache lost an entry immediately after insertion. This is a bug because the cache is not mutated between insertion and lookup. Fix: keep cached chain access in one resolution step.",
        )
        .as_slice()
}

fn append_top_level_instance_fallback(
    engine: &crate::AnalysisEngine,
    chain: &mut Vec<FullyQualifiedName>,
    visited: &mut std::collections::HashSet<FullyQualifiedName>,
) {
    let fallback = engine
        .cached_top_level_method_lookup_chain()
        .unwrap_or_else(|| {
            let mut fallback = Vec::new();
            let mut fallback_visited = std::collections::HashSet::new();
            compute_top_level_instance_fallback(engine, &mut fallback, &mut fallback_visited);
            engine.cache_top_level_method_lookup_chain(fallback.clone());
            fallback
        });
    for fqn in fallback {
        if visited.insert(fqn.clone()) {
            chain.push(fqn);
        }
    }
}

fn append_universal_object_fallback(
    engine: &crate::AnalysisEngine,
    chain: &mut Vec<FullyQualifiedName>,
    visited: &mut std::collections::HashSet<FullyQualifiedName>,
) {
    for fqn in compute_universal_object_fallback(engine) {
        if visited.insert(fqn.clone()) {
            chain.push(fqn);
        }
    }
}

fn compute_universal_object_fallback(engine: &crate::AnalysisEngine) -> Vec<FullyQualifiedName> {
    if let Some(cached) = engine.cached_universal_object_method_lookup_chain() {
        return cached;
    }
    let mut fallback = Vec::new();
    let mut visited = std::collections::HashSet::new();
    let object = top_level_object_instance_fqn();
    if !engine.graph_nodes_for(&object).is_empty() {
        build_mro(engine, &object, &mut fallback, &mut visited, false);
    }
    engine.cache_universal_object_method_lookup_chain(fallback.clone());
    fallback
}

fn unproven_universal_method_exists(
    engine: &crate::AnalysisEngine,
    universal_roots: &[FqnId],
    method: &RubyMethod,
    cache: &mut MethodLookupChainCache,
) -> bool {
    for root_id in universal_roots {
        if !cache.unproven_universal_methods.contains_key(root_id) {
            let root = engine.names.fqn(*root_id).expect(
                "INVARIANT VIOLATED: universal lookup root ID is absent from the name registry. This is a bug because roots are selected from a cached chain owned by the same immutable registry. Fix: invalidate resolution-local caches whenever names change.",
            );
            let mut broad = Vec::new();
            let mut broad_visited = HashSet::new();
            build_mro(engine, root, &mut broad, &mut broad_visited, true);
            let mut proven = Vec::new();
            let mut proven_visited = HashSet::new();
            build_mro(engine, root, &mut proven, &mut proven_visited, false);
            let proven = proven.into_iter().collect::<HashSet<_>>();
            let mut methods = HashSet::new();
            for owner in broad.into_iter().filter(|owner| !proven.contains(owner)) {
                let Some(owner_id) = engine.names.fqn_id(&owner) else {
                    continue;
                };
                methods.extend(engine.ruby_method_names_for_owner_id(owner_id));
            }
            assert!(
                cache
                    .unproven_universal_methods
                    .insert(*root_id, methods)
                    .is_none(),
                "INVARIANT VIOLATED: unproven universal-method cache replaced one root after a confirmed miss. This is a bug because the engine graph is immutable during one resolve pass. Fix: keep root lookup and insertion atomic."
            );
        }
        if cache
            .unproven_universal_methods
            .get(root_id)
            .expect(
                "INVARIANT VIOLATED: unproven universal-method cache lost one root immediately after insertion. This is a bug because the resolution-local cache is not cleared during one pass. Fix: retain root entries for the cache lifetime.",
            )
            .contains(method)
        {
            return true;
        }
    }
    false
}

fn compute_top_level_instance_fallback(
    engine: &crate::AnalysisEngine,
    chain: &mut Vec<FullyQualifiedName>,
    visited: &mut std::collections::HashSet<FullyQualifiedName>,
) {
    let root =
        FullyQualifiedName::namespace_with_kind(Vec::new(), crate::core::NamespaceKind::Instance);
    build_mro(engine, &root, chain, visited, true);

    let object_fqn = top_level_object_instance_fqn();
    if !engine.graph_nodes_for(&object_fqn).is_empty() {
        build_mro(engine, &object_fqn, chain, visited, true);
    }
}

fn top_level_object_instance_fqn() -> FullyQualifiedName {
    FullyQualifiedName::namespace_with_kind(
        vec![RubyConstant::new("Object").expect(
            "INVARIANT VIOLATED: `Object` is not a valid Ruby constant. \
             This is a bug because Ruby core class names must be valid constants. \
             Fix: inspect RubyConstant validation.",
        )],
        crate::core::NamespaceKind::Instance,
    )
}

fn build_mro(
    engine: &crate::AnalysisEngine,
    fqn: &FullyQualifiedName,
    chain: &mut Vec<FullyQualifiedName>,
    visited: &mut std::collections::HashSet<FullyQualifiedName>,
    allow_non_language_universal_edges: bool,
) {
    if !visited.insert(fqn.clone()) {
        return;
    }

    let language_owned_only = is_universal_open_root(fqn) && !allow_non_language_universal_edges;
    let edge_is_allowed = |edge: &GraphEdgeFact| {
        !language_owned_only || method_lookup_edge_is_language_owned(engine, edge)
    };

    let mut prepends = edges_from(engine, fqn, GraphEdgeKind::Prepend)
        .into_iter()
        .filter(&edge_is_allowed)
        .collect::<Vec<_>>();
    for edge in prepends.iter_mut().rev() {
        build_mro(
            engine,
            &edge.target,
            chain,
            visited,
            allow_non_language_universal_edges,
        );
    }

    chain.push(fqn.clone());

    let mut includes = edges_from(engine, fqn, GraphEdgeKind::Include)
        .into_iter()
        .filter(&edge_is_allowed)
        .collect::<Vec<_>>();
    for edge in includes.iter_mut().rev() {
        build_mro(
            engine,
            &edge.target,
            chain,
            visited,
            allow_non_language_universal_edges,
        );
    }

    let mut included_hook_extends = included_hook_extend_edges(engine, fqn, language_owned_only)
        .into_iter()
        .filter(&edge_is_allowed)
        .collect::<Vec<_>>();
    for edge in included_hook_extends.iter_mut().rev() {
        build_mro(
            engine,
            &edge.target,
            chain,
            visited,
            allow_non_language_universal_edges,
        );
    }

    if let Some(superclass) = engine.proven_superclass_edge(fqn).filter(&edge_is_allowed) {
        build_mro(
            engine,
            &superclass.target,
            chain,
            visited,
            allow_non_language_universal_edges,
        );
    }
}

fn method_lookup_edge_is_language_owned(
    engine: &crate::AnalysisEngine,
    edge: &GraphEdgeFact,
) -> bool {
    matches!(
        engine
            .file(edge.range.file_id)
            .unwrap_or_else(|| {
                panic!(
                    "INVARIANT VIOLATED: method lookup graph edge references missing source file {}. This is a bug because every graph edge must remain owned by a registered source. Fix: remove graph edges before unregistering their file.",
                    edge.range.file_id.0,
                )
            })
            .kind,
        SourceKind::Stub | SourceKind::Signature
    )
}

fn included_hook_extend_edges(
    engine: &crate::AnalysisEngine,
    fqn: &FullyQualifiedName,
    language_owned_only: bool,
) -> Vec<GraphEdgeFact> {
    if fqn.namespace_kind() != Some(crate::core::NamespaceKind::Singleton) {
        return Vec::new();
    }
    let Some(instance_fqn) = fqn.to_instance_namespace() else {
        return Vec::new();
    };

    let mut hook_edges = Vec::new();
    for edge in edges_from(engine, &instance_fqn, GraphEdgeKind::Include)
        .into_iter()
        .chain(edges_from(engine, &instance_fqn, GraphEdgeKind::Prepend))
        .filter(|edge| !language_owned_only || method_lookup_edge_is_language_owned(engine, edge))
    {
        hook_edges.extend(
            edges_from(engine, &edge.target, GraphEdgeKind::Extend)
                .into_iter()
                .filter(|hook| {
                    !language_owned_only || method_lookup_edge_is_language_owned(engine, hook)
                }),
        );
    }
    hook_edges
}

fn edges_from(
    engine: &crate::AnalysisEngine,
    fqn: &FullyQualifiedName,
    kind: GraphEdgeKind,
) -> Vec<GraphEdgeFact> {
    engine
        .graph_edges_from(fqn)
        .iter()
        .filter(|edge| edge.kind == kind)
        .cloned()
        .collect()
}

pub(super) fn execution_context_application_targets(
    engine: &crate::AnalysisEngine,
    template: &FullyQualifiedName,
) -> Vec<FullyQualifiedName> {
    let mut targets = edges_from(engine, template, GraphEdgeKind::ExecutionContextApplication)
        .into_iter()
        .map(|edge| edge.target)
        .collect::<Vec<_>>();
    targets.sort_by_key(ToString::to_string);
    targets.dedup();
    targets
}

fn method_callee_in_chain(
    engine: &crate::AnalysisEngine,
    ancestor_chain: &[FullyQualifiedName],
    method: &RubyMethod,
    resolution: MethodCalleeResolution,
    allow_private: bool,
    protected_caller: Option<&FullyQualifiedName>,
) -> Option<ResolvedMethodCallee> {
    let (owner, facts) = method_facts_in_chain(
        engine,
        ancestor_chain,
        method,
        allow_private,
        protected_caller,
    )?;
    Some(ResolvedMethodCallee {
        owner,
        method: *method,
        resolution,
        definition_ranges: facts.into_iter().map(|fact| fact.range).collect(),
    })
}

pub(super) fn method_facts_in_chain(
    engine: &crate::AnalysisEngine,
    ancestor_chain: &[FullyQualifiedName],
    method: &RubyMethod,
    allow_private: bool,
    protected_caller: Option<&FullyQualifiedName>,
) -> Option<(FullyQualifiedName, Vec<MethodFact>)> {
    for ancestor in ancestor_chain {
        let mut facts = engine
            .method_facts_matching_owner_name(ancestor, method)
            .into_iter()
            .filter(|fact| {
                ancestor_chain.iter().any(|chain_fqn| {
                    chain_fqn.namespace_parts() == fact.owner.namespace_parts()
                        && chain_fqn.namespace_kind() == fact.owner.namespace_kind()
                }) && {
                    let (visibility, owner) =
                        effective_method_visibility_for_chain(engine, ancestor_chain, fact, method);
                    method_visibility_allowed(
                        engine,
                        visibility,
                        &owner,
                        allow_private,
                        protected_caller,
                    )
                }
            })
            .collect::<Vec<_>>();

        if facts.iter().any(|fact| {
            engine
                .file(fact.range.file_id)
                .expect(
                    "INVARIANT VIOLATED: method fact references an unregistered source file. \
                     This is a bug because engine facts must never outlive their file metadata. \
                     Fix: register the file before replacing method facts.",
                )
                .kind
                != crate::core::SourceKind::Signature
        }) {
            facts.retain(|fact| {
                engine
                    .file(fact.range.file_id)
                    .expect(
                        "INVARIANT VIOLATED: method fact references an unregistered source file. \
                         This is a bug because source precedence requires valid file metadata. \
                         Fix: remove facts through the per-file replacement lifecycle.",
                    )
                    .kind
                    != crate::core::SourceKind::Signature
            });
        }

        if !facts.is_empty() {
            facts.sort_by_key(|fact| {
                (
                    fact.range.file_id,
                    fact.range.start_byte,
                    fact.range.end_byte,
                    fact.fqn.to_string(),
                )
            });
            facts.dedup();
            return Some((ancestor.clone(), facts));
        }
    }

    None
}

fn private_method_in_chain(
    engine: &crate::AnalysisEngine,
    ancestor_chain: &[FullyQualifiedName],
    method: &RubyMethod,
) -> bool {
    ancestor_chain.iter().any(|ancestor| {
        engine
            .method_facts_matching_owner_name(ancestor, method)
            .iter()
            .any(|fact| {
                ancestor_chain.iter().any(|chain_fqn| {
                    chain_fqn.namespace_parts() == fact.owner.namespace_parts()
                        && chain_fqn.namespace_kind() == fact.owner.namespace_kind()
                }) && effective_method_visibility_for_chain(engine, ancestor_chain, fact, method).0
                    != MethodVisibility::Public
            })
    })
}

pub(super) fn effective_method_visibility_for_chain(
    engine: &crate::AnalysisEngine,
    ancestor_chain: &[FullyQualifiedName],
    fact: &crate::core::MethodFact,
    method: &RubyMethod,
) -> (MethodVisibility, FullyQualifiedName) {
    if let Some(override_fact) =
        method_visibility_override_for_chain(engine, ancestor_chain, &fact.owner, method)
    {
        return (override_fact.visibility, override_fact.owner);
    }
    (fact.visibility, fact.owner.clone())
}

fn method_visibility_override_for_chain(
    engine: &crate::AnalysisEngine,
    ancestor_chain: &[FullyQualifiedName],
    method_owner: &FullyQualifiedName,
    method: &RubyMethod,
) -> Option<crate::core::MethodVisibilityOverrideFact> {
    for ancestor in ancestor_chain {
        let mut overrides =
            engine.method_visibility_overrides_matching_owner_name(ancestor, method);
        overrides.sort_by_key(|fact| {
            (
                fact.range.file_id,
                fact.range.start_byte,
                fact.range.end_byte,
            )
        });
        if let Some(override_fact) = overrides.pop() {
            return Some(override_fact);
        }
        if ancestor.namespace_parts() == method_owner.namespace_parts()
            && ancestor.namespace_kind() == method_owner.namespace_kind()
        {
            break;
        }
    }
    None
}

fn global_visibility_override_for_method_owner(
    engine: &crate::AnalysisEngine,
    method_owner: &FullyQualifiedName,
    method: &RubyMethod,
) -> Option<crate::core::MethodVisibilityOverrideFact> {
    let mut public_overrides = Vec::new();
    let mut non_public_overrides = Vec::new();
    for override_fact in engine.all_method_visibility_overrides() {
        if override_fact.method != *method {
            continue;
        }
        if !method_lookup_chain(engine, &override_fact.owner)
            .iter()
            .any(|ancestor| {
                ancestor.namespace_parts() == method_owner.namespace_parts()
                    && ancestor.namespace_kind() == method_owner.namespace_kind()
            })
        {
            continue;
        }
        if override_fact.visibility == MethodVisibility::Public {
            public_overrides.push(override_fact);
        } else {
            non_public_overrides.push(override_fact);
        }
    }
    let sort_key = |fact: &crate::core::MethodVisibilityOverrideFact| {
        (
            fact.range.file_id,
            fact.range.start_byte,
            fact.range.end_byte,
        )
    };
    public_overrides.sort_by_key(sort_key);
    non_public_overrides.sort_by_key(sort_key);
    non_public_overrides
        .pop()
        .or_else(|| public_overrides.pop())
}

fn global_visibility_override_for_method_owner_matching(
    engine: &crate::AnalysisEngine,
    method_owner: &FullyQualifiedName,
    method: &RubyMethod,
    visibility: MethodVisibility,
) -> Option<crate::core::MethodVisibilityOverrideFact> {
    let mut overrides = engine
        .all_method_visibility_overrides()
        .into_iter()
        .filter(|override_fact| {
            override_fact.method == *method
                && override_fact.visibility == visibility
                && method_lookup_chain(engine, &override_fact.owner)
                    .iter()
                    .any(|ancestor| {
                        ancestor.namespace_parts() == method_owner.namespace_parts()
                            && ancestor.namespace_kind() == method_owner.namespace_kind()
                    })
        })
        .collect::<Vec<_>>();
    overrides.sort_by_key(|fact| {
        (
            fact.range.file_id,
            fact.range.start_byte,
            fact.range.end_byte,
        )
    });
    overrides.pop()
}

fn method_visibility_allowed(
    engine: &crate::AnalysisEngine,
    visibility: MethodVisibility,
    owner: &FullyQualifiedName,
    allow_private: bool,
    protected_caller: Option<&FullyQualifiedName>,
) -> bool {
    match visibility {
        MethodVisibility::Public => true,
        MethodVisibility::Private => allow_private,
        MethodVisibility::Protected => {
            allow_private
                || protected_caller
                    .is_some_and(|caller| protected_method_visible_from(engine, owner, caller))
        }
    }
}

pub(super) fn protected_method_visible_from(
    engine: &crate::AnalysisEngine,
    protected_owner: &FullyQualifiedName,
    caller_namespace: &FullyQualifiedName,
) -> bool {
    method_lookup_chain(engine, caller_namespace)
        .iter()
        .any(|ancestor| {
            ancestor.namespace_parts() == protected_owner.namespace_parts()
                && ancestor.namespace_kind() == protected_owner.namespace_kind()
        })
}

fn receiver_only_callee(owner: FullyQualifiedName, method: &RubyMethod) -> ResolvedMethodCallee {
    ResolvedMethodCallee {
        owner,
        method: *method,
        resolution: MethodCalleeResolution::ReceiverOnly,
        definition_ranges: Vec::new(),
    }
}

fn method_missing_callee_in_chain(
    engine: &crate::AnalysisEngine,
    ancestor_chain: &[FullyQualifiedName],
) -> Option<ResolvedMethodCallee> {
    let method_missing = method_missing_method();
    let callee = method_callee_in_chain(
        engine,
        ancestor_chain,
        &method_missing,
        MethodCalleeResolution::MethodMissing,
        true,
        None,
    )?;
    if default_basic_object_method_missing_callee(engine, &callee) {
        return None;
    }
    Some(callee)
}

fn default_basic_object_method_missing_fact(
    engine: &crate::AnalysisEngine,
    fact: &MethodFact,
) -> bool {
    fact.owner == basic_object_instance_fqn()
        && method_name_from_fact(fact) == method_missing_method()
        && engine.file(fact.range.file_id).is_some_and(|file| {
            matches!(
                file.kind,
                crate::core::SourceKind::Stub | crate::core::SourceKind::Signature
            )
        })
}

fn default_basic_object_method_missing_callee(
    engine: &crate::AnalysisEngine,
    callee: &ResolvedMethodCallee,
) -> bool {
    callee.owner == basic_object_instance_fqn()
        && !callee.definition_ranges.is_empty()
        && callee.definition_ranges.iter().all(|range| {
            engine.file(range.file_id).is_some_and(|file| {
                matches!(
                    file.kind,
                    crate::core::SourceKind::Stub | crate::core::SourceKind::Signature
                )
            })
        })
}

fn basic_object_instance_fqn() -> FullyQualifiedName {
    FullyQualifiedName::namespace_with_kind(
        vec![RubyConstant::new("BasicObject").expect(
            "INVARIANT VIOLATED: `BasicObject` is not a valid Ruby constant. \
             This is a bug because Ruby core class names must be valid constants. \
             Fix: update RubyConstant validation to accept core Ruby class names.",
        )],
        crate::core::NamespaceKind::Instance,
    )
}

fn method_callee_after_owner(
    engine: &crate::AnalysisEngine,
    ancestor_chain: &[FullyQualifiedName],
    owner: &FullyQualifiedName,
    method: &RubyMethod,
) -> Option<ResolvedMethodCallee> {
    let mut seen_owner = false;
    for ancestor in ancestor_chain {
        if !seen_owner {
            seen_owner = ancestor == owner;
            continue;
        }

        let candidate_chain = std::slice::from_ref(ancestor);
        if let Some(callee) = method_callee_in_chain(
            engine,
            candidate_chain,
            method,
            MethodCalleeResolution::Exact,
            true,
            None,
        ) {
            return Some(callee);
        }
    }

    None
}

pub(super) fn method_missing_method() -> RubyMethod {
    RubyMethod::new("method_missing").expect(
        "INVARIANT VIOLATED: `method_missing` is not a valid Ruby method name. \
         This is a bug because Ruby's fallback dispatch method must be representable. \
         Fix: update RubyMethod validation to accept core Ruby method names.",
    )
}

fn resolve_constant_fqn(
    engine: &crate::AnalysisEngine,
    parts: &[RubyConstant],
    absolute: bool,
    context_fqn: &FullyQualifiedName,
) -> Option<FullyQualifiedName> {
    let mut search_namespaces = if absolute {
        Vec::new()
    } else {
        context_fqn.namespace_parts()
    };

    loop {
        let mut probe = search_namespaces.clone();
        probe.extend(parts.iter().cloned());

        let namespace_fqn = FullyQualifiedName::namespace_with_kind(
            probe.clone(),
            crate::core::NamespaceKind::Instance,
        );
        if !engine.graph_nodes_for(&namespace_fqn).is_empty() {
            return Some(namespace_fqn);
        }

        let constant_fqn = FullyQualifiedName::constant(probe);
        if !engine.symbol_facts_for(&constant_fqn).is_empty() {
            return Some(constant_fqn);
        }

        if absolute || search_namespaces.is_empty() {
            break;
        }
        search_namespaces.pop();
    }

    None
}

pub(super) fn node_kind(
    engine: &crate::AnalysisEngine,
    fqn: &FullyQualifiedName,
) -> Option<GraphNodeKind> {
    engine.graph_nodes_for(fqn).first().map(|fact| fact.kind)
}
