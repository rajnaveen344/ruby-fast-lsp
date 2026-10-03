//! Owner-supplied inputs the loader reads, and the owner operations it performs.
//!
//! The server builds one [`LoadContext`] per project load or interactive file
//! pass. Every field is a cheap shared handle: configuration, published
//! require roots, and open buffers are read live at the moment the loader
//! consults them, exactly as reads through the server were. Writes and owner
//! lookups (source registration, document version marks, runtime selections,
//! status, progress, and the facts-ready publication hook) go through [`LoadSink`].
//! Engine writes go through the named operations of a [`LoadTarget`]
//! (`target.rs`): the sink hands out each project's target, never its lock.
use crate::environment::config::runtime::SelectedRuntimeDescriptor;
use crate::environment::config::RubyFastLspConfig;
use crate::environment::extensions::{
    ExtensionRegistryHandle, ProjectContextSeed, ProjectContextSnapshot,
};
use crate::environment::runtime::catalog::DiscoveredRuntime;
use crate::invariant::ExpectInvariant;
use crate::loader::cache::dependency_product::{
    GemBindingStat, GemDependencyProduct, GemDependencyProductKey,
};
use crate::loader::jruby_add_on::JrubyAddOn;
use crate::loader::require_paths::RequireFeatureIndex;
use crate::loader::scheduling::navigation_demand::NavigationDemandController;
use crate::loader::scheduling::status::{IndexingPhase, IndexingRun};
use crate::loader::sources::stdlib::{RuntimeStdlibPathKey, RuntimeStdlibPaths};
use crate::utils::admission::IndexingResourceGovernor;
use crate::utils::persistent_cache::PersistentDerivedProductCache;
use crate::utils::single_flight::BoundedSingleFlightCache;
use anyhow::Result;
use log::warn;
use parking_lot::{Mutex, RwLock, RwLockReadGuard};
use ruby_analysis::core::{SourceFileId, SourceKind};
use ruby_analysis::engine::Project;
use ruby_analysis::indexer::RubyDocument;
use ruby_analysis::stats::StatsRegistry;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tower_lsp::lsp_types::Url;

mod target;
pub(crate) use target::{FileResolution, PathFacts, ProjectSourceCandidate};
pub use target::{LoadTarget, NamedWrite};

/// Everything the loader reads from its owner, as cloneable shared handles.
#[derive(Clone)]
pub struct LoadContext {
    pub config: LoadConfig,
    pub requires: RequireContext,
    pub products: SharedProducts,
    pub resources: IndexingResourceGovernor,
    pub discovery: RuntimeDiscovery,
    pub sources: Arc<dyn SourceReader>,
    pub(crate) sink: Arc<dyn LoadSink>,
}

/// Live view of the editor configuration. Each read observes the current
/// value; nothing is captured when the context is built.
#[derive(Clone)]
pub struct LoadConfig {
    config: Arc<Mutex<RubyFastLspConfig>>,
}
impl LoadConfig {
    pub fn new(config: Arc<Mutex<RubyFastLspConfig>>) -> Self {
        Self { config }
    }

    /// Configured load paths for one project, read now.
    pub fn load_paths_for_project(&self, project_root: &Path) -> Vec<String> {
        self.config
            .lock()
            .indexing
            .load_paths
            .paths_for_project(project_root)
            .to_vec()
    }
}

/// Require resolution inputs of the project that owns this load.
#[derive(Clone)]
pub struct RequireContext {
    /// Root of the owning project; `None` when the file has no project.
    pub project_root: Option<PathBuf>,
    published: PublishedRequires,
}
impl RequireContext {
    pub fn new(project_root: Option<PathBuf>, published: PublishedRequires) -> Self {
        Self {
            project_root,
            published,
        }
    }

    /// Absolute gem/stdlib require roots currently published for the project.
    pub fn dependency_require_paths(&self) -> Vec<PathBuf> {
        self.published.paths()
    }

    /// Feature index currently published for the project.
    pub fn feature_index(&self) -> Arc<RequireFeatureIndex> {
        self.published.feature_index()
    }
}

/// Require roots and feature index a project publishes after dependency
/// indexing. The project owns the handle; readers observe replacements live.
#[derive(Clone)]
pub struct PublishedRequires {
    paths: Arc<RwLock<Vec<PathBuf>>>,
    index: Arc<RwLock<Arc<RequireFeatureIndex>>>,
}
impl Default for PublishedRequires {
    fn default() -> Self {
        Self {
            paths: Arc::default(),
            index: Arc::new(RwLock::new(Arc::new(RequireFeatureIndex::empty()))),
        }
    }
}
impl PublishedRequires {
    pub fn replace(&self, paths: Vec<PathBuf>, index: Arc<RequireFeatureIndex>) {
        *self.paths.write() = paths;
        *self.index.write() = index;
    }

    pub fn paths(&self) -> Vec<PathBuf> {
        self.paths.read().clone()
    }

    pub fn feature_index(&self) -> Arc<RequireFeatureIndex> {
        self.index.read().clone()
    }

    /// Hold this identity guard through delayed require-fact commit/publication.
    pub fn feature_index_guard(&self) -> RwLockReadGuard<'_, Arc<RequireFeatureIndex>> {
        self.index.read()
    }
}

const MIB: u64 = 1024 * 1024;
pub(crate) const CORE_ENGINE_CACHE_MAX_ENTRIES: usize = 8;
pub(crate) const CORE_ENGINE_CACHE_MAX_WEIGHT_BYTES: u64 = 128 * MIB;
const RUNTIME_STDLIB_PATH_CACHE_MAX_ENTRIES: usize = 32;
const RUNTIME_STDLIB_PATH_CACHE_MAX_WEIGHT_BYTES: u64 = MIB;

fn new_core_engine_cache() -> BoundedSingleFlightCache<String, Project> {
    BoundedSingleFlightCache::new(
        CORE_ENGINE_CACHE_MAX_ENTRIES,
        CORE_ENGINE_CACHE_MAX_WEIGHT_BYTES,
        |engine: &Project| {
            u64::try_from(engine.view().estimated_memory_stats().total()).expect_invariant(
                "a core template heap estimate does not fit u64",
                "one in-memory engine cannot exceed the process address space",
                "inspect engine memory estimation overflow",
            )
        },
    )
}

/// Process-wide immutable products shared by every project load. Clones
/// share the same bounded caches; results are bound into isolated engines.
#[derive(Clone)]
pub struct SharedProducts {
    core_templates: BoundedSingleFlightCache<String, Project>,
    stdlib_paths: BoundedSingleFlightCache<RuntimeStdlibPathKey, RuntimeStdlibPaths>,
    gem_dependencies: BoundedSingleFlightCache<GemDependencyProductKey, GemDependencyProduct>,
    classpath_files: crate::environment::runtime::jruby::classpath::ClasspathFileProductCache,
    java_artifacts: crate::environment::runtime::jruby::java_catalog::JavaArtifactProductCache,
    persistent: PersistentDerivedProductCache,
    gem_bindings: Arc<StatsRegistry<GemBindingStat>>,
}
impl SharedProducts {
    pub fn new(cache_root: PathBuf) -> Self {
        Self {
            core_templates: new_core_engine_cache(),
            stdlib_paths: BoundedSingleFlightCache::new(
                RUNTIME_STDLIB_PATH_CACHE_MAX_ENTRIES,
                RUNTIME_STDLIB_PATH_CACHE_MAX_WEIGHT_BYTES,
                RuntimeStdlibPaths::estimated_weight_bytes,
            ),
            gem_dependencies: BoundedSingleFlightCache::ephemeral(
                |product: &GemDependencyProduct| product.estimated_weight_bytes(),
            ),
            classpath_files: Default::default(),
            java_artifacts: Default::default(),
            persistent: PersistentDerivedProductCache::new(cache_root),
            gem_bindings: Arc::default(),
        }
    }
    pub(crate) fn core_templates(&self) -> &BoundedSingleFlightCache<String, Project> {
        &self.core_templates
    }
    pub(crate) fn stdlib_paths(
        &self,
    ) -> &BoundedSingleFlightCache<RuntimeStdlibPathKey, RuntimeStdlibPaths> {
        &self.stdlib_paths
    }
    pub(crate) fn gem_dependencies(
        &self,
    ) -> &BoundedSingleFlightCache<GemDependencyProductKey, GemDependencyProduct> {
        &self.gem_dependencies
    }
    pub(crate) fn classpath_files(
        &self,
    ) -> &crate::environment::runtime::jruby::classpath::ClasspathFileProductCache {
        &self.classpath_files
    }
    pub(crate) fn java_artifacts(
        &self,
    ) -> &crate::environment::runtime::jruby::java_catalog::JavaArtifactProductCache {
        &self.java_artifacts
    }
    pub(crate) fn persistent(&self) -> &PersistentDerivedProductCache {
        &self.persistent
    }
    pub(crate) fn gem_bindings(&self) -> &StatsRegistry<GemBindingStat> {
        &self.gem_bindings
    }
    pub(crate) fn cache_root(&self) -> PathBuf {
        self.persistent.cache_root()
    }
}

/// Process-wide installed-runtime discovery. Discovery runs once, under the
/// indexing resource governor, and every project selects from that snapshot.
#[derive(Clone)]
pub struct RuntimeDiscovery {
    discovered: Arc<tokio::sync::OnceCell<Vec<DiscoveredRuntime>>>,
    resources: IndexingResourceGovernor,
}
impl RuntimeDiscovery {
    pub fn new(
        discovered: Arc<tokio::sync::OnceCell<Vec<DiscoveredRuntime>>>,
        resources: IndexingResourceGovernor,
    ) -> Self {
        Self {
            discovered,
            resources,
        }
    }

    pub async fn discovered_runtimes(&self) -> Vec<DiscoveredRuntime> {
        let resources = self.resources.clone();
        self.discovered
            .get_or_init(|| crate::environment::runtime::catalog::discover_runtimes(resources))
            .await
            .clone()
    }

    /// Select the exact installed runtime named by the project's marker.
    pub async fn resolve_auto_runtime(
        &self,
        project_root: &Path,
    ) -> Result<Option<SelectedRuntimeDescriptor>> {
        let Some(marker) =
            crate::environment::runtime::catalog::project_runtime_marker(project_root)?
        else {
            return Ok(None);
        };
        let runtime = crate::environment::runtime::catalog::select_runtime_for_marker(
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
        if runtime.support_status
            != crate::environment::runtime::catalog::RuntimeSupportStatus::Supported
        {
            return Err(anyhow::anyhow!(
                "project runtime marker `{marker}` selects unsupported {}",
                runtime.display_name
            ));
        }
        Ok(Some(runtime.into()))
    }
}

/// Version state of one open buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpenDocumentVersion {
    pub version: i32,
    pub indexed_version: Option<i32>,
}

/// Live read access to open editor buffers. Open buffers are authoritative
/// over disk; every call observes the buffers at that moment.
pub trait SourceReader: Send + Sync {
    /// URIs of all open buffers, in no particular order.
    fn open_uris(&self) -> Vec<Url>;
    /// A copy of one open buffer.
    fn open_document(&self, uri: &Url) -> Option<RubyDocument>;
    /// Version state of one open buffer.
    fn open_document_version(&self, uri: &Url) -> Option<OpenDocumentVersion>;
    /// Visit every open buffer in URI order without copying it.
    fn visit_open_documents(&self, visit: &mut dyn FnMut(&RubyDocument));
}

/// Whether one indexing generation still owns its project.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum IndexingRunState {
    /// The generation is the project's live run.
    Current,
    /// A newer generation, a terminal phase, or cancellation replaced it.
    Superseded,
    /// The project root is no longer registered.
    Unregistered,
}

/// Owner operations the loader performs while it loads a project or file.
///
/// Every method is one domain operation on the owner's live state; the sink
/// never exposes a mutable store. The loader calls them in its own order, so
/// the owner observes exactly the write sequence of the load.
#[tower_lsp::async_trait]
pub(crate) trait LoadSink: Send + Sync {
    /// The load target of the isolated project engine that owns `uri`; files
    /// outside every project use the orphan engine.
    fn target_for_uri(&self, uri: &Url) -> Arc<dyn LoadTarget>;
    /// Whether `run` is still the live generation of the project at `root`.
    fn indexing_run_state(&self, root: &Path, run: &IndexingRun) -> IndexingRunState;
    /// Advance the status of the project at `root` to `phase` and publish it.
    /// Nothing changes unless `generation` is still current.
    async fn transition_indexing_phase(
        &self,
        root: &Path,
        generation: u64,
        phase: IndexingPhase,
    ) -> IndexingRunState;
    /// The extension registry new file processors run with.
    fn extension_registry(&self) -> ExtensionRegistryHandle;
    /// The extension context seed of the project at `root`.
    fn extension_context_seed(&self, root: &Path) -> Option<Arc<RwLock<ProjectContextSeed>>>;
    /// Record the runtime selected for the project at `root`.
    fn select_runtime(&self, root: &Path, runtime: Option<SelectedRuntimeDescriptor>);
    /// Record the Ruby version detected for the project at `root`.
    fn set_ruby_version(&self, root: &Path, ruby_version: Option<String>);
    /// Ask the client to refresh inlay hints when the project at `root` owns
    /// an open document.
    async fn refresh_inlay_hints(&self, root: &Path);
    /// Publish the dependency require roots and feature index of the project
    /// at `root`, only while `target` is still that project's engine.
    fn publish_require_roots(
        &self,
        root: &Path,
        target: &Arc<dyn LoadTarget>,
        paths: Vec<PathBuf>,
        index: Arc<RequireFeatureIndex>,
    );
    /// Re-resolve unresolved-require diagnostics of the project at `root`
    /// against its published require roots.
    async fn refresh_require_diagnostics(&self, root: &Path);
    /// The navigation demand queue of the project at `root`.
    fn navigation_demands(&self, root: &Path) -> Option<NavigationDemandController>;
    /// The root of the deepest project that owns `uri`.
    fn project_root_for_uri(&self, uri: &Url) -> Option<PathBuf>;
    /// Register or replace the source text of `uri` in its owning engine.
    fn register_source(&self, uri: &Url, content: String, kind: SourceKind) -> SourceFileId;
    /// The extension project context for `uri` read as `kind`.
    fn extension_context_snapshot(
        &self,
        uri: &Url,
        kind: SourceKind,
    ) -> Option<ProjectContextSnapshot>;
    /// Retain `document` as the processed document of `uri` and mark its
    /// current version indexed.
    fn mark_document_indexed(&self, uri: &Url, document: RubyDocument);
    /// Report project file progress for the indexing run `generation`.
    fn report_project_progress(
        &self,
        root: &Path,
        generation: Option<u64>,
        completed: u64,
        total: u64,
    );
    /// Final resolution of the project at `root` completed in `target`:
    /// publish a complete diagnostic projection for each open document that
    /// target owns. Publication stops as soon as `run` is no longer current
    /// and returns that state; a load without a run publishes every document.
    async fn project_facts_ready(
        &self,
        root: &Path,
        target: &Arc<dyn LoadTarget>,
        run: Option<&IndexingRun>,
    ) -> IndexingRunState;
    /// Replace the JRuby add-on of the project at `root` in one write. A run
    /// withdraws it (`None`) before building a new one and then installs the
    /// result, which is `None` when the project's runtime is not JRuby.
    fn set_jruby_add_on(&self, root: &Path, add_on: Option<JrubyAddOn>);
    /// Deterministic interleaving points for tests.
    #[cfg(test)]
    fn test_schedule(&self) -> Arc<crate::loader::scheduling::test_schedule::TestSchedule>;
}
