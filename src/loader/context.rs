//! Owner-supplied inputs the loader reads.
//!
//! The server builds one [`LoadContext`] per project load or interactive file
//! pass and passes it next to itself. Every field is a cheap shared handle:
//! configuration, published require roots, and open buffers are read live at
//! the moment the loader consults them, exactly as reads through the server
//! were. Writes (fact commits, document version marks, runtime selections,
//! publication) remain on the server.
use crate::environment::config::runtime::SelectedRuntimeDescriptor;
use crate::environment::config::RubyFastLspConfig;
use crate::environment::runtime::catalog::DiscoveredRuntime;
use crate::invariant::ExpectInvariant;
use crate::loader::cache::dependency_product::{
    GemBindingStat, GemDependencyProduct, GemDependencyProductKey,
};
use crate::loader::cache::persistent::PersistentDerivedProductCache;
use crate::loader::require_paths::RequireFeatureIndex;
use crate::loader::scheduling::resources::IndexingResourceGovernor;
use crate::loader::sources::stdlib::{RuntimeStdlibPathKey, RuntimeStdlibPaths};
use crate::utils::single_flight::BoundedSingleFlightCache;
use anyhow::Result;
use log::warn;
use parking_lot::{Mutex, RwLock, RwLockReadGuard};
use ruby_analysis::engine::AnalysisEngine;
use ruby_analysis::indexer::RubyDocument;
use ruby_analysis::stats::StatsRegistry;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tower_lsp::lsp_types::Url;

/// Everything the loader reads from its owner, as cloneable shared handles.
#[derive(Clone)]
pub struct LoadContext {
    pub config: LoadConfig,
    pub requires: RequireContext,
    pub products: SharedProducts,
    pub resources: IndexingResourceGovernor,
    pub discovery: RuntimeDiscovery,
    pub sources: Arc<dyn SourceReader>,
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

fn new_core_engine_cache() -> BoundedSingleFlightCache<String, AnalysisEngine> {
    BoundedSingleFlightCache::new(
        CORE_ENGINE_CACHE_MAX_ENTRIES,
        CORE_ENGINE_CACHE_MAX_WEIGHT_BYTES,
        |engine: &AnalysisEngine| {
            u64::try_from(engine.estimated_memory_stats().total()).expect_invariant(
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
    core_templates: BoundedSingleFlightCache<String, AnalysisEngine>,
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
    pub(crate) fn core_templates(&self) -> &BoundedSingleFlightCache<String, AnalysisEngine> {
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
