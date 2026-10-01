//! Union-receiver (grouped) method dispatch: callee resolution, references, and
//! diagnostics across every receiver member.

use std::collections::{HashMap, HashSet};

use super::MethodChainCompletenessCache;
use crate::core::names::fqn_id::FqnId;
use crate::core::{
    DiagnosticFact, FullyQualifiedName, MethodCalleeResolution, MethodFact, MethodReferenceAccess,
    ResolvedMethodCallee, RubyConstant, RubyMethod, RubyType, SourceFileId,
};
use crate::engine::{AnalysisEngine, AnalysisQuery};

impl AnalysisEngine {
    pub(super) fn resolve_grouped_method_callees(
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
            MethodReferenceAccess::Normal
            | MethodReferenceAccess::VisibilityBypass
            | MethodReferenceAccess::InstanceMethodReflection => {
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

    pub(super) fn push_grouped_method_fact_diagnostics(
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

    pub(super) fn push_grouped_unresolved_method_diagnostic(
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
}

pub(super) fn grouped_method_targets(
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
