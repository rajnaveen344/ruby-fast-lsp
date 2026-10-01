//! JRuby classpath, Java catalog, and runtime-source indexing.

use super::resources::{run_cpu_indexing_task, IndexingWorkClass};
use super::IndexingCoordinator;
use crate::environment::config::runtime::SelectedRuntimeDescriptor;
use crate::environment::config::RubyFastLspConfig;
use crate::environment::runtime::catalog::RuntimeImplementation;
use crate::environment::runtime::jruby::classpath::{
    discover_project_classpath, discover_project_classpath_with_cache, ArtifactOrigin,
    ClasspathArtifact, ClasspathFileProductCache, ClasspathInputs, ClasspathLimits,
};
use crate::environment::runtime::jruby::decompiler::{
    discover_bundled_cfr_asset, JavaDecompiler, JavaDecompilerLimits,
};
use crate::environment::runtime::jruby::imports::JrubyImportProvider;
use crate::environment::runtime::jruby::java_catalog::{
    build_project_java_catalog, verify_artifact_discovery_identity, JavaArtifactProduct,
    JavaArtifactProductCache, JavaArtifactProductKey, ProjectJavaCatalog,
    ProjectJavaCatalogBuilder,
};
use crate::environment::runtime::jruby::runtime_sources::materialize_jruby_runtime_sources;
use crate::environment::runtime::jruby::source_navigation::{
    JavaSourceResolutionLimits, JavaSourceResolver,
};
use crate::invariant::ExpectInvariant;
use crate::loader::cache::persistent::{
    PersistentDerivedProductCache, PersistentJavaArtifactLookup,
};
use crate::loader::context::LoadContext;
use crate::loader::file_processor::FileProcessor;
use crate::loader::sources::gems::discover_locked_java_gem_roots;
use crate::server::RubyLanguageServer;
use anyhow::{anyhow, Context, Result};
use log::{info, warn};
use rayon::prelude::*;
use ruby_analysis::engine::AnalysisEngine;
use ruby_fast_lsp_jvm_metadata::ArchiveLimits;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tokio_util::sync::CancellationToken;
use tower_lsp::lsp_types::Url;

pub(super) fn read_jdk_feature(java_home: &Path) -> Result<u16> {
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
        None => crate::utils::cache::ruby_fast_lsp_user_cache_root()?,
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

pub(super) fn build_cached_project_java_catalog(
    classpath: &crate::environment::runtime::jruby::classpath::ProjectClasspath,
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

pub(super) fn build_jruby_import_provider(
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
        (Some(_), None) | (None, Some(_)) => unreachable_invariant!(
            what = "persistent and process-local Java artifact caches were configured independently",
            why = "production lookup must validate persistent products before bounded shared retention",
            fix = "pass both caches together or neither for an isolated uncached test",
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

pub(super) async fn build_jruby_import_provider_off_reactor(
    ctx: &LoadContext,
    workspace_root: PathBuf,
    config: RubyFastLspConfig,
    effective_runtime: Option<SelectedRuntimeDescriptor>,
    user_cache_root_override: Option<PathBuf>,
    cancellation: Option<CancellationToken>,
    work_class: IndexingWorkClass,
) -> Result<(Option<Arc<JrubyImportProvider>>, Option<ClasspathArtifact>)> {
    let persistent_cache = ctx.products.persistent().clone();
    let classpath_file_product_cache = ctx.products.classpath_files().clone();
    let java_artifact_product_cache = ctx.products.java_artifacts().clone();
    run_cpu_indexing_task(
        &ctx.resources,
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

impl IndexingCoordinator {
    #[cfg(test)]
    pub(super) fn setup_jruby_import_provider(&mut self) -> Result<()> {
        let (provider, runtime_archive) = build_jruby_import_provider(
            self.workspace_root.clone(),
            self.config.clone(),
            self.effective_runtime.clone(),
            self.cache_root.clone(),
            None,
            None,
            None,
        )?;
        self.jruby_import_provider = provider;
        self.jruby_runtime_archive = runtime_archive;
        Ok(())
    }

    pub(super) async fn index_jruby_runtime_sources_off_reactor(
        &self,
        ctx: &LoadContext,
        server: &RubyLanguageServer,
        dependency_seed_engine: Arc<parking_lot::RwLock<AnalysisEngine>>,
    ) -> Result<()> {
        let Some(artifact) = self.jruby_runtime_archive.clone() else {
            return Ok(());
        };
        let provider = self.jruby_import_provider.clone().expect_invariant(
            "a JRuby runtime archive exists without its import provider",
            "both are derived transactionally from one isolated classpath",
            "keep JRuby runtime archive and catalog setup in the same coordinator step",
        );
        let processor = self.file_processor.clone().expect_invariant(
            "JRuby runtime source indexing started before FileProcessor setup",
            "runtime sources must use ordinary file-owned facts",
            "keep FileProcessor setup before JRuby runtime source materialization",
        );
        let workspace_root = self.workspace_root.clone();
        let user_cache_root_override = self.cache_root.clone();
        let analysis_engine = self.analysis_engine(server);
        run_cpu_indexing_task(
            &ctx.resources,
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
    pub(super) fn jruby_signature_cache_root(
        &self,
        provider: &JrubyImportProvider,
    ) -> Result<PathBuf> {
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
}
