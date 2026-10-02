//! Negative proof for `unresolved-method`: lookup-chain completeness, explicit
//! absence contracts, owner existence, and spelling suggestions.

use crate::invariant::ExpectInvariant;
use std::collections::HashSet;

use super::helpers::{levenshtein, suggestion_threshold};
use super::MethodChainCompletenessCache;
use crate::core::{
    FullyQualifiedName, GraphEdgeKind, GraphNodeKind, NamespaceKind, RubyConstant, RubyMethod,
};
use crate::engine::Project;

impl Project {
    pub(super) fn method_lookup_chain_is_incomplete_cached(
        &self,
        owner: &FullyQualifiedName,
        unresolved_sources: &HashSet<Vec<RubyConstant>>,
        cache: &mut MethodChainCompletenessCache,
    ) -> bool {
        if let Some(incomplete) = cache.results.get(owner) {
            return *incomplete;
        }
        let incomplete = self.method_lookup_chain_is_incomplete(owner, unresolved_sources, cache);
        cache.results.insert(owner.clone(), incomplete);
        incomplete
    }

    fn method_lookup_chain_is_incomplete(
        &self,
        owner: &FullyQualifiedName,
        unresolved_sources: &HashSet<Vec<RubyConstant>>,
        cache: &mut MethodChainCompletenessCache,
    ) -> bool {
        // Top-level Ruby is an open execution environment. Test/framework DSLs
        // install methods on Object/Kernel at runtime, and an empty namespace
        // has no closed declaration whose absent method set can be proven.
        if owner.namespace_parts().is_empty() {
            return true;
        }
        if owner.namespace_kind() == Some(NamespaceKind::Singleton) {
            let metaclass = match self.first_graph_node_kind(owner) {
                Some(GraphNodeKind::Class) => Some("Class"),
                Some(GraphNodeKind::Module) => Some("Module"),
                None => None,
            };
            if metaclass.is_some_and(|name| {
                let constant = RubyConstant::new(name).unwrap_or_else(|error| {
                    unreachable_invariant!(
                        what = "Ruby metaclass name `{name}` is invalid: {error}",
                        why = "class and Module are universal Ruby constants",
                        fix = "preserve RubyConstant support for language-defined class names",
                        name = name,
                        error = error,
                    )
                });
                !self.has_graph_node(&FullyQualifiedName::namespace(vec![constant]))
            }) {
                return true;
            }
        }
        let mut pending = vec![owner.clone()];
        let mut visited = std::collections::HashSet::new();
        while let Some(current) = pending.pop() {
            if !visited.insert(current.clone()) {
                continue;
            }
            if *cache
                .ambiguous_superclasses
                .entry(current.clone())
                .or_insert_with(|| self.superclass_is_ambiguous(&current))
            {
                return true;
            }
            if unresolved_sources.contains(&current.namespace_parts()) {
                return true;
            }
            if *cache
                .dynamic_mixin_hooks
                .entry(current.clone())
                .or_insert_with(|| self.namespace_has_dynamic_mixin_hook(&current))
            {
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

    pub(super) fn method_absence_has_explicit_contract(
        &self,
        owner: &FullyQualifiedName,
        method: RubyMethod,
    ) -> bool {
        self.method_absence_contract_matches_owner_name(owner, &method)
    }

    /// An include/prepend/extend callback can install methods through arbitrary
    /// Ruby code. Static edges model the common `base.extend(ClassMethods)`
    /// shape, but a custom callback remains an incomplete negative-proof
    /// surface unless every effect is represented. Concrete lookup may still
    /// resolve known methods; only "method is absent" diagnostics fail closed.
    fn namespace_has_dynamic_mixin_hook(&self, namespace: &FullyQualifiedName) -> bool {
        let instance_namespace = match namespace.namespace_kind() {
            Some(NamespaceKind::Instance) => namespace.clone(),
            Some(NamespaceKind::Singleton) => namespace.to_instance_namespace().expect_invariant(
                "singleton namespace cannot produce its instance counterpart",
                "method lookup chains contain only Namespace FQNs",
                "preserve Namespace identity while traversing mixin hooks",
            ),
            None => unreachable_invariant!(
                what = "method lookup completeness received a non-namespace FQN `{namespace}`",
                why = "only namespaces own method lookup chains",
                fix = "convert receiver types to Namespace FQNs before diagnostics",
                namespace = namespace,
            ),
        };

        for edge in self.graph_edges_from(&instance_namespace) {
            let callbacks: &[&str] = match edge.kind {
                GraphEdgeKind::Include => &["included", "append_features"],
                GraphEdgeKind::Prepend => &["prepended", "prepend_features"],
                GraphEdgeKind::Superclass
                | GraphEdgeKind::Extend
                | GraphEdgeKind::ExecutionContextApplication => continue,
            };
            if self.namespace_defines_any_singleton_method(&edge.target, callbacks) {
                return true;
            }
        }

        for edge in self.graph_edges_from(namespace) {
            if edge.kind == GraphEdgeKind::Extend
                && self.namespace_defines_any_singleton_method(
                    &edge.target,
                    &["extended", "extend_object"],
                )
            {
                return true;
            }
        }
        false
    }

    fn namespace_defines_any_singleton_method(
        &self,
        namespace: &FullyQualifiedName,
        names: &[&str],
    ) -> bool {
        let Some(singleton) = namespace.to_singleton_namespace() else {
            return false;
        };
        names.iter().any(|name| {
            let method = RubyMethod::new(name).unwrap_or_else(|error| {
                unreachable_invariant!(
                    what = "Ruby lifecycle method name `{name}` is invalid: {error}",
                    why = "lifecycle names are fixed Ruby identifiers",
                    fix = "preserve RubyMethod support for language-defined callback names",
                    name = name,
                    error = error,
                )
            });
            !self
                .view()
                .method_facts_matching_owner_name(&singleton, &method)
                .is_empty()
        })
    }

    pub(super) fn find_method_suggestion(
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
        for candidate in self.view().method_names_for_owner(owner_fqn) {
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

    pub(super) fn method_namespace_target_exists(&self, fqn: &FullyQualifiedName) -> bool {
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
        self.has_graph_node(&instance_fqn)
            || self.has_graph_node(&singleton_fqn)
            || self
                .view()
                .has_symbol_facts(&FullyQualifiedName::constant(parts))
            || !self.view().method_facts_matching_owner(fqn, "").is_empty()
    }
}
