use std::collections::HashMap;

use crate::core::method_store::MethodVisibility;
use crate::core::{
    FullyQualifiedName, GraphEdgeFact, GraphEdgeKind, GraphNodeKind, MethodCalleeResolution,
    MethodFact, MethodReferenceAccess, ResolvedMethodCallee, RubyConstant, RubyMethod, SymbolKind,
    TextRange,
};
use crate::engine::query::AnalysisQuery;

pub(crate) type MethodLookupChainCache = HashMap<FullyQualifiedName, Vec<FullyQualifiedName>>;

#[derive(Clone)]
pub enum MethodLookupResult {
    Unique(MethodFact),
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

impl<'a> AnalysisQuery<'a> {
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
                self.engine
                    .method_facts_matching_owner_name(&callee.owner, method)
                    .into_iter()
                    .filter(move |fact| callee.definition_ranges.contains(&fact.range))
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

    pub fn resolve_public_method_callees(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Option<Vec<ResolvedMethodCallee>> {
        self.resolve_method_callees_inner(namespace_fqn, method, false, None)
    }

    pub fn resolve_protected_method_callees(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
        caller_namespace_fqn: &FullyQualifiedName,
    ) -> Option<Vec<ResolvedMethodCallee>> {
        self.resolve_method_callees_inner(namespace_fqn, method, false, Some(caller_namespace_fqn))
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

    pub(crate) fn resolve_method_reference_with_chain_cache(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
        chain_cache: &mut MethodLookupChainCache,
    ) -> MethodLookupResult {
        if !namespace_target_exists(self.engine, namespace_fqn) {
            return MethodLookupResult::Missing;
        }

        let ancestor_chain = chain_cache
            .entry(namespace_fqn.clone())
            .or_insert_with(|| method_lookup_chain(self.engine, namespace_fqn))
            .clone();

        for ancestor in &ancestor_chain {
            let mut facts = self
                .engine
                .method_facts_matching_owner_name(ancestor, method);

            facts.sort_by_key(|fact| {
                (
                    fact.range.file_id,
                    fact.range.start_byte,
                    fact.range.end_byte,
                    fact.fqn.to_string(),
                )
            });
            facts.dedup();

            match facts.len() {
                0 => continue,
                1 => {
                    return MethodLookupResult::Unique(facts.pop().expect(
                        "INVARIANT VIOLATED: method fact count changed after len check. \
                         This is a bug because no code mutates facts between len and pop. \
                         Fix: keep method fact vector local and immutable between checks.",
                    ));
                }
                _ => {
                    return MethodLookupResult::Ambiguous {
                        owner: ancestor.clone(),
                        method: *method,
                    };
                }
            }
        }

        if *method != method_missing_method() {
            return self.resolve_method_reference_with_chain_cache(
                namespace_fqn,
                &method_missing_method(),
                chain_cache,
            );
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
        let mut facts = self
            .engine
            .method_facts_matching_owner_name(&callee.owner, method);
        facts.sort_by_key(|fact| {
            (
                fact.range.file_id,
                fact.range.start_byte,
                fact.range.end_byte,
                fact.fqn.to_string(),
            )
        });
        facts.dedup();

        match facts.len() {
            0 => MethodLookupResult::Missing,
            1 => MethodLookupResult::Unique(facts.pop().expect(
                "INVARIANT VIOLATED: super method fact count changed after len check. \
                 This is a bug because no code mutates facts between len and pop. \
                 Fix: keep method fact vector local and immutable between checks.",
            )),
            _ => MethodLookupResult::Ambiguous {
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
        let has_exact = method_callee_in_chain(
            self.engine,
            &ancestor_chain,
            method,
            MethodCalleeResolution::Exact,
            true,
            None,
        )
        .is_some();

        if !has_exact {
            if let Some(callee) = method_missing_callee_in_chain(self.engine, &ancestor_chain) {
                let method_fqn =
                    FullyQualifiedName::method(callee.owner.namespace_parts(), callee.method);
                return vec![method_fqn];
            }
        }

        for ancestor in ancestor_chain {
            let has_method_fact = !self
                .engine
                .method_facts_matching_owner_name(&ancestor, method)
                .is_empty();
            if ancestor != *namespace_fqn
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
        for override_fact in self.engine.all_method_visibility_overrides() {
            if override_fact.method != *method {
                continue;
            }
            if !method_lookup_chain(self.engine, &override_fact.owner)
                .iter()
                .any(|ancestor| {
                    ancestor.namespace_parts() == namespace_fqn.namespace_parts()
                        && ancestor.namespace_kind() == namespace_fqn.namespace_kind()
                })
            {
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

    pub fn constant_definition_ranges(
        &self,
        parts: &[RubyConstant],
        context: &[RubyConstant],
    ) -> Vec<TextRange> {
        let fqn = self
            .resolve_constant_in_context(parts, context)
            .unwrap_or_else(|| FullyQualifiedName::constant(parts.to_vec()));
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
            || self.method_name_has_visibility(method, MethodVisibility::Protected)
            || method_name_declared_private_in_source(self.engine, method);
        for target in self.method_reference_targets(namespace_fqn, method) {
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
        self.engine
            .symbol_facts_for(fqn)
            .iter()
            .filter(|fact| allowed_kinds.contains(&fact.kind))
            .map(|fact| fact.range)
            .collect()
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

fn method_name_declared_private_in_source(
    engine: &crate::AnalysisEngine,
    method: &RubyMethod,
) -> bool {
    let needle = method.as_str();
    for file in engine.files() {
        let Some(source) = file.source.as_ref() else {
            continue;
        };
        let mut visibility = MethodVisibility::Public;
        for line in source.lines() {
            let trimmed = line.trim_start();
            match trimmed {
                "private" => {
                    visibility = MethodVisibility::Private;
                    continue;
                }
                "protected" => {
                    visibility = MethodVisibility::Protected;
                    continue;
                }
                "public" => {
                    visibility = MethodVisibility::Public;
                    continue;
                }
                _ => {}
            }
            let Some(rest) = trimmed.strip_prefix("def ") else {
                continue;
            };
            let name = rest
                .split(|ch: char| ch.is_whitespace() || matches!(ch, '(' | ';'))
                .next()
                .unwrap_or("");
            if name == needle && visibility == MethodVisibility::Private {
                return true;
            }
        }
    }
    false
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
            return chain;
        } else {
            let chain = vec![
                fqn.clone(),
                FullyQualifiedName::namespace_with_kind(
                    Vec::new(),
                    crate::core::NamespaceKind::Instance,
                ),
            ];
            return chain;
        }
    } else {
        let mut chain = Vec::new();
        let mut visited = std::collections::HashSet::new();
        build_mro(engine, fqn, &mut chain, &mut visited);

        let root = FullyQualifiedName::namespace_with_kind(
            Vec::new(),
            crate::core::NamespaceKind::Instance,
        );
        if !chain.contains(&root)
            && !fqn.namespace_parts().is_empty()
            && (is_module_instance_namespace(engine, fqn)
                || fqn.namespace_kind() == Some(crate::core::NamespaceKind::Singleton))
        {
            append_top_level_instance_fallback(engine, &mut chain, &mut visited);
        }

        chain
    }
}

fn append_top_level_instance_fallback(
    engine: &crate::AnalysisEngine,
    chain: &mut Vec<FullyQualifiedName>,
    visited: &mut std::collections::HashSet<FullyQualifiedName>,
) {
    let root =
        FullyQualifiedName::namespace_with_kind(Vec::new(), crate::core::NamespaceKind::Instance);
    build_mro(engine, &root, chain, visited);

    let object_fqn = top_level_object_instance_fqn();
    if !engine.graph_nodes_for(&object_fqn).is_empty() {
        build_mro(engine, &object_fqn, chain, visited);
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
) {
    if !visited.insert(fqn.clone()) {
        return;
    }

    let mut prepends = edges_from(engine, fqn, GraphEdgeKind::Prepend);
    for edge in prepends.iter_mut().rev() {
        build_mro(engine, &edge.target, chain, visited);
    }

    chain.push(fqn.clone());

    let mut includes = edges_from(engine, fqn, GraphEdgeKind::Include);
    for edge in includes.iter_mut().rev() {
        build_mro(engine, &edge.target, chain, visited);
    }

    let mut included_hook_extends = included_hook_extend_edges(engine, fqn);
    for edge in included_hook_extends.iter_mut().rev() {
        build_mro(engine, &edge.target, chain, visited);
    }

    if let Some(superclass) = edges_from(engine, fqn, GraphEdgeKind::Superclass).first() {
        build_mro(engine, &superclass.target, chain, visited);
    }
}

fn included_hook_extend_edges(
    engine: &crate::AnalysisEngine,
    fqn: &FullyQualifiedName,
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
    {
        hook_edges.extend(edges_from(engine, &edge.target, GraphEdgeKind::Extend));
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
    method_callee_in_chain(
        engine,
        ancestor_chain,
        &method_missing,
        MethodCalleeResolution::MethodMissing,
        true,
        None,
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
