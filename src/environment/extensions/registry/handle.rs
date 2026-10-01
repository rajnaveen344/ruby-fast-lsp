use std::collections::{BTreeSet, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::RwLock;
use ruby_analysis::indexer::fact_collector::{FactCollector, FactCollectorExtensionHost};
use ruby_fast_lsp_extension_api::{ExtensionEvent, ResolvedCall};
use ruby_prism::CallNode;
use tower_lsp::lsp_types::{CodeLens, DocumentSymbol, FileEvent, Url};

use crate::environment::config::RubyFastLspConfig;
use crate::environment::extensions::dispatch::call_context::resolved_call_for_stack;
use crate::environment::extensions::dispatch::calls::process_call_node_with_registry;
use crate::environment::extensions::loading::config::ExtensionLoadConfig;
use crate::environment::extensions::processes::{
    handle_watched_file_changes_with_registry, run_extension_process,
    validate_extension_process_request, validate_extension_reindex_files,
};
use crate::environment::extensions::registry::loaded::LoadedWasmExtension;
use crate::environment::extensions::registry::state::{
    extension_applicability_fingerprint, ExtensionApplicabilitySnapshot, ExtensionRegistry,
};
use crate::environment::extensions::registry::status::ExtensionStatusReport;
use crate::environment::extensions::responses::{
    code_lenses_with_registry, document_symbols_with_registry,
};
use crate::environment::extensions::{
    ProjectContextSnapshot, EXTENSION_LOAD_TRANSIENT_MEMORY_BYTES,
    EXTENSION_RESPONSE_TRANSIENT_MEMORY_BYTES,
};
use crate::loader::cache::persistent::PersistentDerivedProductCache;
use crate::loader::scheduling::resources::{
    IndexingResourceGovernor, IndexingResourcePriority, IndexingWorkSpec,
};

#[derive(Clone)]
pub struct ExtensionRegistryHandle {
    pub(in crate::environment::extensions) inner: Arc<RwLock<ExtensionRegistry>>,
    pub(in crate::environment::extensions) reconfiguration: Arc<tokio::sync::Mutex<()>>,
    pub(in crate::environment::extensions) persistent_cache: Option<PersistentDerivedProductCache>,
}

impl std::fmt::Debug for ExtensionRegistryHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExtensionRegistryHandle")
            .field("extension_count", &self.inner.read().extensions.len())
            .finish()
    }
}

impl ExtensionRegistryHandle {
    pub fn empty() -> Self {
        Self::empty_with_persistent_cache(None)
    }

    pub fn empty_with_cache(persistent_cache: PersistentDerivedProductCache) -> Self {
        Self::empty_with_persistent_cache(Some(persistent_cache))
    }

    fn empty_with_persistent_cache(
        persistent_cache: Option<PersistentDerivedProductCache>,
    ) -> Self {
        Self {
            inner: Arc::new(RwLock::new(ExtensionRegistry::empty())),
            reconfiguration: Arc::new(tokio::sync::Mutex::new(())),
            persistent_cache,
        }
    }

    pub fn from_environment() -> Self {
        Self::from_environment_with_persistent_cache(None)
    }

    pub fn from_environment_with_cache(persistent_cache: PersistentDerivedProductCache) -> Self {
        Self::from_environment_with_persistent_cache(Some(persistent_cache))
    }

    fn from_environment_with_persistent_cache(
        persistent_cache: Option<PersistentDerivedProductCache>,
    ) -> Self {
        let config = ExtensionLoadConfig::from_environment();
        let registry =
            ExtensionRegistry::load_with_persistent_cache(&config, persistent_cache.as_ref());
        Self {
            inner: Arc::new(RwLock::new(registry)),
            reconfiguration: Arc::new(tokio::sync::Mutex::new(())),
            persistent_cache,
        }
    }

    pub fn from_config(config: &RubyFastLspConfig) -> Self {
        Self {
            inner: Arc::new(RwLock::new(ExtensionRegistry::load(
                &ExtensionLoadConfig::from_config(config),
            ))),
            reconfiguration: Arc::new(tokio::sync::Mutex::new(())),
            persistent_cache: None,
        }
    }

    pub fn configure_from_config(&self, config: &RubyFastLspConfig) {
        self.configure_from_config_and_workspace_roots(config, &[]);
    }

    pub fn configure_from_config_and_workspace_roots(
        &self,
        config: &RubyFastLspConfig,
        workspace_roots: &[PathBuf],
    ) {
        let load_config =
            ExtensionLoadConfig::from_config_and_workspace_roots(config, workspace_roots);
        self.configure_from_load_config(load_config);
    }

    pub async fn configure_from_config_and_workspace_roots_governed(
        &self,
        config: &RubyFastLspConfig,
        workspace_roots: &[PathBuf],
        indexing_resources: IndexingResourceGovernor,
    ) -> anyhow::Result<()> {
        let _reconfiguration = self.reconfiguration.lock().await;
        let config = config.clone();
        let workspace_roots = workspace_roots.to_vec();
        let registry = self.clone();
        indexing_resources
            .run_with_resources(
                "extension registry reconfiguration",
                IndexingWorkSpec::new(
                    None,
                    IndexingResourcePriority::Background,
                    1,
                    EXTENSION_LOAD_TRANSIENT_MEMORY_BYTES,
                    1,
                ),
                None,
                move || {
                    let load_config = ExtensionLoadConfig::from_config_and_workspace_roots(
                        &config,
                        &workspace_roots,
                    );
                    registry.configure_from_load_config(load_config);
                },
            )
            .await
    }

    fn configure_from_load_config(&self, load_config: ExtensionLoadConfig) {
        let settings_only = {
            let registry = self.inner.read();
            if !registry.same_discovery(&load_config) || !registry.all_extensions_loaded() {
                false
            } else {
                registry.update_settings(load_config.settings.clone());
                true
            }
        };
        if settings_only {
            let mut registry = self.inner.write();
            registry.load_config.settings = load_config.settings;
            return;
        }

        let replacement = ExtensionRegistry::load_with_persistent_cache(
            &load_config,
            self.persistent_cache.as_ref(),
        );
        let mut registry = self.inner.write();
        let previous = std::mem::replace(&mut *registry, replacement);
        drop(registry);
        previous.deactivate();
    }

    pub fn shutdown(&self) {
        self.inner.read().deactivate();
    }

    pub fn status_reports(&self) -> Vec<ExtensionStatusReport> {
        self.inner.read().status_reports()
    }

    pub fn ensure_semantic_seed_facts(
        &self,
        engine: &Arc<RwLock<ruby_analysis::engine::AnalysisEngine>>,
        project: Option<&ruby_fast_lsp_extension_api::ProjectContext>,
    ) {
        let applicability_fingerprint = extension_applicability_fingerprint(project);
        self.inner
            .read()
            .ensure_semantic_seed_facts(engine, project, applicability_fingerprint);
    }

    pub(crate) fn ensure_semantic_seed_facts_for_snapshot(
        &self,
        engine: &Arc<RwLock<ruby_analysis::engine::AnalysisEngine>>,
        snapshot: &ProjectContextSnapshot,
    ) {
        self.inner.read().ensure_semantic_seed_facts(
            engine,
            Some(&snapshot.context),
            snapshot.applicability_fingerprint,
        );
    }

    pub(crate) fn has_applicable_semantic_targets(
        &self,
        project: Option<&ruby_fast_lsp_extension_api::ProjectContext>,
    ) -> bool {
        self.inner.read().extensions.iter().any(|extension| {
            extension.is_loaded()
                && extension.has_semantic_targets()
                && extension.applies_to_source(project)
        })
    }

    pub fn process_call_node(&self, visitor: &mut FactCollector, node: &CallNode) -> bool {
        process_call_node_with_registry(self, visitor, node, None, false)
    }

    pub(crate) fn applicability_snapshot(
        &self,
        project: Option<&ruby_fast_lsp_extension_api::ProjectContext>,
    ) -> ExtensionApplicabilitySnapshot {
        self.inner.read().applicability_snapshot(project)
    }

    pub(crate) fn tracked_call_names(&self) -> Arc<HashSet<String>> {
        Arc::clone(&self.inner.read().tracked_call_names)
    }

    pub(crate) fn process_call_node_with_applicability(
        &self,
        visitor: &mut FactCollector,
        node: &CallNode,
        applicability: &ExtensionApplicabilitySnapshot,
    ) -> bool {
        process_call_node_with_registry(self, visitor, node, Some(applicability), true)
    }

    pub(crate) fn should_track_enclosing_call_with_applicability(
        &self,
        visitor: &FactCollector,
        node: &CallNode,
        applicability: &ExtensionApplicabilitySnapshot,
    ) -> bool {
        self.inner
            .read()
            .should_track_enclosing_call(visitor, node, Some(applicability), true)
    }

    pub(crate) fn resolved_call_for_stack_with_applicability(
        &self,
        visitor: &FactCollector,
        node: &CallNode,
        applicability: &ExtensionApplicabilitySnapshot,
    ) -> ResolvedCall {
        let mut call = resolved_call_for_stack(visitor, node);
        call.frame_extension_ids =
            self.inner
                .read()
                .frame_extension_ids(visitor, node, Some(applicability));
        call
    }

    pub fn document_symbols(
        &self,
        uri: &str,
        text: &str,
        project: Option<ruby_fast_lsp_extension_api::ProjectContext>,
    ) -> Vec<DocumentSymbol> {
        document_symbols_with_registry(self, uri, text, project)
    }

    pub async fn document_symbols_governed(
        &self,
        indexing_resources: IndexingResourceGovernor,
        project_root: Option<PathBuf>,
        uri: String,
        text: String,
        project: Option<ruby_fast_lsp_extension_api::ProjectContext>,
    ) -> anyhow::Result<Vec<DocumentSymbol>> {
        if !self.has_loaded_capability("document_symbol") {
            return Ok(Vec::new());
        }
        let registry = self.clone();
        indexing_resources
            .run_with_resources(
                "extension document symbols",
                IndexingWorkSpec::new(
                    project_root,
                    IndexingResourcePriority::OpenDocument,
                    1,
                    EXTENSION_RESPONSE_TRANSIENT_MEMORY_BYTES,
                    0,
                ),
                None,
                move || registry.document_symbols(&uri, &text, project),
            )
            .await
    }

    pub fn code_lenses(
        &self,
        uri: &str,
        text: &str,
        project: Option<ruby_fast_lsp_extension_api::ProjectContext>,
    ) -> Vec<CodeLens> {
        code_lenses_with_registry(self, uri, text, project)
    }

    pub async fn code_lenses_governed(
        &self,
        indexing_resources: IndexingResourceGovernor,
        project_root: Option<PathBuf>,
        uri: String,
        text: String,
        project: Option<ruby_fast_lsp_extension_api::ProjectContext>,
    ) -> anyhow::Result<Vec<CodeLens>> {
        if !self.has_loaded_capability("code_lens") {
            return Ok(Vec::new());
        }
        let registry = self.clone();
        indexing_resources
            .run_with_resources(
                "extension code lenses",
                IndexingWorkSpec::new(
                    project_root,
                    IndexingResourcePriority::OpenDocument,
                    1,
                    EXTENSION_RESPONSE_TRANSIENT_MEMORY_BYTES,
                    0,
                ),
                None,
                move || registry.code_lenses(&uri, &text, project),
            )
            .await
    }

    fn has_loaded_capability(&self, capability: &str) -> bool {
        self.inner.read().extensions.iter().any(|extension| {
            extension.is_loaded()
                && extension
                    .metadata
                    .capabilities
                    .iter()
                    .any(|candidate| candidate == capability)
        })
    }

    pub fn watcher_globs(&self) -> Vec<String> {
        self.inner.read().watcher_globs()
    }

    pub async fn handle_watched_file_changes(
        &self,
        workspace_trusted: bool,
        workspace_roots: &[PathBuf],
        changes: &[FileEvent],
        indexing_resources: IndexingResourceGovernor,
    ) -> Vec<Url> {
        let pending = handle_watched_file_changes_with_registry(self, workspace_roots, changes);
        let mut reindex_uris = BTreeSet::new();
        for pending in pending {
            if !pending.loaded.is_loaded() {
                continue;
            }
            let validated = match validate_extension_process_request(
                &pending.loaded.metadata.id,
                workspace_trusted,
                &pending.loaded.metadata.permissions,
                &pending.loaded.metadata.process_commands,
                workspace_roots,
                &pending.event_roots,
                &pending.request,
            ) {
                Ok(validated) => validated,
                Err(err) => {
                    pending.loaded.reject(err.to_string());
                    continue;
                }
            };
            let result = run_extension_process(validated, indexing_resources.clone()).await;
            let event = ExtensionEvent {
                event: "process.completed".to_string(),
                call: None,
                document: None,
                project: None,
                settings: None,
                files: None,
                process_results: Some(vec![result]),
            };
            match pending.loaded.handle_event_for_project(&event, None) {
                Ok(output)
                    if output.index_patches.is_empty()
                        && output.execution_contexts.is_empty()
                        && output.response_patches.is_empty()
                        && output.command_patches.is_empty()
                        && output.process_requests.is_empty() =>
                {
                    match validate_extension_reindex_files(
                        &pending.loaded.metadata.id,
                        workspace_roots,
                        &pending.event_roots,
                        &output.reindex_files,
                    ) {
                        Ok(uris) => reindex_uris.extend(uris),
                        Err(err) => {
                            pending.loaded.reject(err.to_string());
                        }
                    }
                }
                Ok(_) => {
                    pending.loaded.reject(format!(
                        "extension `{}` returned output from `process.completed`; process completion callbacks may update private extension state only",
                        pending.loaded.metadata.id
                    ));
                }
                Err(err) => {
                    pending.loaded.fail(format!(
                        "extension `{}` process.completed failed: {err}",
                        pending.loaded.metadata.id
                    ));
                }
            }
        }
        reindex_uris.into_iter().collect()
    }

    pub(in crate::environment::extensions) fn extensions(&self) -> Vec<Arc<LoadedWasmExtension>> {
        self.inner.read().extensions()
    }

    pub(in crate::environment::extensions) fn extensions_with_applicability(
        &self,
        project: Option<&ruby_fast_lsp_extension_api::ProjectContext>,
        applicability: Option<&ExtensionApplicabilitySnapshot>,
    ) -> Vec<(Arc<LoadedWasmExtension>, bool)> {
        let registry = self.inner.read();
        registry
            .extensions
            .iter()
            .enumerate()
            .map(|(index, extension)| {
                (
                    extension.clone(),
                    registry.extension_applies_to_source(index, extension, project, applicability),
                )
            })
            .collect()
    }
}

impl FactCollectorExtensionHost for ExtensionRegistryHandle {
    fn process_call_node(&self, visitor: &mut FactCollector, node: &CallNode) -> bool {
        ExtensionRegistryHandle::process_call_node(self, visitor, node)
    }

    fn should_track_enclosing_call(&self, visitor: &FactCollector, node: &CallNode) -> bool {
        self.inner
            .read()
            .should_track_enclosing_call(visitor, node, None, false)
    }

    fn resolved_call_for_stack(&self, visitor: &FactCollector, node: &CallNode) -> ResolvedCall {
        let mut call = resolved_call_for_stack(visitor, node);
        call.frame_extension_ids = self.inner.read().frame_extension_ids(visitor, node, None);
        call
    }
}
