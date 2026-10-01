//! Process-wide immutable products. Projects bind results into isolated engines.
use super::RubyLanguageServer;
use crate::environment::config::runtime::SelectedRuntimeDescriptor;
use crate::environment::runtime::catalog::{
    DiscoveredRuntime, ProjectRuntimeStatus, RuntimeCatalog, RuntimeDiscoverParams, RuntimeStatus,
    RuntimeStatusParams,
};
#[cfg(test)]
use crate::invariant::ExpectInvariant;
use crate::loader::cache::dependency_product::GemBindingStat;
use crate::loader::cache::persistent::PersistentProductStat;
use crate::loader::context::{RuntimeDiscovery, SharedProducts};
use crate::loader::scheduling::resources::IndexingResourceGovernor;
use crate::utils::single_flight::SingleFlightStat;
use anyhow::Result;
use ruby_analysis::stats::StatsSnapshot;
use std::path::PathBuf;
use std::sync::Arc;
use tower_lsp::jsonrpc::Result as LspResult;

/// Detached process-wide telemetry. This view owns no caches or project facts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeProductSnapshot {
    pub core_templates: StatsSnapshot<SingleFlightStat>,
    pub stdlib_paths: StatsSnapshot<SingleFlightStat>,
    pub gem_dependencies: StatsSnapshot<SingleFlightStat>,
    pub classpath_files: StatsSnapshot<SingleFlightStat>,
    pub java_artifacts: StatsSnapshot<SingleFlightStat>,
    pub persistent_gems: StatsSnapshot<PersistentProductStat>,
    pub persistent_java: StatsSnapshot<PersistentProductStat>,
    pub compiled_wasm: StatsSnapshot<PersistentProductStat>,
    pub gem_bindings: StatsSnapshot<GemBindingStat>,
}

/// Server-held products: the loader-visible shared products plus the
/// process-wide runtime discovery snapshot.
#[derive(Clone)]
pub(crate) struct RuntimeProducts {
    discovered_runtimes: Arc<tokio::sync::OnceCell<Vec<DiscoveredRuntime>>>,
    shared: SharedProducts,
}
impl std::ops::Deref for RuntimeProducts {
    type Target = SharedProducts;
    fn deref(&self) -> &SharedProducts {
        &self.shared
    }
}
impl RuntimeProducts {
    fn snapshot(&self) -> RuntimeProductSnapshot {
        RuntimeProductSnapshot {
            core_templates: self.core_templates().snapshot(),
            stdlib_paths: self.stdlib_paths().snapshot(),
            gem_dependencies: self.gem_dependencies().snapshot(),
            classpath_files: self.classpath_files().snapshot(),
            java_artifacts: self.java_artifacts().snapshot(),
            persistent_gems: self.persistent().gem_product_snapshot(),
            persistent_java: self.persistent().java_artifact_snapshot(),
            compiled_wasm: self.persistent().compiled_wasm_snapshot(),
            gem_bindings: self.gem_bindings().snapshot(),
        }
    }

    pub(super) fn new(root: PathBuf) -> Self {
        Self {
            discovered_runtimes: Arc::new(tokio::sync::OnceCell::new()),
            shared: SharedProducts::new(root),
        }
    }

    /// Shared product handles for one loader context.
    pub(crate) fn shared(&self) -> SharedProducts {
        self.shared.clone()
    }

    /// Runtime discovery bound to the given resource governor.
    pub(crate) fn discovery(&self, resources: &IndexingResourceGovernor) -> RuntimeDiscovery {
        RuntimeDiscovery::new(self.discovered_runtimes.clone(), resources.clone())
    }
}

impl RubyLanguageServer {
    /// Read process-wide counters and retained weights without exposing cache handles.
    /// Each component is observed independently, matching its own synchronization.
    pub fn runtime_product_snapshot(&self) -> RuntimeProductSnapshot {
        self.products.snapshot()
    }

    pub fn compiled_wasm_cache_snapshot(&self) -> StatsSnapshot<PersistentProductStat> {
        self.products.persistent().compiled_wasm_snapshot()
    }

    /// Handle `ruby-fast-lsp/runtime/discover` without exposing editor policy
    /// or mutating any project runtime selection.
    pub async fn handle_runtime_discover(
        &self,
        _params: RuntimeDiscoverParams,
    ) -> LspResult<RuntimeCatalog> {
        Ok(
            crate::environment::runtime::catalog::runtime_catalog_for_projects(
                self.workspace_root_paths(),
                self.discovered_runtimes().await,
            ),
        )
    }

    async fn discovered_runtimes(&self) -> Vec<DiscoveredRuntime> {
        self.products
            .discovery(self.indexing.resources())
            .discovered_runtimes()
            .await
    }

    #[cfg(test)]
    pub(crate) fn set_discovered_runtimes_for_tests(&self, runtimes: Vec<DiscoveredRuntime>) {
        self.products
            .discovered_runtimes
            .set(runtimes)
            .expect_invariant(
                "test runtime catalog was initialized more than once",
                "each test server must own one immutable discovery snapshot",
                "create a fresh RubyLanguageServer per runtime test",
            );
    }

    pub(crate) async fn resolve_auto_runtime(
        &self,
        project_root: &std::path::Path,
    ) -> Result<Option<SelectedRuntimeDescriptor>> {
        self.products
            .discovery(self.indexing.resources())
            .resolve_auto_runtime(project_root)
            .await
    }

    pub async fn handle_runtime_status(
        &self,
        params: RuntimeStatusParams,
    ) -> LspResult<RuntimeStatus> {
        let config = self.config.lock().clone();
        let mut projects = self
            .list_workspaces()
            .into_iter()
            .filter(|workspace| {
                params
                    .project_root
                    .as_ref()
                    .is_none_or(|root| root == &workspace.root_path)
            })
            .map(|workspace| {
                let root = workspace.root_path.to_string_lossy();
                let selection = config
                    .runtime
                    .selection_for_project(&root, &config.ruby_version);
                let (
                    mode,
                    implementation,
                    family,
                    engine_version,
                    compatibility_version,
                    executable,
                    java_home,
                    stub_overlay,
                ) = match selection {
                    crate::environment::config::runtime::EffectiveRuntimeSelection::Explicit(runtime) => {
                        let stub_overlay = (runtime.implementation
                            == crate::environment::runtime::catalog::RuntimeImplementation::Jruby)
                            .then(|| runtime.family.clone());
                        (
                            "explicit".to_string(),
                            Some(runtime.implementation),
                            Some(runtime.family),
                            Some(runtime.engine_version),
                            Some(runtime.compatibility_version),
                            Some(runtime.executable),
                            runtime.java_home,
                            stub_overlay,
                        )
                    }
                    crate::environment::config::runtime::EffectiveRuntimeSelection::Auto => {
                        if let Some(runtime) = workspace.runtime.selected().read().clone() {
                            let stub_overlay = (runtime.implementation
                                == crate::environment::runtime::catalog::RuntimeImplementation::Jruby)
                                .then(|| runtime.family.clone());
                            (
                                "auto".to_string(),
                                Some(runtime.implementation),
                                Some(runtime.family),
                                Some(runtime.engine_version),
                                Some(runtime.compatibility_version),
                                Some(runtime.executable),
                                runtime.java_home,
                                stub_overlay,
                            )
                        } else {
                            ("auto".to_string(), None, None, None, None, None, None, None)
                        }
                    }
                    crate::environment::config::runtime::EffectiveRuntimeSelection::LegacyMriCompatibility {
                        major,
                        minor,
                    } => {
                        let compatibility = format!("{major}.{minor}");
                        (
                            "legacy".to_string(),
                            Some(crate::environment::runtime::catalog::RuntimeImplementation::Mri),
                            Some(compatibility.clone()),
                            None,
                            Some(compatibility),
                            None,
                            None,
                            None,
                        )
                    }
                };
                ProjectRuntimeStatus {
                    root: workspace.root_path,
                    mode,
                    implementation,
                    family,
                    engine_version,
                    compatibility_version,
                    executable,
                    java_home,
                    stub_overlay,
                    classpath_fingerprint_sha256: workspace
                        .runtime
                        .classpath_fingerprint()
                        .read()
                        .clone(),
                    indexing_complete: workspace.indexing_status.snapshot().is_ready(),
                    indexing: workspace.indexing_status.snapshot(),
                }
            })
            .collect::<Vec<_>>();
        projects.sort_by(|left, right| left.root.cmp(&right.root));
        Ok(RuntimeStatus { projects })
    }
}
