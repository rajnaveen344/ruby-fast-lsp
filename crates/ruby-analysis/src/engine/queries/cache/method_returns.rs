//! Method return type queries, including cached receiver lookups.

use crate::invariant::ExpectInvariant;
use std::collections::HashSet;

use crate::core::{
    FullyQualifiedName, MethodFact, NamespaceKind, ResolvedMethodCallee, RubyMethod, RubyType,
    SourceFileId, TypeResolution, TypeSubject,
};
use crate::engine::lookup::{self, LookupReceiver, MethodRequest, MethodWant};
use crate::engine::queries::View;
use crate::engine::resolution::{
    chain_has_custom_method_missing, execution_context_application_targets, method_lookup_chain,
    method_missing_method, module_instance_receivers, namespace_target_exists,
};
use crate::inference::semantics::ReceiverAccess;

use super::thread_memo::thread_protected_return_may_differ;

type MethodVisitKey = (FullyQualifiedName, SourceFileId, u32, u32);

impl<'a> View<'a> {
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

        match self.type_at(
            &TypeSubject::MethodReturn(fact.fqn.clone()),
            fact.range.file_id,
            fact.range.end_byte,
        ) {
            TypeResolution::Resolved(type_fact) => return Some(type_fact.ruby_type),
            TypeResolution::Ambiguous(_) | TypeResolution::Unresolved => {}
        }

        let FullyQualifiedName::Method(_, method) = &fact.fqn else {
            unreachable_invariant!(
                what = "method return lookup received a non-method fact {}",
                why = "MethodFact FQNs must always use the Method variant",
                fix = "validate method facts before engine insertion",
                fact.fqn,
            );
        };
        let signatures = self
            .method_facts_matching_owner_name(&fact.owner, method)
            .into_iter()
            .filter(|signature| {
                self.file(signature.range.file_id)
                    .expect_invariant(
                        "RBS method fact references an unregistered source file",
                        "type overlay requires stable signature metadata",
                        "remove signature facts through per-file replacement",
                    )
                    .kind
                    == crate::core::SourceKind::Signature
            })
            .collect::<Vec<_>>();
        if !signatures.is_empty() {
            return RubyType::union_from_proven(signatures, |signature| {
                match self.type_at(
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

    /// A return type through receiver dispatch with the given visibility.
    pub(in crate::engine) fn method_return_type_for_receiver_access(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
        access: ReceiverAccess<'_>,
    ) -> Option<RubyType> {
        self.method_return_type_for_receiver_inner(
            namespace_fqn,
            method,
            access,
            &mut HashSet::new(),
        )
    }

    /// The access a memoized return lookup is keyed and computed with. A
    /// protected lookup that the return walk would answer exactly like a
    /// public one shares the public entry, so the caller namespace is not
    /// part of the hot key.
    pub(in crate::engine) fn return_memo_access<'r>(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
        access: ReceiverAccess<'r>,
    ) -> ReceiverAccess<'r> {
        match access {
            ReceiverAccess::Protected { .. }
                if !thread_protected_return_may_differ(
                    self.engine.query_cache_identity(),
                    namespace_fqn,
                    *method,
                    || self.protected_return_may_differ(namespace_fqn, method),
                ) =>
            {
                ReceiverAccess::Public
            }
            ReceiverAccess::Any | ReceiverAccess::Protected { .. } | ReceiverAccess::Public => {
                access
            }
        }
    }

    /// Whether the return walk may answer a protected lookup differently from
    /// a public one. Only the receiver's own chain is compared, so a walk
    /// that leaves it, through module includers, execution-context
    /// applications, or a custom `method_missing`, may differ.
    fn protected_return_may_differ(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> bool {
        if !module_instance_receivers(self.engine, namespace_fqn).is_empty() {
            return true;
        }
        let private_facts = self.own_chain_method_facts(namespace_fqn, method, ReceiverAccess::Any);
        let public_facts =
            self.own_chain_method_facts(namespace_fqn, method, ReceiverAccess::Public);
        match (private_facts, public_facts) {
            (None, None) => {
                !execution_context_application_targets(self.engine, namespace_fqn).is_empty()
                    || (*method != method_missing_method()
                        && chain_has_custom_method_missing(
                            self.engine,
                            &method_lookup_chain(self.engine, namespace_fqn),
                        ))
            }
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

    /// The winning facts on `namespace_fqn`'s own ancestor chain. Reflection
    /// is the right receiver: callers dispatch to module includers first.
    fn own_chain_method_facts(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
        access: ReceiverAccess<'_>,
    ) -> Option<(FullyQualifiedName, Vec<MethodFact>)> {
        lookup::method(
            self,
            MethodRequest {
                receiver: LookupReceiver::Reflection(namespace_fqn),
                method: *method,
                access,
                want: MethodWant::Facts,
            },
        )
        .into_facts()
    }

    fn method_return_type_for_receiver_inner(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
        access: ReceiverAccess<'_>,
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
                    access,
                    &mut seen.clone(),
                )
            });
        }

        if let Some((_owner, facts)) = self.own_chain_method_facts(namespace_fqn, method, access) {
            return RubyType::union_from_proven(facts, |fact| {
                self.method_return_type_inner(&fact, seen)
            });
        }

        let applications = execution_context_application_targets(self.engine, namespace_fqn);
        if !applications.is_empty() {
            return RubyType::union_from_proven(applications, |application| {
                self.method_return_type_for_receiver_inner(&application, method, access, seen)
            });
        }

        // Default BasicObject#method_missing is language fallback, not a proven
        // return. Navigation already treats that stub as Missing; walking it
        // here redoes MRO on every unresolved call.
        if *method != method_missing_method()
            && chain_has_custom_method_missing(
                self.engine,
                &method_lookup_chain(self.engine, namespace_fqn),
            )
        {
            return self.method_return_type_for_receiver_inner(
                namespace_fqn,
                &method_missing_method(),
                access,
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
            ReceiverAccess::Any,
            seen,
        )?;

        RubyType::union_from_proven(
            crate::core::receiver_type_to_method_namespaces(&receiver_type),
            |namespace| {
                self.method_return_type_for_receiver_inner(
                    &namespace,
                    delegated_method,
                    ReceiverAccess::Any,
                    seen,
                )
            },
        )
    }
}
