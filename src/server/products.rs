//! Process-wide immutable products. Projects bind results into isolated engines.
use super::RubyLanguageServer;
use crate::config::runtime::SelectedRuntimeDescriptor;
use crate::runtime::catalog::{
    DiscoveredRuntime, ProjectRuntimeStatus, RuntimeCatalog, RuntimeDiscoverParams, RuntimeStatus,
    RuntimeStatusParams,
};
use anyhow::Result;
use log::warn;
use ruby_analysis::engine::AnalysisEngine;
use std::path::PathBuf;
use std::sync::Arc;
use tower_lsp::jsonrpc::Result as LspResult;

const MIB: u64 = 1024 * 1024;
pub(super) const CORE_ENGINE_CACHE_MAX_ENTRIES: usize = 8;
pub(super) const CORE_ENGINE_CACHE_MAX_WEIGHT_BYTES: u64 = 128 * MIB;
const RUNTIME_STDLIB_PATH_CACHE_MAX_ENTRIES: usize = 32;
const RUNTIME_STDLIB_PATH_CACHE_MAX_WEIGHT_BYTES: u64 = MIB;
fn new_core_engine_cache(
) -> crate::single_flight::BoundedSingleFlightCache<String, ruby_analysis::engine::AnalysisEngine> {
    crate::single_flight::BoundedSingleFlightCache::new(
        CORE_ENGINE_CACHE_MAX_ENTRIES,
        CORE_ENGINE_CACHE_MAX_WEIGHT_BYTES,
        |engine: &ruby_analysis::engine::AnalysisEngine| {
            u64::try_from(engine.estimated_memory_stats().total()).expect(
                "INVARIANT VIOLATED: a core template heap estimate does not fit u64. This is a bug because one in-memory engine cannot exceed the process address space. Fix: inspect engine memory estimation overflow.",
            )
        },
    )
}

fn new_gem_dependency_cache() -> crate::single_flight::BoundedSingleFlightCache<
    crate::dependency_product::GemDependencyProductKey,
    crate::dependency_product::GemDependencyProduct,
> {
    crate::single_flight::BoundedSingleFlightCache::ephemeral(
        |product: &crate::dependency_product::GemDependencyProduct| {
            product.estimated_weight_bytes()
        },
    )
}

fn new_runtime_stdlib_path_cache() -> crate::single_flight::BoundedSingleFlightCache<
    crate::indexer::indexer_stdlib::RuntimeStdlibPathKey,
    crate::indexer::indexer_stdlib::RuntimeStdlibPaths,
> {
    crate::single_flight::BoundedSingleFlightCache::new(
        RUNTIME_STDLIB_PATH_CACHE_MAX_ENTRIES,
        RUNTIME_STDLIB_PATH_CACHE_MAX_WEIGHT_BYTES,
        crate::indexer::indexer_stdlib::RuntimeStdlibPaths::estimated_weight_bytes,
    )
}

/// Detached counters and estimated retained bytes for one immutable-product cache.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RetainedProductSnapshot {
    pub reuse: crate::single_flight::SingleFlightSnapshot,
    pub retained_weight_bytes: u64,
}

/// Detached process-wide telemetry. This view owns no caches or project facts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RuntimeProductSnapshot {
    pub core_templates: RetainedProductSnapshot,
    pub stdlib_paths: RetainedProductSnapshot,
    pub gem_dependencies: RetainedProductSnapshot,
    pub classpath_files: RetainedProductSnapshot,
    pub java_artifacts: RetainedProductSnapshot,
    pub persistent_gems: crate::persistent_cache::PersistentProductSnapshot,
    pub persistent_java: crate::persistent_cache::PersistentProductSnapshot,
    pub compiled_wasm: crate::persistent_cache::PersistentProductSnapshot,
    pub gem_bindings: crate::dependency_product::GemDependencyBindingSnapshot,
}

#[derive(Clone)]
pub(crate) struct RuntimeProducts {
    pub(super) discovered_runtimes: Arc<tokio::sync::OnceCell<Vec<DiscoveredRuntime>>>,
    core_templates: crate::single_flight::BoundedSingleFlightCache<String, AnalysisEngine>,
    stdlib_paths: crate::single_flight::BoundedSingleFlightCache<
        crate::indexer::indexer_stdlib::RuntimeStdlibPathKey,
        crate::indexer::indexer_stdlib::RuntimeStdlibPaths,
    >,
    gem_dependencies: crate::single_flight::BoundedSingleFlightCache<
        crate::dependency_product::GemDependencyProductKey,
        crate::dependency_product::GemDependencyProduct,
    >,
    classpath_files: crate::runtime::jruby::classpath::ClasspathFileProductCache,
    java_artifacts: crate::runtime::jruby::java_catalog::JavaArtifactProductCache,
    persistent: crate::persistent_cache::PersistentDerivedProductCache,
    gem_bindings: Arc<crate::dependency_product::GemDependencyBindingCounters>,
}
impl RuntimeProducts {
    fn snapshot(&self) -> RuntimeProductSnapshot {
        RuntimeProductSnapshot {
            core_templates: RetainedProductSnapshot {
                reuse: self.core_templates.snapshot(),
                retained_weight_bytes: self.core_templates.retained_weight(),
            },
            stdlib_paths: RetainedProductSnapshot {
                reuse: self.stdlib_paths.snapshot(),
                retained_weight_bytes: self.stdlib_paths.retained_weight(),
            },
            gem_dependencies: RetainedProductSnapshot {
                reuse: self.gem_dependencies.snapshot(),
                retained_weight_bytes: self.gem_dependencies.retained_weight(),
            },
            classpath_files: RetainedProductSnapshot {
                reuse: self.classpath_files.snapshot(),
                retained_weight_bytes: self.classpath_files.retained_weight_bytes(),
            },
            java_artifacts: RetainedProductSnapshot {
                reuse: self.java_artifacts.snapshot(),
                retained_weight_bytes: self.java_artifacts.retained_weight_bytes(),
            },
            persistent_gems: self.persistent.gem_product_snapshot(),
            persistent_java: self.persistent.java_artifact_snapshot(),
            compiled_wasm: self.persistent.compiled_wasm_snapshot(),
            gem_bindings: self.gem_bindings.snapshot(),
        }
    }

    pub(super) fn new(root: PathBuf) -> Self {
        Self {
            discovered_runtimes: Arc::new(tokio::sync::OnceCell::new()),
            core_templates: new_core_engine_cache(),
            stdlib_paths: new_runtime_stdlib_path_cache(),
            gem_dependencies: new_gem_dependency_cache(),
            classpath_files: Default::default(),
            java_artifacts: Default::default(),
            persistent: crate::persistent_cache::PersistentDerivedProductCache::new(root),
            gem_bindings: Arc::default(),
        }
    }
    pub(crate) fn core_templates(
        &self,
    ) -> &crate::single_flight::BoundedSingleFlightCache<String, AnalysisEngine> {
        &self.core_templates
    }
    pub(crate) fn stdlib_paths(
        &self,
    ) -> &crate::single_flight::BoundedSingleFlightCache<
        crate::indexer::indexer_stdlib::RuntimeStdlibPathKey,
        crate::indexer::indexer_stdlib::RuntimeStdlibPaths,
    > {
        &self.stdlib_paths
    }
    pub(crate) fn gem_dependencies(
        &self,
    ) -> &crate::single_flight::BoundedSingleFlightCache<
        crate::dependency_product::GemDependencyProductKey,
        crate::dependency_product::GemDependencyProduct,
    > {
        &self.gem_dependencies
    }
    pub(crate) fn classpath_files(
        &self,
    ) -> &crate::runtime::jruby::classpath::ClasspathFileProductCache {
        &self.classpath_files
    }
    pub(crate) fn java_artifacts(
        &self,
    ) -> &crate::runtime::jruby::java_catalog::JavaArtifactProductCache {
        &self.java_artifacts
    }
    pub(crate) fn persistent(&self) -> &crate::persistent_cache::PersistentDerivedProductCache {
        &self.persistent
    }
    pub(crate) fn gem_bindings(
        &self,
    ) -> &Arc<crate::dependency_product::GemDependencyBindingCounters> {
        &self.gem_bindings
    }

    pub(crate) fn cache_root(&self) -> PathBuf {
        self.persistent.cache_root()
    }
}

impl RubyLanguageServer {
    /// Read process-wide counters and retained weights without exposing cache handles.
    /// Each component is observed independently, matching its own synchronization.
    pub fn runtime_product_snapshot(&self) -> RuntimeProductSnapshot {
        self.products.snapshot()
    }

    pub fn compiled_wasm_cache_snapshot(
        &self,
    ) -> crate::persistent_cache::PersistentProductSnapshot {
        self.products.persistent().compiled_wasm_snapshot()
    }

    /// Handle `ruby-fast-lsp/runtime/discover` without exposing editor policy
    /// or mutating any project runtime selection.
    pub async fn handle_runtime_discover(
        &self,
        _params: RuntimeDiscoverParams,
    ) -> LspResult<RuntimeCatalog> {
        Ok(crate::runtime::catalog::runtime_catalog_for_projects(
            self.workspace_root_paths(),
            self.discovered_runtimes().await,
        ))
    }

    async fn discovered_runtimes(&self) -> Vec<DiscoveredRuntime> {
        let indexing_resources = self.indexing.resources().clone();
        self.products
            .discovered_runtimes
            .get_or_init(|| crate::runtime::catalog::discover_runtimes(indexing_resources))
            .await
            .clone()
    }

    #[cfg(test)]
    pub(crate) fn set_discovered_runtimes_for_tests(&self, runtimes: Vec<DiscoveredRuntime>) {
        self.products.discovered_runtimes.set(runtimes).expect(
            "INVARIANT VIOLATED: test runtime catalog was initialized more than once. This is a bug because each test server must own one immutable discovery snapshot. Fix: create a fresh RubyLanguageServer per runtime test.",
        );
    }

    pub(crate) async fn resolve_auto_runtime(
        &self,
        project_root: &std::path::Path,
    ) -> Result<Option<SelectedRuntimeDescriptor>> {
        let Some(marker) = crate::runtime::catalog::project_runtime_marker(project_root)? else {
            return Ok(None);
        };
        let runtime = crate::runtime::catalog::select_runtime_for_marker(
            &marker,
            &self.discovered_runtimes().await,
        )?;
        let Some(runtime) = runtime else {
            warn!(
                "Project runtime marker `{marker}` has no exact installed runtime for {}",
                project_root.display()
            );
            return Ok(None);
        };
        if runtime.support_status != crate::runtime::catalog::RuntimeSupportStatus::Supported {
            return Err(anyhow::anyhow!(
                "project runtime marker `{marker}` selects unsupported {}",
                runtime.display_name
            ));
        }
        Ok(Some(runtime.into()))
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
                    crate::config::runtime::EffectiveRuntimeSelection::Explicit(runtime) => {
                        let stub_overlay = (runtime.implementation
                            == crate::runtime::catalog::RuntimeImplementation::Jruby)
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
                    crate::config::runtime::EffectiveRuntimeSelection::Auto => {
                        if let Some(runtime) = workspace.runtime.selected().read().clone() {
                            let stub_overlay = (runtime.implementation
                                == crate::runtime::catalog::RuntimeImplementation::Jruby)
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
                    crate::config::runtime::EffectiveRuntimeSelection::LegacyMriCompatibility {
                        major,
                        minor,
                    } => {
                        let compatibility = format!("{major}.{minor}");
                        (
                            "legacy".to_string(),
                            Some(crate::runtime::catalog::RuntimeImplementation::Mri),
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
