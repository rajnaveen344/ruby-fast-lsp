use std::collections::HashMap;

use crate::core::{
    DiagnosticCandidateKind, DiagnosticFact, FullyQualifiedName, GraphEdgeKind,
    MethodCallSignatureCandidate, MethodFact, RaiseArgCandidate, ReferenceCandidateKind,
    ReferenceFact, RubyConstant, RubyMethod, RubyType, SourceFileId, TextRange,
};
use crate::engine::diagnostic_helpers::{
    arity_mismatch, closest_keyword, levenshtein, suggestion_threshold, MethodArity,
    EXCEPTION_WHITELIST, NON_EXCEPTION_TYPES,
};
use crate::{AnalysisEngine, AnalysisQuery};

impl AnalysisEngine {
    pub(super) fn resolve_reference_candidates(&mut self) {
        let candidates = self.reference_candidate_store.all_candidates();
        let mut candidate_file_ids = self.reference_candidate_store.file_ids();
        for file_id in self.diagnostic_candidate_store.file_ids() {
            if !candidate_file_ids.contains(&file_id) {
                candidate_file_ids.push(file_id);
            }
        }
        self.reference_store.clear();

        let mut unresolved_constants = self.resolve_diagnostic_candidates();
        for candidate in candidates {
            match candidate.kind {
                ReferenceCandidateKind::Resolved { target, caller } => {
                    self.reference_store
                        .add(ReferenceFact::new(target, candidate.range, caller));
                }
                ReferenceCandidateKind::Constant {
                    parts,
                    current_namespace,
                    name,
                } => {
                    if let Some(target) =
                        self.resolve_constant_reference(&parts, &current_namespace)
                    {
                        self.reference_store
                            .add(ReferenceFact::new(target, candidate.range, None));
                    } else {
                        unresolved_constants
                            .entry(candidate.range.file_id)
                            .or_default()
                            .push(DiagnosticFact::new(
                                candidate.range,
                                crate::core::DiagnosticSeverity::Error,
                                "unresolved-constant",
                                format!("Unresolved constant `{}`", name),
                            ));
                    }
                }
                ReferenceCandidateKind::Method {
                    owner,
                    owner_kind,
                    method,
                    caller,
                    diagnostic_range,
                    receiver_label,
                    diagnose_unresolved,
                    signature,
                } => {
                    let owner_fqn = FullyQualifiedName::namespace_with_kind(owner, owner_kind);
                    let fact =
                        AnalysisQuery::new(self).method_fact_for_receiver(&owner_fqn, &method);
                    if let Some(fact) = fact {
                        let target =
                            FullyQualifiedName::method(fact.owner.namespace_parts(), method);
                        self.reference_store.add(ReferenceFact::new(
                            target,
                            candidate.range,
                            caller,
                        ));
                        self.push_signature_diagnostics(
                            &fact,
                            &method,
                            &signature,
                            diagnostic_range,
                            &mut unresolved_constants,
                        );
                    } else if self.method_namespace_target_exists(&owner_fqn) {
                        let target =
                            FullyQualifiedName::method(owner_fqn.namespace_parts(), method);
                        self.reference_store.add(ReferenceFact::new(
                            target,
                            candidate.range,
                            caller,
                        ));

                        if diagnose_unresolved {
                            let suggestion =
                                self.find_method_suggestion(&owner_fqn, method.as_str());
                            let mut message = match receiver_label {
                                Some(label) => format!(
                                    "Unresolved method `{}` on `{}`",
                                    method.as_str(),
                                    label
                                ),
                                None => format!("Unresolved method `{}`", method.as_str()),
                            };
                            if let Some(suggestion) = suggestion {
                                message.push_str(&format!(". Did you mean `{}`?", suggestion));
                            }
                            unresolved_constants
                                .entry(diagnostic_range.file_id)
                                .or_default()
                                .push(DiagnosticFact::new(
                                    diagnostic_range,
                                    crate::core::DiagnosticSeverity::Warning,
                                    "unresolved-method",
                                    message,
                                ));
                        }
                    }
                }
            }
        }

        for file_id in candidate_file_ids {
            let mut diagnostics = self
                .diagnostic_store
                .facts_in_file(file_id)
                .into_iter()
                .filter(|fact| fact.code != "unresolved-constant")
                .filter(|fact| fact.code != "unresolved-method")
                .filter(|fact| fact.code != "wrong-arity")
                .filter(|fact| fact.code != "unknown-kwarg")
                .filter(|fact| fact.code != "missing-kwarg")
                .filter(|fact| fact.code != "raise-non-exception")
                .filter(|fact| fact.code != "bad-splat")
                .collect::<Vec<_>>();
            diagnostics.extend(unresolved_constants.remove(&file_id).unwrap_or_default());
            self.diagnostic_store.replace_file(file_id, diagnostics);
        }
    }

    fn push_signature_diagnostics(
        &self,
        fact: &MethodFact,
        method: &crate::core::RubyMethod,
        signature: &MethodCallSignatureCandidate,
        diagnostic_range: TextRange,
        diagnostics_by_file: &mut HashMap<SourceFileId, Vec<DiagnosticFact>>,
    ) {
        let arity = MethodArity::from_params(&fact.param_facts);
        if let Some((min, max, actual)) = arity_mismatch(signature, &arity) {
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

        if !arity.has_kwrest && !signature.has_keyword_splat {
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

        if !arity.required_keywords.is_empty() && !signature.has_keyword_splat {
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

    fn resolve_diagnostic_candidates(&self) -> HashMap<SourceFileId, Vec<DiagnosticFact>> {
        let mut diagnostics = HashMap::new();
        for candidate in self.diagnostic_candidate_store.all_candidates() {
            match candidate.kind {
                DiagnosticCandidateKind::BadSplat {
                    operator,
                    arg_repr,
                    expected,
                } => {
                    diagnostics
                        .entry(candidate.range.file_id)
                        .or_insert_with(Vec::new)
                        .push(DiagnosticFact::new(
                            candidate.range,
                            crate::core::DiagnosticSeverity::Warning,
                            "bad-splat",
                            format!(
                                "`{}{}` expected {} but got non-{} value",
                                operator, arg_repr, expected, expected
                            ),
                        ));
                }
                DiagnosticCandidateKind::RaiseNonException { arg_repr, arg } => {
                    if self.raise_arg_is_exception(arg) {
                        continue;
                    }
                    diagnostics
                        .entry(candidate.range.file_id)
                        .or_insert_with(Vec::new)
                        .push(DiagnosticFact::new(
                            candidate.range,
                            crate::core::DiagnosticSeverity::Warning,
                            "raise-non-exception",
                            format!(
                                "`raise` argument `{}` is not an Exception subclass",
                                arg_repr
                            ),
                        ));
                }
            }
        }
        diagnostics
    }

    fn raise_arg_is_exception(&self, arg: RaiseArgCandidate) -> bool {
        match arg {
            RaiseArgCandidate::StringLiteral | RaiseArgCandidate::Unknown => true,
            RaiseArgCandidate::NonExceptionLiteral => false,
            RaiseArgCandidate::Constant(name) => self.is_exception_class_name(&name),
            RaiseArgCandidate::Type(ruby_type) => self.ruby_type_is_exception(ruby_type),
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
            if let Some(fact) = query.method_fact_for_receiver(&namespace_fqn, method) {
                return query.method_return_type(&fact).or(Some(RubyType::Unknown));
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
            RubyType::Module(_) | RubyType::ModuleReference(_) => false,
            RubyType::Union(_) | RubyType::Unknown => true,
            RubyType::Array(_) | RubyType::Hash(_, _) => false,
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
        if self.graph_store.nodes_for(&ns_fqn).is_empty()
            && self.symbol_store.facts_for(&ns_fqn).is_empty()
        {
            return true;
        }

        let mut current = ns_fqn;
        let mut visited = std::collections::HashSet::new();
        while visited.insert(current.clone()) {
            let mut advanced = false;
            for edge in self.graph_store.all_edges() {
                if edge.kind != GraphEdgeKind::Superclass || edge.source != current {
                    continue;
                }
                let last = edge.target.namespace_parts().last().map(|c| c.to_string());
                if let Some(target_name) = last {
                    if EXCEPTION_WHITELIST.contains(&target_name.as_str()) {
                        return true;
                    }
                }
                current = edge.target;
                advanced = true;
                break;
            }
            if !advanced {
                break;
            }
        }

        false
    }

    fn find_method_suggestion(
        &self,
        owner_fqn: &FullyQualifiedName,
        target: &str,
    ) -> Option<String> {
        let threshold = suggestion_threshold(target.len());
        if threshold == 0 {
            return None;
        }

        let target_len = target.len();
        let mut best: Option<(String, usize)> = None;
        for fact in AnalysisQuery::new(self).method_facts_matching(owner_fqn, "") {
            let FullyQualifiedName::Method(_, method) = &fact.fqn else {
                continue;
            };
            let candidate = method.as_str();
            if candidate == target {
                continue;
            }
            if candidate.len().abs_diff(target_len) > threshold {
                continue;
            }
            let dist = levenshtein(candidate, target);
            if dist > threshold {
                continue;
            }
            match &best {
                Some((_, d)) if *d <= dist => {}
                Some(_) | None => best = Some((candidate.to_string(), dist)),
            }
        }
        best.map(|(name, _)| name)
    }

    fn method_namespace_target_exists(&self, fqn: &FullyQualifiedName) -> bool {
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
        !self.graph_store.nodes_for(&instance_fqn).is_empty()
            || !self.graph_store.nodes_for(&singleton_fqn).is_empty()
            || !self
                .symbol_store
                .facts_for(&FullyQualifiedName::constant(parts))
                .is_empty()
    }

    fn resolve_constant_reference(
        &self,
        parts: &[crate::core::RubyConstant],
        current_namespace: &[crate::core::RubyConstant],
    ) -> Option<FullyQualifiedName> {
        let mut search = current_namespace.to_vec();

        loop {
            let mut probe = search.clone();
            probe.extend(parts.iter().cloned());

            let namespace_fqn = FullyQualifiedName::namespace(probe.clone());
            if !self.graph_store.nodes_for(&namespace_fqn).is_empty()
                || !self.symbol_store.facts_for(&namespace_fqn).is_empty()
            {
                return Some(namespace_fqn);
            }

            let constant_fqn = FullyQualifiedName::constant(probe);
            if !self.symbol_store.facts_for(&constant_fqn).is_empty() {
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
