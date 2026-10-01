//! Indexer-produced diagnostic candidates: `nil-call`, `bad-splat`, and
//! `raise-non-exception` derivation.

use std::collections::HashMap;

use super::helpers::{EXCEPTION_WHITELIST, NON_EXCEPTION_TYPES};
use crate::core::{
    DiagnosticCandidate, DiagnosticCandidateKind, DiagnosticFact, FullyQualifiedName,
    RaiseArgCandidate, RubyConstant, RubyMethod, RubyType, SourceFileId,
};
use crate::engine::{AnalysisEngine, AnalysisQuery};

impl AnalysisEngine {
    pub(super) fn resolve_diagnostic_candidates(
        &self,
    ) -> HashMap<SourceFileId, Vec<DiagnosticFact>> {
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

    pub(super) fn resolve_diagnostic_candidates_in_file(
        &self,
        file_id: SourceFileId,
    ) -> Vec<DiagnosticFact> {
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
            DiagnosticCandidateKind::NilCall {
                local_read,
                variable,
                method,
            } => {
                let ruby_type = AnalysisQuery::new(self)
                    .exact_call_receiver_type(candidate.range, *local_read)?;
                if ruby_type != RubyType::nil_class() {
                    return None;
                }
                Some(DiagnosticFact::new(
                    candidate.range,
                    crate::core::DiagnosticSeverity::Warning,
                    "nil-call",
                    format!("Calling `{method}` on `{variable}` which is `nil` here."),
                ))
            }
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
            RubyType::Literal(value) => self.ruby_type_is_exception(value.widened_type()),
            RubyType::Module(_) | RubyType::ModuleReference(_) => false,
            RubyType::Union(_) | RubyType::Unknown => true,
            RubyType::Array(_) | RubyType::Hash(_, _) | RubyType::Shape(_) => false,
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
        if !self.has_graph_node(&ns_fqn) && !self.has_symbol_facts(&ns_fqn) {
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
}
