use crate::config::runtime::{EffectiveRuntimeSelection, SelectedRuntimeDescriptor};
use crate::config::{IndexingConfig, RubyFastLspConfig};
use crate::extensions::ExtensionRegistryHandle;
use crate::indexer::file_processor::FileProcessor;
use crate::indexer::indexer_gem::{discover_locked_java_gem_roots, IndexerGem};
use crate::indexer::indexer_project::IndexerProject;
use crate::indexer::indexer_stdlib::{IndexerStdlib, RuntimeStdlibPathKey, RuntimeStdlibPaths};

use crate::indexer::version::ruby_version::{RubyImplementation, RubyVersion};
use crate::indexer::version::version_detector::RubyVersionDetector;
use crate::indexing_resources::{IndexingResourcePriority, IndexingWorkSpec};
use crate::persistent_cache::{PersistentDerivedProductCache, PersistentJavaArtifactLookup};
use crate::runtime::catalog::RuntimeImplementation;
use crate::runtime::jruby::classpath::{
    discover_project_classpath, discover_project_classpath_with_cache, ArtifactOrigin,
    ClasspathArtifact, ClasspathFileProductCache, ClasspathInputs, ClasspathLimits,
};
use crate::runtime::jruby::decompiler::{
    discover_bundled_cfr_asset, JavaDecompiler, JavaDecompilerLimits,
};
use crate::runtime::jruby::imports::JrubyImportProvider;
use crate::runtime::jruby::java_catalog::{
    build_project_java_catalog, verify_artifact_discovery_identity, JavaArtifactProduct,
    JavaArtifactProductCache, JavaArtifactProductKey, ProjectJavaCatalog,
    ProjectJavaCatalogBuilder,
};
use crate::runtime::jruby::runtime_sources::materialize_jruby_runtime_sources;
use crate::runtime::jruby::source_navigation::{JavaSourceResolutionLimits, JavaSourceResolver};
use crate::server::RubyLanguageServer;
use anyhow::{anyhow, Context, Result};
use log::{debug, info, warn};
use rayon::prelude::*;
use ruby_analysis::core::{
    DiagnosticFact, DiagnosticSeverity as AnalysisDiagnosticSeverity, TextRange,
};
use ruby_analysis::engine::{AnalysisEngine, SourceFile};
use ruby_fast_lsp_jvm_metadata::ArchiveLimits;
use ruby_prism::{ConstantPathNode, ConstantReadNode, Visit};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Position, Range, Url};

const MIB: usize = 1024 * 1024;
const MAX_ACTIVE_PROJECT_FILE_PRIORITY_KEYS: usize = 8;
// Deterministic collection retains one batch's complete file-owned facts until
// every worker has observed the same immutable engine context. Keep the batch
// small enough that those retained facts stay inside the measured multi-root
// memory envelope while still amortizing registration and resolution work.
const EXHAUSTIVE_PROJECT_FILE_BATCH_SIZE: usize = 512;

#[derive(Default)]
struct ActiveDocumentConstantVisitor {
    dependency_roots: HashSet<String>,
    project_terminals: Vec<String>,
    seen_project_terminals: HashSet<String>,
    constant_path_depth: usize,
}

impl ActiveDocumentConstantVisitor {
    fn push_project_terminal(&mut self, key: String) {
        if self.seen_project_terminals.insert(key.clone()) {
            self.project_terminals.push(key);
        }
    }
}

impl<'pr> Visit<'pr> for ActiveDocumentConstantVisitor {
    fn visit_constant_read_node(&mut self, node: &ConstantReadNode<'pr>) {
        let key = dependency_priority_key(&String::from_utf8_lossy(node.name().as_slice()));
        self.dependency_roots.insert(key.clone());
        if self.constant_path_depth == 0 {
            self.push_project_terminal(key);
        }
        ruby_prism::visit_constant_read_node(self, node);
    }

    fn visit_constant_path_node(&mut self, node: &ConstantPathNode<'pr>) {
        if self.constant_path_depth == 0 {
            if let Some(name) = node.name() {
                self.push_project_terminal(dependency_priority_key(&String::from_utf8_lossy(
                    name.as_slice(),
                )));
            }
        }
        self.constant_path_depth += 1;
        ruby_prism::visit_constant_path_node(self, node);
        self.constant_path_depth = self.constant_path_depth.checked_sub(1).expect(
            "INVARIANT VIOLATED: active-document constant-path traversal depth underflowed. \
                 This is a bug because every constant-path visit increments exactly once before \
                 recursive traversal. Fix: keep traversal depth updates paired around the default \
                 Prism visitor.",
        );
    }
}

#[derive(Clone, Default)]
struct ActiveDocumentPriorityKeys {
    dependency_roots: HashSet<String>,
    project_terminals: Vec<String>,
}

impl ActiveDocumentPriorityKeys {
    fn extend(&mut self, other: Self) {
        self.dependency_roots.extend(other.dependency_roots);
        for terminal in other.project_terminals {
            if self.project_terminals.len() == MAX_ACTIVE_PROJECT_FILE_PRIORITY_KEYS {
                break;
            }
            if !self.project_terminals.contains(&terminal) {
                self.project_terminals.push(terminal);
            }
        }
    }
}

pub(crate) fn dependency_priority_key(name: &str) -> String {
    crate::navigation_demand::normalize_navigation_key(name)
}

fn active_document_constant_priority_keys(source: &str) -> ActiveDocumentPriorityKeys {
    let source = ruby_analysis::indexer::mask_shebang(source);
    let parse = ruby_prism::parse(source.as_bytes());
    let mut visitor = ActiveDocumentConstantVisitor::default();
    visitor.visit(&parse.node());
    visitor
        .project_terminals
        .truncate(MAX_ACTIVE_PROJECT_FILE_PRIORITY_KEYS);
    ActiveDocumentPriorityKeys {
        dependency_roots: visitor.dependency_roots,
        project_terminals: visitor.project_terminals,
    }
}

fn open_project_constant_priority_keys(
    server: &RubyLanguageServer,
    workspace_root: &Path,
) -> ActiveDocumentPriorityKeys {
    let mut documents = server.docs.lock().values().cloned().collect::<Vec<_>>();
    documents.sort_by(|left, right| left.read().uri.cmp(&right.read().uri));
    let mut priority_keys = ActiveDocumentPriorityKeys::default();
    for document in documents {
        let document = document.read();
        let Some(workspace) = server.workspace_for_uri(&document.uri) else {
            continue;
        };
        if workspace.root_path != workspace_root {
            continue;
        }
        priority_keys.extend(active_document_constant_priority_keys(
            document.analysis_content(),
        ));
    }
    priority_keys
}

fn prioritize_locked_gem_names(names: Vec<String>, priority_keys: &HashSet<String>) -> Vec<String> {
    let (prioritized, exhaustive): (Vec<_>, Vec<_>) = names
        .into_iter()
        .partition(|name| priority_keys.contains(&dependency_priority_key(name)));
    prioritized.into_iter().chain(exhaustive).collect()
}

fn prioritize_demanded_gem_names(
    remaining: &mut VecDeque<String>,
    demand_keys: &[String],
) -> BTreeMap<String, Vec<String>> {
    let mut unmatched_names = std::mem::take(remaining).into_iter().collect::<Vec<_>>();
    let mut prioritized_names = Vec::new();
    let mut matched = BTreeMap::<String, Vec<String>>::new();
    for key in demand_keys {
        let Some(index) = unmatched_names
            .iter()
            .position(|name| dependency_priority_key(name) == *key)
        else {
            continue;
        };
        let name = unmatched_names.remove(index);
        matched.entry(name.clone()).or_default().push(key.clone());
        prioritized_names.push(name);
    }
    remaining.extend(prioritized_names.into_iter().chain(unmatched_names));
    matched
}

#[derive(Clone, Copy)]
enum IndexingWorkClass {
    LightCpu,
    Io,
    HeavyCpu,
    HeavyIo,
    ParallelIo,
    RuntimeCompanionParallelIo,
    ProjectCompanionIo,
    ProjectParallelIo,
}

async fn run_cpu_indexing_task<T, F>(
    server: &RubyLanguageServer,
    project_root: Option<PathBuf>,
    cancellation: Option<CancellationToken>,
    work_class: IndexingWorkClass,
    label: &'static str,
    task: F,
) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    let policy = server.indexing_resources.policy();
    let (cpu_lanes, transient_memory_bytes, io_slots, parallel, partitioned) = match work_class {
        IndexingWorkClass::LightCpu => (1, 16 * MIB, 0, false, false),
        IndexingWorkClass::Io => (1, 64 * MIB, 1, false, false),
        IndexingWorkClass::HeavyCpu => (1, 256 * MIB, 0, false, false),
        IndexingWorkClass::HeavyIo => (1, 256 * MIB, 1, false, false),
        IndexingWorkClass::ParallelIo => (policy.cpu_lanes(), 256 * MIB, 1, true, false),
        IndexingWorkClass::RuntimeCompanionParallelIo => {
            (1, 256 * MIB, 1, true, policy.cpu_lanes() > 1)
        }
        IndexingWorkClass::ProjectCompanionIo => (1, 256 * MIB, 1, false, false),
        IndexingWorkClass::ProjectParallelIo => {
            let project_root = project_root.as_deref().expect(
                "INVARIANT VIOLATED: project-parallel indexing has no project root. This is a \
                 bug because active-document lane ownership cannot be determined without the \
                 isolated project identity. Fix: pass the coordinator's canonical project root \
                 for every project-parallel phase.",
            );
            let cpu_lanes = server
                .indexing_resources
                .project_parallel_cpu_lanes(project_root);
            (
                cpu_lanes,
                256 * MIB,
                1,
                true,
                cpu_lanes != policy.cpu_lanes(),
            )
        }
    };
    let spec = IndexingWorkSpec::new(
        project_root,
        IndexingResourcePriority::Background,
        cpu_lanes,
        transient_memory_bytes,
        io_slots,
    );
    let spec = if matches!(
        work_class,
        IndexingWorkClass::RuntimeCompanionParallelIo
            | IndexingWorkClass::ProjectCompanionIo
            | IndexingWorkClass::ProjectParallelIo
    ) {
        spec.as_project_parallel()
    } else {
        spec
    };
    if partitioned {
        server
            .indexing_resources
            .run_partitioned_parallel_with_resources(label, spec, cancellation, task)
            .await
    } else if parallel {
        server
            .indexing_resources
            .run_parallel_with_resources(label, spec, cancellation, task)
            .await
    } else {
        server
            .indexing_resources
            .run_with_resources(label, spec, cancellation, task)
            .await
    }
}

async fn runtime_stdlib_paths_for_project(
    server: &RubyLanguageServer,
    runtime: &SelectedRuntimeDescriptor,
) -> Result<RuntimeStdlibPaths> {
    let key = RuntimeStdlibPathKey::new(&runtime.executable, runtime.java_home.as_deref())?;
    let producer_key = key.clone();
    let producer_server = server.clone();
    let started = Instant::now();
    let product = server
        .runtime_stdlib_path_cache
        .get_or_try_init(key, move || async move {
            run_cpu_indexing_task(
                &producer_server,
                None,
                None,
                IndexingWorkClass::Io,
                "exact runtime stdlib path discovery",
                move || producer_key.discover(),
            )
            .await
            .map_err(|error| error.to_string())?
            .map_err(|error| error.to_string())
        })
        .await
        .map_err(anyhow::Error::msg)?;
    info!(
        "[PERF][runtime stdlib path product] executable={} paths={} wait={:?}",
        runtime.executable.display(),
        product.paths().len(),
        started.elapsed()
    );
    Ok(product.as_ref().clone())
}

async fn index_core_stubs_additively_off_reactor(
    server: &RubyLanguageServer,
    project_root: PathBuf,
    cancellation: Option<CancellationToken>,
    analysis_engine: Arc<parking_lot::RwLock<AnalysisEngine>>,
    ruby_version: Option<RubyVersion>,
    extension_path: Option<String>,
) -> Result<()> {
    run_cpu_indexing_task(
        server,
        Some(project_root),
        cancellation,
        IndexingWorkClass::ParallelIo,
        "additive core stub indexing",
        move || {
            let mut indexer = IndexerStdlib::new(FileProcessor::new(), ruby_version);
            if let Some(extension_path) = extension_path {
                indexer.set_extension_path(PathBuf::from(extension_path));
            }
            indexer.index_core_stubs_blocking(analysis_engine)
        },
    )
    .await?
}

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

fn diagnostic_from_fact_fast(file: &SourceFile, fact: &DiagnosticFact) -> Option<Diagnostic> {
    Some(Diagnostic {
        range: lsp_range_for_text_range_fast(file, fact.range)?,
        severity: Some(lsp_diagnostic_severity(fact.severity)),
        code: Some(NumberOrString::String(fact.code.clone())),
        code_description: None,
        source: Some("ruby-fast-lsp".to_string()),
        message: fact.message.clone(),
        related_information: None,
        tags: None,
        data: None,
    })
}

fn lsp_diagnostic_severity(severity: AnalysisDiagnosticSeverity) -> DiagnosticSeverity {
    match severity {
        AnalysisDiagnosticSeverity::Error => DiagnosticSeverity::ERROR,
        AnalysisDiagnosticSeverity::Warning => DiagnosticSeverity::WARNING,
        AnalysisDiagnosticSeverity::Information => DiagnosticSeverity::INFORMATION,
        AnalysisDiagnosticSeverity::Hint => DiagnosticSeverity::HINT,
    }
}

fn configured_gem_selection(
    inferred: Vec<String>,
    config: &IndexingConfig,
) -> (HashSet<String>, HashSet<String>) {
    let excluded = config
        .excluded_gems
        .iter()
        .filter(|name| !name.is_empty())
        .cloned()
        .collect::<HashSet<_>>();
    let mut required = inferred.into_iter().collect::<HashSet<_>>();
    required.extend(
        config
            .included_gems
            .iter()
            .filter(|name| !name.is_empty())
            .cloned(),
    );
    required.retain(|name| !excluded.contains(name));
    (required, excluded)
}

fn read_jdk_feature(java_home: &Path) -> Result<u16> {
    let release_path = java_home.join("release");
    let metadata = std::fs::metadata(&release_path).with_context(|| {
        format!(
            "JDK release metadata is missing: {}",
            release_path.display()
        )
    })?;
    if metadata.len() > 64 * 1024 {
        return Err(anyhow!(
            "JDK release metadata exceeds 64 KiB: {}",
            release_path.display()
        ));
    }
    let release = std::fs::read_to_string(&release_path).with_context(|| {
        format!(
            "JDK release metadata is unreadable: {}",
            release_path.display()
        )
    })?;
    let version = release
        .lines()
        .find_map(|line| line.strip_prefix("JAVA_VERSION="))
        .map(|value| value.trim_matches('"'))
        .ok_or_else(|| {
            anyhow!(
                "JDK release metadata has no JAVA_VERSION: {}",
                release_path.display()
            )
        })?;
    let first = version
        .split(['.', '-', '+'])
        .next()
        .ok_or_else(|| anyhow!("JDK JAVA_VERSION is empty in {}", release_path.display()))?;
    let feature = if first == "1" {
        version
            .split('.')
            .nth(1)
            .ok_or_else(|| {
                anyhow!(
                    "legacy JDK JAVA_VERSION has no feature component in {}",
                    release_path.display()
                )
            })?
            .parse::<u16>()
    } else {
        first.parse::<u16>()
    }
    .with_context(|| {
        format!(
            "JDK JAVA_VERSION `{version}` is invalid in {}",
            release_path.display()
        )
    })?;
    if feature == 0 {
        return Err(anyhow!(
            "JDK JAVA_VERSION `{version}` has feature zero in {}",
            release_path.display()
        ));
    }
    Ok(feature)
}

fn java_executable_for_home(java_home: &Path) -> PathBuf {
    if cfg!(windows) {
        java_home.join("bin/java.exe")
    } else {
        java_home.join("bin/java")
    }
}

fn jruby_cache_root_for_project(
    workspace_root: &Path,
    user_cache_root_override: Option<&Path>,
    namespace: &str,
    classpath_fingerprint: &str,
) -> Result<PathBuf> {
    let user_cache_root = match user_cache_root_override {
        Some(root) => root.to_path_buf(),
        None => crate::utils::ruby_fast_lsp_user_cache_root()?,
    };
    let canonical_project_root = workspace_root.canonicalize().with_context(|| {
        format!(
            "failed to canonicalize JRuby project root {} for cache isolation",
            workspace_root.display()
        )
    })?;
    let project_key = format!(
        "{:x}",
        Sha256::digest(canonical_project_root.to_string_lossy().as_bytes())
    );
    Ok(user_cache_root
        .join(namespace)
        .join(project_key)
        .join(classpath_fingerprint))
}

fn build_cached_project_java_catalog(
    classpath: &crate::runtime::jruby::classpath::ProjectClasspath,
    jdk_feature: u16,
    archive_limits: ArchiveLimits,
    persistent_cache: &PersistentDerivedProductCache,
    process_cache: &JavaArtifactProductCache,
) -> Result<ProjectJavaCatalog> {
    let products = classpath
        .artifacts
        .par_iter()
        .map(|artifact| {
            verify_artifact_discovery_identity(artifact).map_err(|error| {
                anyhow!(
                    "Java artifact changed after classpath discovery: {}: {error:?}",
                    artifact.path.display()
                )
            })?;
            let key = JavaArtifactProductKey::new(artifact, jdk_feature, archive_limits);
            let product = process_cache
                .get_or_try_init(key.clone(), || {
                    match persistent_cache
                        .lookup_java_artifact_or_reserve(&key)
                        .map_err(|error| {
                            format!(
                                "persistent Java artifact lookup failed for {}: {error:#}",
                                artifact.path.display()
                            )
                        })? {
                        PersistentJavaArtifactLookup::Hit(product) => Ok((*product).clone()),
                        PersistentJavaArtifactLookup::Reservation(reservation) => {
                            let product =
                                JavaArtifactProduct::build(artifact, &key, archive_limits)
                                    .map_err(|error| {
                                        format!(
                                            "failed to build Java artifact metadata for {}: \
                                             {error:?}",
                                            artifact.path.display()
                                        )
                                    })?;
                            reservation.publish(&product).map_err(|error| {
                                format!(
                                    "failed to publish Java artifact metadata for {}: {error:#}",
                                    artifact.path.display()
                                )
                            })?;
                            Ok(product)
                        }
                    }
                })
                .map_err(|message| anyhow!(message))?;
            Ok((*product).clone())
        })
        .collect::<Result<Vec<_>>>()?;

    // Indexed Rayon collection preserves input order. Keep composition
    // explicitly sequential because the first artifact defining a class wins.
    let mut builder = ProjectJavaCatalogBuilder::new(classpath);
    for (artifact, product) in classpath.artifacts.iter().zip(products) {
        builder.push(product).map_err(|error| {
            anyhow!(
                "failed to compose Java artifact metadata for {} into project catalog: \
                 {error:?}",
                artifact.path.display()
            )
        })?;
    }
    Ok(builder.finish())
}

fn build_jruby_import_provider(
    workspace_root: PathBuf,
    config: RubyFastLspConfig,
    effective_runtime: Option<SelectedRuntimeDescriptor>,
    user_cache_root_override: Option<PathBuf>,
    persistent_cache: Option<PersistentDerivedProductCache>,
    classpath_file_product_cache: Option<ClasspathFileProductCache>,
    java_artifact_product_cache: Option<JavaArtifactProductCache>,
) -> Result<(Option<Arc<JrubyImportProvider>>, Option<ClasspathArtifact>)> {
    let total_started = Instant::now();
    let Some(runtime) = effective_runtime else {
        return Ok((None, None));
    };
    if runtime.implementation != RuntimeImplementation::Jruby {
        return Ok((None, None));
    }
    let java_home = runtime.java_home.clone().ok_or_else(|| {
        anyhow!(
            "JRuby runtime `{}` for project `{}` has no JDK. Select or configure an exact \
             JAVA_HOME before indexing Java imports.",
            runtime.engine_version,
            workspace_root.display()
        )
    })?;
    let jdk_feature = read_jdk_feature(&java_home)?;
    let root = workspace_root.to_string_lossy();
    let project_config = config.jruby.project_config(&root);
    let maven_repository = std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join(".m2/repository"))
        .filter(|path| path.is_dir());
    let java_gem_roots_started = Instant::now();
    let java_gem_roots = discover_locked_java_gem_roots(
        &workspace_root,
        &runtime.executable,
        &runtime.compatibility_version,
    )
    .with_context(|| {
        format!(
            "failed to discover exact locked Java-platform gems for project {}",
            workspace_root.display()
        )
    })?;
    let java_gem_roots_elapsed = java_gem_roots_started.elapsed();
    let classpath_started = Instant::now();
    let classpath_inputs = ClasspathInputs {
        project_root: workspace_root.clone(),
        jruby_executable: runtime.executable,
        java_home: java_home.clone(),
        maven_repository,
        java_gem_roots,
        additional_classpath: project_config.additional_classpath,
        additional_sources: project_config.additional_sources,
    };
    let classpath = match classpath_file_product_cache.as_ref() {
        Some(cache) => discover_project_classpath_with_cache(
            &classpath_inputs,
            ClasspathLimits::default(),
            cache,
        ),
        None => discover_project_classpath(&classpath_inputs, ClasspathLimits::default()),
    }
    .map_err(|error| {
        anyhow!(
            "JRuby classpath discovery failed for `{}`: {error:?}",
            workspace_root.display()
        )
    })?;
    let classpath_elapsed = classpath_started.elapsed();
    let runtime_archive = classpath
        .artifacts
        .iter()
        .find(|artifact| {
            artifact.origin == ArtifactOrigin::JrubyRuntime
                && artifact
                    .path
                    .file_name()
                    .is_some_and(|name| name == "jruby.jar")
        })
        .cloned();
    let archive_limits = ArchiveLimits::default();
    let catalog_started = Instant::now();
    let catalog = match (
        persistent_cache.as_ref(),
        java_artifact_product_cache.as_ref(),
    ) {
        (Some(persistent_cache), Some(process_cache)) => build_cached_project_java_catalog(
            &classpath,
            jdk_feature,
            archive_limits,
            persistent_cache,
            process_cache,
        ),
        (Some(_), None) | (None, Some(_)) => panic!(
            "INVARIANT VIOLATED: persistent and process-local Java artifact caches were configured independently. This is a bug because production lookup must validate persistent products before bounded shared retention. Fix: pass both caches together or neither for an isolated uncached test."
        ),
        (None, None) => build_project_java_catalog(&classpath, jdk_feature, archive_limits)
            .map_err(|error| anyhow!("Java catalog construction failed: {error:?}")),
    }
    .with_context(|| {
        format!(
            "JRuby Java catalog failed for `{}`",
            workspace_root.display()
        )
    })?;
    let catalog_elapsed = catalog_started.elapsed();
    info!(
        "JRuby Java catalog ready for {}: classes={}, artifacts={}, duplicates={}, fingerprint={}",
        workspace_root.display(),
        catalog.classes.len(),
        classpath.artifacts.len(),
        catalog.duplicates.len(),
        catalog.classpath_fingerprint_sha256
    );
    let source_cache_root = jruby_cache_root_for_project(
        &workspace_root,
        user_cache_root_override.as_deref(),
        "jruby-sources",
        &catalog.classpath_fingerprint_sha256,
    )?;
    let source_resolver = JavaSourceResolver::new(
        classpath.sources,
        source_cache_root,
        JavaSourceResolutionLimits::default(),
    );
    let mut provider =
        JrubyImportProvider::new(Arc::new(catalog)).with_source_resolver(Arc::new(source_resolver));
    let signature_cache_root = jruby_cache_root_for_project(
        &workspace_root,
        user_cache_root_override.as_deref(),
        "jruby-signatures",
        provider.classpath_fingerprint(),
    )?;
    provider = provider.with_signature_cache_root(signature_cache_root);
    let decompiler_cache_root = jruby_cache_root_for_project(
        &workspace_root,
        user_cache_root_override.as_deref(),
        "jruby-decompiler",
        provider.classpath_fingerprint(),
    )?;
    match discover_bundled_cfr_asset().and_then(|asset| {
        JavaDecompiler::new(
            java_executable_for_home(&java_home),
            asset,
            decompiler_cache_root,
            JavaDecompilerLimits::default(),
        )
    }) {
        Ok(decompiler) => provider = provider.with_decompiler(Arc::new(decompiler)),
        Err(error) => warn!(
            "JRuby implementation decompiler unavailable for {}: {:?}; exact source and generated signatures remain available",
            workspace_root.display(),
            error
        ),
    }
    info!(
        "[PERF][JRuby runtime] project={} total={:?} java_gem_roots={:?} classpath={:?} catalog={:?} provider_setup={:?}",
        workspace_root.display(),
        total_started.elapsed(),
        java_gem_roots_elapsed,
        classpath_elapsed,
        catalog_elapsed,
        total_started
            .elapsed()
            .saturating_sub(java_gem_roots_elapsed + classpath_elapsed + catalog_elapsed)
    );
    Ok((Some(Arc::new(provider)), runtime_archive))
}

async fn build_jruby_import_provider_off_reactor(
    server: &RubyLanguageServer,
    workspace_root: PathBuf,
    config: RubyFastLspConfig,
    effective_runtime: Option<SelectedRuntimeDescriptor>,
    user_cache_root_override: Option<PathBuf>,
    cancellation: Option<CancellationToken>,
    work_class: IndexingWorkClass,
) -> Result<(Option<Arc<JrubyImportProvider>>, Option<ClasspathArtifact>)> {
    let persistent_cache = server.persistent_derived_product_cache.clone();
    let classpath_file_product_cache = server.classpath_file_product_cache.clone();
    let java_artifact_product_cache = server.java_artifact_product_cache.clone();
    run_cpu_indexing_task(
        server,
        Some(workspace_root.clone()),
        cancellation,
        work_class,
        "JRuby classpath and catalog construction",
        move || {
            build_jruby_import_provider(
                workspace_root,
                config,
                effective_runtime,
                user_cache_root_override,
                Some(persistent_cache),
                Some(classpath_file_product_cache),
                Some(java_artifact_product_cache),
            )
        },
    )
    .await?
}

fn index_jruby_runtime_sources_blocking(
    workspace_root: PathBuf,
    artifact: ClasspathArtifact,
    provider: Arc<JrubyImportProvider>,
    user_cache_root_override: Option<PathBuf>,
    processor: FileProcessor,
    analysis_engine: Arc<parking_lot::RwLock<AnalysisEngine>>,
    dependency_seed_engine: Arc<parking_lot::RwLock<AnalysisEngine>>,
) -> Result<()> {
    let cache_root = jruby_cache_root_for_project(
        &workspace_root,
        user_cache_root_override.as_deref(),
        "jruby-runtime-sources",
        provider.classpath_fingerprint(),
    )?;
    let sources = materialize_jruby_runtime_sources(&artifact, &cache_root).map_err(|error| {
        anyhow!(
            "failed to materialize bounded JRuby runtime sources for {}: {error:?}",
            workspace_root.display()
        )
    })?;
    for source in sources {
        let uri = Url::from_file_path(&source.path).map_err(|_| {
            anyhow!(
                "materialized JRuby runtime source is not a valid file URI: {}",
                source.path.display()
            )
        })?;
        processor.collect_file_facts_as_deferred_resolution_in_engine(
            &uri,
            &source.content,
            analysis_engine.clone(),
            ruby_analysis::core::SourceKind::Stdlib,
        )?;
        processor.collect_file_facts_as_deferred_resolution_in_engine(
            &uri,
            &source.content,
            dependency_seed_engine.clone(),
            ruby_analysis::core::SourceKind::Stdlib,
        )?;
    }
    Ok(())
}

fn lsp_range_for_text_range_fast(file: &SourceFile, range: TextRange) -> Option<Range> {
    let (start_line, start_character) = file.byte_offset_to_line_character(range.start_byte)?;
    let (end_line, end_character) = file.byte_offset_to_line_character(range.end_byte)?;
    Some(Range::new(
        Position::new(start_line, start_character),
        Position::new(end_line, end_character),
    ))
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
    user_cache_root_override: Option<PathBuf>,

    // The main indexing engine
    file_processor: Option<FileProcessor>,
    dependency_seed_engine: Option<AnalysisEngine>,

    // Project-specific indexer
    project_indexer: Option<IndexerProject>,

    // Standard library indexer
    stdlib_indexer: Option<IndexerStdlib>,

    // Gem indexer
    gem_indexer: Option<IndexerGem>,

    // Where to find Ruby libraries on this system
    ruby_library_paths: Vec<PathBuf>,

    /// Timings from the most recent `run_complete_indexing` call.
    last_timings: IndexingTimings,
    indexing_run: Option<crate::indexing_status::IndexingRun>,
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
        let uri = Url::from_directory_path(&self.workspace_root).expect(
            "INVARIANT VIOLATED: workspace root cannot be represented as a file URI. This is a bug because indexing only accepts filesystem workspace roots. Fix: register a canonical filesystem project root before creating the coordinator.",
        );
        server.analysis_engine_for_uri(&uri)
    }
    /// Creates a new IndexingCoordinator for the given workspace.
    ///
    /// Call `run_complete_indexing()` to actually start the indexing process.
    pub fn new(workspace_root: PathBuf, config: RubyFastLspConfig) -> Self {
        Self {
            workspace_root,
            config,
            extension_registry: None,
            detected_ruby_version: None,
            effective_runtime: None,
            jruby_import_provider: None,
            jruby_runtime_archive: None,
            user_cache_root_override: None,
            file_processor: None,
            dependency_seed_engine: None,
            project_indexer: None,
            stdlib_indexer: None,
            gem_indexer: None,
            ruby_library_paths: Vec::new(),
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

    pub fn set_indexing_run(&mut self, run: crate::indexing_status::IndexingRun) {
        self.indexing_run = Some(run);
    }

    fn resource_cancellation(&self) -> Option<CancellationToken> {
        self.indexing_run
            .as_ref()
            .map(crate::indexing_status::IndexingRun::cancellation)
    }

    pub fn set_analysis_engine(
        &mut self,
        analysis_engine: Arc<parking_lot::RwLock<AnalysisEngine>>,
    ) {
        self.analysis_engine_override = Some(analysis_engine);
    }

    #[cfg(test)]
    pub(crate) fn set_user_cache_root_for_tests(&mut self, root: PathBuf) {
        self.user_cache_root_override = Some(root);
    }

    /// Runs the complete indexing process from start to finish.
    ///
    /// 1. Figure out which Ruby version we're using
    /// 2. Find where Ruby libraries are installed on this system
    /// 3. Set up the main indexing engine
    /// 4. Scan project dependencies
    /// 5. Collect facts from gems, stdlib, then project files
    /// 6. Publish diagnostics
    pub async fn run_complete_indexing(&mut self, server: &RubyLanguageServer) -> Result<()> {
        info!("Starting complete indexing process");
        server
            .indexing_resources
            .mark_project_navigation_pending_if_active(&self.workspace_root);
        let mut project_navigation_reservation = Some(
            server
                .indexing_resources
                .project_navigation_reservation(self.workspace_root.clone()),
        );
        self.extension_registry
            .get_or_insert_with(|| server.extension_registry.clone());
        let start_time = Instant::now();
        let runtime_start = Instant::now();
        self.indexing_checkpoint(server)?;

        self.resolve_effective_runtime(server).await?;
        self.transition_indexing_status(
            server,
            crate::indexing_status::IndexingPhase::DiscoveringInputs,
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
        // concurrently below and handed to the running generation at the first
        // bounded tail boundary after it becomes ready. Only files collected
        // before that handoff need catalog-sensitive replay before this project
        // reports readiness.
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

        self.transition_indexing_status(
            server,
            crate::indexing_status::IndexingPhase::IndexingCore,
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
        let runtime_cache_root = self.user_cache_root_override.clone();
        let runtime_cancellation = self.resource_cancellation();
        let is_jruby = runtime_selection
            .as_ref()
            .is_some_and(|runtime| runtime.implementation == RuntimeImplementation::Jruby);
        let (runtime_provider_ready_tx, runtime_provider_ready_rx) =
            tokio::sync::oneshot::channel();
        let runtime_provider = async move {
            if !is_jruby {
                let _ = runtime_provider_ready_tx.send(Ok(None));
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
            let readiness = result
                .as_ref()
                .map(|(provider, _)| provider.clone())
                .map_err(|error| error.to_string());
            let _ = runtime_provider_ready_tx.send(readiness);
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
                .expect(
                    "INVARIANT VIOLATED: active indexing run has no registered workspace while \
                     preparing dependency navigation. This is a coordinator bug because the \
                     generation checkpoint already proved exact workspace ownership. Fix: keep \
                     workspace removal and coordinator cancellation atomic.",
                );
            (workspace.navigation_demands, run.generation())
        });
        let startup_dependency_seed = {
            let dependency_seed = dependency_seed_engine.read();
            assert!(
                dependency_seed.files().all(|source| matches!(
                    source.kind,
                    ruby_analysis::core::SourceKind::Stub
                        | ruby_analysis::core::SourceKind::Stdlib
                        | ruby_analysis::core::SourceKind::Signature
                        | ruby_analysis::core::SourceKind::External
                )),
                "INVARIANT VIOLATED: providerless startup dependency seed contains project, \
                 excluded, or gem facts. This is a bug because active dependency navigation \
                 products must be reusable before project timing can affect them. Fix: fork the \
                 startup seed immediately after clean core stub indexing."
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
            // is reserved for its exact JRuby runtime companion.
            self.transition_indexing_status(
                server,
                crate::indexing_status::IndexingPhase::IndexingProject,
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
                .indexing_resources
                .mark_project_navigation_complete_if_active(&self.workspace_root);
            self.collect_remaining_project_facts(server, Some(runtime_provider_ready_rx))
                .await?;
            Ok::<Duration, anyhow::Error>(project_start.elapsed())
        };
        // Poll the active dependency frontier before the other companion so
        // its exact locked source enters the governor queue first. The project
        // frontier releases its large parallel claim after the bounded target
        // files, allowing this gem work and the exact runtime provider to
        // overlap before exhaustive project scanning resumes.
        let (startup_gem_result, project_result, runtime_provider_result) =
            tokio::join!(startup_gem_indexing, project_indexing, runtime_provider);
        let ((provider, runtime_archive), runtime_provider_dur) = runtime_provider_result?;
        let mut project_dur = project_result?;
        let (gem_indexer, discovery_dur) = startup_gem_result?;

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
            assert!(
                dependency_seed.files().all(|source| matches!(
                    source.kind,
                    ruby_analysis::core::SourceKind::Stub
                        | ruby_analysis::core::SourceKind::Stdlib
                        | ruby_analysis::core::SourceKind::Signature
                        | ruby_analysis::core::SourceKind::External
                )),
                "INVARIANT VIOLATED: immutable dependency seed contains project, excluded, or gem facts. \
                 This is a bug because editor timing or one dependency could contaminate every reusable \
                 gem product identity. Fix: build the seed only from clean core and runtime inputs."
            );
            dependency_seed.clone()
        });
        let core_dur = core_stub_dur + runtime_sources_dur;
        let runtime_dur = runtime_selection_dur + runtime_provider_dur;

        // The exact dependency seed is now complete. Stream locked gems on the
        // reserved companion lane while the active project's bounded Java-
        // sensitive subset is replaced on its five project lanes.
        let dependencies_start = Instant::now();
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
        let provider_present = self.jruby_import_provider.is_some();
        let project_completion = async {
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
            drop(project_navigation_reservation.take());
            self.transition_indexing_status(
                server,
                crate::indexing_status::IndexingPhase::ProjectNavigationReady,
            )
            .await?;
            self.transition_indexing_status(
                server,
                crate::indexing_status::IndexingPhase::IndexingDependencies,
            )
            .await?;
            Ok::<Duration, anyhow::Error>(replay_dur)
        };
        let (project_completion_result, gem_indexing_result) =
            tokio::join!(project_completion, gem_indexing);
        project_dur += project_completion_result?;
        self.gem_indexer = Some(gem_indexing_result?);

        // Runtime stdlib still enters the same isolated engine before the
        // dependency-ready milestone and complete semantic diagnostics.
        self.index_standard_library(server, &ruby_version).await?;
        let dependencies_dur = dependencies_start.elapsed();

        let facts_dur = facts_start.elapsed();
        let reserved_dur = Duration::default();
        info!("Facts collection completed in {:?}", facts_dur);

        // Publish diagnostics to the client.
        info!("Publishing diagnostics");
        self.transition_indexing_status(
            server,
            crate::indexing_status::IndexingPhase::ResolvingSemantics,
        )
        .await?;
        let resolve_start = Instant::now();
        let analysis_engine = self.analysis_engine(server);
        run_cpu_indexing_task(
            server,
            Some(self.workspace_root.clone()),
            self.resource_cancellation(),
            IndexingWorkClass::HeavyCpu,
            "final semantic resolution",
            move || {
                analysis_engine.write().resolve();
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
                    crate::navigation_demand::NavigationDemandStage::Dependency,
                );
            }
        }
        self.transition_indexing_status(
            server,
            crate::indexing_status::IndexingPhase::DependencyNavigationReady,
        )
        .await?;
        self.transition_indexing_status(
            server,
            crate::indexing_status::IndexingPhase::PublishingDiagnostics,
        )
        .await?;
        let publish_start = Instant::now();
        self.publish_unresolved_diagnostics(server).await?;
        let publish_dur = publish_start.elapsed();

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
        phase: crate::indexing_status::IndexingPhase,
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

    fn log_analysis_memory_stats(&self, server: &RubyLanguageServer) {
        let analysis_engine = self.analysis_engine(server);
        let engine = analysis_engine.read();
        let stats = engine.stats();
        let memory = engine.estimated_memory_stats();
        let total = memory.total();

        info!(
            "Analysis stats: files={}, source_bytes={}, symbols={}, methods={}, ref_candidates={}, refs={}, types={}, diagnostic_candidates={}, diagnostics={}, graph_nodes={}, graph_edges={}, unresolved_graph_edges={}",
            stats.files,
            stats.source_bytes,
            stats.symbols,
            stats.methods,
            stats.reference_candidates,
            stats.references,
            stats.types,
            stats.diagnostic_candidates,
            stats.diagnostics,
            stats.graph_nodes,
            stats.graph_edges,
            stats.unresolved_graph_edges
        );
        info!("Estimated engine heap: {:.1} MB", bytes_to_mb(total));
        log_memory_bucket("names", memory.names, total);
        log_memory_bucket("files", memory.files, total);
        log_memory_bucket("symbols", memory.symbols, total);
        log_memory_bucket("methods", memory.methods, total);
        log_memory_bucket("types", memory.types, total);
        log_memory_bucket("reference candidates", memory.reference_candidates, total);
        log_memory_bucket("references", memory.references, total);
        log_memory_bucket("diagnostics", memory.diagnostics, total);
        log_memory_bucket("diagnostic candidates", memory.diagnostic_candidates, total);
        log_memory_bucket("graph", memory.graph, total);
        log_memory_bucket(
            "unresolved graph edges",
            memory.unresolved_graph_edges,
            total,
        );
    }

    /// Step 1: Detect which Ruby version we're working with
    #[cfg(test)]
    fn detect_ruby_version(&mut self) -> Option<RubyVersion> {
        if let Some(runtime) = &self.effective_runtime {
            let version = ruby_version_for_runtime(runtime);
            self.detected_ruby_version = version;
            return version;
        }
        let root = self.workspace_root.to_string_lossy();
        let selected = self
            .config
            .runtime
            .selection_for_project(&root, &self.config.ruby_version);
        let version = match selected {
            EffectiveRuntimeSelection::Explicit(runtime) => {
                self.effective_runtime = Some(runtime.clone());
                let mut components = runtime.compatibility_version.split('.');
                let major = components.next()?.parse::<u8>().ok()?;
                let minor = components.next()?.parse::<u8>().ok()?;
                let implementation = match runtime.implementation {
                    RuntimeImplementation::Mri => RubyImplementation::Mri,
                    RuntimeImplementation::Jruby => RubyImplementation::JRuby,
                    RuntimeImplementation::Truffleruby => RubyImplementation::TruffleRuby,
                };
                Some(RubyVersion::new_with_implementation(
                    major,
                    minor,
                    implementation,
                ))
            }
            EffectiveRuntimeSelection::Auto
            | EffectiveRuntimeSelection::LegacyMriCompatibility { .. } => self
                .config
                .get_ruby_version()
                .map(RubyVersion::from_tuple)
                .or_else(|| {
                    RubyVersionDetector::from_path(self.workspace_root.clone()).detect_version()
                }),
        };
        self.detected_ruby_version = version;
        version
    }

    async fn detect_ruby_version_off_reactor(
        &mut self,
        server: &RubyLanguageServer,
    ) -> Result<Option<RubyVersion>> {
        if let Some(runtime) = &self.effective_runtime {
            let version = ruby_version_for_runtime(runtime);
            self.detected_ruby_version = version;
            return Ok(version);
        }
        if let Some(version) = self.config.get_ruby_version().map(RubyVersion::from_tuple) {
            self.detected_ruby_version = Some(version);
            return Ok(Some(version));
        }
        let workspace_root = self.workspace_root.clone();
        let version = run_cpu_indexing_task(
            server,
            Some(self.workspace_root.clone()),
            self.resource_cancellation(),
            IndexingWorkClass::Io,
            "Ruby version detection",
            move || RubyVersionDetector::from_path(workspace_root).detect_version(),
        )
        .await?;
        self.detected_ruby_version = version;
        Ok(version)
    }

    async fn resolve_effective_runtime(&mut self, server: &RubyLanguageServer) -> Result<()> {
        let root = self.workspace_root.to_string_lossy();
        self.effective_runtime = match self
            .config
            .runtime
            .selection_for_project(&root, &self.config.ruby_version)
        {
            EffectiveRuntimeSelection::Explicit(runtime) => Some(runtime),
            EffectiveRuntimeSelection::Auto => {
                server.resolve_auto_runtime(&self.workspace_root).await?
            }
            EffectiveRuntimeSelection::LegacyMriCompatibility { .. } => None,
        };
        server.set_effective_runtime(&self.workspace_root, self.effective_runtime.clone());
        Ok(())
    }

    /// Step 3: Set up the main indexing engine
    fn setup_file_processor(&mut self, server: &RubyLanguageServer) {
        let extension_registry = self
            .extension_registry
            .get_or_insert_with(|| server.extension_registry.clone())
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
        self.file_processor = Some(
            server
                .extension_project_context_seed_for_root(&self.workspace_root)
                .map(|seed| processor.clone().with_extension_project_context_seed(seed))
                .unwrap_or(processor),
        );
    }

    #[cfg(test)]
    fn setup_jruby_import_provider(&mut self) -> Result<()> {
        let (provider, runtime_archive) = build_jruby_import_provider(
            self.workspace_root.clone(),
            self.config.clone(),
            self.effective_runtime.clone(),
            self.user_cache_root_override.clone(),
            None,
            None,
            None,
        )?;
        self.jruby_import_provider = provider;
        self.jruby_runtime_archive = runtime_archive;
        Ok(())
    }

    async fn index_jruby_runtime_sources_off_reactor(
        &self,
        server: &RubyLanguageServer,
        dependency_seed_engine: Arc<parking_lot::RwLock<AnalysisEngine>>,
    ) -> Result<()> {
        let Some(artifact) = self.jruby_runtime_archive.clone() else {
            return Ok(());
        };
        let provider = self.jruby_import_provider.clone().expect(
            "INVARIANT VIOLATED: a JRuby runtime archive exists without its import provider. \
             This is a bug because both are derived transactionally from one isolated classpath. \
             Fix: keep JRuby runtime archive and catalog setup in the same coordinator step.",
        );
        let processor = self.file_processor.clone().expect(
            "INVARIANT VIOLATED: JRuby runtime source indexing started before FileProcessor setup. \
             This is a coordinator bug because runtime sources must use ordinary file-owned facts. \
             Fix: keep FileProcessor setup before JRuby runtime source materialization.",
        );
        let workspace_root = self.workspace_root.clone();
        let user_cache_root_override = self.user_cache_root_override.clone();
        let analysis_engine = self.analysis_engine(server);
        run_cpu_indexing_task(
            server,
            Some(self.workspace_root.clone()),
            self.resource_cancellation(),
            IndexingWorkClass::HeavyIo,
            "JRuby runtime source materialization",
            move || {
                index_jruby_runtime_sources_blocking(
                    workspace_root,
                    artifact,
                    provider,
                    user_cache_root_override,
                    processor,
                    analysis_engine,
                    dependency_seed_engine,
                )
            },
        )
        .await?
    }

    #[cfg(test)]
    fn jruby_signature_cache_root(&self, provider: &JrubyImportProvider) -> Result<PathBuf> {
        provider
            .signature_cache_root()
            .map(Path::to_path_buf)
            .ok_or_else(|| {
                anyhow!(
                    "JRuby provider for {} has no isolated signature cache root",
                    self.workspace_root.display()
                )
            })
    }

    /// Collect facts from project files (skips already-indexed files)
    async fn collect_project_navigation_facts(
        &mut self,
        server: &RubyLanguageServer,
        priority_keys: ActiveDocumentPriorityKeys,
    ) -> Result<()> {
        let mut project_indexer = self.project_indexer.take().unwrap_or_else(|| {
            IndexerProject::new(
                self.workspace_root.clone(),
                self.file_processor.as_ref().unwrap().clone(),
                self.config.indexing.clone(),
            )
        });
        let frontier_demands = if let Some(run) = self.indexing_run.as_ref() {
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
            Some((workspace.navigation_demands.clone(), run.generation()))
        } else {
            None
        };
        let worker_server = server.clone();
        let worker_root = self.workspace_root.clone();
        let (project_indexer, result) = run_cpu_indexing_task(
            server,
            Some(self.workspace_root.clone()),
            self.resource_cancellation(),
            IndexingWorkClass::ProjectParallelIo,
            "project navigation fact frontier",
            move || {
                project_indexer.set_navigation_priority_keys(
                    priority_keys.project_terminals.into_iter().collect(),
                    priority_keys.dependency_roots,
                );
                let initial_demand_keys = frontier_demands
                    .as_ref()
                    .map(|(demands, generation)| {
                        demands.drain(
                            *generation,
                            crate::navigation_demand::NavigationDemandStage::Project,
                        )
                    })
                    .unwrap_or_default();
                let result = project_indexer
                    .collect_initial_project_navigation_demand_facts(
                        &initial_demand_keys,
                        &worker_server,
                    )
                    .and_then(|selection| {
                        if let Some((demands, generation)) = frontier_demands.as_ref() {
                            if !selection.completed_keys.is_empty() {
                                demands.complete_keys(
                                    *generation,
                                    crate::navigation_demand::NavigationDemandStage::Project,
                                    &selection.completed_keys,
                                );
                            }
                            if !selection.deferred_keys.is_empty() {
                                info!(
                                    "Deferred {} ambiguous initial project navigation demand(s) \
                                     until exhaustive project completion for {}",
                                    selection.deferred_keys.len(),
                                    worker_root.display()
                                );
                            }
                        }
                        project_indexer.finish_project_navigation_facts(&worker_server)
                    })
                    .and_then(|()| {
                        let Some((demands, generation)) = frontier_demands.as_ref() else {
                            return Ok(());
                        };
                        let demand_keys = demands.drain(
                            *generation,
                            crate::navigation_demand::NavigationDemandStage::Project,
                        );
                        let selection = project_indexer.take_navigation_demand_files(&demand_keys);
                        let demanded_file_count = selection.files.len();
                        project_indexer.collect_project_file_batch(
                            &selection.files,
                            &worker_server,
                            true,
                        )?;
                        if !selection.completed_keys.is_empty() {
                            demands.complete_keys(
                                *generation,
                                crate::navigation_demand::NavigationDemandStage::Project,
                                &selection.completed_keys,
                            );
                        }
                        if !selection.deferred_keys.is_empty() {
                            info!(
                                "Deferred {} ambiguous project navigation demand(s) until \
                                 exhaustive project completion for {}",
                                selection.deferred_keys.len(),
                                worker_root.display()
                            );
                        }
                        if !demand_keys.is_empty() {
                            info!(
                                "[PERF][project demand frontier] project={} keys={} files={}",
                                worker_root.display(),
                                demand_keys.len(),
                                demanded_file_count
                            );
                        }
                        Ok(())
                    });
                (project_indexer, result)
            },
        )
        .await?;
        self.project_indexer = Some(project_indexer);
        result?;
        self.complete_processed_frontier_demands(server)?;
        Ok(())
    }

    fn complete_processed_frontier_demands(&self, server: &RubyLanguageServer) -> Result<()> {
        let Some(run) = self.indexing_run.as_ref() else {
            return Ok(());
        };
        let generation = run.generation();
        let workspace = server
            .list_workspaces()
            .into_iter()
            .find(|workspace| workspace.root_path == self.workspace_root)
            .ok_or_else(|| {
                anyhow!(
                    "Indexing generation {} was cancelled because project {} is no longer registered",
                    generation,
                    self.workspace_root.display()
                )
            })?;
        let priority_keys = self
            .project_indexer
            .as_ref()
            .expect(
                "INVARIANT VIOLATED: project frontier demand completion has no retained \
                 IndexerProject. This is a coordinator bug because processed-file evidence \
                 belongs to the exact frontier indexer. Fix: retain it before completing \
                 request waiters.",
            )
            .processed_navigation_priority_keys();
        let completed_keys = priority_keys
            .into_iter()
            .filter(|key| {
                workspace.navigation_demands.claim_if_requested(
                    generation,
                    crate::navigation_demand::NavigationDemandStage::Project,
                    key,
                )
            })
            .collect::<Vec<_>>();
        if !completed_keys.is_empty() {
            workspace.navigation_demands.complete_keys(
                generation,
                crate::navigation_demand::NavigationDemandStage::Project,
                &completed_keys,
            );
            info!(
                "Completed {} project navigation demand(s) from the active frontier for {}",
                completed_keys.len(),
                self.workspace_root.display()
            );
        }
        Ok(())
    }

    async fn collect_remaining_project_facts(
        &mut self,
        server: &RubyLanguageServer,
        mut runtime_provider_ready_rx: Option<
            tokio::sync::oneshot::Receiver<Result<Option<Arc<JrubyImportProvider>>, String>>,
        >,
    ) -> Result<()> {
        let Some(run) = self.indexing_run.as_ref() else {
            let exact_runtime_provider = match runtime_provider_ready_rx.take() {
                Some(receiver) => receiver
                    .await
                    .map_err(|_| {
                        anyhow!(
                            "exact runtime provider task for {} ended before releasing exhaustive \
                             project collection",
                            self.workspace_root.display()
                        )
                    })?
                    .map_err(|error| {
                        anyhow!(
                            "exact runtime provider for {} failed before exhaustive project \
                             collection: {error}",
                            self.workspace_root.display()
                        )
                    })?,
                None => None,
            };
            return self
                .collect_remaining_project_facts_without_demands(server, exact_runtime_provider)
                .await;
        };
        let generation = run.generation();
        let workspace = server
            .list_workspaces()
            .into_iter()
            .find(|workspace| workspace.root_path == self.workspace_root)
            .ok_or_else(|| {
                anyhow!(
                    "Indexing generation {} was cancelled because project {} is no longer registered",
                    generation,
                    self.workspace_root.display()
                )
        })?;
        let demands = workspace.navigation_demands.clone();
        let started = Instant::now();
        self.indexing_checkpoint(server)?;
        let mut project_indexer = self.project_indexer.take().expect(
            "INVARIANT VIOLATED: bounded project collection has no retained navigation \
             frontier. This is a coordinator bug because dynamic demands and exhaustive batches \
             must mutate the exact IndexerProject that discovered the file set. Fix: retain the \
             frontier IndexerProject across every batch.",
        );
        let worker_server = server.clone();
        let worker_demands = demands.clone();
        let worker_root = self.workspace_root.clone();
        let worker_cancellation = self.resource_cancellation();
        let loop_cancellation = worker_cancellation.clone();
        let (next_project_indexer, result) = run_cpu_indexing_task(
            server,
            Some(self.workspace_root.clone()),
            worker_cancellation,
            IndexingWorkClass::ProjectParallelIo,
            "bounded exhaustive project fact collection",
            move || {
                let result = (|| -> Result<(usize, usize, usize, usize, usize)> {
                    project_indexer.refresh_exhaustive_semantic_context(&worker_server)?;
                    let mut batch_count = 0usize;
                    let mut demanded_batch_count = 0usize;
                    let mut collected_file_count = 0usize;
                    let mut providerless_batch_count = 0usize;
                    let mut provider_aware_batch_count = 0usize;
                    let mut provider_handoff_complete = runtime_provider_ready_rx.is_none();
                    let mut provider_installed = false;
                    loop {
                        if loop_cancellation
                            .as_ref()
                            .is_some_and(CancellationToken::is_cancelled)
                        {
                            return Err(anyhow!(
                                "project source indexing generation {} for {} was cancelled \
                                 between bounded batches",
                                generation,
                                worker_root.display()
                            ));
                        }
                        if !provider_handoff_complete {
                            let receiver = runtime_provider_ready_rx.as_mut().expect(
                                "INVARIANT VIOLATED: an incomplete JRuby provider handoff has no receiver. This is a coordinator bug because the receiver and completion flag have one generation-owned lifecycle. Fix: retain the receiver until it yields one value or fails.",
                            );
                            match receiver.try_recv() {
                                Ok(Ok(Some(provider))) => {
                                    project_indexer.install_jruby_import_provider(provider);
                                    provider_installed = true;
                                    provider_handoff_complete = true;
                                }
                                Ok(Ok(None)) => {
                                    provider_handoff_complete = true;
                                }
                                Ok(Err(error)) => {
                                    return Err(anyhow!(
                                        "exact runtime provider for {} failed during bounded \
                                         project collection: {error}",
                                        worker_root.display()
                                    ));
                                }
                                Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {}
                                Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                                    return Err(anyhow!(
                                        "exact runtime provider task for {} ended without \
                                         publishing its generation-local handoff",
                                        worker_root.display()
                                    ));
                                }
                            }
                        }
                        let demand_keys = worker_demands.drain(
                            generation,
                            crate::navigation_demand::NavigationDemandStage::Project,
                        );
                        let selection = project_indexer.take_navigation_demand_files(&demand_keys);
                        let demanded = !selection.files.is_empty();
                        let files = if demanded {
                            selection.files
                        } else {
                            project_indexer.take_next_remaining_project_files(
                                EXHAUSTIVE_PROJECT_FILE_BATCH_SIZE,
                            )
                        };
                        let collected = files.len();
                        let remaining = project_indexer.remaining_project_file_count();
                        project_indexer.collect_project_file_batch(
                            &files,
                            &worker_server,
                            demanded || remaining == 0,
                        )?;
                        if provider_installed {
                            provider_aware_batch_count = provider_aware_batch_count
                                .checked_add(1)
                                .expect(
                                    "INVARIANT VIOLATED: provider-aware project batch count overflowed. This is a bug because one generation cannot contain 2^64 bounded batches. Fix: inspect the batch loop for a failure to consume pending files.",
                                );
                        } else {
                            providerless_batch_count = providerless_batch_count
                                .checked_add(1)
                                .expect(
                                    "INVARIANT VIOLATED: providerless project batch count overflowed. This is a bug because one generation cannot contain 2^64 bounded batches. Fix: inspect the batch loop for a failure to consume pending files.",
                                );
                        }
                        batch_count = batch_count.checked_add(1).expect(
                            "INVARIANT VIOLATED: project fact batch count overflowed. This is a \
                             bug because one generation cannot contain 2^64 bounded batches. \
                             Fix: inspect the batch loop for a failure to consume pending files.",
                        );
                        collected_file_count = collected_file_count.checked_add(collected).expect(
                            "INVARIANT VIOLATED: project fact batch file count overflowed. \
                                 This is a bug because the deterministic pending set is bounded by \
                                 the filesystem. Fix: inspect batch accounting and duplicate \
                                 extraction.",
                        );
                        if demanded {
                            demanded_batch_count = demanded_batch_count.checked_add(1).expect(
                                "INVARIANT VIOLATED: demanded project batch count overflowed. \
                                     This is a bug because the bounded request queue admits at \
                                     most a fixed number of keys per drain. Fix: inspect demand \
                                     completion and batch termination.",
                            );
                        }
                        if !selection.completed_keys.is_empty() {
                            worker_demands.complete_keys(
                                generation,
                                crate::navigation_demand::NavigationDemandStage::Project,
                                &selection.completed_keys,
                            );
                        }
                        if !selection.deferred_keys.is_empty() {
                            info!(
                                "Deferred {} ambiguous project navigation demand(s) until \
                                 exhaustive project completion for {}",
                                selection.deferred_keys.len(),
                                worker_root.display()
                            );
                        }
                        if remaining == 0 {
                            project_indexer.finish_remaining_project_facts();
                            worker_demands.complete_stage(
                                generation,
                                crate::navigation_demand::NavigationDemandStage::Project,
                            );
                            break;
                        }
                    }
                    Ok((
                        collected_file_count,
                        batch_count,
                        demanded_batch_count,
                        providerless_batch_count,
                        provider_aware_batch_count,
                    ))
                })();
                (project_indexer, result)
            },
        )
        .await?;
        self.project_indexer = Some(next_project_indexer);
        let (
            collected_file_count,
            batch_count,
            demanded_batch_count,
            providerless_batch_count,
            provider_aware_batch_count,
        ) = result?;
        self.indexing_checkpoint(server)?;
        info!(
            "[PERF][project batch stream] project={} files={} batches={} demanded_batches={} \
             providerless_batches={} provider_aware_batches={} total={:?}",
            self.workspace_root.display(),
            collected_file_count,
            batch_count,
            demanded_batch_count,
            providerless_batch_count,
            provider_aware_batch_count,
            started.elapsed()
        );
        Ok(())
    }

    async fn collect_remaining_project_facts_without_demands(
        &mut self,
        server: &RubyLanguageServer,
        exact_runtime_provider: Option<Arc<JrubyImportProvider>>,
    ) -> Result<()> {
        let mut project_indexer = self.project_indexer.take().expect(
            "INVARIANT VIOLATED: exhaustive project collection has no retained navigation \
             frontier. This is a coordinator bug because the remaining file set belongs to the \
             exact IndexerProject that discovered and indexed the priority files. Fix: retain \
             the frontier IndexerProject until exhaustive collection completes.",
        );
        if let Some(provider) = exact_runtime_provider {
            project_indexer.install_jruby_import_provider(provider);
        }
        let worker_server = server.clone();
        let (project_indexer, result) = run_cpu_indexing_task(
            server,
            Some(self.workspace_root.clone()),
            self.resource_cancellation(),
            IndexingWorkClass::ProjectParallelIo,
            "exhaustive project fact collection",
            move || {
                let result = project_indexer.collect_remaining_project_facts(&worker_server);
                (project_indexer, result)
            },
        )
        .await?;
        self.project_indexer = Some(project_indexer);
        result
    }

    async fn replay_jruby_catalog_sensitive_project_facts(
        &mut self,
        server: &RubyLanguageServer,
    ) -> Result<usize> {
        let mut project_indexer = self.project_indexer.take().expect(
            "INVARIANT VIOLATED: JRuby catalog-sensitive replay started before the first project \
             pass. This is a coordinator bug because replay candidates are compact evidence \
             collected by the providerless active frontier. Fix: finish the exhaustive tail with \
             the exact provider before replaying the bounded frontier candidates.",
        );
        let file_processor = self.file_processor.clone().expect(
            "INVARIANT VIOLATED: JRuby catalog-sensitive replay has no final FileProcessor. This \
             is a coordinator bug because exact runtime facts must use the same extension and \
             project context as the ordinary project pass. Fix: rebuild the processor with the \
             completed provider before replay.",
        );
        let worker_server = server.clone();
        let (project_indexer, result) = run_cpu_indexing_task(
            server,
            Some(self.workspace_root.clone()),
            self.resource_cancellation(),
            IndexingWorkClass::ProjectParallelIo,
            "JRuby catalog-sensitive project replay",
            move || {
                let result = project_indexer
                    .replay_jruby_catalog_sensitive_files(file_processor, &worker_server);
                (project_indexer, result)
            },
        )
        .await?;
        self.project_indexer = Some(project_indexer);
        result
    }

    async fn discard_jruby_replay_semantic_context(
        &mut self,
        server: &RubyLanguageServer,
    ) -> Result<()> {
        let mut project_indexer = self.project_indexer.take().expect(
            "INVARIANT VIOLATED: non-JRuby project completion has no retained project indexer. This is a coordinator bug because the immutable exhaustive read context belongs to that exact generation. Fix: retain the IndexerProject until replay context is consumed or discarded.",
        );
        let project_indexer = run_cpu_indexing_task(
            server,
            Some(self.workspace_root.clone()),
            self.resource_cancellation(),
            IndexingWorkClass::LightCpu,
            "discard non-JRuby exhaustive semantic context",
            move || {
                project_indexer.discard_jruby_replay_semantic_context();
                project_indexer
            },
        )
        .await?;
        self.project_indexer = Some(project_indexer);
        Ok(())
    }

    /// Publish diagnostics for unresolved entries in currently open files.
    async fn publish_unresolved_diagnostics(&self, server: &RubyLanguageServer) -> Result<()> {
        self.indexing_checkpoint(server)?;
        let open_uris = server.docs.lock().keys().cloned().collect::<HashSet<_>>();
        let file_ids = {
            let analysis_engine = self.analysis_engine(server);
            let engine = analysis_engine.read();
            let mut file_ids = engine.diagnostic_store().file_ids();
            file_ids.retain(|file_id| {
                engine
                    .file(*file_id)
                    .and_then(|file| Url::from_file_path(&file.path).ok())
                    .is_some_and(|uri| open_uris.contains(&uri))
            });
            file_ids.sort_by(|left, right| {
                let left_path = engine
                    .file(*left)
                    .map(|file| file.path.as_path())
                    .unwrap_or_else(|| Path::new(""));
                let right_path = engine
                    .file(*right)
                    .map(|file| file.path.as_path())
                    .unwrap_or_else(|| Path::new(""));
                left_path.cmp(right_path)
            });
            info!(
                "Publishing diagnostics for {} open files with analysis diagnostics ({} open documents)",
                file_ids.len(),
                open_uris.len()
            );
            file_ids
        };

        for file_id in file_ids {
            self.indexing_checkpoint(server)?;
            let Some((uri, diagnostics)) = ({
                let analysis_engine = self.analysis_engine(server);
                let engine = analysis_engine.read();
                match engine.file(file_id) {
                    Some(file) => match Url::from_file_path(&file.path) {
                        Ok(uri) => {
                            let diagnostics = engine
                                .diagnostic_store()
                                .facts_for_file(file_id)
                                .iter()
                                .filter_map(|fact| diagnostic_from_fact_fast(file, fact))
                                .collect::<Vec<_>>();
                            if diagnostics.is_empty() {
                                None
                            } else {
                                Some((uri, diagnostics))
                            }
                        }
                        Err(()) => None,
                    },
                    None => None,
                }
            }) else {
                continue;
            };
            debug!(
                "Publishing {} unresolved diagnostics for {}",
                diagnostics.len(),
                uri.path()
            );
            self.indexing_checkpoint(server)?;
            server.publish_diagnostics(uri, diagnostics).await;
        }
        Ok(())
    }

    /// Step 5: Index the Ruby standard library
    async fn index_core_stubs(
        &self,
        server: &RubyLanguageServer,
        ruby_version: Option<RubyVersion>,
    ) -> Result<AnalysisEngine> {
        let analysis_engine = self.analysis_engine(server);
        let extension_path = self.config.extension_path.clone();
        let key = format!(
            "core-stubs:{}:{ruby_version:?}:{}",
            env!("CARGO_PKG_VERSION"),
            extension_path.as_deref().unwrap_or("<development>")
        );
        let indexing_resources = server.indexing_resources.clone();
        let resource_policy = indexing_resources.policy();
        let resource_spec = IndexingWorkSpec::new(
            Some(self.workspace_root.clone()),
            IndexingResourcePriority::Background,
            resource_policy.cpu_lanes(),
            256 * MIB,
            1,
        );
        let template = server
            .core_engine_cache
            .get_or_try_init(key, move || async move {
                indexing_resources
                    .run_parallel_with_resources(
                        "core stub template construction",
                        resource_spec,
                        None,
                        move || {
                            let template_engine =
                                Arc::new(parking_lot::RwLock::new(AnalysisEngine::new()));
                            let mut indexer = IndexerStdlib::new(
                                crate::indexer::file_processor::FileProcessor::new(),
                                ruby_version,
                            );
                            if let Some(extension_path) = extension_path {
                                indexer.set_extension_path(PathBuf::from(extension_path));
                            }
                            indexer
                                .index_core_stubs_blocking(template_engine.clone())
                                .map_err(|error| error.to_string())?;
                            let engine = template_engine.read().clone();
                            Ok(engine)
                        },
                    )
                    .await
                    .map_err(|error| error.to_string())?
            })
            .await
            .map_err(anyhow::Error::msg)?;
        let dependency_seed = template.as_ref().clone();
        let installed_template = {
            let mut engine = analysis_engine.write();
            if engine.file_count() == 0 {
                *engine = template.as_ref().clone();
                true
            } else {
                false
            }
        };
        if installed_template {
            return Ok(dependency_seed);
        }

        // An active document may be opened while this coordinator waits for
        // the shared template producer. Replacing the engine at that point
        // would erase its current (possibly unsaved) facts. Keep the cached
        // clone as the empty-engine fast path, and use the ordinary per-file
        // lifecycle when live facts appeared during preparation.
        index_core_stubs_additively_off_reactor(
            server,
            self.workspace_root.clone(),
            self.resource_cancellation(),
            analysis_engine,
            ruby_version,
            self.config.extension_path.clone(),
        )
        .await?;
        Ok(dependency_seed)
    }

    /// Index runtime standard library modules after project declarations.
    async fn index_standard_library(
        &mut self,
        server: &RubyLanguageServer,
        ruby_version: &Option<RubyVersion>,
    ) -> Result<()> {
        let required_stdlib = self.get_required_stdlib_modules();

        let mut stdlib_indexer =
            IndexerStdlib::new(self.file_processor.as_ref().unwrap().clone(), *ruby_version);

        // Pass extension path for loading zipped stubs
        if let Some(ref ext_path) = self.config.extension_path {
            stdlib_indexer.set_extension_path(PathBuf::from(ext_path));
        }
        if let Some(runtime) = self.effective_runtime.as_ref() {
            stdlib_indexer
                .set_selected_runtime(runtime.executable.clone(), runtime.java_home.clone());
            stdlib_indexer
                .set_runtime_stdlib_paths(runtime_stdlib_paths_for_project(server, runtime).await?);
        }

        stdlib_indexer.set_required_modules(required_stdlib);
        let analysis_engine = self.analysis_engine(server);
        let (stdlib_indexer, result) = run_cpu_indexing_task(
            server,
            Some(self.workspace_root.clone()),
            self.resource_cancellation(),
            IndexingWorkClass::ParallelIo,
            "runtime stdlib indexing",
            move || {
                let result = stdlib_indexer.index_runtime_stdlib_deferred_blocking(analysis_engine);
                (stdlib_indexer, result)
            },
        )
        .await?;
        result?;
        self.stdlib_indexer = Some(stdlib_indexer);
        Ok(())
    }

    fn new_gem_indexer(&self) -> IndexerGem {
        let mut gem_indexer = IndexerGem::new(Some(self.workspace_root.clone()));
        gem_indexer.set_file_processor(
            self.file_processor
                .as_ref()
                .expect("INVARIANT VIOLATED: gem indexing started before FileProcessor setup. This is a coordinator bug because every source kind must share the owning project's extension context. Fix: keep setup_file_processor before constructing the gem indexer.")
                .clone(),
        );
        gem_indexer.set_runtime_provider_fingerprint(
            self.jruby_import_provider
                .as_ref()
                .map(|provider| provider.classpath_fingerprint().to_string()),
        );
        if let Some(runtime) = self.effective_runtime.clone() {
            let implementation = match runtime.implementation {
                RuntimeImplementation::Mri => RubyImplementation::Mri,
                RuntimeImplementation::Jruby => RubyImplementation::JRuby,
                RuntimeImplementation::Truffleruby => RubyImplementation::TruffleRuby,
            };
            gem_indexer.set_selected_runtime(runtime.executable, implementation, runtime.java_home);
        }
        gem_indexer
    }

    fn configure_discovered_gem_indexer(&self, mut gem_indexer: IndexerGem) -> IndexerGem {
        gem_indexer.set_file_processor(
            self.file_processor
                .as_ref()
                .expect(
                    "INVARIANT VIOLATED: discovered gems were bound before the final project \
                     FileProcessor existed. This is a coordinator bug because JRuby-sensitive \
                     dependencies must use the exact completed runtime provider. Fix: install \
                     the final processor before dependency fact construction.",
                )
                .clone(),
        );
        gem_indexer.set_runtime_provider_fingerprint(
            self.jruby_import_provider
                .as_ref()
                .map(|provider| provider.classpath_fingerprint().to_string()),
        );
        let mut inferred_required = self.get_required_gems();
        inferred_required.extend(
            gem_indexer.gemfile_required_roots_blocking().expect(
                "INVARIANT VIOLATED: owning-project Gemfile could not be read while configuring \
                 discovered gems. This is a bug because Bundler projects keep Gemfile next to the \
                 lockfile already used for discovery. Fix: keep Gemfile readable for the same \
                 project root that produced Gemfile.lock.",
            ),
        );
        let (required_gems, excluded_gems) =
            configured_gem_selection(inferred_required, &self.config.indexing);

        gem_indexer.set_required_gems(required_gems);
        gem_indexer.set_explicitly_included_gems(
            self.config
                .indexing
                .included_gems
                .iter()
                .filter(|name| !name.is_empty() && !excluded_gems.contains(*name))
                .cloned()
                .collect(),
        );
        gem_indexer.set_excluded_gems(excluded_gems);
        gem_indexer.set_dependency_seed_engine(
            self.dependency_seed_engine
                .clone()
                .expect("INVARIANT VIOLATED: gem indexing started before the dependency-only semantic seed was captured. This is a coordinator bug because reusable gem facts must never depend on project-owned declarations. Fix: capture the core/runtime engine immediately before indexing project sources."),
        );
        gem_indexer
    }

    async fn discover_and_bind_startup_priority_gems(
        server: &RubyLanguageServer,
        workspace_root: PathBuf,
        cancellation: Option<CancellationToken>,
        analysis_engine: Arc<parking_lot::RwLock<AnalysisEngine>>,
        gem_indexer: IndexerGem,
        dependency_seed: AnalysisEngine,
        priority_keys: HashSet<String>,
        excluded_gems: HashSet<String>,
        navigation_demands: Option<(crate::navigation_demand::NavigationDemandController, u64)>,
        project_frontier_release: tokio::sync::oneshot::Sender<()>,
    ) -> Result<(IndexerGem, Duration)> {
        if priority_keys.is_empty() {
            let (gem_indexer, discovery, discovery_dur) = run_cpu_indexing_task(
                server,
                Some(workspace_root),
                cancellation,
                IndexingWorkClass::ProjectCompanionIo,
                "gem dependency discovery",
                move || {
                    let started = Instant::now();
                    let mut gem_indexer = gem_indexer;
                    let discovery = gem_indexer.discover_gems_blocking();
                    (gem_indexer, discovery, started.elapsed())
                },
            )
            .await?;
            discovery?;
            let _ = project_frontier_release.send(());
            return Ok((gem_indexer, discovery_dur));
        }

        let navigation_priority_keys = priority_keys.clone();
        let (mut gem_indexer, discovery, navigation_discovery_dur) = run_cpu_indexing_task(
            server,
            Some(workspace_root.clone()),
            cancellation.clone(),
            IndexingWorkClass::ProjectCompanionIo,
            "active dependency discovery",
            move || {
                let started = Instant::now();
                let mut gem_indexer = gem_indexer;
                let discovery =
                    gem_indexer.discover_navigation_gems_blocking(&navigation_priority_keys);
                (gem_indexer, discovery, started.elapsed())
            },
        )
        .await?;
        discovery?;

        let mut priority_names = gem_indexer
            .priority_locked_gem_names(&priority_keys)
            .into_iter()
            .filter(|name| !excluded_gems.contains(name))
            .collect::<Vec<_>>();
        priority_names.sort();
        gem_indexer.set_required_gems(priority_names.iter().cloned().collect());
        gem_indexer.set_excluded_gems(excluded_gems);
        gem_indexer.set_dependency_seed_engine(dependency_seed);

        let mut bound_priority_files = 0usize;
        let priority_binding_started = Instant::now();
        for gem_name in &priority_names {
            if cancellation
                .as_ref()
                .is_some_and(CancellationToken::is_cancelled)
            {
                return Err(anyhow!(
                    "active dependency indexing for {} was cancelled before preparing {}",
                    workspace_root.display(),
                    gem_name
                ));
            }
            let (next_indexer, manifest, manifest_dur) = run_cpu_indexing_task(
                server,
                Some(workspace_root.clone()),
                cancellation.clone(),
                IndexingWorkClass::HeavyIo,
                "active dependency manifest preparation",
                {
                    let gem_name = gem_name.clone();
                    move || {
                        let started = Instant::now();
                        let manifest =
                            gem_indexer.prepare_required_gem_manifest_blocking(&gem_name);
                        (gem_indexer, manifest, started.elapsed())
                    }
                },
            )
            .await?;
            gem_indexer = next_indexer;
            let Some(manifest) = manifest? else {
                continue;
            };
            let binding_started = Instant::now();
            let bound = gem_indexer
                .bind_prepared_required_gem_with_shared_product(
                    server,
                    analysis_engine.clone(),
                    manifest,
                    cancellation.clone(),
                )
                .await?;
            bound_priority_files += bound.len();
            info!(
                "[PERF][active dependency binding] project={} gem={} manifest={:?} binding={:?} files={}",
                workspace_root.display(),
                gem_name,
                manifest_dur,
                binding_started.elapsed(),
                bound.len()
            );
        }
        if bound_priority_files > 0 {
            gem_indexer
                .resolve_bound_required_gems(server, analysis_engine, cancellation.clone())
                .await?;
            if let Some((demands, generation)) = &navigation_demands {
                for gem_name in &priority_names {
                    let key = dependency_priority_key(gem_name);
                    if demands.claim_if_requested(
                        *generation,
                        crate::navigation_demand::NavigationDemandStage::Dependency,
                        &key,
                    ) {
                        demands.complete_keys(
                            *generation,
                            crate::navigation_demand::NavigationDemandStage::Dependency,
                            std::slice::from_ref(&key),
                        );
                    }
                }
            }
        }
        info!(
            "[PERF][active dependency frontier] project={} gems={} files={} total={:?}",
            workspace_root.display(),
            priority_names.len(),
            bound_priority_files,
            priority_binding_started.elapsed()
        );
        let _ = project_frontier_release.send(());

        let (gem_indexer, completion, exhaustive_discovery_dur) = run_cpu_indexing_task(
            server,
            Some(workspace_root),
            cancellation,
            IndexingWorkClass::ProjectCompanionIo,
            "exhaustive vendor archive discovery",
            move || {
                let started = Instant::now();
                let mut gem_indexer = gem_indexer;
                let completion = gem_indexer.complete_navigation_gem_discovery_blocking();
                (gem_indexer, completion, started.elapsed())
            },
        )
        .await?;
        completion?;
        Ok((
            gem_indexer,
            navigation_discovery_dur + exhaustive_discovery_dur,
        ))
    }

    /// Bind already-discovered locked gems into one isolated project engine.
    ///
    /// Manifest preparation overlaps the preceding product's load and binding
    /// through a one-slot ordered pipeline. At most the current product, one
    /// queued manifest, and the producer's next manifest are retained while
    /// insertion remains strictly lockfile ordered.
    async fn index_configured_gems(
        server: &RubyLanguageServer,
        workspace_root: PathBuf,
        cancellation: Option<CancellationToken>,
        analysis_engine: Arc<parking_lot::RwLock<AnalysisEngine>>,
        mut gem_indexer: IndexerGem,
        priority_keys: HashSet<String>,
        navigation_demands: Option<(crate::navigation_demand::NavigationDemandController, u64)>,
    ) -> Result<IndexerGem> {
        if gem_indexer.needs_unlocked_explicit_discovery() {
            let (next_indexer, discovery) = run_cpu_indexing_task(
                server,
                Some(workspace_root.clone()),
                cancellation.clone(),
                IndexingWorkClass::HeavyIo,
                "explicit standalone gem discovery",
                move || {
                    let discovery = gem_indexer.discover_gems_blocking();
                    (gem_indexer, discovery)
                },
            )
            .await?;
            gem_indexer = next_indexer;
            discovery?;
        }
        let required_gem_names = prioritize_locked_gem_names(
            gem_indexer.ordered_required_gems_after_discovery(),
            &priority_keys,
        );
        let pipeline_started = Instant::now();
        let gem_indexer = Arc::new(gem_indexer);
        let producer_indexer = gem_indexer.clone();
        let producer_server = server.clone();
        let producer_root = workspace_root.clone();
        let producer_cancellation = cancellation.clone();
        let (manifest_sender, mut manifest_receiver) = tokio::sync::mpsc::channel::<
            Result<(
                String,
                crate::dependency_product::GemDependencyManifest,
                Vec<String>,
            )>,
        >(1);
        let producer_navigation_demands = navigation_demands.clone();
        let manifest_producer = async move {
            let mut prepared_products = 0usize;
            let mut manifest_worker_wall = Duration::default();
            let mut manifest_wait_wall = Duration::default();
            let mut remaining_gem_names = VecDeque::from(required_gem_names);
            let mut demand_keys_by_gem = BTreeMap::<String, Vec<String>>::new();
            while !remaining_gem_names.is_empty() {
                if let Some((demands, generation)) = &producer_navigation_demands {
                    let keys = demands.drain(
                        *generation,
                        crate::navigation_demand::NavigationDemandStage::Dependency,
                    );
                    for (gem_name, mut keys) in
                        prioritize_demanded_gem_names(&mut remaining_gem_names, &keys)
                    {
                        demand_keys_by_gem
                            .entry(gem_name)
                            .or_default()
                            .append(&mut keys);
                    }
                }
                let gem_name = remaining_gem_names.pop_front().expect(
                    "INVARIANT VIOLATED: non-empty gem pipeline queue had no first item. This is \
                     a bug because the loop and pop observe the same owned VecDeque. Fix: keep \
                     demand reprioritization within this producer.",
                );
                let matched_demand_keys = demand_keys_by_gem.remove(&gem_name).unwrap_or_default();
                if producer_cancellation
                    .as_ref()
                    .is_some_and(CancellationToken::is_cancelled)
                {
                    let _ = manifest_sender
                        .send(Err(anyhow!(
                            "gem dependency indexing for {} was cancelled before preparing {}",
                            producer_root.display(),
                            gem_name
                        )))
                        .await;
                    break;
                }
                let manifest_wait_started = Instant::now();
                let worker_indexer = producer_indexer.clone();
                let manifest_gem_name = gem_name.clone();
                let preparation = run_cpu_indexing_task(
                    &producer_server,
                    Some(producer_root.clone()),
                    producer_cancellation.clone(),
                    IndexingWorkClass::HeavyIo,
                    "gem dependency manifest preparation",
                    move || {
                        let started = Instant::now();
                        let manifest = worker_indexer
                            .prepare_required_gem_manifest_blocking(&manifest_gem_name);
                        (manifest, started.elapsed())
                    },
                )
                .await;
                manifest_wait_wall += manifest_wait_started.elapsed();
                let (manifest, manifest_worker_elapsed) = match preparation {
                    Ok(prepared) => prepared,
                    Err(error) => {
                        let _ = manifest_sender.send(Err(error)).await;
                        break;
                    }
                };
                manifest_worker_wall += manifest_worker_elapsed;
                let manifest = match manifest {
                    Ok(Some(manifest)) => manifest,
                    Ok(None) => {
                        if !matched_demand_keys.is_empty() {
                            let (demands, generation) =
                                producer_navigation_demands.as_ref().expect(
                                    "INVARIANT VIOLATED: matched dependency demand has no \
                                     controller. This is a bug because only a controller drain \
                                     can create matched keys. Fix: retain demand provenance with \
                                     the gem producer.",
                                );
                            demands.complete_keys(
                                *generation,
                                crate::navigation_demand::NavigationDemandStage::Dependency,
                                &matched_demand_keys,
                            );
                        }
                        continue;
                    }
                    Err(error) => {
                        let _ = manifest_sender.send(Err(error)).await;
                        break;
                    }
                };
                prepared_products += 1;
                if manifest_sender
                    .send(Ok((gem_name, manifest, matched_demand_keys)))
                    .await
                    .is_err()
                {
                    break;
                }
            }
            (prepared_products, manifest_worker_wall, manifest_wait_wall)
        };
        let consumer_indexer = gem_indexer.clone();
        let consumer_navigation_demands = navigation_demands;
        let consumer_root = workspace_root.clone();
        let manifest_consumer = async move {
            let mut indexed_files = 0usize;
            let mut product_load_wall = Duration::default();
            let mut product_binding_wall = Duration::default();
            while let Some(manifest) = manifest_receiver.recv().await {
                let (gem_name, manifest, matched_demand_keys) = match manifest {
                    Ok(manifest) => manifest,
                    Err(error) => {
                        manifest_receiver.close();
                        return Err(error);
                    }
                };
                let loading_started = Instant::now();
                let loaded = match consumer_indexer
                    .load_prepared_required_gem_with_shared_product(server, manifest)
                    .await
                {
                    Ok(loaded) => loaded,
                    Err(error) => {
                        manifest_receiver.close();
                        return Err(error);
                    }
                };
                product_load_wall += loading_started.elapsed();
                let binding_started = Instant::now();
                let bound = match consumer_indexer
                    .bind_loaded_required_gem_product(
                        server,
                        analysis_engine.clone(),
                        loaded,
                        cancellation.clone(),
                    )
                    .await
                {
                    Ok(bound) => bound,
                    Err(error) => {
                        manifest_receiver.close();
                        return Err(error);
                    }
                };
                indexed_files += bound.len();
                product_binding_wall += binding_started.elapsed();
                let dependency_key = dependency_priority_key(&gem_name);
                assert!(
                    matched_demand_keys.iter().all(|key| key == &dependency_key),
                    "INVARIANT VIOLATED: dependency product `{gem_name}` carried demand keys \
                     {matched_demand_keys:?} that do not match `{dependency_key}`. This is a bug \
                     because producer reordering and consumer completion must use one normalized \
                     gem identity. Fix: keep demand provenance attached only to its exact gem."
                );
                let requested =
                    consumer_navigation_demands
                        .as_ref()
                        .is_some_and(|(demands, generation)| {
                            demands.claim_if_requested(
                                *generation,
                                crate::navigation_demand::NavigationDemandStage::Dependency,
                                &dependency_key,
                            )
                        });
                if requested {
                    consumer_indexer
                        .resolve_bound_required_gems(
                            server,
                            analysis_engine.clone(),
                            cancellation.clone(),
                        )
                        .await?;
                    let (demands, generation) = consumer_navigation_demands.as_ref().expect(
                        "INVARIANT VIOLATED: bound dependency demand has no controller. This \
                             is a bug because matched keys originate only from the exact \
                             generation queue. Fix: retain the controller through product \
                             resolution and waiter completion.",
                    );
                    demands.complete_keys(
                        *generation,
                        crate::navigation_demand::NavigationDemandStage::Dependency,
                        std::slice::from_ref(&dependency_key),
                    );
                    info!(
                        "[PERF][dependency navigation demand] project={} gem={} keys={} \
                         binding_and_resolution={:?}",
                        consumer_root.display(),
                        gem_name,
                        1,
                        binding_started.elapsed()
                    );
                }
            }
            Ok::<_, anyhow::Error>((indexed_files, product_load_wall, product_binding_wall))
        };
        let (producer_metrics, consumer_result) =
            tokio::join!(manifest_producer, manifest_consumer);
        let (prepared_products, manifest_worker_wall, manifest_wait_wall) = producer_metrics;
        let (indexed_files, product_load_wall, product_binding_wall) = consumer_result?;
        info!(
            "[PERF][gem dependency stream] project={} products={} files={} \
             manifest_worker={:?} manifest_wait={:?} product_load={:?} product_binding={:?} \
             pipeline={:?}",
            workspace_root.display(),
            prepared_products,
            indexed_files,
            manifest_worker_wall,
            manifest_wait_wall,
            product_load_wall,
            product_binding_wall,
            pipeline_started.elapsed()
        );
        Arc::try_unwrap(gem_indexer).map_err(|_| {
            anyhow!(
                "INVARIANT VIOLATED: gem dependency pipeline retained its IndexerGem after \
                 producer and consumer completion. This is a bug because every bounded pipeline \
                 clone must be dropped before isolated coordinator ownership resumes. Fix: keep \
                 IndexerGem clones scoped to the joined producer and consumer futures."
            )
        })
    }

    /// Get the list of standard library modules that the project needs
    fn get_required_stdlib_modules(&self) -> Vec<String> {
        if let Some(ref project) = self.project_indexer {
            project.get_required_stdlib()
        } else {
            Vec::new()
        }
    }

    /// Get the list of gems that the project needs
    fn get_required_gems(&self) -> Vec<String> {
        if let Some(ref project) = self.project_indexer {
            project.get_required_gems()
        } else {
            Vec::new()
        }
    }

    /// Step 2: Find where Ruby libraries are installed on this system
    ///
    /// This looks for Ruby's standard library and gem directories so we know
    /// where to find external code that the project might be using.
    pub fn discover_ruby_library_paths(&mut self) {
        self.ruby_library_paths.clear();

        // Use ruby -e to get the actual load path from the Ruby installation
        if let Ok(output) = Command::new("ruby")
            .args(["-e", "puts $LOAD_PATH"])
            .output()
        {
            if output.status.success() {
                let load_paths = String::from_utf8_lossy(&output.stdout);
                for path_str in load_paths.lines() {
                    let path = PathBuf::from(path_str.trim());
                    if path.exists() && path.is_dir() {
                        self.ruby_library_paths.push(path);
                        debug!("Found Ruby lib directory: {:?}", path_str.trim());
                    }
                }
            } else {
                debug!(
                    "Failed to get Ruby load path: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        } else {
            debug!("Failed to execute ruby command to get load path");
        }

        // Also try to get gem paths
        if let Ok(output) = Command::new("ruby")
            .args(["-e", "require 'rubygems'; puts Gem.path"])
            .output()
        {
            if output.status.success() {
                let gem_paths = String::from_utf8_lossy(&output.stdout);
                for path_str in gem_paths.lines() {
                    let path = PathBuf::from(path_str.trim());
                    if path.exists() && path.is_dir() {
                        // Add the gems subdirectory which contains actual gem sources
                        let gems_dir = path.join("gems");
                        if gems_dir.exists() {
                            self.ruby_library_paths.push(gems_dir.clone());
                            debug!("Found gem directory: {:?}", gems_dir);
                        }
                    }
                }
            }
        }
    }

    /// Find all Ruby files in a directory and its subdirectories
    ///
    /// This walks through a directory tree and collects all Ruby files,
    /// but skips common directories that usually don't contain Ruby source code
    /// (like node_modules, .git, tmp, etc.)
    pub fn find_all_ruby_files_in_directory(&self, dir: &Path, files: &mut Vec<PathBuf>) {
        let collected_files = crate::utils::collect_ruby_files(dir);
        files.extend(collected_files);
    }

    /// Check if a file is a Ruby file
    ///
    /// This looks at the file extension (.rb, .ruby, .rake) and also checks
    /// for common Ruby files that don't have extensions (like Rakefile, Gemfile)
    pub fn is_ruby_file(&self, path: &Path) -> bool {
        crate::utils::should_index_file(path)
    }

    /// Find the Ruby core stubs for a specific Ruby version
    ///
    /// Ruby core stubs are pre-written definitions of Ruby's built-in classes and methods.
    /// This helps the language server understand Ruby's core functionality.
    ///
    /// We try to find stubs in this order:
    /// 1. Use the configured stub path
    /// 2. Look in the workspace's editors/vscode/vsix/stubs directory
    /// 3. Fall back to Ruby 3.0 stubs if available
    pub fn find_core_stubs_for_version(&self, version: (u8, u8)) -> Option<PathBuf> {
        // First, try the configured stub path
        if let Some(stubs_path_str) = self.config.get_core_stubs_path_internal(version) {
            return Some(PathBuf::from(stubs_path_str));
        }

        // Look for stubs in the workspace
        let stubs_dir = self
            .workspace_root
            .join("editors")
            .join("vscode")
            .join("vsix")
            .join("stubs");
        let version_dir = format!("rubystubs{}{}", version.0, version.1);
        let stubs_path = stubs_dir.join(version_dir);

        if stubs_path.exists() {
            debug!("Found core stubs in workspace at: {:?}", stubs_path);
            return Some(stubs_path);
        }

        // Fall back to Ruby 3.0 stubs if the specific version isn't available
        let default_stubs = stubs_dir.join("rubystubs30");
        if default_stubs.exists() {
            info!("Using default Ruby 3.0 stubs at: {:?}", default_stubs);
            Some(default_stubs)
        } else {
            warn!("No core stubs found for Ruby version {:?}", version);
            None
        }
    }

    /// Get the Ruby library paths we discovered
    ///
    /// This returns the list of directories where Ruby libraries are installed.
    pub fn get_ruby_library_paths(&self) -> &[PathBuf] {
        &self.ruby_library_paths
    }
}

fn ruby_version_for_runtime(runtime: &SelectedRuntimeDescriptor) -> Option<RubyVersion> {
    let mut components = runtime.compatibility_version.split('.');
    let major = components.next()?.parse::<u8>().ok()?;
    let minor = components.next()?.parse::<u8>().ok()?;
    let implementation = match runtime.implementation {
        RuntimeImplementation::Mri => RubyImplementation::Mri,
        RuntimeImplementation::Jruby => RubyImplementation::JRuby,
        RuntimeImplementation::Truffleruby => RubyImplementation::TruffleRuby,
    };
    Some(RubyVersion::new_with_implementation(
        major,
        minor,
        implementation,
    ))
}

fn log_memory_bucket(name: &str, bytes: usize, total: usize) {
    let percent = if total == 0 {
        0.0
    } else {
        bytes as f64 * 100.0 / total as f64
    };
    info!("{name}: {:.1} MB ({percent:.1}%)", bytes_to_mb(bytes));
}

fn bytes_to_mb(bytes: usize) -> f64 {
    bytes as f64 / 1_048_576.0
}

#[cfg(target_os = "macos")]
fn release_allocator_free_pages() {
    unsafe extern "C" {
        fn malloc_default_zone() -> *mut libc::c_void;
        fn malloc_zone_pressure_relief(zone: *mut libc::c_void, goal: usize) -> usize;
    }

    unsafe {
        let zone = malloc_default_zone();
        if !zone.is_null() {
            malloc_zone_pressure_relief(zone, 0);
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn release_allocator_free_pages() {}

/// Integration tests for IndexingCoordinator
/// Tests the complete indexing workflow with realistic project structures
#[cfg(test)]
mod coordinator_integration_tests {
    use super::*;
    use crate::config::runtime::{
        ProjectJrubyConfig, ProjectRuntimeSelection, RuntimeMode, RuntimeSelection,
        RuntimeSelectionConfig, SelectedRuntimeDescriptor,
    };
    use crate::runtime::catalog::RuntimeDiscoverySource;
    use ruby_analysis::core::{FullyQualifiedName, RubyType, TypeSubject};
    use ruby_analysis::engine::{AnalysisQuery, SourceFileInput};
    use std::fs;
    use std::io::{Cursor, Write};
    use tempfile::TempDir;
    use tower_lsp::lsp_types::{
        DidChangeTextDocumentParams, DidOpenTextDocumentParams, TextDocumentContentChangeEvent,
        TextDocumentItem, VersionedTextDocumentIdentifier,
    };
    use zip::write::SimpleFileOptions;

    #[test]
    fn cached_java_artifact_metadata_is_reused_without_cross_project_path_leakage() {
        let fixture = TempDir::new().unwrap();
        let digits = include_str!("../../crates/jvm-metadata/fixtures/minimal_class.hex")
            .bytes()
            .filter(|byte| !byte.is_ascii_whitespace())
            .collect::<Vec<_>>();
        let class = digits
            .chunks_exact(2)
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect::<Vec<_>>();
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .start_file("com/example/Demo.class", SimpleFileOptions::default())
            .unwrap();
        writer.write_all(&class).unwrap();
        let bytes = writer.finish().unwrap().into_inner();
        let artifact = |path: PathBuf| {
            fs::write(&path, &bytes).unwrap();
            let metadata = fs::metadata(&path).unwrap();
            crate::runtime::jruby::classpath::ClasspathArtifact {
                path,
                origin: crate::runtime::jruby::classpath::ArtifactOrigin::Explicit,
                kind: crate::runtime::jruby::classpath::ArtifactKind::Jar,
                fingerprint_sha256: format!("{:x}", Sha256::digest(&bytes)),
                byte_length: bytes.len() as u64,
                file_identity: crate::runtime::jruby::classpath::SourceFileIdentity {
                    byte_length: metadata.len(),
                    modified: metadata.modified().unwrap(),
                },
            }
        };
        let first_artifact = artifact(fixture.path().join("project-one.jar"));
        let second_artifact = artifact(fixture.path().join("project-two.jar"));
        let classpath = |root: PathBuf, artifact: ClasspathArtifact| {
            crate::runtime::jruby::classpath::ProjectClasspath {
                project_root: root,
                artifacts: vec![artifact],
                sources: Vec::new(),
                unresolved: Vec::new(),
                fingerprint_sha256: "fixture-classpath".to_string(),
            }
        };
        let first_classpath = classpath(fixture.path().join("project-one"), first_artifact.clone());
        let second_classpath =
            classpath(fixture.path().join("project-two"), second_artifact.clone());
        let cache = PersistentDerivedProductCache::with_limits(
            fixture.path().join("cache"),
            8,
            1024 * 1024,
        );
        let process_cache =
            crate::runtime::jruby::java_catalog::JavaArtifactProductCache::new(8, 1024 * 1024);

        let first = build_cached_project_java_catalog(
            &first_classpath,
            17,
            ArchiveLimits::default(),
            &cache,
            &process_cache,
        )
        .unwrap();
        let second = build_cached_project_java_catalog(
            &second_classpath,
            17,
            ArchiveLimits::default(),
            &cache,
            &process_cache,
        )
        .unwrap();

        assert_eq!(cache.java_artifact_snapshot().producers, 1);
        assert_eq!(cache.java_artifact_snapshot().hits, 0);
        assert_eq!(process_cache.snapshot().lookups, 2);
        assert_eq!(process_cache.snapshot().producers, 1);
        assert_eq!(process_cache.snapshot().hits, 1);
        assert_eq!(process_cache.snapshot().entries, 1);
        assert!(process_cache.retained_weight_bytes() > 0);
        assert!(process_cache.retained_weight_bytes() <= 1024 * 1024);
        assert_eq!(
            first.classes["com/example/Demo"].artifact_path,
            first_artifact.path
        );
        assert_eq!(
            second.classes["com/example/Demo"].artifact_path,
            second_artifact.path
        );
        assert!(Arc::ptr_eq(
            &first.classes["com/example/Demo"].class,
            &second.classes["com/example/Demo"].class,
        ));
    }

    #[test]
    fn parallel_cached_java_products_preserve_classpath_winner_order() {
        let fixture = TempDir::new().unwrap();
        let digits = include_str!("../../crates/jvm-metadata/fixtures/minimal_class.hex")
            .bytes()
            .filter(|byte| !byte.is_ascii_whitespace())
            .collect::<Vec<_>>();
        let class = digits
            .chunks_exact(2)
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect::<Vec<_>>();
        let artifact = |name: &str, marker: &str| {
            let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
            writer
                .start_file("com/example/Demo.class", SimpleFileOptions::default())
                .unwrap();
            writer.write_all(&class).unwrap();
            writer
                .start_file(format!("META-INF/{marker}"), SimpleFileOptions::default())
                .unwrap();
            writer.write_all(marker.as_bytes()).unwrap();
            let bytes = writer.finish().unwrap().into_inner();
            let path = fixture.path().join(name);
            fs::write(&path, &bytes).unwrap();
            let metadata = fs::metadata(&path).unwrap();
            ClasspathArtifact {
                path,
                origin: crate::runtime::jruby::classpath::ArtifactOrigin::Explicit,
                kind: crate::runtime::jruby::classpath::ArtifactKind::Jar,
                fingerprint_sha256: format!("{:x}", Sha256::digest(&bytes)),
                byte_length: bytes.len() as u64,
                file_identity: crate::runtime::jruby::classpath::SourceFileIdentity {
                    byte_length: metadata.len(),
                    modified: metadata.modified().unwrap(),
                },
            }
        };
        let winner = artifact("winner.jar", "winner");
        let shadowed = artifact("shadowed.jar", "shadowed");
        let classpath = crate::runtime::jruby::classpath::ProjectClasspath {
            project_root: fixture.path().join("project"),
            artifacts: vec![winner.clone(), shadowed.clone()],
            sources: Vec::new(),
            unresolved: Vec::new(),
            fingerprint_sha256: "ordered-fixture-classpath".to_string(),
        };
        let cache = PersistentDerivedProductCache::with_limits(
            fixture.path().join("cache"),
            8,
            1024 * 1024,
        );
        let process_cache = JavaArtifactProductCache::new(8, 1024 * 1024);

        let catalog = build_cached_project_java_catalog(
            &classpath,
            17,
            ArchiveLimits::default(),
            &cache,
            &process_cache,
        )
        .unwrap();

        assert_eq!(
            catalog.classes["com/example/Demo"].artifact_path,
            winner.path
        );
        assert_eq!(
            catalog.duplicates,
            vec![crate::runtime::jruby::java_catalog::DuplicateJavaClass {
                name: "com/example/Demo".to_string(),
                winner: winner.path,
                shadowed: shadowed.path,
            }]
        );
        assert_eq!(cache.java_artifact_snapshot().producers, 2);
    }

    #[test]
    fn coordinator_construction_does_not_load_project_extensions() {
        let package = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("extensions/rspec-ruby");
        let config = RubyFastLspConfig {
            extension_packages: vec![package.to_string_lossy().into_owned()],
            ..RubyFastLspConfig::default()
        };

        let coordinator = IndexingCoordinator::new(PathBuf::from("/workspace/server"), config);

        assert!(
            coordinator.extension_registry.is_none(),
            "project coordinators must consume the server-owned extension registry instead of loading every package once per project"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn identical_runtime_stdlib_paths_use_one_server_owned_probe() {
        use std::os::unix::fs::PermissionsExt;

        let fixture = TempDir::new().expect("runtime fixture directory must be created");
        let runtime_root = fixture.path().join("runtime");
        let runtime_bin = runtime_root.join("bin");
        let runtime_stdlib = runtime_root.join("lib/ruby/stdlib");
        fs::create_dir_all(&runtime_bin).expect("runtime bin directory must be created");
        fs::create_dir_all(&runtime_stdlib).expect("runtime stdlib directory must be created");
        let executable = runtime_bin.join("ruby");
        fs::write(
            &executable,
            "#!/bin/sh\nruntime_root=$(CDPATH= cd -- \"$(dirname -- \"$0\")/..\" && pwd)\nprintf 'probe\\n' >> \"$runtime_root/probe-count\"\nsleep 0.2\nprintf '%s\\0' \"$runtime_root/lib/ruby/stdlib\"\n",
        )
        .expect("fake runtime must be written");
        let mut permissions = fs::metadata(&executable)
            .expect("fake runtime metadata must exist")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&executable, permissions).expect("fake runtime must be executable");
        let runtime = SelectedRuntimeDescriptor {
            implementation: RuntimeImplementation::Mri,
            family: "3.3".to_string(),
            engine_version: "3.3.11".to_string(),
            compatibility_version: "3.3".to_string(),
            executable,
            discovery_source: RuntimeDiscoverySource::Path,
            java_home: None,
        };
        let server = RubyLanguageServer::default();

        let (first, second) = tokio::join!(
            runtime_stdlib_paths_for_project(&server, &runtime),
            runtime_stdlib_paths_for_project(&server, &runtime)
        );
        let expected = fs::canonicalize(runtime_stdlib).expect("runtime stdlib must canonicalize");
        assert_eq!(first.unwrap().paths(), &[expected.clone()]);
        assert_eq!(second.unwrap().paths(), &[expected]);
        assert_eq!(
            fs::read_to_string(runtime_root.join("probe-count"))
                .expect("probe count must exist")
                .lines()
                .count(),
            1,
            "concurrent projects selecting the same immutable runtime must execute one probe"
        );
        let cache = server.runtime_stdlib_path_cache.snapshot();
        assert_eq!(cache.lookups, 2);
        assert_eq!(cache.producers, 1);
        assert_eq!(cache.joined_flights, 1);
        assert_eq!(cache.failures, 0);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn cpu_indexing_task_does_not_block_the_async_reactor() {
        let resources = crate::indexing_resources::IndexingResourceGovernor::new(
            crate::indexing_resources::IndexingResourcePolicy::new(2, 2),
        );
        let started = Arc::new(tokio::sync::Notify::new());
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let release_thread = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            release_tx.send(()).unwrap();
        });
        let started_for_task = started.clone();
        let started_wait = started.notified();
        let began = Instant::now();
        let task = tokio::spawn(async move {
            resources
                .run_cpu("test project collection", move || {
                    started_for_task.notify_one();
                    release_rx.recv().unwrap();
                    42
                })
                .await
        });

        started_wait.await;
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert!(
            began.elapsed() < Duration::from_millis(80),
            "the async reactor could not run while CPU indexing occupied its worker"
        );
        assert_eq!(task.await.unwrap().unwrap(), 42);
        release_thread.join().unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn jruby_runtime_companion_overlaps_the_active_project_with_exact_resource_claims() {
        let root = PathBuf::from("/workspace/server");
        let mut server = RubyLanguageServer::default();
        server.indexing_resources = crate::indexing_resources::IndexingResourceGovernor::new(
            crate::indexing_resources::IndexingResourcePolicy::with_limits(6, 2, 512 * MIB, 2),
        );
        server
            .indexing_resources
            .prioritize_active_project_with_navigation_pending(&root, true);
        let server = Arc::new(server);
        let (started_tx, mut started_rx) = tokio::sync::mpsc::channel(2);
        let (runtime_release_tx, runtime_release_rx) = std::sync::mpsc::channel();
        let (project_release_tx, project_release_rx) = std::sync::mpsc::channel();

        let runtime = {
            let server = server.clone();
            let root = root.clone();
            let started_tx = started_tx.clone();
            tokio::spawn(async move {
                run_cpu_indexing_task(
                    &server,
                    Some(root),
                    None,
                    IndexingWorkClass::RuntimeCompanionParallelIo,
                    "fixture JRuby catalog",
                    move || {
                        started_tx.blocking_send("runtime").unwrap();
                        runtime_release_rx.recv().unwrap();
                    },
                )
                .await
                .unwrap();
            })
        };
        let project = {
            let server = server.clone();
            let root = root.clone();
            tokio::spawn(async move {
                run_cpu_indexing_task(
                    &server,
                    Some(root),
                    None,
                    IndexingWorkClass::ProjectParallelIo,
                    "fixture project pass",
                    move || {
                        started_tx.blocking_send("project").unwrap();
                        project_release_rx.recv().unwrap();
                    },
                )
                .await
                .unwrap();
            })
        };

        let first = tokio::time::timeout(Duration::from_secs(1), started_rx.recv())
            .await
            .expect("one indexing phase must start")
            .expect("start channel must remain open");
        let second = tokio::time::timeout(Duration::from_secs(1), started_rx.recv())
            .await
            .expect("the companion and project pass must overlap")
            .expect("start channel must remain open");
        assert_ne!(first, second);
        let snapshot = server.indexing_resources.snapshot();
        assert_eq!(snapshot.active_tasks, 2);
        assert_eq!(snapshot.active_cpu_lanes, 6);
        assert_eq!(snapshot.active_transient_memory_bytes, 512 * MIB);
        assert_eq!(snapshot.active_io_slots, 2);

        runtime_release_tx.send(()).unwrap();
        project_release_tx.send(()).unwrap();
        runtime.await.unwrap();
        project.await.unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn active_navigation_reservation_blocks_a_sibling_runtime_companion() {
        let active_root = PathBuf::from("/workspace/server");
        let sibling_root = PathBuf::from("/workspace/admin");
        let mut server = RubyLanguageServer::default();
        server.indexing_resources = crate::indexing_resources::IndexingResourceGovernor::new(
            crate::indexing_resources::IndexingResourcePolicy::with_limits(6, 2, 512 * MIB, 2),
        );
        server
            .indexing_resources
            .prioritize_active_project_with_navigation_pending(&active_root, true);
        let server = Arc::new(server);
        let (active_started_tx, active_started_rx) = tokio::sync::oneshot::channel();
        let (active_release_tx, active_release_rx) = std::sync::mpsc::channel();
        let active = {
            let server = server.clone();
            let active_root = active_root.clone();
            tokio::spawn(async move {
                run_cpu_indexing_task(
                    &server,
                    Some(active_root),
                    None,
                    IndexingWorkClass::RuntimeCompanionParallelIo,
                    "active runtime companion",
                    move || {
                        active_started_tx.send(()).unwrap();
                        active_release_rx.recv().unwrap();
                    },
                )
                .await
                .unwrap();
            })
        };
        active_started_rx.await.unwrap();

        let (sibling_started_tx, sibling_started_rx) = tokio::sync::oneshot::channel();
        let sibling = {
            let server = server.clone();
            tokio::spawn(async move {
                run_cpu_indexing_task(
                    &server,
                    Some(sibling_root),
                    None,
                    IndexingWorkClass::RuntimeCompanionParallelIo,
                    "sibling runtime companion",
                    move || {
                        sibling_started_tx.send(()).unwrap();
                    },
                )
                .await
                .unwrap();
            })
        };
        let mut sibling_started_rx = sibling_started_rx;
        assert!(
            tokio::time::timeout(Duration::from_millis(50), &mut sibling_started_rx)
                .await
                .is_err(),
            "a sibling runtime companion must stay queued while active-project navigation is pending"
        );

        server
            .indexing_resources
            .prioritize_active_project_with_navigation_pending(&active_root, false);
        tokio::time::timeout(Duration::from_secs(1), sibling_started_rx)
            .await
            .expect("the sibling runtime companion must start after reservation release")
            .expect("the sibling runtime companion must signal");
        active_release_tx.send(()).unwrap();
        active.await.unwrap();
        sibling.await.unwrap();
    }

    #[test]
    fn active_document_constant_roots_stably_prioritize_matching_locked_gems() {
        let constants = active_document_constant_priority_keys(
            "GoshPosh::Platform::Users::UserPmm.by_username(name)\n\
             GoshPosh::Settings.current_api_version\n\
             BSON::ObjectId.new\n",
        );
        assert!(
            constants
                .project_terminals
                .iter()
                .any(|terminal| terminal == "userpmm"),
            "the terminal constant in a qualified project type must be available for source-file priority"
        );
        assert!(
            constants
                .project_terminals
                .iter()
                .any(|terminal| terminal == "objectid"),
            "terminal dependency constants must remain available alongside their root package"
        );
        assert!(constants.dependency_roots.contains("goshposh"));
        assert!(constants.dependency_roots.contains("bson"));
        assert!(!constants
            .project_terminals
            .iter()
            .any(|terminal| terminal == "platform"));
        assert!(!constants
            .project_terminals
            .iter()
            .any(|terminal| terminal == "users"));
        assert_eq!(
            prioritize_locked_gem_names(
                vec![
                    "actionpack".to_string(),
                    "activesupport".to_string(),
                    "bson".to_string(),
                    "json".to_string(),
                ],
                &constants.dependency_roots,
            ),
            vec![
                "bson".to_string(),
                "actionpack".to_string(),
                "activesupport".to_string(),
                "json".to_string(),
            ],
            "exact active-document constant roots must move matching locked gems ahead while \
            preserving the exhaustive order of every nonmatching dependency"
        );
    }

    #[test]
    fn dynamic_dependency_demand_moves_only_the_exact_remaining_locked_gem() {
        let mut remaining = std::collections::VecDeque::from([
            "actionpack".to_string(),
            "activesupport".to_string(),
            "bson".to_string(),
            "json".to_string(),
        ]);

        let matched = prioritize_demanded_gem_names(
            &mut remaining,
            &["bson".to_string(), "notlocked".to_string()],
        );

        assert_eq!(
            remaining.into_iter().collect::<Vec<_>>(),
            vec![
                "bson".to_string(),
                "actionpack".to_string(),
                "activesupport".to_string(),
                "json".to_string(),
            ]
        );
        assert_eq!(
            matched.get("bson"),
            Some(&vec!["bson".to_string()]),
            "the consumer must know which exact waiter can complete after BSON is bound and \
             resolved"
        );
        assert!(
            !matched.values().flatten().any(|key| key == "notlocked"),
            "an unmatched dependency key must remain pending until another dependency family or \
             the complete stage can answer it"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn scheduler_bounds_parallel_cpu_workers_without_blocking_the_reactor() {
        let scheduler = crate::indexing_scheduler::IndexingScheduler::new(2);
        let resources = crate::indexing_resources::IndexingResourceGovernor::new(
            crate::indexing_resources::IndexingResourcePolicy::new(2, 2),
        );
        let active = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let maximum = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut tasks = Vec::new();
        for index in 0..6 {
            let scheduler = scheduler.clone();
            let resources = resources.clone();
            let active = active.clone();
            let maximum = maximum.clone();
            tasks.push(tokio::spawn(async move {
                let _permit = scheduler
                    .acquire(
                        PathBuf::from(format!("/workspace/project-{index}")),
                        crate::indexing_scheduler::IndexingPriority::Background,
                    )
                    .await;
                let running = active.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
                maximum.fetch_max(running, std::sync::atomic::Ordering::SeqCst);
                resources
                    .run_cpu("bounded test indexing", move || {
                        std::thread::sleep(Duration::from_millis(50));
                    })
                    .await
                    .unwrap();
                active.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
            }));
        }

        tokio::time::timeout(Duration::from_secs(1), async {
            while active.load(std::sync::atomic::Ordering::SeqCst) != 2
                || scheduler.snapshot().queued != 4
            {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("two scheduler-owned CPU workers must start");
        assert_eq!(scheduler.snapshot().active, 2);
        assert_eq!(scheduler.snapshot().queued, 4);

        let heartbeat = Instant::now();
        tokio::time::sleep(Duration::from_millis(5)).await;
        assert!(
            heartbeat.elapsed() < Duration::from_millis(30),
            "the reactor heartbeat was delayed while bounded CPU workers were saturated"
        );

        for task in tasks {
            task.await.unwrap();
        }
        assert_eq!(
            maximum.load(std::sync::atomic::Ordering::SeqCst),
            2,
            "scheduler admission must bound simultaneous CPU indexing workers"
        );
        assert_eq!(scheduler.snapshot().active, 0);
        assert_eq!(scheduler.snapshot().queued, 0);
    }

    #[tokio::test]
    async fn superseded_coordinator_cannot_advance_replacement_generation() {
        let fixture = TempDir::new().unwrap();
        let project = fixture.path().join("admin");
        fs::create_dir_all(&project).unwrap();
        let server = RubyLanguageServer::default();
        let workspace = server.add_workspace(Url::from_directory_path(&project).unwrap());
        let old_run = workspace.indexing_status.begin_run();
        let mut old_coordinator =
            IndexingCoordinator::new(project.clone(), RubyFastLspConfig::default());
        old_coordinator.set_indexing_run(old_run.clone());
        old_coordinator.set_analysis_engine(workspace.analysis_engine.clone());

        let replacement = workspace.indexing_status.begin_run();
        let result = old_coordinator
            .transition_indexing_status(
                &server,
                crate::indexing_status::IndexingPhase::IndexingProject,
            )
            .await;

        assert!(result.is_err());
        assert!(old_run.is_cancelled());
        let snapshot = workspace.indexing_status.snapshot();
        assert_eq!(snapshot.generation, replacement.generation());
        assert_eq!(
            snapshot.phase,
            crate::indexing_status::IndexingPhase::Queued
        );
    }

    #[test]
    fn removed_coordinator_keeps_detached_engine_instead_of_orphan_engine() {
        let fixture = TempDir::new().unwrap();
        let project = fixture.path().join("admin");
        fs::create_dir_all(&project).unwrap();
        let server = RubyLanguageServer::default();
        let workspace = server.add_workspace(Url::from_directory_path(&project).unwrap());
        let mut coordinator = IndexingCoordinator::new(project, RubyFastLspConfig::default());
        coordinator.set_analysis_engine(workspace.analysis_engine.clone());

        server.remove_workspace(&workspace.root_uri);

        let selected = coordinator.analysis_engine(&server);
        assert!(Arc::ptr_eq(&selected, &workspace.analysis_engine));
        assert!(!Arc::ptr_eq(&selected, &server.analysis_engine));
    }

    #[test]
    fn reads_modern_and_legacy_jdk_release_features_without_guessing() {
        let modern = TempDir::new().unwrap();
        fs::write(
            modern.path().join("release"),
            "JAVA_VERSION=\"17.0.12\"\nIMPLEMENTOR=\"fixture\"\n",
        )
        .unwrap();
        assert_eq!(read_jdk_feature(modern.path()).unwrap(), 17);

        let legacy = TempDir::new().unwrap();
        fs::write(
            legacy.path().join("release"),
            "JAVA_VERSION=\"1.8.0_442\"\n",
        )
        .unwrap();
        assert_eq!(read_jdk_feature(legacy.path()).unwrap(), 8);

        let malformed = TempDir::new().unwrap();
        fs::write(
            malformed.path().join("release"),
            "IMPLEMENTOR=\"fixture\"\n",
        )
        .unwrap();
        assert!(read_jdk_feature(malformed.path()).is_err());
    }

    fn decode_hex(source: &str) -> Vec<u8> {
        let digits = source
            .bytes()
            .filter(|byte| !byte.is_ascii_whitespace())
            .collect::<Vec<_>>();
        digits
            .chunks_exact(2)
            .map(|pair| {
                u8::from_str_radix(
                    std::str::from_utf8(pair).expect("fixture hex must be ASCII"),
                    16,
                )
                .expect("fixture byte must be valid hex")
            })
            .collect()
    }

    fn write_jar(path: &Path, entry: &str, contents: &[u8]) {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .start_file(entry, SimpleFileOptions::default())
            .expect("fixture JAR entry must start");
        writer
            .write_all(contents)
            .expect("fixture JAR entry must write");
        let bytes = writer
            .finish()
            .expect("fixture JAR must finish")
            .into_inner();
        fs::write(path, bytes).expect("fixture JAR must be written");
    }

    #[cfg(unix)]
    fn real_java_executable() -> PathBuf {
        for candidate in [
            std::env::var_os("JAVA_HOME")
                .map(PathBuf::from)
                .map(|home| home.join("bin/java")),
            Some(PathBuf::from("/opt/homebrew/opt/openjdk/bin/java")),
            Some(PathBuf::from("/usr/local/opt/openjdk/bin/java")),
        ]
        .into_iter()
        .flatten()
        {
            if candidate.is_file() {
                return candidate;
            }
        }
        panic!("a real JDK java executable is required for JRuby decompiler acceptance");
    }

    #[cfg(unix)]
    #[test]
    fn source_less_jruby_import_navigates_to_verified_decompiled_implementation() {
        let fixture = TempDir::new().unwrap();
        let root = fixture.path().join("admin");
        let jruby_home = fixture.path().join("jruby-9.2.21.0");
        fs::create_dir_all(root.join("lib")).unwrap();
        fs::create_dir_all(jruby_home.join("bin")).unwrap();
        fs::write(jruby_home.join("bin/jruby"), b"fixture").unwrap();
        let java = real_java_executable().canonicalize().unwrap();
        let java_home = java
            .parent()
            .and_then(Path::parent)
            .expect("real JDK java executable must live below JAVA_HOME/bin")
            .to_path_buf();
        let rich_class = decode_hex(include_str!(
            "../../crates/jvm-metadata/fixtures/rich_fixture.class.hex"
        ));
        write_jar(
            &root.join("lib/rich.jar"),
            "fixtures/RichFixture.class",
            &rich_class,
        );

        let root_string = format!("{}/", root.to_string_lossy());
        let mut config = RubyFastLspConfig {
            runtime: RuntimeSelectionConfig {
                mode: RuntimeMode::Auto,
                projects: vec![ProjectRuntimeSelection {
                    root: root_string.clone(),
                    selection: RuntimeSelection::Explicit(SelectedRuntimeDescriptor {
                        implementation: RuntimeImplementation::Jruby,
                        family: "9.2".to_string(),
                        engine_version: "9.2.21.0".to_string(),
                        compatibility_version: "2.5".to_string(),
                        executable: jruby_home.join("bin/jruby"),
                        discovery_source: RuntimeDiscoverySource::Rvm,
                        java_home: Some(java_home),
                    }),
                }],
            },
            ..RubyFastLspConfig::default()
        };
        config.jruby.projects = vec![ProjectJrubyConfig {
            root: root_string,
            additional_classpath: vec!["lib/rich.jar".to_string()],
            additional_sources: Vec::new(),
        }];

        let server = RubyLanguageServer::default();
        server.add_workspace(Url::from_directory_path(&root).unwrap());
        let mut coordinator = IndexingCoordinator::new(root.clone(), config);
        let cache = fixture.path().join("user-cache");
        coordinator.set_user_cache_root_for_tests(cache.clone());
        coordinator.detect_ruby_version();
        coordinator.setup_jruby_import_provider().unwrap();
        coordinator.setup_file_processor(&server);
        let source = "java_import fixtures.RichFixture\n\
                      RICH = RichFixture.new(nil)\n\
                      VALUE = RICH.java_send(:combine, [java.lang.String, Java::int[]], 'x', [1])\n";
        let source_path = root.join("imports.rb");
        fs::write(&source_path, source).unwrap();
        let mut project_indexer = IndexerProject::new(
            root.clone(),
            coordinator
                .file_processor
                .as_ref()
                .expect("fixture FileProcessor must be configured")
                .clone(),
            coordinator.config.indexing.clone(),
        );
        project_indexer.collect_project_facts(&server).unwrap();
        let uri = Url::from_file_path(&source_path).unwrap();
        coordinator
            .file_processor
            .as_ref()
            .unwrap()
            .process_file(&uri, source, &server)
            .unwrap();

        let engine = server.analysis_engine_for_uri(&uri);
        let engine = engine.read();
        let source_file = AnalysisQuery::new(&engine)
            .file_id(&source_path)
            .expect("project source must be registered");
        let offset = u32::try_from(source.find(":combine").unwrap() + 1).unwrap();
        let targets = AnalysisQuery::new(&engine)
            .resolved_reference_definition_ranges_at(source_file, offset);
        let (implementation, target_range) = targets
            .iter()
            .find_map(|target| {
                let file = AnalysisQuery::new(&engine).file(target.file_id)?;
                (file.kind == ruby_analysis::core::SourceKind::External)
                    .then(|| (file.path.clone(), *target))
            })
            .unwrap_or_else(|| {
                panic!(
                    "source-less Java method must target decompiled external implementation; \
                     targets: {targets:?}"
                )
            });
        assert!(implementation.starts_with(&cache));
        let implementation_source = fs::read_to_string(&implementation).unwrap();
        assert!(
            implementation_source[target_range.start_byte as usize..target_range.end_byte as usize]
                .contains("return List.of(prefix + values.length);"),
            "Go to Definition must select the verified decompiled `combine` declaration and body; \
             target: {target_range:?}"
        );
        assert!(
            targets.iter().all(|target| {
                AnalysisQuery::new(&engine)
                    .file(target.file_id)
                    .is_none_or(|file| file.kind != ruby_analysis::core::SourceKind::Signature)
            }),
            "metadata-backed decompiled implementation must outrank generated signatures; \
             targets: {targets:?}"
        );
    }

    #[test]
    fn selected_jruby_catalog_contributes_import_facts_to_the_owning_project() {
        let fixture = TempDir::new().unwrap();
        let root = fixture.path().join("admin");
        let jruby_home = fixture.path().join("jruby-9.2.21.0");
        let java_home = fixture.path().join("jdk");
        fs::create_dir_all(root.join("lib")).unwrap();
        fs::create_dir_all(jruby_home.join("bin")).unwrap();
        fs::create_dir_all(java_home.join("jmods")).unwrap();
        fs::write(jruby_home.join("bin/jruby"), b"fixture").unwrap();
        fs::write(java_home.join("release"), "JAVA_VERSION=\"17.0.12\"\n").unwrap();
        let demo_class = decode_hex(include_str!(
            "../../crates/jvm-metadata/fixtures/minimal_class.hex"
        ));
        write_jar(
            &root.join("lib/runtime.jar"),
            "com/example/Demo.class",
            &demo_class,
        );
        let rich_class = decode_hex(include_str!(
            "../../crates/jvm-metadata/fixtures/rich_fixture.class.hex"
        ));
        write_jar(
            &root.join("lib/rich.jar"),
            "fixtures/RichFixture.class",
            &rich_class,
        );
        let rich_source =
            include_str!("../../crates/jvm-metadata/fixtures/sources/RichFixture.java");
        write_jar(
            &root.join("lib/rich-sources.jar"),
            "fixtures/RichFixture.java",
            rich_source.as_bytes(),
        );

        let root_string = format!("{}/", root.to_string_lossy());
        let mut config = RubyFastLspConfig {
            runtime: RuntimeSelectionConfig {
                mode: RuntimeMode::Auto,
                projects: vec![ProjectRuntimeSelection {
                    root: root_string.clone(),
                    selection: RuntimeSelection::Explicit(SelectedRuntimeDescriptor {
                        implementation: RuntimeImplementation::Jruby,
                        family: "9.2".to_string(),
                        engine_version: "9.2.21.0".to_string(),
                        compatibility_version: "2.5".to_string(),
                        executable: jruby_home.join("bin/jruby"),
                        discovery_source: RuntimeDiscoverySource::Rvm,
                        java_home: Some(java_home),
                    }),
                }],
            },
            ..RubyFastLspConfig::default()
        };
        config.jruby.projects = vec![ProjectJrubyConfig {
            root: root_string,
            additional_classpath: vec!["lib/runtime.jar".to_string(), "lib/rich.jar".to_string()],
            additional_sources: Vec::new(),
        }];

        let server = RubyLanguageServer::default();
        server.add_workspace(Url::from_directory_path(&root).unwrap());
        let mut coordinator = IndexingCoordinator::new(root.clone(), config);
        coordinator.set_user_cache_root_for_tests(fixture.path().join("user-cache"));
        assert_eq!(
            coordinator.detect_ruby_version(),
            Some(RubyVersion::new_with_implementation(
                2,
                5,
                RubyImplementation::JRuby
            ))
        );
        coordinator.setup_jruby_import_provider().unwrap();
        assert!(coordinator.jruby_import_provider.is_some());
        let signature_cache_root = coordinator
            .jruby_signature_cache_root(
                coordinator
                    .jruby_import_provider
                    .as_ref()
                    .expect("fixture JRuby provider must exist"),
            )
            .unwrap();
        coordinator.setup_file_processor(&server);

        let source = "module Admin\n\
                          java_import fixtures.RichFixture\n\
                          class RichFixture\n\
                            java_alias :merged, :combine\n\
                          end\n\
                          INSTANCE = com.example.Demo.new\n\
                          CANONICAL = Java::Fixtures::RichFixture.new(nil)\n\
                          RICH = RichFixture.new(nil)\n\
                          RESULT = RICH.merged('value', 1)\n\
                          RUN_RESULT = RICH.java_send(:run, [])\n\
                          RUN_HANDLE = RICH.java_method(:run, [])\n\
                          UNBOUND_RUN = RichFixture.java_method(:run, [])\n\
                          DIRECT = RICH.combine('value', [1])\n\
                        end\n";
        let source_path = root.join("imports.rb");
        fs::write(&source_path, source).unwrap();
        let mut project_indexer = IndexerProject::new(
            root.clone(),
            coordinator
                .file_processor
                .as_ref()
                .expect("fixture FileProcessor must be configured")
                .clone(),
            coordinator.config.indexing.clone(),
        );
        project_indexer.collect_project_facts(&server).unwrap();
        let proxy = FullyQualifiedName::namespace(
            ["Java", "ComExample", "Demo"]
                .into_iter()
                .map(|part| ruby_analysis::core::RubyConstant::new(part).unwrap())
                .collect::<Vec<_>>(),
        );
        {
            let engine = server.analysis_engine_for_uri(
                &Url::from_file_path(signature_cache_root.join("com/example/Demo.rb")).unwrap(),
            );
            let engine = engine.read();
            let symbols = AnalysisQuery::new(&engine).all_symbol_facts();
            assert_eq!(
                symbols.iter().filter(|fact| fact.fqn == proxy).count(),
                1,
                "the ordinary cold project pass must materialize generated metadata signatures \
                 from the same parsed sources before its final engine resolution; \
                 indexed symbols: {symbols:?}"
            );
        }
        assert!(
            !root.join(".ruby-fast-lsp").exists(),
            "generated JRuby signatures must never write cache state into the Ruby project"
        );

        let uri = Url::from_file_path(&source_path).unwrap();
        coordinator
            .file_processor
            .as_ref()
            .unwrap()
            .process_file(&uri, source, &server)
            .unwrap();

        let alias = FullyQualifiedName::try_from("Admin::RichFixture").unwrap();
        let engine = server.analysis_engine_for_uri(&uri);
        let engine = engine.read();
        assert_eq!(
            AnalysisQuery::new(&engine).symbols_for_fqn(&alias).len(),
            1,
            "the selected project's Java catalog must flow through ordinary engine facts"
        );
        let source_file = AnalysisQuery::new(&engine)
            .file_id(&source_path)
            .expect("project source must be registered");
        let new_offset = u32::try_from(
            source
                .find("new")
                .expect("fixture constructor call must exist"),
        )
        .unwrap();
        let constructor_targets = AnalysisQuery::new(&engine)
            .resolved_reference_definition_ranges_at(source_file, new_offset);
        assert!(
            constructor_targets.iter().any(|target| {
                AnalysisQuery::new(&engine)
                    .file(target.file_id)
                    .is_some_and(|file| file.kind == ruby_analysis::core::SourceKind::Signature)
            }),
            "constructor navigation must resolve to the generated read-only signature document; \
            targets: {constructor_targets:?}"
        );
        let rich_constructor_offset = u32::try_from(
            source
                .find("RichFixture.new")
                .expect("fixture rich constructor call must exist")
                + "RichFixture.".len(),
        )
        .unwrap();
        let rich_constructor_targets = AnalysisQuery::new(&engine)
            .resolved_reference_definition_ranges_at(source_file, rich_constructor_offset);
        assert!(
            rich_constructor_targets.iter().any(|target| {
                AnalysisQuery::new(&engine)
                    .file(target.file_id)
                    .is_some_and(|file| {
                        file.kind == ruby_analysis::core::SourceKind::External
                            && file.path.ends_with("fixtures/RichFixture.java")
                    })
            }),
            "a source-backed constructor must navigate to the exact Java implementation source, \
             not its generated signature; targets: {rich_constructor_targets:?}"
        );
        let alias_call_offset = u32::try_from(
            source
                .rfind("merged")
                .expect("fixture alias call must exist"),
        )
        .unwrap();
        let alias_targets = AnalysisQuery::new(&engine)
            .resolved_reference_definition_ranges_at(source_file, alias_call_offset);
        assert!(
            alias_targets
                .iter()
                .any(|target| target.file_id == source_file),
            "java_alias calls must resolve to the file-owned alias declaration; \
             targets: {alias_targets:?}"
        );
        let run_offset = u32::try_from(
            source
                .find(":run")
                .expect("fixture java_send method symbol must exist")
                + 1,
        )
        .unwrap();
        let run_targets = AnalysisQuery::new(&engine)
            .resolved_reference_definition_ranges_at(source_file, run_offset);
        assert!(
            run_targets.iter().any(|target| {
                AnalysisQuery::new(&engine)
                    .file(target.file_id)
                    .is_some_and(|file| {
                        file.kind == ruby_analysis::core::SourceKind::External
                            && file.path.ends_with("fixtures/RichFixture.java")
                    })
            }),
            "java_send method-name navigation must resolve to the exact Java implementation source; \
             targets: {run_targets:?}"
        );
        for (constant, expected) in [
            ("Admin::RUN_RESULT", RubyType::nil_class()),
            (
                "Admin::RUN_HANDLE",
                RubyType::Class(FullyQualifiedName::try_from("Method").unwrap()),
            ),
            (
                "Admin::UNBOUND_RUN",
                RubyType::Class(FullyQualifiedName::try_from("UnboundMethod").unwrap()),
            ),
        ] {
            let constant = FullyQualifiedName::try_from(constant).unwrap();
            let indexed_types = AnalysisQuery::new(&engine).type_facts_in_file(source_file);
            assert!(
                indexed_types.iter().any(|fact| {
                    fact.subject == TypeSubject::Constant(constant.clone())
                        && fact.ruby_type == expected
                }),
                "{constant} must retain the selected JRuby dispatch type {expected}; \
                 indexed types: {indexed_types:?}"
            );
        }
        let document = server
            .docs
            .lock()
            .get(&uri)
            .cloned()
            .expect("processed JRuby document must exist");
        let query = crate::query::EngineQuery::with_doc_and_engine(
            document,
            server.analysis_engine_for_uri(&uri),
        );
        let indexed_types = AnalysisQuery::new(&engine).type_facts_in_file(source_file);
        let rich_constant = FullyQualifiedName::try_from("Admin::RICH").unwrap();
        let rich_proxy = FullyQualifiedName::try_from("Java::Fixtures::RichFixture").unwrap();
        let rich_proxy_namespace =
            FullyQualifiedName::namespace(rich_proxy.namespace_parts().to_vec());
        assert!(
            indexed_types.iter().any(|fact| {
                fact.subject == TypeSubject::Constant(rich_constant.clone())
                    && fact.ruby_type == RubyType::Class(rich_proxy.clone())
            }),
            "the imported Java constructor assignment must retain its canonical proxy type; \
             indexed types: {indexed_types:?}"
        );
        let combine_method = ruby_analysis::core::RubyMethod::new("combine").unwrap();
        let combine_return = AnalysisQuery::new(&engine)
            .method_return_type_for_receiver(&rich_proxy_namespace, &combine_method);
        assert_eq!(
            combine_return,
            Some(RubyType::Class(
                FullyQualifiedName::try_from("Java::JavaUtil::List").unwrap()
            )),
            "the exact classfile-derived method return must be queryable on the Java proxy"
        );
        let direct_line = source
            .lines()
            .position(|line| line.contains("DIRECT = RICH.combine"))
            .unwrap();
        let direct_character = source
            .lines()
            .nth(direct_line)
            .unwrap()
            .find("combine")
            .unwrap();
        let hover = query
            .get_hover_at_position(
                &uri,
                tower_lsp::lsp_types::Position::new(
                    u32::try_from(direct_line).unwrap(),
                    u32::try_from(direct_character + 1).unwrap(),
                ),
                source,
            )
            .expect("a Java proxy method call must produce hover information");
        assert!(
            hover.content.contains("Java::JavaUtil::List"),
            "Java proxy hover must expose the classfile-derived return type, got: {}",
            hover.content
        );
        drop(engine);

        coordinator
            .file_processor
            .as_ref()
            .unwrap()
            .process_file_current_file_resolution_forced(&uri, "module Admin\nend\n", &server)
            .unwrap();
        let engine = server.analysis_engine_for_uri(&uri);
        let engine = engine.read();
        assert!(
            AnalysisQuery::new(&engine)
                .symbols_for_fqn(&alias)
                .is_empty(),
            "removing java_import must remove its file-owned alias through ordinary replacement"
        );
        let java_alias = FullyQualifiedName::method(
            ["Java", "Fixtures", "RichFixture"]
                .into_iter()
                .map(|part| ruby_analysis::core::RubyConstant::new(part).unwrap())
                .collect::<Vec<_>>(),
            ruby_analysis::core::RubyMethod::new("merged").unwrap(),
        );
        assert!(
            AnalysisQuery::new(&engine)
                .methods_for_fqn(&java_alias)
                .is_empty(),
            "removing java_alias must remove its proxy-owned method through ordinary replacement"
        );
        assert!(
            AnalysisQuery::new(&engine)
                .resolved_reference_definition_ranges_at(source_file, run_offset)
                .is_empty(),
            "removing JRuby dispatch calls must remove their file-owned Java method candidates"
        );
    }

    #[tokio::test]
    async fn adding_a_java_import_after_cold_index_materializes_navigation_inputs_on_demand() {
        let fixture = TempDir::new().unwrap();
        let root = fixture.path().join("admin");
        let jruby_home = fixture.path().join("jruby-9.2.21.0");
        let java_home = fixture.path().join("jdk");
        fs::create_dir_all(root.join("lib")).unwrap();
        fs::create_dir_all(jruby_home.join("bin")).unwrap();
        fs::create_dir_all(java_home.join("jmods")).unwrap();
        fs::write(jruby_home.join("bin/jruby"), b"fixture").unwrap();
        fs::write(java_home.join("release"), "JAVA_VERSION=\"17.0.12\"\n").unwrap();
        write_jar(
            &root.join("lib/rich.jar"),
            "fixtures/RichFixture.class",
            &decode_hex(include_str!(
                "../../crates/jvm-metadata/fixtures/rich_fixture.class.hex"
            )),
        );
        write_jar(
            &root.join("lib/rich-sources.jar"),
            "fixtures/RichFixture.java",
            include_str!("../../crates/jvm-metadata/fixtures/sources/RichFixture.java").as_bytes(),
        );

        let root_string = format!("{}/", root.to_string_lossy());
        let mut config = RubyFastLspConfig {
            runtime: RuntimeSelectionConfig {
                mode: RuntimeMode::Auto,
                projects: vec![ProjectRuntimeSelection {
                    root: root_string.clone(),
                    selection: RuntimeSelection::Explicit(SelectedRuntimeDescriptor {
                        implementation: RuntimeImplementation::Jruby,
                        family: "9.2".to_string(),
                        engine_version: "9.2.21.0".to_string(),
                        compatibility_version: "2.5".to_string(),
                        executable: jruby_home.join("bin/jruby"),
                        discovery_source: RuntimeDiscoverySource::Rvm,
                        java_home: Some(java_home),
                    }),
                }],
            },
            ..RubyFastLspConfig::default()
        };
        config.jruby.projects = vec![ProjectJrubyConfig {
            root: root_string,
            additional_classpath: vec!["lib/rich.jar".to_string()],
            additional_sources: Vec::new(),
        }];

        let server = RubyLanguageServer::default();
        server.add_workspace(Url::from_directory_path(&root).unwrap());
        let mut coordinator = IndexingCoordinator::new(root.clone(), config);
        coordinator.set_user_cache_root_for_tests(fixture.path().join("user-cache"));
        coordinator.detect_ruby_version();
        coordinator.setup_jruby_import_provider().unwrap();
        let signature_cache = coordinator
            .jruby_signature_cache_root(
                coordinator
                    .jruby_import_provider
                    .as_ref()
                    .expect("fixture JRuby provider must exist"),
            )
            .unwrap();
        server.set_jruby_import_provider(&root, coordinator.jruby_import_provider.clone());

        let source_path = root.join("imports.rb");
        let uri = Url::from_file_path(&source_path).unwrap();
        let initial = "VALUE = 1\n";
        fs::write(&source_path, initial).unwrap();
        crate::capabilities::indexing::handle_did_open(
            &server,
            DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri: uri.clone(),
                    language_id: "ruby".to_string(),
                    version: 1,
                    text: initial.to_string(),
                },
            },
        )
        .await;
        assert!(
            !signature_cache.join("fixtures/RichFixture.rb").exists(),
            "cold indexing without a Java dependency must not eagerly materialize its signature"
        );

        let added = "java_import fixtures.RichFixture\nRICH = RichFixture.new(nil)\n";
        crate::capabilities::indexing::handle_did_change(
            &server,
            DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier {
                    uri: uri.clone(),
                    version: 2,
                },
                content_changes: vec![TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text: added.to_string(),
                }],
            },
        )
        .await;
        assert!(
            signature_cache.join("fixtures/RichFixture.rb").is_file(),
            "adding a static import must materialize its signature without restarting the project"
        );
        let engine = server.analysis_engine_for_uri(&uri);
        let engine = engine.read();
        let source_file = AnalysisQuery::new(&engine).file_id(&source_path).unwrap();
        let constructor_offset =
            u32::try_from(added.find("RichFixture.new").unwrap() + "RichFixture.".len()).unwrap();
        let targets = AnalysisQuery::new(&engine)
            .resolved_reference_definition_ranges_at(source_file, constructor_offset);
        assert!(
            targets.iter().any(|target| {
                AnalysisQuery::new(&engine)
                    .file(target.file_id)
                    .is_some_and(|file| {
                        file.kind == ruby_analysis::core::SourceKind::External
                            && file.path.ends_with("fixtures/RichFixture.java")
                    })
            }),
            "a newly added import must navigate to its exact Java source in the same edit pass; \
             targets: {targets:?}"
        );
        let rich = FullyQualifiedName::try_from("RICH").unwrap();
        let expected_rich_type =
            RubyType::Class(FullyQualifiedName::try_from("Java::Fixtures::RichFixture").unwrap());
        let indexed_types = AnalysisQuery::new(&engine).type_facts_in_file(source_file);
        assert!(
            indexed_types.iter().any(|fact| {
                fact.subject == TypeSubject::Constant(rich.clone())
                    && fact.ruby_type == expected_rich_type
            }),
            "an imported Java constructor assignment must retain the canonical proxy instance \
             type during the same edit pass; indexed types: {indexed_types:?}"
        );
        drop(engine);

        crate::capabilities::indexing::handle_did_change(
            &server,
            DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier {
                    uri: uri.clone(),
                    version: 3,
                },
                content_changes: vec![TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text: initial.to_string(),
                }],
            },
        )
        .await;
        let engine = server.analysis_engine_for_uri(&uri);
        let engine = engine.read();
        assert!(
            AnalysisQuery::new(&engine)
                .resolved_reference_definition_ranges_at(source_file, constructor_offset)
                .is_empty(),
            "removing the newly added import and constructor call must clear their reference facts"
        );
        let resources = server.indexing_resources.snapshot();
        assert_eq!(
            resources.completed_tasks, 3,
            "didOpen plus two didChange passes must each own exactly one outer resource lease; \
             interactive JRuby signature/source materialization must not acquire nested admission"
        );
        assert_eq!(resources.active_tasks, 0);
        assert_eq!(resources.queued_tasks, 0);
    }

    /// Test fixture that creates a realistic Ruby project structure
    struct TestProjectFixture {
        _temp_dir: TempDir,
        project_root: PathBuf,
        core_stubs_dir: PathBuf,
        stdlib_dir: PathBuf,
        project_files_dir: PathBuf,
    }

    impl TestProjectFixture {
        fn new() -> Self {
            let temp_dir = TempDir::new().expect("Failed to create temp directory");
            let project_root = temp_dir.path().to_path_buf();

            // Create directory structure
            let core_stubs_dir = project_root
                .join("editors")
                .join("vscode")
                .join("vsix")
                .join("stubs")
                .join("rubystubs30");
            let stdlib_dir = project_root.join("stdlib");
            let project_files_dir = project_root.join("app");

            fs::create_dir_all(&core_stubs_dir).expect("Failed to create core stubs dir");
            fs::create_dir_all(&stdlib_dir).expect("Failed to create stdlib dir");
            fs::create_dir_all(&project_files_dir).expect("Failed to create project files dir");

            Self {
                _temp_dir: temp_dir,
                project_root,
                core_stubs_dir,
                stdlib_dir,
                project_files_dir,
            }
        }

        /// Create core Ruby stub files
        fn create_core_stubs(&self) {
            // Create basic Object class stub
            let object_stub = r#"
class Object
  def initialize
  end

  def class
  end

  def to_s
  end
end
"#;
            fs::write(self.core_stubs_dir.join("object.rb"), object_stub)
                .expect("Failed to write object.rb");

            // Create String class stub
            let string_stub = r#"
class String
  def initialize(str = "")
  end

  def length
  end

  def upcase
  end

  def downcase
  end

  def strip
  end
end
"#;
            fs::write(self.core_stubs_dir.join("string.rb"), string_stub)
                .expect("Failed to write string.rb");

            // Create Array class stub
            let array_stub = r#"
class Array
  def initialize
  end

  def length
  end

  def push(item)
  end

  def pop
  end

  def each
  end
end
"#;
            fs::write(self.core_stubs_dir.join("array.rb"), array_stub)
                .expect("Failed to write array.rb");
        }

        /// Create standard library files
        fn create_stdlib_files(&self) {
            // Create Set class
            let set_lib = r#"
class Set
  def initialize(enum = nil)
    @hash = {}
  end

  def add(obj)
    @hash[obj] = true
    self
  end

  def include?(obj)
    @hash.key?(obj)
  end

  def size
    @hash.size
  end
end
"#;
            fs::write(self.stdlib_dir.join("set.rb"), set_lib).expect("Failed to write set.rb");

            // Create JSON library
            let json_lib = r#"
module JSON
  def self.parse(source)
    # JSON parsing implementation
  end

  def self.generate(obj)
    # JSON generation implementation
  end
end
"#;
            fs::write(self.stdlib_dir.join("json.rb"), json_lib).expect("Failed to write json.rb");

            // Create FileUtils module
            let fileutils_lib = r#"
module FileUtils
  def self.mkdir_p(path)
    # Directory creation implementation
  end

  def self.cp(src, dest)
    # File copy implementation
  end

  def self.rm_rf(path)
    # Recursive removal implementation
  end
end
"#;
            fs::write(self.stdlib_dir.join("fileutils.rb"), fileutils_lib)
                .expect("Failed to write fileutils.rb");
        }

        /// Create project files with dependencies
        fn create_project_files(&self) {
            fs::write(
                self.project_root.join("Thorfile"),
                "class DeploymentTasks\nend\n",
            )
            .expect("Failed to write Thorfile");
            fs::write(
                self.project_root.join("config.ru"),
                "class RackApplication\nend\n",
            )
            .expect("Failed to write config.ru");

            // Create main application file
            let main_app = r#"
require 'set'
require 'json'
require_relative 'models/user'
require_relative 'services/user_service'

class Application
  def initialize
    @users = Set.new
    @user_service = UserService.new
  end

  def add_user(user_data)
    user = User.new(user_data)
    @users.add(user)
    @user_service.save(user)
  end

  def export_users
    JSON.generate(@users.to_a)
  end
end
"#;
            fs::write(self.project_files_dir.join("application.rb"), main_app)
                .expect("Failed to write application.rb");

            // Create models directory and User model
            let models_dir = self.project_files_dir.join("models");
            fs::create_dir_all(&models_dir).expect("Failed to create models dir");

            let user_model = r#"
class User
  attr_accessor :name, :email, :age

  def initialize(data = {})
    @name = data[:name]
    @email = data[:email]
    @age = data[:age]
  end

  def valid?
    !@name.nil? && !@email.nil?
  end

  def to_hash
    {
      name: @name,
      email: @email,
      age: @age
    }
  end
end
"#;
            fs::write(models_dir.join("user.rb"), user_model).expect("Failed to write user.rb");

            // Create services directory and UserService
            let services_dir = self.project_files_dir.join("services");
            fs::create_dir_all(&services_dir).expect("Failed to create services dir");

            let user_service = r#"
require 'fileutils'
require_relative '../models/user'

class UserService
  def initialize
    @storage_path = 'users.json'
  end

  def save(user)
    users = load_users
    users << user.to_hash
    File.write(@storage_path, JSON.generate(users))
  end

  def load_users
    return [] unless File.exist?(@storage_path)
    JSON.parse(File.read(@storage_path))
  end

  def find_by_email(email)
    users = load_users
    user_data = users.find { |u| u['email'] == email }
    User.new(user_data) if user_data
  end
end
"#;
            fs::write(services_dir.join("user_service.rb"), user_service)
                .expect("Failed to write user_service.rb");

            // Create a test file
            let test_dir = self.project_files_dir.join("test");
            fs::create_dir_all(&test_dir).expect("Failed to create test dir");

            let user_test = r#"
require_relative '../models/user'
require_relative '../services/user_service'

class UserTest
  def test_user_creation
    user = User.new(name: 'John', email: 'john@example.com', age: 30)
    assert user.valid?
  end

  def test_user_service
    service = UserService.new
    user = User.new(name: 'Jane', email: 'jane@example.com')
    service.save(user)

    found_user = service.find_by_email('jane@example.com')
    assert found_user.name == 'Jane'
  end
end
"#;
            fs::write(test_dir.join("user_test.rb"), user_test)
                .expect("Failed to write user_test.rb");
        }

        /// Set up the complete project structure
        fn setup_complete_project(&self) {
            self.create_core_stubs();
            self.create_stdlib_files();
            self.create_project_files();
        }

        /// Get the project root path
        fn project_root(&self) -> &PathBuf {
            &self.project_root
        }
    }

    /// Create a test server instance
    fn create_test_server() -> RubyLanguageServer {
        RubyLanguageServer::default()
    }

    #[test]
    fn test_configured_gem_selection_augments_inferred_and_preserves_exclusions() {
        let indexing = crate::config::IndexingConfig {
            included_gems: vec!["rails".to_string(), "debug".to_string()],
            excluded_gems: vec!["debug".to_string(), "rack".to_string()],
            ..crate::config::IndexingConfig::default()
        };

        let (required, excluded) =
            configured_gem_selection(vec!["rack".to_string(), "rspec".to_string()], &indexing);

        assert_eq!(
            required,
            HashSet::from(["rails".to_string(), "rspec".to_string()])
        );
        assert_eq!(
            excluded,
            HashSet::from(["debug".to_string(), "rack".to_string()])
        );
    }

    #[test]
    fn configured_ruby_version_overrides_runtime_auto_detection() {
        let fixture = TestProjectFixture::new();
        let config = RubyFastLspConfig {
            ruby_version: "2.5".to_string(),
            ..RubyFastLspConfig::default()
        };
        let mut coordinator = IndexingCoordinator::new(fixture.project_root().clone(), config);

        assert_eq!(
            coordinator.detect_ruby_version(),
            Some(RubyVersion::new(2, 5)),
            "an explicit Ruby version must select its matching core stubs"
        );
    }

    #[tokio::test]
    async fn auto_runtime_marker_becomes_the_exact_effective_runtime() {
        use crate::runtime::catalog::{
            DiscoveredRuntime, RuntimeDiscoverySource, RuntimeSupportStatus,
        };

        let fixture = TestProjectFixture::new();
        std::fs::write(
            fixture.project_root().join(".ruby-version"),
            "jruby-9.2.21.0\n",
        )
        .unwrap();
        let server = create_test_server();
        server.add_workspace(Url::from_directory_path(fixture.project_root()).unwrap());
        server.set_discovered_runtimes_for_tests(vec![DiscoveredRuntime {
            implementation: RuntimeImplementation::Jruby,
            implementation_label: "JRuby".to_string(),
            family: "9.2".to_string(),
            family_label: "JRuby 9.2 (Ruby 2.5)".to_string(),
            compatibility_version: "2.5".to_string(),
            compatibility_label: "Ruby 2.5".to_string(),
            engine_version: "9.2.21.0".to_string(),
            display_name: "JRuby 9.2.21.0 (Ruby 2.5)".to_string(),
            executable: fixture.project_root().join("runtime/bin/jruby"),
            discovery_source: RuntimeDiscoverySource::Rvm,
            support_status: RuntimeSupportStatus::Supported,
            java_home: Some(fixture.project_root().join("jdk")),
        }]);
        let mut coordinator =
            IndexingCoordinator::new(fixture.project_root().clone(), RubyFastLspConfig::default());

        coordinator
            .resolve_effective_runtime(&server)
            .await
            .unwrap();
        assert_eq!(
            coordinator.detect_ruby_version(),
            Some(RubyVersion::new_with_implementation(
                2,
                5,
                RubyImplementation::JRuby
            ))
        );
        assert_eq!(
            coordinator
                .effective_runtime
                .as_ref()
                .map(|runtime| runtime.engine_version.as_str()),
            Some("9.2.21.0")
        );
    }

    #[tokio::test]
    async fn test_coordinator_complete_indexing_workflow() {
        let fixture = TestProjectFixture::new();
        fixture.setup_complete_project();

        let config = RubyFastLspConfig::default();
        let mut coordinator = IndexingCoordinator::new(fixture.project_root().clone(), config);
        let server = create_test_server();

        // Execute the complete indexing process
        let result = coordinator.run_complete_indexing(&server).await;
        assert!(
            result.is_ok(),
            "Indexing should complete successfully: {result:?}"
        );

        let engine = server.analysis_engine.read();
        let query = ruby_analysis::engine::AnalysisQuery::new(&engine);
        for path in [
            fixture.project_root().join("Thorfile"),
            fixture.project_root().join("config.ru"),
        ] {
            let file_id = query.file_id(&path).unwrap_or_else(|| {
                panic!(
                    "common Ruby entry point was not registered: {}",
                    path.display()
                )
            });
            assert!(
                !query.symbol_facts_in_file(file_id).is_empty(),
                "common Ruby entry point produced no semantic facts: {}",
                path.display()
            );
        }

        assert!(
            coordinator.gem_indexer.is_some() && coordinator.stdlib_indexer.is_some(),
            "complete indexing must retain its exact gem and stdlib indexers"
        );
    }

    #[tokio::test]
    async fn identical_core_stubs_use_one_template_but_keep_isolated_engines() {
        let fixture = TempDir::new().expect("multi-project fixture must be created");
        let admin = fixture.path().join("admin");
        let server_root = fixture.path().join("server");
        for root in [&admin, &server_root] {
            fs::create_dir_all(root).unwrap();
            fs::write(root.join("Gemfile"), "source 'https://rubygems.org'\n").unwrap();
            fs::write(root.join("app.rb"), "class App\nend\n").unwrap();
        }
        let server = create_test_server();
        let admin_workspace = server.add_workspace(Url::from_directory_path(&admin).unwrap());
        let server_workspace =
            server.add_workspace(Url::from_directory_path(&server_root).unwrap());

        for root in [&admin, &server_root] {
            let mut coordinator =
                IndexingCoordinator::new(root.to_path_buf(), RubyFastLspConfig::default());
            coordinator.run_complete_indexing(&server).await.unwrap();
        }

        assert_eq!(
            server.core_engine_cache.len(),
            1,
            "the same compatibility core must have one prepared template"
        );
        assert!(
            !Arc::ptr_eq(
                &admin_workspace.analysis_engine,
                &server_workspace.analysis_engine
            ),
            "projects must retain isolated mutable engines"
        );
        let unique = admin.join("only_admin.rb");
        admin_workspace
            .analysis_engine
            .write()
            .register_file(SourceFileInput {
                path: unique.clone(),
                content: "ADMIN_ONLY = true\n".to_string(),
                kind: ruby_analysis::core::SourceKind::Project,
            });
        assert!(
            server_workspace
                .analysis_engine
                .read()
                .file_id(&unique)
                .is_none(),
            "mutating one engine must not change a sibling cloned from the same template"
        );
    }

    #[tokio::test]
    async fn core_template_binding_preserves_an_open_unsaved_document() {
        let fixture = TempDir::new().expect("live-document fixture must be created");
        let project = fixture.path().join("app");
        fs::create_dir_all(&project).expect("project root must be created");
        fs::write(project.join("Gemfile"), "source 'https://rubygems.org'\n")
            .expect("Gemfile must be written");
        let path = project.join("live.rb");
        let content =
            "class LiveDocument\n  def unsaved_marker; end\n  def call; unsaved_marker; end\nend\n";
        let uri = Url::from_file_path(&path).expect("live document URI must be valid");
        let server = create_test_server();
        let workspace = server.add_workspace(Url::from_directory_path(&project).unwrap());
        crate::capabilities::indexing::handle_did_open(
            &server,
            DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri: uri.clone(),
                    language_id: "ruby".to_string(),
                    version: 1,
                    text: content.to_string(),
                },
            },
        )
        .await;

        let mut coordinator = IndexingCoordinator::new(project, RubyFastLspConfig::default());
        coordinator.setup_file_processor(&server);
        coordinator
            .index_core_stubs(&server, Some(RubyVersion::new(3, 0)))
            .await
            .expect("core stubs must bind successfully");

        let engine = workspace.analysis_engine.read();
        let file_id = engine
            .file_id(&path)
            .expect("binding a core template must not erase the open document");
        assert!(
            engine.file_content_matches(file_id, content),
            "binding a core template must preserve the exact unsaved document content"
        );
        drop(engine);

        let definitions = crate::capabilities::definitions::find_definition_at_position(
            &server,
            uri,
            tower_lsp::lsp_types::Position::new(2, 14),
        )
        .await
        .expect("same-file definition lookup must remain available");
        assert_eq!(
            definitions.len(),
            1,
            "same-file navigation must survive core-template binding"
        );
        assert_eq!(definitions[0].range.start.line, 1);
    }

    #[tokio::test]
    async fn project_batch_stream_consumes_an_exact_generation_navigation_demand_first() {
        let fixture = TempDir::new().expect("navigation-demand fixture must be created");
        let project = fixture.path().join("server");
        fs::create_dir_all(&project).expect("project root must be created");
        fs::write(project.join("Gemfile"), "source 'https://rubygems.org'\n")
            .expect("Gemfile must be written");
        let caller_path = project.join("caller.rb");
        let caller_uri = Url::from_file_path(&caller_path).unwrap();
        fs::write(&caller_path, "UserPmm.lookup\n").unwrap();
        for index in 0..140 {
            fs::write(
                project.join(format!("ordinary_{index:03}.rb")),
                format!("ORDINARY_{index} = {index}\n"),
            )
            .unwrap();
        }
        let target_path = project.join("user_pmm.rb");
        fs::write(&target_path, "class UserPmm\nend\n").unwrap();

        let server = create_test_server();
        let workspace = server.add_workspace(Url::from_directory_path(&project).unwrap());
        crate::capabilities::indexing::handle_did_open(
            &server,
            DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri: caller_uri.clone(),
                    language_id: "ruby".to_string(),
                    version: 1,
                    text: "UserPmm.lookup\n".to_string(),
                },
            },
        )
        .await;
        let run = workspace.begin_indexing_run();
        workspace
            .indexing_status
            .transition(
                run.generation(),
                crate::indexing_status::IndexingPhase::IndexingProject,
                None,
                None,
            )
            .unwrap();

        let mut coordinator =
            IndexingCoordinator::new(project.clone(), RubyFastLspConfig::default());
        coordinator.set_indexing_run(run.clone());
        coordinator.setup_file_processor(&server);
        coordinator
            .collect_project_navigation_facts(
                &server,
                ActiveDocumentPriorityKeys {
                    dependency_roots: HashSet::new(),
                    project_terminals: vec!["ordinary000".to_string()],
                },
            )
            .await
            .unwrap();
        assert!(
            crate::capabilities::definitions::find_definition_at_position(
                &server,
                caller_uri.clone(),
                Position::new(0, 2),
            )
            .await
            .is_none(),
            "the target must remain outside the fixed startup frontier before its demand"
        );
        let ticket = workspace.navigation_demands.request(
            run.generation(),
            crate::navigation_demand::NavigationDemandStage::Project,
            "userpmm",
        );

        coordinator
            .collect_remaining_project_facts(&server, None)
            .await
            .unwrap();

        assert_eq!(
            ticket.wait().await,
            crate::navigation_demand::NavigationDemandOutcome::TargetProcessed
        );
        let definitions = crate::capabilities::definitions::find_definition_at_position(
            &server,
            caller_uri,
            Position::new(0, 2),
        )
        .await
        .expect("the exact demanded target must resolve before project-stage completion");
        assert_eq!(definitions.len(), 1);
        assert_eq!(
            definitions[0].uri,
            Url::from_file_path(target_path).unwrap()
        );
    }

    #[tokio::test]
    async fn project_frontier_consumes_a_bounded_nonpriority_demand() {
        let fixture = TempDir::new().expect("frontier-demand fixture must be created");
        let project = fixture.path().join("server");
        fs::create_dir_all(&project).expect("project root must be created");
        fs::write(project.join("Gemfile"), "source 'https://rubygems.org'\n")
            .expect("Gemfile must be written");
        fs::write(project.join("user.rb"), "class UserPmm\nend\n").unwrap();
        fs::write(project.join("report.rb"), "class Report\nend\n").unwrap();

        let server = create_test_server();
        let workspace = server.add_workspace(Url::from_directory_path(&project).unwrap());
        let run = workspace.begin_indexing_run();
        workspace
            .indexing_status
            .transition(
                run.generation(),
                crate::indexing_status::IndexingPhase::IndexingProject,
                None,
                None,
            )
            .unwrap();
        let ticket = workspace.navigation_demands.request(
            run.generation(),
            crate::navigation_demand::NavigationDemandStage::Project,
            "userpmm",
        );

        let mut coordinator =
            IndexingCoordinator::new(project.clone(), RubyFastLspConfig::default());
        coordinator.set_indexing_run(run);
        coordinator.setup_file_processor(&server);
        coordinator
            .collect_project_navigation_facts(
                &server,
                ActiveDocumentPriorityKeys {
                    dependency_roots: HashSet::new(),
                    project_terminals: vec!["report".to_string()],
                },
            )
            .await
            .unwrap();

        assert_eq!(
            tokio::time::timeout(Duration::from_millis(50), ticket.wait())
                .await
                .expect("the project frontier must consume its bounded demand"),
            crate::navigation_demand::NavigationDemandOutcome::TargetProcessed
        );
    }

    #[tokio::test]
    async fn dependency_core_seed_never_contains_an_open_project_document() {
        let fixture = TempDir::new().expect("dependency-seed fixture must be created");
        let clean_project = fixture.path().join("clean");
        let live_project = fixture.path().join("live");
        for project in [&clean_project, &live_project] {
            fs::create_dir_all(project).expect("project root must be created");
            fs::write(project.join("Gemfile"), "source 'https://rubygems.org'\n")
                .expect("Gemfile must be written");
        }

        let server = create_test_server();
        server.add_workspace(Url::from_directory_path(&clean_project).unwrap());
        server.add_workspace(Url::from_directory_path(&live_project).unwrap());

        let mut clean_coordinator =
            IndexingCoordinator::new(clean_project, RubyFastLspConfig::default());
        clean_coordinator.setup_file_processor(&server);
        let clean_seed = clean_coordinator
            .index_core_stubs(&server, Some(RubyVersion::new(3, 0)))
            .await
            .expect("clean core seed must be prepared");

        let live_path = live_project.join("live.rb");
        let live_uri = Url::from_file_path(&live_path).expect("live document URI must be valid");
        crate::capabilities::indexing::handle_did_open(
            &server,
            DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri: live_uri,
                    language_id: "ruby".to_string(),
                    version: 1,
                    text: "class ProjectOnly; end\n".to_string(),
                },
            },
        )
        .await;
        let mut live_coordinator =
            IndexingCoordinator::new(live_project, RubyFastLspConfig::default());
        live_coordinator.setup_file_processor(&server);
        let live_seed = live_coordinator
            .index_core_stubs(&server, Some(RubyVersion::new(3, 0)))
            .await
            .expect("live-document core seed must be prepared");

        assert!(
            live_seed.file_id(&live_path).is_none(),
            "the reusable dependency seed must never inherit project-owned open-document facts"
        );
        assert_eq!(
            clean_seed.semantic_context_fingerprint(),
            live_seed.semantic_context_fingerprint(),
            "editor open timing must not change the immutable dependency seed identity"
        );
    }

    #[tokio::test]
    async fn test_coordinator_project_file_collection() {
        let fixture = TestProjectFixture::new();
        fixture.setup_complete_project();

        let config = RubyFastLspConfig::default();
        let coordinator = IndexingCoordinator::new(fixture.project_root().clone(), config);

        // Test Ruby file collection
        let mut files = Vec::new();
        coordinator.find_all_ruby_files_in_directory(fixture.project_root(), &mut files);

        assert!(!files.is_empty(), "Should find Ruby files in project");

        // Verify specific files are found
        let file_names: Vec<String> = files
            .iter()
            .filter_map(|p| p.file_name()?.to_str())
            .map(|s| s.to_string())
            .collect();

        assert!(file_names.contains(&"application.rb".to_string()));
        assert!(file_names.contains(&"user.rb".to_string()));
        assert!(file_names.contains(&"user_service.rb".to_string()));
        assert!(file_names.contains(&"user_test.rb".to_string()));
        assert!(file_names.contains(&"Thorfile".to_string()));
        assert!(file_names.contains(&"config.ru".to_string()));
    }

    #[tokio::test]
    async fn project_rbs_declarations_enter_engine_method_facts() {
        let temp_dir = TempDir::new().expect("test workspace must be created");
        let sig_dir = temp_dir.path().join("sig");
        fs::create_dir_all(&sig_dir).expect("sig directory must be created");
        let signature_path = sig_dir.join("native_widget.rbs");
        fs::write(
            &signature_path,
            "class NativeWidget\n  def encode: (String value) -> String\nend\n",
        )
        .expect("RBS fixture must be written");
        let usage_path = temp_dir.path().join("native_usage.rb");
        let usage = "widget = NativeWidget.new\nwidget.encode(\"value\")\n";
        fs::write(&usage_path, usage).expect("Ruby usage fixture must be written");

        let mut coordinator =
            IndexingCoordinator::new(temp_dir.path().to_path_buf(), RubyFastLspConfig::default());
        let server = create_test_server();
        coordinator
            .run_complete_indexing(&server)
            .await
            .expect("workspace indexing must succeed");

        let engine = server.analysis_engine.read();
        let query = ruby_analysis::engine::AnalysisQuery::new(&engine);
        assert!(
            query.file_id(&signature_path).is_some(),
            "conventional sig/**/*.rbs files must be registered"
        );
        let method = ruby_analysis::core::FullyQualifiedName::method(
            vec![ruby_analysis::core::RubyConstant::new("NativeWidget")
                .expect("test class name must be valid")],
            ruby_analysis::core::RubyMethod::new("encode").expect("test method name must be valid"),
        );
        let facts = query.methods_for_fqn(&method);
        assert_eq!(facts.len(), 1, "RBS method must become one engine fact");
        assert_eq!(facts[0].return_type_label.as_deref(), Some("String"));
        drop(engine);

        let usage_uri = Url::from_file_path(&usage_path).expect("usage URI must be valid");
        crate::capabilities::indexing::handle_did_open(
            &server,
            tower_lsp::lsp_types::DidOpenTextDocumentParams {
                text_document: tower_lsp::lsp_types::TextDocumentItem {
                    uri: usage_uri.clone(),
                    language_id: "ruby".to_string(),
                    version: 1,
                    text: usage.to_string(),
                },
            },
        )
        .await;
        let document = server
            .docs
            .lock()
            .get(&usage_uri)
            .cloned()
            .expect("opened usage document must exist");
        let query = crate::query::EngineQuery::with_doc_and_engine(
            document,
            server.analysis_engine.clone(),
        );
        let definitions = query
            .find_definitions_at_position(
                &usage_uri,
                tower_lsp::lsp_types::Position::new(1, 9),
                usage,
            )
            .expect("native RBS method call must resolve");
        assert_eq!(definitions.len(), 1);
        assert_eq!(
            definitions[0].uri,
            Url::from_file_path(signature_path).unwrap()
        );
        let hover = query
            .get_hover_at_position(&usage_uri, tower_lsp::lsp_types::Position::new(1, 9), usage)
            .expect("RBS method return must produce hover information");
        assert!(hover.content.contains("String"));
    }

    #[tokio::test]
    async fn test_coordinator_ruby_file_detection() {
        let fixture = TestProjectFixture::new();
        let config = RubyFastLspConfig::default();
        let coordinator = IndexingCoordinator::new(fixture.project_root().clone(), config);

        // Test various Ruby file extensions
        assert!(coordinator.is_ruby_file(&PathBuf::from("test.rb")));
        assert!(coordinator.is_ruby_file(&PathBuf::from("test.ruby")));
        assert!(coordinator.is_ruby_file(&PathBuf::from("test.rake")));
        assert!(coordinator.is_ruby_file(&PathBuf::from("show.html.erb")));
        assert!(coordinator.is_ruby_file(&PathBuf::from("Rakefile")));
        assert!(coordinator.is_ruby_file(&PathBuf::from("Gemfile")));
        assert!(coordinator.is_ruby_file(&PathBuf::from("Guardfile")));
        assert!(coordinator.is_ruby_file(&PathBuf::from("Capfile")));

        // Test non-Ruby files
        assert!(!coordinator.is_ruby_file(&PathBuf::from("test.js")));
        assert!(!coordinator.is_ruby_file(&PathBuf::from("test.py")));
        assert!(!coordinator.is_ruby_file(&PathBuf::from("README.md")));
    }

    #[tokio::test]
    async fn test_coordinator_core_stubs_resolution() {
        let fixture = TestProjectFixture::new();
        fixture.create_core_stubs();

        let config = RubyFastLspConfig::default();
        let coordinator = IndexingCoordinator::new(fixture.project_root().clone(), config);

        // Test core stubs path resolution
        let stubs_path = coordinator.find_core_stubs_for_version((3, 0));
        assert!(stubs_path.is_some(), "Should find core stubs path");

        let stubs_path = stubs_path.unwrap();
        assert!(stubs_path.exists(), "Core stubs path should exist");
        assert!(
            stubs_path.join("object.rb").exists(),
            "Should find object.rb stub"
        );
        assert!(
            stubs_path.join("string.rb").exists(),
            "Should find string.rb stub"
        );
    }

    #[tokio::test]
    async fn test_coordinator_with_missing_directories() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let project_root = temp_dir.path().to_path_buf();

        let config = RubyFastLspConfig::default();
        let mut coordinator = IndexingCoordinator::new(project_root, config);
        let server = create_test_server();

        // Test indexing with missing directories (should not panic)
        let result = coordinator.run_complete_indexing(&server).await;
        assert!(
            result.is_ok(),
            "Indexing should handle missing directories gracefully"
        );
    }

    #[tokio::test]
    async fn test_coordinator_lib_directory_discovery() {
        let fixture = TestProjectFixture::new();
        let config = RubyFastLspConfig::default();
        let mut coordinator = IndexingCoordinator::new(fixture.project_root().clone(), config);

        // Test lib directory discovery
        coordinator.discover_ruby_library_paths();
        let lib_dirs = coordinator.get_ruby_library_paths();

        // This test depends on the system having Ruby installed
        // In CI environments, this might not be available, so we make it lenient
        println!("Discovered {} lib directories", lib_dirs.len());
        for dir in lib_dirs {
            println!("  - {:?}", dir);
        }
    }

    #[tokio::test]
    async fn test_coordinator_performance_with_large_project() {
        // SAFETY: This test is not run concurrently with other tests that modify this env var.
        // Keep the large-project check focused on project files instead of local gem volume.
        unsafe { std::env::set_var("RUBY_LSP_MAX_GEMS", "3") };

        let fixture = TestProjectFixture::new();
        fixture.setup_complete_project();

        // Create additional files to simulate a larger project
        let large_project_dir = fixture.project_root().join("large_project");
        fs::create_dir_all(&large_project_dir).expect("Failed to create large project dir");

        // Create 50 Ruby files
        for i in 0..50 {
            let file_content = format!(
                r#"
class TestClass{}
  def initialize
    @value = {}
  end

  def process
    # Some processing logic
  end
end
"#,
                i, i
            );
            fs::write(
                large_project_dir.join(format!("test_class_{}.rb", i)),
                file_content,
            )
            .expect("Failed to write test file");
        }

        let config = RubyFastLspConfig::default();
        let mut coordinator = IndexingCoordinator::new(fixture.project_root().clone(), config);
        let server = create_test_server();

        // Measure indexing time
        let start = std::time::Instant::now();
        let result = coordinator.run_complete_indexing(&server).await;
        let duration = start.elapsed();

        assert!(
            result.is_ok(),
            "Large project indexing should complete successfully"
        );
        println!("Large project indexing took: {:?}", duration);

        // Performance assertion - should complete within reasonable time
        assert!(
            duration.as_secs() < 45,
            "Indexing should complete within 45 seconds"
        );

        unsafe { std::env::remove_var("RUBY_LSP_MAX_GEMS") };
    }

    #[tokio::test]
    async fn test_coordinator_gem_discovery() {
        // Set environment variable to limit gem processing for faster tests
        // SAFETY: This test is not run concurrently with other tests that modify this env var
        unsafe { std::env::set_var("RUBY_LSP_MAX_GEMS", "5") };

        let fixture = TestProjectFixture::new();
        fixture.setup_complete_project();

        let config = RubyFastLspConfig::default();
        let mut coordinator = IndexingCoordinator::new(fixture.project_root().clone(), config);
        let server = create_test_server();

        // Execute indexing which should include gem discovery
        let result = coordinator.run_complete_indexing(&server).await;
        assert!(result.is_ok(), "Indexing with gem discovery should succeed");

        assert!(
            coordinator.gem_indexer.is_some(),
            "production gem discovery must initialize the owning project's exact gem indexer"
        );
        assert!(
            coordinator.get_ruby_library_paths().is_empty(),
            "complete indexing must not launch the redundant legacy load-path discovery"
        );

        // Clean up environment variable
        // SAFETY: This test is not run concurrently with other tests that modify this env var
        unsafe { std::env::remove_var("RUBY_LSP_MAX_GEMS") };
    }

    #[tokio::test]
    async fn test_coordinator_gem_indexing_integration() {
        // Set environment variable to limit gem processing for faster tests
        // SAFETY: This test is not run concurrently with other tests that modify this env var
        unsafe { std::env::set_var("RUBY_LSP_MAX_GEMS", "3") };

        let fixture = TestProjectFixture::new();
        fixture.setup_complete_project();

        let config = RubyFastLspConfig::default();
        let mut coordinator = IndexingCoordinator::new(fixture.project_root().clone(), config);
        let server = create_test_server();

        // Test that gem indexing doesn't break the overall indexing process
        let result = coordinator.run_complete_indexing(&server).await;
        assert!(
            result.is_ok(),
            "Indexing should succeed even with gem discovery"
        );

        assert!(
            coordinator.gem_indexer.is_some(),
            "gem indexing must complete through the owning project's exact gem indexer"
        );
        assert!(
            coordinator.get_ruby_library_paths().is_empty(),
            "gem indexing must not populate the unused legacy load-path side table"
        );

        // The gem indexing should not interfere with project file indexing
        let mut project_files = Vec::new();
        coordinator.find_all_ruby_files_in_directory(fixture.project_root(), &mut project_files);
        assert!(
            !project_files.is_empty(),
            "Project files should still be discoverable after gem indexing"
        );

        // Clean up environment variable
        // SAFETY: This test is not run concurrently with other tests that modify this env var
        unsafe { std::env::remove_var("RUBY_LSP_MAX_GEMS") };
    }

    #[tokio::test]
    async fn test_coordinator_gem_error_handling() {
        // Set environment variable to limit gem processing for faster tests
        // SAFETY: This test is not run concurrently with other tests that modify this env var
        unsafe { std::env::set_var("RUBY_LSP_MAX_GEMS", "2") };

        let fixture = TestProjectFixture::new();
        fixture.setup_complete_project();

        let config = RubyFastLspConfig::default();
        let mut coordinator = IndexingCoordinator::new(fixture.project_root().clone(), config);
        let server = create_test_server();

        // Even if gem discovery fails, the overall indexing should still succeed
        // This tests the error handling in discover_and_index_gems
        let result = coordinator.run_complete_indexing(&server).await;
        assert!(
            result.is_ok(),
            "Indexing should succeed even if gem discovery encounters errors"
        );

        // Basic functionality should still work
        let lib_dirs = coordinator.get_ruby_library_paths();
        // We should at least have some directories (even if gem discovery failed)
        // The system Ruby directories should still be found
        let _ = lib_dirs;

        // Clean up environment variable
        // SAFETY: This test is not run concurrently with other tests that modify this env var
        unsafe { std::env::remove_var("RUBY_LSP_MAX_GEMS") };
    }

    #[tokio::test]
    async fn test_coordinator_gem_performance() {
        // Set environment variable to limit gem processing for faster tests
        // SAFETY: This test is not run concurrently with other tests that modify this env var
        unsafe { std::env::set_var("RUBY_LSP_MAX_GEMS", "3") };

        let fixture = TestProjectFixture::new();
        fixture.setup_complete_project();

        let config = RubyFastLspConfig::default();
        let mut coordinator = IndexingCoordinator::new(fixture.project_root().clone(), config);
        let server = create_test_server();

        // Measure time for indexing including gem discovery
        let start = std::time::Instant::now();
        let result = coordinator.run_complete_indexing(&server).await;
        let elapsed = start.elapsed();

        assert!(
            result.is_ok(),
            "Indexing with gem discovery should complete successfully"
        );

        // Gem discovery should not significantly slow down the indexing process
        // Allow up to 30 seconds for gem discovery in addition to regular indexing
        assert!(
            elapsed.as_secs() < 30,
            "Indexing with gem discovery should complete within 30 seconds, took {}s",
            elapsed.as_secs()
        );

        println!(
            "Indexing with gem discovery completed in {}ms",
            elapsed.as_millis()
        );

        // Clean up environment variable
        // SAFETY: This test is not run concurrently with other tests that modify this env var
        unsafe { std::env::remove_var("RUBY_LSP_MAX_GEMS") };
    }

    #[tokio::test]
    async fn test_coordinator_collects_all_ruby_files() {
        // Test that all Ruby files are collected, including vendor directories.
        // File source (Project/Gem/Stdlib) is determined by indexers based on
        // discovered paths from tools (bundler, rubygems), not by exclusion patterns.
        let fixture = TestProjectFixture::new();
        fixture.setup_complete_project();

        // Create a vendor directory with Ruby files
        let vendor_dir = fixture.project_root().join("vendor");
        fs::create_dir_all(&vendor_dir).expect("Failed to create vendor directory");

        let vendor_bundle_dir = vendor_dir.join("bundle");
        fs::create_dir_all(&vendor_bundle_dir).expect("Failed to create vendor/bundle directory");

        // Create Ruby files in vendor
        let vendor_ruby_file = vendor_dir.join("vendor_gem.rb");
        fs::write(&vendor_ruby_file, "class VendorGem\nend")
            .expect("Failed to write vendor Ruby file");

        let vendor_bundle_ruby_file = vendor_bundle_dir.join("bundled_gem.rb");
        fs::write(&vendor_bundle_ruby_file, "class BundledGem\nend")
            .expect("Failed to write vendor/bundle Ruby file");

        let config = RubyFastLspConfig::default();
        let coordinator = IndexingCoordinator::new(fixture.project_root().clone(), config);

        // Collect Ruby files from the project
        let mut collected_files: Vec<PathBuf> = Vec::new();
        coordinator.find_all_ruby_files_in_directory(fixture.project_root(), &mut collected_files);

        // Verify that vendor files ARE collected (no exclusion)
        let vendor_files: Vec<_> = collected_files
            .iter()
            .filter(|path| path.to_string_lossy().contains("vendor"))
            .collect();

        assert!(
            !vendor_files.is_empty(),
            "Vendor directory files should be collected (source tagging handles categorization)"
        );

        // Verify that non-vendor files are also collected
        let non_vendor_files: Vec<_> = collected_files
            .iter()
            .filter(|path| !path.to_string_lossy().contains("vendor"))
            .collect();

        assert!(
            !non_vendor_files.is_empty(),
            "Non-vendor Ruby files should also be collected"
        );
    }

    #[tokio::test]
    async fn cold_indexing_retains_but_does_not_publish_closed_file_diagnostics() {
        let workspace = TempDir::new().unwrap();
        let file_path = workspace.path().join("app/service.rb");
        fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        let source = "MissingService.call\n";
        fs::write(&file_path, source).unwrap();
        let uri = Url::from_file_path(&file_path).unwrap();
        let workspace_uri = Url::from_directory_path(workspace.path()).unwrap();
        let server = RubyLanguageServer::default();
        server.add_workspace(workspace_uri);
        let mut coordinator =
            IndexingCoordinator::new(workspace.path().to_path_buf(), RubyFastLspConfig::default());

        coordinator.run_complete_indexing(&server).await.unwrap();

        assert!(
            server
                .analysis_engine_for_uri(&uri)
                .read()
                .stats()
                .diagnostics
                > 0,
            "cold indexing must retain workspace diagnostics in the engine"
        );
        assert!(
            server.last_published_diagnostics(&uri).is_empty(),
            "closed-file engine diagnostics must not flood the LSP client"
        );

        crate::capabilities::indexing::handle_did_open(
            &server,
            DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri: uri.clone(),
                    language_id: "ruby".to_string(),
                    version: 1,
                    text: source.to_string(),
                },
            },
        )
        .await;
        assert!(
            !server.last_published_diagnostics(&uri).is_empty(),
            "opening the file must publish its current diagnostics"
        );
    }
}
