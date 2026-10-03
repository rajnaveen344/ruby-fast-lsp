#[cfg(test)]
use crate::invariant::ExpectInvariant;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::{Arc, Weak};

use parking_lot::Mutex;
use ruby_analysis::core::FullyQualifiedName;
use ruby_analysis::indexer as utils;
use ruby_analysis::indexer::fact_collector::FactCollector;
use ruby_prism::CallNode;

use crate::environment::extensions::loading::config::ExtensionLoadConfig;
use crate::environment::extensions::loading::manifest::ExtensionMethodTarget;
use crate::environment::extensions::loading::packages::{
    discover_extension_packages, extension_packages_fingerprint,
    load_wasm_extensions_from_packages_with_cache,
};
#[cfg(test)]
use crate::environment::extensions::registry::handle::ExtensionRegistryHandle;
use crate::environment::extensions::registry::loaded::LoadedWasmExtension;
use crate::environment::extensions::registry::seed::ExtensionSemanticSeed;
use crate::environment::extensions::registry::status::ExtensionStatusReport;
use crate::environment::extensions::ExtensionApplicabilityFingerprint;
use crate::utils::persistent_cache::PersistentDerivedProductCache;

pub(in crate::environment::extensions) struct ExtensionRegistry {
    pub(super) extensions: Vec<Arc<LoadedWasmExtension>>,
    pub(in crate::environment::extensions) tracked_call_names: Arc<HashSet<String>>,
    semantic_seeded_engines: Mutex<Vec<SeededExtensionEngine>>,
    pub(super) load_config: ExtensionLoadConfig,
    discovery_fingerprint: [u8; 32],
}

/// File-traversal-scoped extension applicability decisions.
///
/// This intentionally retains only one bit per extension plus the immutable
/// registry identity. It must never retain extension instances or semantic
/// engine state across project generations.
#[derive(Clone, Debug)]
pub(crate) struct ExtensionApplicabilitySnapshot {
    registry_fingerprint: [u8; 32],
    applies_to_source: Vec<bool>,
}

/// The engine identity the ledger records a seed against: an allocation
/// that lives exactly as long as the engine it identifies.
struct SeededExtensionEngine {
    engine: Weak<dyn Send + Sync>,
    applicability_fingerprint: ExtensionApplicabilityFingerprint,
}

impl ExtensionRegistry {
    pub(super) fn empty() -> Self {
        let load_config = ExtensionLoadConfig::default();
        Self {
            extensions: Vec::new(),
            tracked_call_names: tracked_call_names(&[]),
            semantic_seeded_engines: Mutex::new(Vec::new()),
            discovery_fingerprint: extension_packages_fingerprint(&[]),
            load_config,
        }
    }

    pub(in crate::environment::extensions) fn load(config: &ExtensionLoadConfig) -> Self {
        Self::load_with_persistent_cache(config, None)
    }

    pub(super) fn load_with_persistent_cache(
        config: &ExtensionLoadConfig,
        persistent_cache: Option<&PersistentDerivedProductCache>,
    ) -> Self {
        let packages = discover_extension_packages(config);
        let discovery_fingerprint = extension_packages_fingerprint(&packages);
        let extensions = load_wasm_extensions_from_packages_with_cache(packages, persistent_cache);
        let tracked_call_names = tracked_call_names(&extensions);
        let registry = Self {
            extensions,
            tracked_call_names,
            semantic_seeded_engines: Mutex::new(Vec::new()),
            load_config: config.clone(),
            discovery_fingerprint,
        };
        registry.activate();
        registry
    }

    pub(super) fn same_discovery(&self, config: &ExtensionLoadConfig) -> bool {
        self.load_config.package_paths == config.package_paths
            && self.load_config.directory_paths == config.directory_paths
            && self.load_config.project_package_paths == config.project_package_paths
            && self.discovery_fingerprint
                == extension_packages_fingerprint(&discover_extension_packages(config))
    }

    pub(super) fn all_extensions_loaded(&self) -> bool {
        self.extensions
            .iter()
            .all(|extension| extension.is_loaded())
    }

    fn activate(&self) {
        for extension in &self.extensions {
            let settings = self
                .load_config
                .settings
                .get(&extension.metadata.id)
                .cloned();
            extension.handle_lifecycle_event("lifecycle.activate", settings);
        }
    }

    pub(super) fn update_settings(&self, settings: BTreeMap<String, serde_json::Value>) {
        if self.load_config.settings == settings {
            return;
        }
        for extension in &self.extensions {
            if !extension.is_loaded() {
                continue;
            }
            let previous = self.load_config.settings.get(&extension.metadata.id);
            let current = settings.get(&extension.metadata.id);
            if previous != current {
                extension.handle_lifecycle_event("settings.changed", current.cloned());
            }
        }
    }

    pub(super) fn deactivate(&self) {
        for extension in &self.extensions {
            if extension.is_loaded() {
                extension.handle_lifecycle_event("lifecycle.deactivate", None);
            }
        }
    }

    pub(super) fn extensions(&self) -> Vec<Arc<LoadedWasmExtension>> {
        self.extensions.clone()
    }

    pub(super) fn applicability_snapshot(
        &self,
        project: Option<&ruby_fast_lsp_extension_api::ProjectContext>,
    ) -> ExtensionApplicabilitySnapshot {
        ExtensionApplicabilitySnapshot {
            registry_fingerprint: self.discovery_fingerprint,
            applies_to_source: self
                .extensions
                .iter()
                .map(|extension| extension.applies_to_source(project))
                .collect(),
        }
    }

    pub(super) fn extension_applies_to_source(
        &self,
        extension_index: usize,
        extension: &LoadedWasmExtension,
        project: Option<&ruby_fast_lsp_extension_api::ProjectContext>,
        applicability: Option<&ExtensionApplicabilitySnapshot>,
    ) -> bool {
        let Some(applicability) = applicability else {
            return extension.applies_to_source(project);
        };
        if applicability.registry_fingerprint != self.discovery_fingerprint
            || applicability.applies_to_source.len() != self.extensions.len()
        {
            // Registry replacement is rare and may race an already-running file
            // traversal. Preserve exact current-registry semantics in that case;
            // the owning indexing generation will replace the file normally.
            return extension.applies_to_source(project);
        }
        applicability.applies_to_source[extension_index]
    }

    pub(super) fn should_track_enclosing_call(
        &self,
        visitor: &FactCollector,
        node: &CallNode,
        applicability: Option<&ExtensionApplicabilitySnapshot>,
        tracked_call_prechecked: bool,
    ) -> bool {
        let method_name = utils::utf8_str(node.name().as_slice());
        if !tracked_call_prechecked && !self.tracked_call_names.contains(method_name) {
            return false;
        }

        if self
            .extensions
            .iter()
            .enumerate()
            .any(|(index, extension)| {
                extension.is_loaded()
                    && self.extension_applies_to_source(
                        index,
                        extension,
                        visitor.extension_project_context(),
                        applicability,
                    )
                    && extension
                        .semantic_targets
                        .iter()
                        .any(|target| target.frame && target.method.as_str() == method_name)
                    && extension.semantically_matches_call(visitor, node)
            })
        {
            return true;
        }

        if self
            .extensions
            .iter()
            .enumerate()
            .any(|(index, extension)| {
                extension.is_loaded()
                    && self.extension_applies_to_source(
                        index,
                        extension,
                        visitor.extension_project_context(),
                        applicability,
                    )
                    && extension.frame_call_names.contains(method_name)
            })
        {
            return true;
        }

        !visitor.enclosing_extension_calls().is_empty()
            && self
                .extensions
                .iter()
                .enumerate()
                .any(|(index, extension)| {
                    extension.is_loaded()
                        && self.extension_applies_to_source(
                            index,
                            extension,
                            visitor.extension_project_context(),
                            applicability,
                        )
                        && extension.can_run_inside_extension_frame(visitor, node)
                })
    }

    pub(super) fn frame_extension_ids(
        &self,
        visitor: &FactCollector,
        node: &CallNode,
        applicability: Option<&ExtensionApplicabilitySnapshot>,
    ) -> Vec<String> {
        let method_name = utils::utf8_str(node.name().as_slice());
        let active_frame_ids = visitor
            .enclosing_extension_calls()
            .iter()
            .flat_map(|call| call.frame_extension_ids.iter())
            .collect::<BTreeSet<_>>();
        let explicitly_switches_receiver = node
            .receiver()
            .is_some_and(|receiver| receiver.as_self_node().is_none());
        self.extensions
            .iter()
            .enumerate()
            .filter(|(index, extension)| {
                if !extension.is_loaded()
                    || !self.extension_applies_to_source(
                        *index,
                        extension,
                        visitor.extension_project_context(),
                        applicability,
                    )
                {
                    return false;
                }
                let inherits_frame = visitor.enclosing_extension_calls().iter().any(|call| {
                    call.frame_extension_ids
                        .iter()
                        .any(|id| id == &extension.metadata.id)
                });
                if !active_frame_ids.is_empty() && !inherits_frame && !explicitly_switches_receiver
                {
                    return false;
                }
                extension.semantically_matches_frame_call(visitor, node)
                    || (inherits_frame
                        && (extension.handles_call(method_name)
                            || extension.frame_call_names.contains(method_name)))
                    || (!extension.has_semantic_targets()
                        && extension.frame_call_names.contains(method_name))
            })
            .map(|(_, extension)| extension.metadata.id.clone())
            .collect()
    }

    pub(in crate::environment::extensions) fn status_reports(&self) -> Vec<ExtensionStatusReport> {
        self.extensions
            .iter()
            .map(|extension| extension.status_report())
            .collect()
    }

    pub(super) fn watcher_globs(&self) -> Vec<String> {
        self.extensions
            .iter()
            .filter(|extension| extension.is_loaded())
            .flat_map(|extension| extension.metadata.watched_files.iter().cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    /// Hand `commit` the semantic seed that `engine` lacks for this project
    /// applicability, then record it as applied. Nothing is produced when
    /// the engine already holds the seed for `applicability_fingerprint`.
    ///
    /// The seed ledger stays locked while `commit` runs, so concurrent seeds
    /// of one engine commit in the order the ledger records them and the
    /// last recorded applicability is the one the engine holds.
    pub(super) fn with_semantic_seed(
        &self,
        engine: &Arc<dyn Send + Sync>,
        project: Option<&ruby_fast_lsp_extension_api::ProjectContext>,
        applicability_fingerprint: ExtensionApplicabilityFingerprint,
        commit: impl FnOnce(ExtensionSemanticSeed),
    ) {
        let mut seeded_engines = self.semantic_seeded_engines.lock();
        seeded_engines.retain(|seeded| seeded.engine.strong_count() > 0);
        if let Some(seeded) = seeded_engines.iter_mut().find(|seeded| {
            seeded.engine.upgrade().is_some_and(|seeded_engine| {
                std::ptr::addr_eq(Arc::as_ptr(&seeded_engine), Arc::as_ptr(engine))
            })
        }) {
            if seeded.applicability_fingerprint == applicability_fingerprint {
                return;
            }
            seeded.applicability_fingerprint = applicability_fingerprint;
        }

        let seed = ExtensionSemanticSeed::from_extensions(
            self.extensions
                .iter()
                .filter(|extension| extension.is_loaded() && extension.applies_to(project))
                .map(Arc::as_ref),
        );
        commit(seed);
        if !seeded_engines.iter().any(|seeded| {
            seeded.engine.upgrade().is_some_and(|seeded_engine| {
                std::ptr::addr_eq(Arc::as_ptr(&seeded_engine), Arc::as_ptr(engine))
            })
        }) {
            seeded_engines.push(SeededExtensionEngine {
                engine: Arc::downgrade(engine),
                applicability_fingerprint,
            });
        }
    }

    /// Forget that `engine` holds any semantic seed, so the next file pass
    /// commits one again.
    pub(super) fn forget_semantic_seed(&self, engine: &Arc<dyn Send + Sync>) {
        self.semantic_seeded_engines.lock().retain(|seeded| {
            seeded.engine.upgrade().is_some_and(|seeded_engine| {
                !std::ptr::addr_eq(Arc::as_ptr(&seeded_engine), Arc::as_ptr(engine))
            })
        });
    }
}

#[cfg(test)]
impl ExtensionApplicabilitySnapshot {
    pub(in crate::environment::extensions) fn applies_to_extension(
        &self,
        registry: &ExtensionRegistryHandle,
        extension_id: &str,
        project: Option<&ruby_fast_lsp_extension_api::ProjectContext>,
    ) -> bool {
        let registry = registry.inner.read();
        let (index, extension) = registry
            .extensions
            .iter()
            .enumerate()
            .find(|(_, extension)| extension.metadata.id == extension_id)
            .expect_invariant(
                "applicability test requested an unknown extension",
                "snapshots contain one decision per loaded registry extension",
                "load the extension before querying its decision",
            );
        registry.extension_applies_to_source(index, extension, project, Some(self))
    }
}

pub(in crate::environment::extensions) fn extension_applicability_fingerprint(
    project: Option<&ruby_fast_lsp_extension_api::ProjectContext>,
) -> ExtensionApplicabilityFingerprint {
    ExtensionApplicabilityFingerprint::from_project_context(project)
}

fn tracked_call_names(extensions: &[Arc<LoadedWasmExtension>]) -> Arc<HashSet<String>> {
    let mut names = HashSet::new();
    for extension in extensions {
        names.extend(extension.indexed_call_names.iter().cloned());
        names.extend(extension.frame_call_names.iter().cloned());
    }
    Arc::new(names)
}

pub(super) fn extension_target_owner_exists(
    visitor: &FactCollector,
    target: &ExtensionMethodTarget,
) -> bool {
    let required_owner = FullyQualifiedName::namespace(target.owner.clone());
    visitor.project_namespace_exists(&required_owner)
}
