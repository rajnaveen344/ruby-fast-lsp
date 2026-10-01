//! Single-file resolve pass: re-resolves one file's reference candidates and
//! replaces its references, call outcomes, and diagnostics.

use std::collections::HashMap;

use super::grouped_methods::grouped_method_targets;
use super::{constant_name, MethodCallOutcomeCaches, MethodChainCompletenessCache};
use crate::core::{
    ConstLookup, ConstantPath, DiagnosticFact, FullyQualifiedName, MethodReferenceAccess,
    ReferenceFact, RubyMethod, RubyType, SourceFileId, StoredReferenceCandidateKind,
    TypeInferenceOutcome, UnknownReason,
};
use crate::engine::resolution::{MethodLookupChainCache, MethodLookupResult};
use crate::engine::{AnalysisEngine, AnalysisQuery};

impl AnalysisEngine {
    pub(in crate::engine) fn resolve_reference_candidates_in_file(
        &mut self,
        file_id: SourceFileId,
    ) {
        let reference_candidates = self.facts.references.candidates.candidates_in_file(file_id);
        let mut unresolved =
            HashMap::from([(file_id, self.resolve_diagnostic_candidates_in_file(file_id))]);
        let mut method_fact_cache: HashMap<
            (FullyQualifiedName, RubyMethod, bool, bool),
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
                    let reflects_instance =
                        access == MethodReferenceAccess::InstanceMethodReflection;
                    let mut fact = method_fact_cache
                        .entry((owner_fqn.clone(), method, is_super, reflects_instance))
                        .or_insert_with(|| {
                            let query = AnalysisQuery::new(self);
                            if is_super {
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
            .filter(|fact| fact.code != "nil-call")
            .collect::<Vec<_>>();
        diagnostics.extend(unresolved.remove(&file_id).unwrap_or_default());
        self.facts
            .diagnostics
            .resolved
            .replace_file(file_id, diagnostics);
    }
}
