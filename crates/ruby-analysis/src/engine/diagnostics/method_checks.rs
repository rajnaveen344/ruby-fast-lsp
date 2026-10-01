//! Checks on calls that resolve to a method: runtime availability and
//! signature (arity and keyword) diagnostics.

use std::collections::HashMap;

use log::debug;

use super::helpers::{arity_mismatch, closest_keyword, MethodArity};
use crate::core::{
    DiagnosticFact, FullyQualifiedName, MethodAvailability, MethodCallSignatureCandidate,
    MethodFact, SourceFileId, TextRange,
};
use crate::engine::resolution::method_lookup_chain;
use crate::engine::AnalysisEngine;

impl AnalysisEngine {
    pub(super) fn push_unavailable_method_diagnostic(
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

    pub(super) fn push_signature_diagnostics(
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
}
