use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::{Arc, Weak};

use parking_lot::{Mutex, RwLock};
use ruby_analysis::core::{
    FullyQualifiedName, GraphNodeFact, MethodFact, NamespaceKind, SourceKind, SymbolFact,
    SymbolKind as AnalysisSymbolKind, TextRange,
};
use ruby_analysis::engine::{FileFacts, ResolveMode, SourceFileInput};
use ruby_analysis::indexer as utils;
use ruby_analysis::indexer::fact_collector::FactCollector;
use ruby_fast_lsp_extension_api::Extension;
use ruby_prism::CallNode;

use crate::extensions::loading::config::ExtensionLoadConfig;
use crate::extensions::loading::manifest::ExtensionMethodTarget;
use crate::extensions::loading::packages::{
    discover_extension_packages, extension_packages_fingerprint,
    load_wasm_extensions_from_packages_with_cache,
};
#[cfg(test)]
use crate::extensions::registry::handle::ExtensionRegistryHandle;
use crate::extensions::registry::loaded::LoadedWasmExtension;
use crate::extensions::registry::status::ExtensionStatusReport;
use crate::extensions::ExtensionApplicabilityFingerprint;
use crate::persistent_cache::PersistentDerivedProductCache;

pub(in crate::extensions) struct ExtensionRegistry {
    pub(super) extensions: Vec<Arc<LoadedWasmExtension>>,
    pub(in crate::extensions) tracked_call_names: Arc<HashSet<String>>,
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

struct SeededExtensionEngine {
    engine: Weak<RwLock<ruby_analysis::engine::AnalysisEngine>>,
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

    pub(in crate::extensions) fn load(config: &ExtensionLoadConfig) -> Self {
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

        if !visitor.enclosing_extension_calls().is_empty()
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
        {
            return true;
        }

        !self.has_loaded_wasm_for_call(method_name)
            && ruby_fast_lsp_extension_rspec::extension()
                .indexed_call_names()
                .contains(&method_name)
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

    pub(in crate::extensions) fn status_reports(&self) -> Vec<ExtensionStatusReport> {
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

    pub(in crate::extensions) fn has_loaded_wasm_for_call(&self, method_name: &str) -> bool {
        self.extensions
            .iter()
            .any(|extension| extension.is_loaded() && extension.handles_call(method_name))
    }

    pub(super) fn ensure_semantic_seed_facts(
        &self,
        engine: &Arc<RwLock<ruby_analysis::engine::AnalysisEngine>>,
        project: Option<&ruby_fast_lsp_extension_api::ProjectContext>,
        applicability_fingerprint: ExtensionApplicabilityFingerprint,
    ) {
        let mut seeded_engines = self.semantic_seeded_engines.lock();
        seeded_engines.retain(|seeded| seeded.engine.strong_count() > 0);
        if let Some(seeded) = seeded_engines.iter_mut().find(|seeded| {
            seeded
                .engine
                .upgrade()
                .is_some_and(|seeded_engine| Arc::ptr_eq(&seeded_engine, engine))
        }) {
            if seeded.applicability_fingerprint == applicability_fingerprint {
                return;
            }
            seeded.applicability_fingerprint = applicability_fingerprint;
        }

        let mut engine_guard = engine.write();
        let file_id = engine_guard.register_file(SourceFileInput {
            path: std::path::absolute("/__ruby_fast_lsp_extension__/semantic_targets.rb")
                .expect(
                    "INVARIANT VIOLATED: extension semantic source has no absolute native path. \
                     This is a bug because registered definitions need a valid file URI for navigation. \
                     Fix: retain the filesystem root when constructing the synthetic source path.",
                ),
            content: String::new(),
            kind: SourceKind::Stub,
        });
        let range = TextRange::new(file_id, 0, 0);
        let mut facts = FileFacts::default();
        for extension in &self.extensions {
            if !extension.is_loaded() || !extension.applies_to(project) {
                continue;
            }
            for namespace in &extension.semantic_namespaces {
                let instance = FullyQualifiedName::namespace_with_kind(
                    namespace.owner.clone(),
                    NamespaceKind::Instance,
                );
                let singleton = FullyQualifiedName::namespace_with_kind(
                    namespace.owner.clone(),
                    NamespaceKind::Singleton,
                );
                for owner in [instance, singleton] {
                    let fact = GraphNodeFact::new(owner, namespace.declaration_kind, range);
                    if !facts.graph_nodes.contains(&fact) {
                        facts.graph_nodes.push(fact);
                    }
                }
            }
            for target in &extension.semantic_targets {
                let owner = FullyQualifiedName::namespace_with_kind(
                    target.owner.clone(),
                    target.owner_kind,
                );
                let fqn = FullyQualifiedName::method(target.owner.clone(), target.method);
                facts.symbols.push(SymbolFact::new(
                    fqn.clone(),
                    AnalysisSymbolKind::Method,
                    range,
                ));
                facts.methods.push(MethodFact::new(fqn, owner, range));
            }
        }
        engine_guard.replace_facts(file_id, facts, ResolveMode::Deferred);
        drop(engine_guard);
        if !seeded_engines.iter().any(|seeded| {
            seeded
                .engine
                .upgrade()
                .is_some_and(|seeded_engine| Arc::ptr_eq(&seeded_engine, engine))
        }) {
            seeded_engines.push(SeededExtensionEngine {
                engine: Arc::downgrade(engine),
                applicability_fingerprint,
            });
        }
    }
}

#[cfg(test)]
impl ExtensionApplicabilitySnapshot {
    pub(in crate::extensions) fn applies_to_extension(
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
            .expect(
                "INVARIANT VIOLATED: applicability test requested an unknown extension. This is a broken test because snapshots contain one decision per loaded registry extension. Fix: load the extension before querying its decision.",
            );
        registry.extension_applies_to_source(index, extension, project, Some(self))
    }
}

pub(in crate::extensions) fn extension_applicability_fingerprint(
    project: Option<&ruby_fast_lsp_extension_api::ProjectContext>,
) -> ExtensionApplicabilityFingerprint {
    ExtensionApplicabilityFingerprint::from_project_context(project)
}

fn tracked_call_names(extensions: &[Arc<LoadedWasmExtension>]) -> Arc<HashSet<String>> {
    let mut names = ruby_fast_lsp_extension_rspec::extension()
        .indexed_call_names()
        .iter()
        .map(|name| (*name).to_string())
        .collect::<HashSet<_>>();
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
    let engine = visitor.analysis_engine().read();
    ruby_analysis::engine::AnalysisQuery::new(&engine).namespace_exists(&required_owner)
}
