//! Owner existence and spelling suggestions for `unresolved-method`. When a
//! claim may be made at all is `policy::MethodAbsenceClaims`.

use super::policy::{levenshtein, suggestion_threshold};
use crate::core::FullyQualifiedName;
use crate::engine::Project;

impl Project {
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
        self.view().has_graph_node(&instance_fqn)
            || self.view().has_graph_node(&singleton_fqn)
            || self
                .view()
                .has_symbol_facts(&FullyQualifiedName::constant(parts))
    }
}
