//! Extension call tracking host used while collecting project file facts.

use super::FileProcessor;
use crate::environment::extensions::{ExtensionApplicabilitySnapshot, ExtensionRegistryHandle};
use crate::environment::runtime::jruby::imports::JrubyImportProvider;
use ruby_analysis::indexer::fact_collector::{FactCollector, FactCollectorExtensionHost};
use ruby_fast_lsp_extension_api::{ProjectContext, ResolvedCall};
use ruby_prism::CallNode;
use std::collections::HashSet;
use std::sync::{Arc, OnceLock};

/// Name-only prefilter cloned once per file. Ordinary Ruby calls (`save`,
/// `to_s`, ...) never take the registry lock. Hits still dispatch and classify
/// frames as separate locked operations; do not merge those or retain a full
/// registry snapshot for the visit.
fn empty_tracked_call_names() -> Arc<HashSet<String>> {
    static EMPTY: OnceLock<Arc<HashSet<String>>> = OnceLock::new();
    Arc::clone(EMPTY.get_or_init(|| Arc::new(HashSet::new())))
}

/// Server-owned composition of built-in runtime semantics and public Wasm
/// extensions. Runtime providers may add ordinary facts, while the extension
/// registry remains the sole owner of extension frame tracking and resolved
/// call payloads.
#[derive(Debug)]
struct ProjectFactCollectorHost {
    extension_registry: ExtensionRegistryHandle,
    extension_applicability: OnceLock<ExtensionApplicabilitySnapshot>,
    tracked_call_names: Arc<HashSet<String>>,
    jruby_import_provider: Option<Arc<JrubyImportProvider>>,
    extensions_enabled: bool,
    jruby_call_host_enabled: bool,
}

impl ProjectFactCollectorHost {
    fn extension_applicability(
        &self,
        project: Option<&ProjectContext>,
    ) -> &ExtensionApplicabilitySnapshot {
        assert!(
            self.extensions_enabled,
            "INVARIANT VIOLATED: disabled extension traversal requested an applicability snapshot. This is a bug because non-project sources must not execute extension hooks. Fix: keep all snapshot access behind the extensions_enabled gate."
        );
        self.extension_applicability
            .get_or_init(|| self.extension_registry.applicability_snapshot(project))
    }

    fn tracks_call_name(&self, node: &CallNode<'_>) -> bool {
        let Ok(name) = std::str::from_utf8(node.name().as_slice()) else {
            return false;
        };
        self.tracked_call_names.contains(name)
    }
}

impl FactCollectorExtensionHost for ProjectFactCollectorHost {
    fn process_call_node(&self, visitor: &mut FactCollector, node: &CallNode<'_>) -> bool {
        if self.jruby_call_host_enabled {
            if let Some(provider) = &self.jruby_import_provider {
                let _ = provider.process_call_node(visitor, node);
            }
        }
        if self.extensions_enabled && self.tracks_call_name(node) {
            return self
                .extension_registry
                .process_call_node_with_applicability(
                    visitor,
                    node,
                    self.extension_applicability(visitor.extension_project_context()),
                );
        }
        false
    }

    fn should_track_enclosing_call(&self, visitor: &FactCollector, node: &CallNode<'_>) -> bool {
        self.extensions_enabled
            && self.tracks_call_name(node)
            && self
                .extension_registry
                .should_track_enclosing_call_with_applicability(
                    visitor,
                    node,
                    self.extension_applicability(visitor.extension_project_context()),
                )
    }

    fn resolved_call_for_stack(
        &self,
        visitor: &FactCollector,
        node: &CallNode<'_>,
    ) -> ResolvedCall {
        assert!(
            self.extensions_enabled,
            "INVARIANT VIOLATED: an extension call frame was resolved for a source kind that disables extensions. This is a bug because disabled extension hosts must reject frame tracking before payload construction. Fix: keep should_track_enclosing_call gated by extensions_enabled."
        );
        self.extension_registry
            .resolved_call_for_stack_with_applicability(
                visitor,
                node,
                self.extension_applicability(visitor.extension_project_context()),
            )
    }
}

impl FileProcessor {
    pub(super) fn fact_collector_host(
        &self,
        extensions_enabled: bool,
        jruby_call_host_enabled: bool,
    ) -> Arc<dyn FactCollectorExtensionHost> {
        Arc::new(ProjectFactCollectorHost {
            extension_registry: self.extension_registry.clone(),
            extension_applicability: OnceLock::new(),
            tracked_call_names: if extensions_enabled {
                self.extension_registry.tracked_call_names()
            } else {
                empty_tracked_call_names()
            },
            jruby_import_provider: self.jruby_import_provider.clone(),
            extensions_enabled,
            jruby_call_host_enabled,
        })
    }
}
