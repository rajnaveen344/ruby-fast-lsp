use std::collections::{HashMap, HashSet};

use crate::core::{
    ConstLookupId, DiagnosticCandidate, DiagnosticCandidateKind, DiagnosticFact, FqnId,
    FullyQualifiedName, GraphEdgeKind, MethodCallSignatureCandidate, MethodFact, NamespaceKind,
    RaiseArgCandidate, ReferenceFact, RubyConstant, RubyMethod, RubyType, SourceFileId,
    StoredReferenceCandidateKind, StoredReferenceCandidateRef, TextRange,
};
use crate::engine::diagnostic_helpers::{
    arity_mismatch, closest_keyword, levenshtein, suggestion_threshold, MethodArity,
    EXCEPTION_WHITELIST, NON_EXCEPTION_TYPES,
};
use crate::engine::resolution::{MethodLookupChainCache, MethodLookupResult};
use crate::{AnalysisEngine, AnalysisQuery};

type MethodReferenceCacheKey = (ConstLookupId, NamespaceKind, RubyMethod, bool);

impl AnalysisEngine {
    pub(super) fn resolve_reference_candidates(&mut self) {
        let mut candidate_file_ids = self.facts.references.candidates.file_ids();
        for file_id in self.facts.diagnostics.candidates.file_ids() {
            if !candidate_file_ids.contains(&file_id) {
                candidate_file_ids.push(file_id);
            }
        }

        let reference_candidate_store = std::mem::take(&mut self.facts.references.candidates);
        let mut unresolved_constants = self.resolve_diagnostic_candidates();
        let mut method_fact_cache: HashMap<MethodReferenceCacheKey, MethodLookupResult> =
            HashMap::new();
        let mut method_namespace_exists_cache: HashMap<FullyQualifiedName, bool> = HashMap::new();
        let mut method_suggestion_cache: HashMap<(FullyQualifiedName, RubyMethod), Option<String>> =
            HashMap::new();
        let mut constant_target_cache: HashMap<ConstLookupId, Option<FqnId>> = HashMap::new();
        let mut method_lookup_chain_cache: MethodLookupChainCache = HashMap::new();
        let unresolved_method_edge_sources = self.unresolved_method_edge_sources();
        let mut incomplete_method_chain_cache: HashMap<FullyQualifiedName, bool> = HashMap::new();
        self.facts.references.resolved.clear();
        for candidate in reference_candidate_store.iter_candidates() {
            match candidate {
                StoredReferenceCandidateRef::Resolved(candidate) => {
                    self.facts.references.resolved.add(
                        candidate.target,
                        ReferenceFact::new(candidate.range, candidate.caller),
                    );
                }
                StoredReferenceCandidateRef::Constant(candidate) => {
                    let lookup = self
                        .names
                        .const_lookup(candidate.lookup)
                        .expect(
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
                    let target = if let Some(target) = constant_target_cache.get(&candidate.lookup)
                    {
                        *target
                    } else {
                        let target = self
                            .resolve_constant_reference(
                                &parts,
                                &if lookup.absolute {
                                    Vec::new()
                                } else {
                                    context.namespace_parts()
                                },
                            )
                            .map(|target| self.names.intern_fqn(target));
                        constant_target_cache.insert(candidate.lookup, target);
                        target
                    };
                    if let Some(target) = target {
                        self.facts
                            .references
                            .resolved
                            .add(target, ReferenceFact::new(candidate.range, None));
                    } else {
                        unresolved_constants
                            .entry(candidate.range.file_id)
                            .or_default()
                            .push(DiagnosticFact::new(
                                candidate.range,
                                crate::core::DiagnosticSeverity::Error,
                                "unresolved-constant",
                                format!("Unresolved constant `{}`", constant_name(&parts)),
                            ));
                    }
                }
                StoredReferenceCandidateRef::Method(candidate) => {
                    let method_cache_key = (
                        candidate.owner,
                        candidate.owner_kind,
                        candidate.method,
                        candidate.is_super,
                    );
                    let fact = method_fact_cache
                        .entry(method_cache_key)
                        .or_insert_with(|| {
                            let owner_lookup = self.names.const_lookup(candidate.owner).expect(
                                "INVARIANT VIOLATED: method reference candidate points to missing owner lookup. \
                                 This is a bug because stored reference candidates must only contain interned lookup ids. \
                                 Fix: intern constant lookups before inserting candidates.",
                            );
                            let owner_fqn = FullyQualifiedName::namespace_with_kind(
                                owner_lookup.path.to_vec(),
                                candidate.owner_kind,
                            );
                            let query = AnalysisQuery::new(self);
                            if candidate.is_super {
                                query.resolve_super_method_reference(
                                    &owner_fqn,
                                    &candidate.method,
                                )
                            } else {
                                query.resolve_method_reference_with_chain_cache(
                                    &owner_fqn,
                                    &candidate.method,
                                    &mut method_lookup_chain_cache,
                                )
                            }
                        })
                        .clone();
                    if let Some((owner, resolved_method, fact)) = fact.reference_parts() {
                        let target =
                            FullyQualifiedName::method(owner.namespace_parts(), resolved_method);
                        let target = self.names.intern_fqn(target);
                        self.facts.references.resolved.add(
                            target,
                            ReferenceFact::method(
                                candidate.range,
                                candidate.caller,
                                candidate.access,
                            ),
                        );
                        if resolved_method == candidate.method {
                            if let Some(diagnostics) = candidate.diagnostics.as_deref() {
                                if let Some(fact) = fact {
                                    self.push_signature_diagnostics(
                                        fact,
                                        &candidate.method,
                                        &diagnostics.signature,
                                        diagnostics.diagnostic_range,
                                        &mut unresolved_constants,
                                    );
                                }
                            }
                        }
                    } else if fact.is_missing() {
                        let owner_fqn =
                            method_reference_owner_fqn(self, candidate.owner, candidate.owner_kind);
                        let namespace_exists = *method_namespace_exists_cache
                            .entry(owner_fqn.clone())
                            .or_insert_with_key(|owner_fqn| {
                                self.method_namespace_target_exists(owner_fqn)
                            });
                        let allow_unindexed_owner = candidate
                            .diagnostics
                            .as_deref()
                            .is_some_and(|diagnostics| diagnostics.allow_unindexed_owner);
                        if !namespace_exists && !allow_unindexed_owner {
                            continue;
                        }
                        let target = FullyQualifiedName::method(
                            owner_fqn.namespace_parts(),
                            candidate.method,
                        );
                        let target = self.names.intern_fqn(target);
                        self.facts.references.resolved.add(
                            target,
                            ReferenceFact::method(
                                candidate.range,
                                candidate.caller,
                                candidate.access,
                            ),
                        );

                        if let Some(diagnostics) = candidate.diagnostics.as_deref() {
                            if !diagnostics.diagnose_unresolved {
                                continue;
                            }
                            if *incomplete_method_chain_cache
                                .entry(owner_fqn.clone())
                                .or_insert_with(|| {
                                    self.method_lookup_chain_is_incomplete(
                                        &owner_fqn,
                                        &unresolved_method_edge_sources,
                                    )
                                })
                            {
                                continue;
                            }
                            let suggestion = namespace_exists
                                .then(|| {
                                    method_suggestion_cache
                                        .entry((owner_fqn.clone(), candidate.method))
                                        .or_insert_with(|| {
                                            self.find_method_suggestion(
                                                &owner_fqn,
                                                candidate.method.as_str(),
                                            )
                                        })
                                        .clone()
                                })
                                .flatten();
                            let mut message = match &diagnostics.receiver_label {
                                Some(label) => format!(
                                    "Unresolved method `{}` on `{}`",
                                    candidate.method.as_str(),
                                    label
                                ),
                                None => {
                                    format!("Unresolved method `{}`", candidate.method.as_str())
                                }
                            };
                            if let Some(suggestion) = suggestion {
                                message.push_str(&format!(". Did you mean `{}`?", suggestion));
                            }
                            unresolved_constants
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
                }
            }
        }
        self.facts.references.candidates = reference_candidate_store;
        self.facts.references.resolved.sort_all();

        for file_id in candidate_file_ids {
            let mut diagnostics = self
                .facts
                .diagnostics
                .resolved
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
            self.facts
                .diagnostics
                .resolved
                .replace_file(file_id, diagnostics);
        }
    }

    pub(super) fn resolve_reference_candidates_in_file(&mut self, file_id: SourceFileId) {
        let reference_candidates = self.facts.references.candidates.candidates_in_file(file_id);
        let mut unresolved =
            HashMap::from([(file_id, self.resolve_diagnostic_candidates_in_file(file_id))]);
        let mut method_fact_cache: HashMap<
            (FullyQualifiedName, RubyMethod, bool),
            MethodLookupResult,
        > = HashMap::new();
        let mut method_namespace_exists_cache: HashMap<FullyQualifiedName, bool> = HashMap::new();
        let mut method_suggestion_cache: HashMap<(FullyQualifiedName, RubyMethod), Option<String>> =
            HashMap::new();
        let mut method_lookup_chain_cache: MethodLookupChainCache = HashMap::new();
        let unresolved_method_edge_sources = self.unresolved_method_edge_sources();
        let mut incomplete_method_chain_cache: HashMap<FullyQualifiedName, bool> = HashMap::new();
        let mut resolved_refs = Vec::new();

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
                    diagnostics,
                } => {
                    let owner_lookup = self.names.const_lookup(owner).expect(
                        "INVARIANT VIOLATED: method reference candidate points to missing owner lookup. \
                         This is a bug because stored reference candidates must only contain interned lookup ids. \
                         Fix: intern constant lookups before inserting candidates.",
                    );
                    let owner = owner_lookup.path.to_vec();
                    let owner_fqn = FullyQualifiedName::namespace_with_kind(owner, owner_kind);
                    let fact = method_fact_cache
                        .entry((owner_fqn.clone(), method, is_super))
                        .or_insert_with(|| {
                            let query = AnalysisQuery::new(self);
                            if is_super {
                                query.resolve_super_method_reference(&owner_fqn, &method)
                            } else {
                                query.resolve_method_reference_with_chain_cache(
                                    &owner_fqn,
                                    &method,
                                    &mut method_lookup_chain_cache,
                                )
                            }
                        })
                        .clone();
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
                                    self.push_signature_diagnostics(
                                        fact,
                                        &method,
                                        &diagnostics.signature,
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
                            if *incomplete_method_chain_cache
                                .entry(owner_fqn.clone())
                                .or_insert_with(|| {
                                    self.method_lookup_chain_is_incomplete(
                                        &owner_fqn,
                                        &unresolved_method_edge_sources,
                                    )
                                })
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
        let mut diagnostics = self
            .facts
            .diagnostics
            .resolved
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
        diagnostics.extend(unresolved.remove(&file_id).unwrap_or_default());
        self.facts
            .diagnostics
            .resolved
            .replace_file(file_id, diagnostics);
    }

    fn unresolved_method_edge_sources(&self) -> HashSet<Vec<RubyConstant>> {
        self.graph
            .unresolved_edges()
            .into_iter()
            .filter(|edge| {
                let lookup = self.names.const_lookup(edge.target).expect(
                    "INVARIANT VIOLATED: unresolved graph edge points to a missing constant lookup. This is a bug because graph edges must retain valid interned targets. Fix: intern and retain every unresolved graph target for the edge lifetime.",
                );
                !(edge.kind == GraphEdgeKind::Superclass
                    && lookup.absolute
                    && lookup.path.len() == 1
                    && lookup.path[0].as_str() == "Object")
            })
            .filter_map(|edge| {
                self.names
                    .fqn(edge.source)
                    .map(FullyQualifiedName::namespace_parts)
            })
            .collect()
    }

    fn method_lookup_chain_is_incomplete(
        &self,
        owner: &FullyQualifiedName,
        unresolved_sources: &HashSet<Vec<RubyConstant>>,
    ) -> bool {
        let mut pending = vec![owner.clone()];
        let mut visited = std::collections::HashSet::new();
        while let Some(current) = pending.pop() {
            if !visited.insert(current.clone()) {
                continue;
            }
            if unresolved_sources.contains(&current.namespace_parts()) {
                return true;
            }
            pending.extend(
                self.graph_edges_from(&current)
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

    fn push_signature_diagnostics(
        &self,
        fact: &MethodFact,
        method: &crate::core::RubyMethod,
        signature: &MethodCallSignatureCandidate,
        diagnostic_range: TextRange,
        diagnostics_by_file: &mut HashMap<SourceFileId, Vec<DiagnosticFact>>,
    ) {
        let arity = MethodArity::from_params(&fact.param_facts);
        let declares_keywords = arity.has_kwrest
            || !arity.required_keywords.is_empty()
            || !arity.optional_keywords.is_empty();
        let keywords_form_options_hash = !declares_keywords
            && (!signature.keyword_args.is_empty() || signature.has_keyword_splat);
        let mut effective_signature = signature.clone();
        if keywords_form_options_hash {
            effective_signature.positional_count += 1;
        }
        if let Some((min, max, actual)) = arity_mismatch(&effective_signature, &arity) {
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

    fn resolve_diagnostic_candidates_in_file(&self, file_id: SourceFileId) -> Vec<DiagnosticFact> {
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
        if self.graph_nodes_for(&ns_fqn).is_empty() && self.symbol_facts_for(&ns_fqn).is_empty() {
            return true;
        }

        let mut current = ns_fqn;
        let mut visited = std::collections::HashSet::new();
        while visited.insert(current.clone()) {
            let mut advanced = false;
            for edge in self.all_graph_edges() {
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
        for candidate in self.method_names_for_owner(owner_fqn) {
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
        !self.graph_nodes_for(&instance_fqn).is_empty()
            || !self.graph_nodes_for(&singleton_fqn).is_empty()
            || !self
                .symbol_facts_for(&FullyQualifiedName::constant(parts))
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
            if !self.graph_nodes_for(&namespace_fqn).is_empty()
                || !self.symbol_facts_for(&namespace_fqn).is_empty()
            {
                return Some(namespace_fqn);
            }

            let constant_fqn = FullyQualifiedName::constant(probe);
            if !self.symbol_facts_for(&constant_fqn).is_empty() {
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

fn constant_name(parts: &[RubyConstant]) -> String {
    parts
        .iter()
        .map(RubyConstant::as_str)
        .collect::<Vec<_>>()
        .join("::")
}

fn method_reference_owner_fqn(
    engine: &AnalysisEngine,
    owner: ConstLookupId,
    owner_kind: NamespaceKind,
) -> FullyQualifiedName {
    let owner_lookup = engine.names.const_lookup(owner).expect(
        "INVARIANT VIOLATED: method reference candidate points to missing owner lookup. \
         This is a bug because stored reference candidates must only contain interned lookup ids. \
         Fix: intern constant lookups before inserting candidates.",
    );
    FullyQualifiedName::namespace_with_kind(owner_lookup.path.to_vec(), owner_kind)
}
