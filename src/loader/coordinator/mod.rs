use crate::environment::config::runtime::SelectedRuntimeDescriptor;
use crate::environment::config::RubyFastLspConfig;
use crate::environment::extensions::ExtensionRegistryHandle;
use crate::environment::runtime::catalog::RuntimeImplementation;
use crate::environment::runtime::jruby::classpath::ClasspathArtifact;
use crate::environment::runtime::jruby::imports::JrubyImportProvider;
use crate::invariant::ExpectInvariant;
use crate::loader::context::LoadContext;
use crate::loader::file_processor::FileProcessor;
use crate::loader::sources::gems::IndexerGem;
use crate::loader::sources::project::IndexerProject;
use crate::loader::sources::stdlib::IndexerStdlib;
use crate::loader::version::ruby_version::RubyVersion;
use crate::server::RubyLanguageServer;
use anyhow::{anyhow, Result};
use gems::configured_gem_selection;
use jruby::build_jruby_import_provider_off_reactor;
use log::info;
pub(crate) use priority::dependency_priority_key;
use priority::open_project_constant_priority_keys;
use resources::{release_allocator_free_pages, run_cpu_indexing_task, IndexingWorkClass};
use ruby_analysis::engine::AnalysisEngine;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;
use tower_lsp::lsp_types::Url;

mod diagnostics;
mod gems;
mod jruby;
mod priority;
mod project;
mod resources;
mod runtime;
mod standard_library;

/// Wall-clock timings captured by the coordinator during the most recent
/// [`IndexingCoordinator::run_complete_indexing`] call. Consumed by the
/// perf bench binary and perf regression tests.
#[derive(Debug, Clone, Copy, Default)]
pub struct IndexingTimings {
    /// Runtime selection, library discovery, and project processor setup.
    pub runtime: Duration,
    /// Project dependency/source discovery before semantic collection.
    pub discovery: Duration,
    /// Built-in runtime implementation and generated signature inputs.
    pub core: Duration,
    /// Project-owned source fact collection and first resolution.
    pub project: Duration,
    /// Locked gems, stdlib, and other external dependency inputs.
    pub dependencies: Duration,
    /// Final complete graph resolution after every required input.
    pub resolve: Duration,
    /// Fact collection (gems + stdlib + project) + mixin/reference resolution.
    pub facts: Duration,
    /// Reserved for old perf consumers. References now emit during fact collection.
    pub reserved: Duration,
    /// Publish diagnostics to the client.
    pub publish: Duration,
    pub total: Duration,
}

/// The IndexingCoordinator manages the entire indexing process.
///
/// It works in 5 simple steps:
/// 1. Find out which Ruby version we're using
/// 2. Set up the basic indexing tools
/// 3. Index the project files (and track what libraries they need)
/// 4. Index the Ruby standard library
/// 5. Index the gems (external libraries)
///
/// Think of it like organizing a library - first you figure out what system you're using,
/// then you organize your own books, then you add the reference books, and finally
/// you add books from other collections.
pub struct IndexingCoordinator {
    // Basic setup
    workspace_root: PathBuf,
    config: RubyFastLspConfig,

    extension_registry: Option<ExtensionRegistryHandle>,

    // Ruby version info
    detected_ruby_version: Option<RubyVersion>,
    effective_runtime: Option<SelectedRuntimeDescriptor>,
    jruby_import_provider: Option<Arc<JrubyImportProvider>>,
    jruby_runtime_archive: Option<ClasspathArtifact>,
    cache_root: Option<PathBuf>,

    // The main indexing engine
    file_processor: Option<FileProcessor>,
    dependency_seed_engine: Option<AnalysisEngine>,

    // Project-specific indexer
    project_indexer: Option<IndexerProject>,

    // Standard library indexer
    stdlib_indexer: Option<IndexerStdlib>,

    // Gem indexer
    gem_indexer: Option<IndexerGem>,

    /// Timings from the most recent `run_complete_indexing` call.
    last_timings: IndexingTimings,
    indexing_run: Option<crate::loader::scheduling::status::IndexingRun>,
    analysis_engine_override: Option<Arc<parking_lot::RwLock<AnalysisEngine>>>,
}

impl IndexingCoordinator {
    fn analysis_engine(
        &self,
        server: &RubyLanguageServer,
    ) -> Arc<parking_lot::RwLock<ruby_analysis::engine::AnalysisEngine>> {
        if let Some(engine) = &self.analysis_engine_override {
            return engine.clone();
        }
        let uri = Url::from_directory_path(&self.workspace_root).expect_invariant(
            "workspace root cannot be represented as a file URI",
            "indexing only accepts filesystem workspace roots",
            "register a canonical filesystem project root before creating the coordinator",
        );
        server.analysis_engine_for_uri(&uri)
    }
    /// Creates a new IndexingCoordinator for the given workspace.
    ///
    /// Call `run_complete_indexing()` to actually start the indexing process.
    /// Root of the project this coordinator loads.
    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    pub fn new(workspace_root: PathBuf, config: RubyFastLspConfig) -> Self {
        Self {
            workspace_root,
            config,
            extension_registry: None,
            detected_ruby_version: None,
            effective_runtime: None,
            jruby_import_provider: None,
            jruby_runtime_archive: None,
            cache_root: None,
            file_processor: None,
            dependency_seed_engine: None,
            project_indexer: None,
            stdlib_indexer: None,
            gem_indexer: None,
            last_timings: IndexingTimings::default(),
            indexing_run: None,
            analysis_engine_override: None,
        }
    }

    /// Returns the timings captured by the most recent call to
    /// `run_complete_indexing`. All-zero before the first call.
    pub fn last_timings(&self) -> IndexingTimings {
        self.last_timings
    }

    pub fn set_extension_registry(&mut self, extension_registry: ExtensionRegistryHandle) {
        self.extension_registry = Some(extension_registry);
    }

    pub fn set_indexing_run(&mut self, run: crate::loader::scheduling::status::IndexingRun) {
        self.indexing_run = Some(run);
    }

    fn resource_cancellation(&self) -> Option<CancellationToken> {
        self.indexing_run
            .as_ref()
            .map(crate::loader::scheduling::status::IndexingRun::cancellation)
    }

    pub fn set_analysis_engine(
        &mut self,
        analysis_engine: Arc<parking_lot::RwLock<AnalysisEngine>>,
    ) {
        self.analysis_engine_override = Some(analysis_engine);
    }

    pub(crate) fn set_cache_root(&mut self, root: PathBuf) {
        invariant!(
            root.is_absolute(),
            what = "coordinator cache root is relative",
            why = "runtime products require stable paths",
            fix = "supply the owning server's absolute cache root",
        );
        self.cache_root = Some(root);
    }

    /// Runs the complete indexing process from start to finish.
    ///
    /// 1. Figure out which Ruby version we're using
    /// 2. Find where Ruby libraries are installed on this system
    /// 3. Set up the main indexing engine
    /// 4. Scan project dependencies
    /// 5. Collect facts from gems, stdlib, then project files
    /// 6. Publish diagnostics
    pub async fn run_complete_indexing(
        &mut self,
        _ctx: &LoadContext,
        server: &RubyLanguageServer,
    ) -> Result<()> {
        info!("Starting complete indexing process");
        if self.cache_root.is_none() {
            self.set_cache_root(server.products.cache_root());
        }
        server
            .indexing
            .resources()
            .mark_project_navigation_pending_if_active(&self.workspace_root);
        let mut project_navigation_reservation = Some(
            server
                .indexing
                .resources()
                .project_navigation_reservation(self.workspace_root.clone()),
        );
        self.extension_registry
            .get_or_insert_with(|| server.extensions.registry().clone());
        let start_time = Instant::now();
        let runtime_start = Instant::now();
        self.indexing_checkpoint(server)?;

        self.resolve_effective_runtime(server).await?;
        self.transition_indexing_status(
            server,
            crate::loader::scheduling::status::IndexingPhase::DiscoveringInputs,
        )
        .await?;

        // Step 1: Figure out which Ruby version we're using
        let ruby_version = self.detect_ruby_version_off_reactor(server).await?;
        server.set_extension_project_ruby_version(
            &self.workspace_root,
            ruby_version.map(|version| version.to_string()),
        );
        info!("Detected Ruby version: {:?}", ruby_version);

        // Install a providerless processor for the active-file frontier.
        // Ordinary Ruby facts do not depend on the JVM catalog and are the
        // interactive critical path. The exact JRuby provider is built
        // concurrently with that frontier. Exhaustive project files and locked
        // gems start after the catalog exists so they can share the cooperative
        // partition; only frontier files collected before that need catalog-
        // sensitive replay before this project reports readiness.
        server.set_runtime_classpath_fingerprint(&self.workspace_root, None);
        server.set_jruby_import_provider(&self.workspace_root, None);
        self.jruby_import_provider = None;
        self.jruby_runtime_archive = None;
        self.setup_file_processor(server);
        let runtime_selection_dur = runtime_start.elapsed();

        // Project facts, exact JRuby runtime metadata, and immutable dependency
        // discovery overlap under one process resource budget. Semantic binding
        // remains isolated and waits until the exact owning-project inputs exist.
        info!("Collecting analysis facts");
        let facts_start = Instant::now();
        crate::environment::runtime::jruby::imports::reset_jruby_call_host_probe();

        self.transition_indexing_status(
            server,
            crate::loader::scheduling::status::IndexingPhase::IndexingCore,
        )
        .await?;

        let core_start = Instant::now();
        let dependency_seed_engine = Arc::new(parking_lot::RwLock::new(
            self.index_core_stubs(server, ruby_version).await?,
        ));
        let core_stub_dur = core_start.elapsed();
        let priority_server = server.clone();
        let priority_workspace_root = self.workspace_root.clone();
        let active_priority_keys = run_cpu_indexing_task(
            server,
            Some(self.workspace_root.clone()),
            self.resource_cancellation(),
            IndexingWorkClass::LightCpu,
            "active document dependency frontier",
            move || open_project_constant_priority_keys(&priority_server, &priority_workspace_root),
        )
        .await?;

        let runtime_workspace_root = self.workspace_root.clone();
        let runtime_config = self.config.clone();
        let runtime_selection = self.effective_runtime.clone();
        let runtime_cache_root = self.cache_root.clone();
        let runtime_cancellation = self.resource_cancellation();
        let is_jruby = runtime_selection
            .as_ref()
            .is_some_and(|runtime| runtime.implementation == RuntimeImplementation::Jruby);
        let runtime_provider = async move {
            if !is_jruby {
                return Ok::<_, anyhow::Error>(((None, None), Duration::default()));
            }
            let started = Instant::now();
            let result = build_jruby_import_provider_off_reactor(
                server,
                runtime_workspace_root,
                runtime_config,
                runtime_selection,
                runtime_cache_root,
                runtime_cancellation,
                IndexingWorkClass::RuntimeCompanionParallelIo,
            )
            .await;
            Ok((result?, started.elapsed()))
        };

        let gem_indexer = self.new_gem_indexer();
        let startup_gem_root = self.workspace_root.clone();
        let startup_gem_cancellation = self.resource_cancellation();
        let startup_gem_analysis_engine = self.analysis_engine(server);
        let startup_gem_priority_keys = active_priority_keys.dependency_roots.clone();
        let (_, startup_excluded_gems) =
            configured_gem_selection(Vec::new(), &self.config.indexing);
        let dependency_navigation_demands = self.indexing_run.as_ref().map(|run| {
            let workspace = server
                .list_workspaces()
                .into_iter()
                .find(|workspace| workspace.root_path == self.workspace_root)
                .expect_invariant(
                    "active indexing run has no registered workspace while preparing dependency navigation",
                    "the generation checkpoint already proved exact workspace ownership",
                    "keep workspace removal and coordinator cancellation atomic",
                );
            (workspace.navigation_demands, run.generation())
        });
        let startup_dependency_seed = {
            let dependency_seed = dependency_seed_engine.read();
            invariant!(
                dependency_seed.files().all(|source| matches!(
                    source.kind,
                    ruby_analysis::core::SourceKind::Stub
                        | ruby_analysis::core::SourceKind::Stdlib
                        | ruby_analysis::core::SourceKind::Signature
                        | ruby_analysis::core::SourceKind::External
                )),
                what =
                    "providerless startup dependency seed contains project, excluded, or gem facts",
                why = "dependency products must be reusable before project timing",
                fix = "fork the seed right after core stub indexing",
            );
            dependency_seed.clone()
        };
        let (project_frontier_release, project_frontier_wait) = tokio::sync::oneshot::channel();
        let startup_gem_indexing = Self::discover_and_bind_startup_priority_gems(
            server,
            startup_gem_root,
            startup_gem_cancellation,
            startup_gem_analysis_engine,
            gem_indexer,
            startup_dependency_seed,
            startup_gem_priority_keys,
            startup_excluded_gems,
            dependency_navigation_demands.clone(),
            project_frontier_release,
        );

        let project_priority_keys = active_priority_keys.clone();
        let project_indexing = async {
            // Project declarations are the interactive navigation critical
            // path. The active pass owns all but one CPU lane; that final lane
            // is reserved for its exact JRuby runtime companion. Exhaustive
            // remaining files wait until this join ends so they can overlap
            // locked gem construction on the cooperative partition.
            self.transition_indexing_status(
                server,
                crate::loader::scheduling::status::IndexingPhase::IndexingProject,
            )
            .await?;
            let project_start = Instant::now();
            self.collect_project_navigation_facts(server, project_priority_keys)
                .await?;
            project_frontier_wait.await.map_err(|_| {
                anyhow!(
                    "active dependency frontier for {} ended before releasing exhaustive project \
                     collection",
                    self.workspace_root.display()
                )
            })?;
            server
                .indexing
                .resources()
                .mark_project_navigation_complete_if_active(&self.workspace_root);
            Ok::<Duration, anyhow::Error>(project_start.elapsed())
        };
        // Poll the active dependency frontier before the other companion so
        // its exact locked source enters the governor queue first. The project
        // frontier releases its large parallel claim after the bounded target
        // files, allowing exhaustive gems and remaining project files to share
        // the cooperative partition instead of gems waiting behind the 50s tail.
        let (startup_gem_result, project_result, runtime_provider_result) =
            tokio::join!(startup_gem_indexing, project_indexing, runtime_provider);
        let ((provider, runtime_archive), runtime_provider_dur) = runtime_provider_result?;
        // Surface the frontier task's own failure first. A frontier error
        // drops the release sender, so the project pass reports only the
        // receiver-side message; the real cause lives in `startup_gem_result`.
        let (gem_indexer, discovery_dur) = startup_gem_result?;
        let mut project_dur = project_result?;

        self.jruby_import_provider = provider;
        self.jruby_runtime_archive = runtime_archive;
        server.set_jruby_import_provider(&self.workspace_root, self.jruby_import_provider.clone());
        server.set_runtime_classpath_fingerprint(
            &self.workspace_root,
            self.jruby_import_provider
                .as_ref()
                .map(|provider| provider.classpath_fingerprint().to_string()),
        );
        self.setup_file_processor(server);

        // JRuby ships the Ruby implementation of java_import/include_package
        // inside jruby.jar. Materialize only the bounded runtime source allowlist
        // so implementation navigation outranks compatibility declarations.
        let runtime_sources_start = Instant::now();
        self.index_jruby_runtime_sources_off_reactor(server, dependency_seed_engine.clone())
            .await?;
        let runtime_sources_dur = runtime_sources_start.elapsed();
        self.dependency_seed_engine = Some({
            let dependency_seed = dependency_seed_engine.read();
            invariant!(
                dependency_seed.files().all(|source| matches!(
                    source.kind,
                    ruby_analysis::core::SourceKind::Stub
                        | ruby_analysis::core::SourceKind::Stdlib
                        | ruby_analysis::core::SourceKind::Signature
                        | ruby_analysis::core::SourceKind::External
                )),
                what = "immutable dependency seed contains project, excluded, or gem facts",
                why = "editor timing or one dependency could contaminate every reusable gem product identity",
                fix = "build the seed only from clean core and runtime inputs",
            );
            dependency_seed.clone()
        });
        let core_dur = core_stub_dur + runtime_sources_dur;
        let runtime_dur = runtime_selection_dur + runtime_provider_dur;

        // The exact dependency seed is now complete. Stream locked gems on the
        // cooperative companion partition while exhaustive project files use the
        // other partition. Waiting for the tail before gem work left gems and
        // JARs queued behind a 50s visitor with idle lanes.
        let dependencies_start = Instant::now();
        if let Some(provider) = self.jruby_import_provider.clone() {
            if let Some(project_indexer) = self.project_indexer.as_mut() {
                project_indexer.install_jruby_import_provider(provider);
            }
        }
        let gem_indexer = self.configure_discovered_gem_indexer(gem_indexer);
        let gem_workspace_root = self.workspace_root.clone();
        let gem_cancellation = self.resource_cancellation();
        let gem_analysis_engine = self.analysis_engine(server);
        let gem_priority_keys = self
            .project_indexer
            .as_ref()
            .map(IndexerProject::dependency_navigation_priority_keys)
            .unwrap_or_default();
        let gem_indexing = Self::index_configured_gems(
            server,
            gem_workspace_root,
            gem_cancellation,
            gem_analysis_engine,
            gem_indexer,
            gem_priority_keys,
            dependency_navigation_demands,
        );
        let remaining_project = async {
            let remaining_start = Instant::now();
            self.collect_remaining_project_facts(server, None).await?;
            Ok::<Duration, anyhow::Error>(remaining_start.elapsed())
        };
        let (remaining_project_result, gem_indexing_result) =
            tokio::join!(remaining_project, gem_indexing);
        project_dur += remaining_project_result?;
        self.gem_indexer = Some(gem_indexing_result?);

        let provider_present = self.jruby_import_provider.is_some();
        let replay_dur = if provider_present {
            let replay_start = Instant::now();
            let replayed = self
                .replay_jruby_catalog_sensitive_project_facts(server)
                .await?;
            info!(
                "Replaced {} JRuby catalog-sensitive project file(s) after exact provider setup",
                replayed
            );
            replay_start.elapsed()
        } else {
            self.discard_jruby_replay_semantic_context(server).await?;
            Duration::default()
        };
        project_dur += replay_dur;
        drop(project_navigation_reservation.take());
        crate::environment::runtime::jruby::imports::log_jruby_call_host_probe(
            "project_navigation_ready",
            &self.workspace_root,
        );
        self.transition_indexing_status(
            server,
            crate::loader::scheduling::status::IndexingPhase::ProjectNavigationReady,
        )
        .await?;
        self.transition_indexing_status(
            server,
            crate::loader::scheduling::status::IndexingPhase::IndexingDependencies,
        )
        .await?;
        crate::environment::runtime::jruby::imports::log_jruby_call_host_probe(
            "after_dependencies",
            &self.workspace_root,
        );

        // Runtime stdlib still enters the same isolated engine before the
        // dependency-ready milestone and complete semantic diagnostics.
        self.index_standard_library(server, &ruby_version).await?;
        self.publish_dependency_require_paths(server)?;
        if let Some(workspace) = server
            .list_workspaces()
            .into_iter()
            .find(|workspace| workspace.root_path == self.workspace_root)
        {
            server
                .refresh_unresolved_require_diagnostics_for_workspace(&workspace)
                .await;
        }
        self.indexing_checkpoint(server)?;
        let dependencies_dur = dependencies_start.elapsed();

        let facts_dur = facts_start.elapsed();
        let reserved_dur = Duration::default();
        info!("Facts collection completed in {:?}", facts_dur);

        // Publish diagnostics to the client.
        info!("Publishing diagnostics");
        self.transition_indexing_status(
            server,
            crate::loader::scheduling::status::IndexingPhase::ResolvingSemantics,
        )
        .await?;
        let resolve_start = Instant::now();
        let analysis_engine = self.analysis_engine(server);
        let resolve_project = self.workspace_root.clone();
        run_cpu_indexing_task(
            server,
            Some(self.workspace_root.clone()),
            self.resource_cancellation(),
            IndexingWorkClass::HeavyCpu,
            "final semantic resolution",
            move || {
                // Project, dependency, and JRuby collection leave large freed
                // buffers in the process allocator. Return those pages before
                // the final resolve pass materializes reference and diagnostic
                // indexes, otherwise the retired collection pages and the live
                // resolved stores overlap in peak RSS.
                let malloc_started = Instant::now();
                release_allocator_free_pages();
                let malloc_elapsed = malloc_started.elapsed();
                let resolve_started = Instant::now();
                analysis_engine.write().resolve();
                info!(
                    "[PERF][final semantic resolution] project={} malloc_relief={:?} resolve={:?}",
                    resolve_project.display(),
                    malloc_elapsed,
                    resolve_started.elapsed()
                );
            },
        )
        .await?;
        let resolve_dur = resolve_start.elapsed();
        if let Some(run) = &self.indexing_run {
            if let Some(workspace) = server
                .list_workspaces()
                .into_iter()
                .find(|workspace| workspace.root_path == self.workspace_root)
            {
                workspace.navigation_demands.complete_stage(
                    run.generation(),
                    crate::loader::scheduling::navigation_demand::NavigationDemandStage::Dependency,
                );
            }
        }
        self.transition_indexing_status(
            server,
            crate::loader::scheduling::status::IndexingPhase::DependencyNavigationReady,
        )
        .await?;
        self.transition_indexing_status(
            server,
            crate::loader::scheduling::status::IndexingPhase::PublishingDiagnostics,
        )
        .await?;
        let publish_start = Instant::now();
        self.publish_open_project_diagnostics(server).await?;
        let publish_dur = publish_start.elapsed();

        // Open consumers may have been analyzed before a closed definition
        // file supplied the value-constant equation that final resolution just
        // solved. Their stored engine facts are now authoritative, but VS Code
        // will not ask for inlay hints again without an explicit invalidation.
        // Check the generation immediately before publishing that invalidation
        // so a superseded cold-index run cannot refresh the client with stale
        // semantic state.
        self.indexing_checkpoint(server)?;
        server
            .refresh_inlay_hints_for_workspace(&self.workspace_root)
            .await;
        self.indexing_checkpoint(server)?;

        let total_dur = start_time.elapsed();
        info!("Complete indexing finished in {:?}", total_dur);
        let analysis_engine = self.analysis_engine(server);
        run_cpu_indexing_task(
            server,
            Some(self.workspace_root.clone()),
            self.resource_cancellation(),
            IndexingWorkClass::HeavyCpu,
            "analysis engine compaction",
            move || {
                analysis_engine.write().shrink_to_fit();
                release_allocator_free_pages();
            },
        )
        .await?;
        self.log_analysis_memory_stats(server);

        self.last_timings = IndexingTimings {
            runtime: runtime_dur,
            discovery: discovery_dur,
            core: core_dur,
            project: project_dur,
            dependencies: dependencies_dur,
            resolve: resolve_dur,
            facts: facts_dur,
            reserved: reserved_dur,
            publish: publish_dur,
            total: total_dur,
        };
        Ok(())
    }

    async fn transition_indexing_status(
        &self,
        server: &RubyLanguageServer,
        phase: crate::loader::scheduling::status::IndexingPhase,
    ) -> Result<()> {
        let Some(run) = &self.indexing_run else {
            return Ok(());
        };
        self.indexing_checkpoint(server)?;
        let workspace = server
            .list_workspaces()
            .into_iter()
            .find(|workspace| workspace.root_path == self.workspace_root);
        let workspace = workspace.ok_or_else(|| {
            anyhow!(
                "Indexing generation {} was cancelled because project {} is no longer registered",
                run.generation(),
                self.workspace_root.display()
            )
        })?;
        if workspace
            .indexing_status
            .transition(run.generation(), phase, None, None)
            .is_some()
        {
            server.publish_indexing_status().await;
            Ok(())
        } else {
            Err(anyhow!(
                "Indexing generation {} was superseded for project {}",
                run.generation(),
                self.workspace_root.display()
            ))
        }
    }

    fn indexing_checkpoint(&self, server: &RubyLanguageServer) -> Result<()> {
        let Some(run) = &self.indexing_run else {
            return Ok(());
        };
        if run.is_cancelled() {
            return Err(anyhow!(
                "Indexing generation {} was cancelled for project {}",
                run.generation(),
                self.workspace_root.display()
            ));
        }
        let workspace = server
            .list_workspaces()
            .into_iter()
            .find(|workspace| workspace.root_path == self.workspace_root)
            .ok_or_else(|| {
                anyhow!(
                    "Indexing generation {} was cancelled because project {} is no longer registered",
                    run.generation(),
                    self.workspace_root.display()
                )
            })?;
        if !workspace.indexing_status.is_current_run(run) {
            return Err(anyhow!(
                "Indexing generation {} was superseded for project {}",
                run.generation(),
                self.workspace_root.display()
            ));
        }
        Ok(())
    }

    /// Step 3: Set up the main indexing engine
    fn setup_file_processor(&mut self, server: &RubyLanguageServer) {
        let extension_registry = self
            .extension_registry
            .get_or_insert_with(|| server.extensions.registry().clone())
            .clone();
        let processor = FileProcessor::with_extension_registry(extension_registry);
        let processor = self
            .jruby_import_provider
            .as_ref()
            .map(|provider| {
                processor
                    .clone()
                    .with_jruby_import_provider(provider.clone())
            })
            .unwrap_or(processor);
        let require_load_paths = server
            .config
            .lock()
            .indexing
            .load_paths
            .paths_for_project(&self.workspace_root)
            .to_vec();
        let require_dependency_roots = server
            .list_workspaces()
            .into_iter()
            .find(|workspace| workspace.root_path == self.workspace_root)
            .map(|workspace| workspace.dependency_require_paths())
            .unwrap_or_default();
        let processor = processor.with_require_resolve_context(
            self.workspace_root.clone(),
            require_load_paths,
            require_dependency_roots,
        );
        self.file_processor = Some(
            server
                .extension_project_context_seed_for_root(&self.workspace_root)
                .map(|seed| processor.clone().with_extension_project_context_seed(seed))
                .unwrap_or(processor),
        );
    }
}

/// Integration tests for IndexingCoordinator
/// Tests the complete indexing workflow with realistic project structures
#[cfg(test)]
mod tests;
