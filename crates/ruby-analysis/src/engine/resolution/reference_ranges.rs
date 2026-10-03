//! Reference ranges for constants, variables, and methods, including visibility
//! filtering of method references.

use std::collections::HashSet;

use super::chain_methods::{
    effective_method_visibility_for_chain, global_visibility_override_for_method_owner,
    global_visibility_override_for_method_owner_matching, protected_method_visible_from,
};
use super::lookup_chain::method_lookup_chain;
use crate::core::MethodVisibility;
use crate::core::{
    FullyQualifiedName, MethodCalleeResolution, MethodReferenceAccess, RubyConstant, RubyMethod,
    TextRange,
};
use crate::engine::queries::View;

impl<'a> View<'a> {
    pub fn reference_ranges_for_fqn(&self, fqn: &FullyQualifiedName) -> Vec<TextRange> {
        self.reference_facts_for(fqn)
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
        self.method_reference_ranges_with_private(namespace_fqn, method, true, None, None)
    }

    pub fn method_reference_ranges_public_receiver(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Vec<TextRange> {
        self.method_reference_ranges_with_private(namespace_fqn, method, false, None, None)
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
            None,
        )
    }

    pub(in crate::engine) fn method_reference_ranges_for_exact_target(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
        target: &FullyQualifiedName,
    ) -> Vec<TextRange> {
        self.method_reference_ranges_with_private(namespace_fqn, method, true, None, Some(target))
    }

    fn method_reference_ranges_with_private(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
        allow_private: bool,
        protected_caller: Option<&FullyQualifiedName>,
        exact_target: Option<&FullyQualifiedName>,
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
        let targets = match exact_target {
            Some(target) => vec![target.clone()],
            None => self.method_reference_targets(namespace_fqn, method),
        };
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
            ranges.extend(self.reference_facts_for(&target).iter().filter_map(|fact| {
                if target_non_public && fact.access == MethodReferenceAccess::ExplicitReceiver {
                    if target_visibility_owner
                        .as_ref()
                        .is_some_and(|(visibility, owner)| {
                            *visibility == MethodVisibility::Protected
                                && self.reference_caller_can_see_protected(fact, owner)
                        })
                    {
                        Some(fact.range)
                    } else {
                        None
                    }
                } else {
                    Some(fact.range)
                }
            }));
        }
        for candidate in self
            .engine
            .uses
            .candidates()
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
        self.all_method_facts().iter().find_map(|fact| {
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
            self.method_facts_matching_owner_name(owner, method)
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
        self.all_method_facts().iter().any(|fact| {
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
        self.reference_facts_for(&target)
            .iter()
            .map(|fact| fact.range)
            .collect()
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
}
