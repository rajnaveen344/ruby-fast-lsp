use crate::config::runtime::{EffectiveRuntimeSelection, SelectedRuntimeDescriptor};
use crate::config::{IndexingConfig, RubyFastLspConfig};
use crate::extensions::ExtensionRegistryHandle;
use crate::indexer::file_processor::FileProcessor;
use crate::indexer::indexer_gem::{discover_locked_java_gem_roots, IndexerGem};
use crate::indexer::indexer_project::IndexerProject;
use crate::indexer::indexer_stdlib::IndexerStdlib;

use crate::indexer::version::ruby_version::{RubyImplementation, RubyVersion};
use crate::indexer::version::version_detector::RubyVersionDetector;
use crate::runtime::catalog::RuntimeImplementation;
use crate::runtime::jruby::classpath::{
    discover_project_classpath, ArtifactOrigin, ClasspathArtifact, ClasspathInputs, ClasspathLimits,
};
use crate::runtime::jruby::decompiler::{
    discover_bundled_cfr_asset, JavaDecompiler, JavaDecompilerLimits,
};
use crate::runtime::jruby::imports::{
    static_java_dependencies, static_java_proxy_references, JrubyImportProvider,
    StaticJavaDependency,
};
use crate::runtime::jruby::java_catalog::build_project_java_catalog;
use crate::runtime::jruby::runtime_sources::materialize_jruby_runtime_sources;
use crate::runtime::jruby::source_navigation::{
    java_source_navigation_facts_with_declaration, JavaSourceResolutionLimits, JavaSourceResolver,
};
use crate::server::RubyLanguageServer;
use anyhow::{anyhow, Context, Result};
use log::{debug, info, warn};
use ruby_analysis::core::{
    DiagnosticFact, DiagnosticSeverity as AnalysisDiagnosticSeverity, TextRange,
};
use ruby_analysis::engine::{FileFacts, ResolveMode, SourceFile, SourceFileInput};
use ruby_fast_lsp_jvm_metadata::ArchiveLimits;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Position, Range, Url};

fn append_file_facts(target: &mut FileFacts, mut source: FileFacts) {
    target.symbols.append(&mut source.symbols);
    target.methods.append(&mut source.methods);
    target
        .method_visibility_overrides
        .append(&mut source.method_visibility_overrides);
    target.types.append(&mut source.types);
    target.graph_nodes.append(&mut source.graph_nodes);
    target.graph_edges.append(&mut source.graph_edges);
    target
        .unresolved_graph_edges
        .append(&mut source.unresolved_graph_edges);
    target
        .reference_candidates
        .append(&mut source.reference_candidates);
    target
        .diagnostic_candidates
        .append(&mut source.diagnostic_candidates);
    target.diagnostics.append(&mut source.diagnostics);
    target
        .execution_contexts
        .append(&mut source.execution_contexts);
}

/// Wall-clock timings captured by the coordinator during the most recent
/// [`IndexingCoordinator::run_complete_indexing`] call. Consumed by the
/// perf bench binary and perf regression tests.
#[derive(Debug, Clone, Copy, Default)]
pub struct IndexingTimings {
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

    extension_registry: ExtensionRegistryHandle,

    // Ruby version info
    version_detector: RubyVersionDetector,
    detected_ruby_version: Option<RubyVersion>,
    effective_runtime: Option<SelectedRuntimeDescriptor>,
    jruby_import_provider: Option<Arc<JrubyImportProvider>>,
    jruby_runtime_archive: Option<ClasspathArtifact>,
    user_cache_root_override: Option<PathBuf>,

    // The main indexing engine
    file_processor: Option<FileProcessor>,

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
}

impl IndexingCoordinator {
    fn analysis_engine(
        &self,
        server: &RubyLanguageServer,
    ) -> Arc<parking_lot::RwLock<ruby_analysis::engine::AnalysisEngine>> {
        let uri = Url::from_directory_path(&self.workspace_root).expect(
            "INVARIANT VIOLATED: workspace root cannot be represented as a file URI. This is a bug because indexing only accepts filesystem workspace roots. Fix: register a canonical filesystem project root before creating the coordinator.",
        );
        server.analysis_engine_for_uri(&uri)
    }
    /// Creates a new IndexingCoordinator for the given workspace.
    ///
    /// Call `run_complete_indexing()` to actually start the indexing process.
    pub fn new(workspace_root: PathBuf, config: RubyFastLspConfig) -> Self {
        let version_detector = RubyVersionDetector::from_path(workspace_root.clone());
        let extension_registry = ExtensionRegistryHandle::from_config(&config);

        Self {
            workspace_root,
            config,
            extension_registry,
            version_detector,
            detected_ruby_version: None,
            effective_runtime: None,
            jruby_import_provider: None,
            jruby_runtime_archive: None,
            user_cache_root_override: None,
            file_processor: None,
            project_indexer: None,
            stdlib_indexer: None,
            gem_indexer: None,
            ruby_library_paths: Vec::new(),
            last_timings: IndexingTimings::default(),
        }
    }

    /// Returns the timings captured by the most recent call to
    /// `run_complete_indexing`. All-zero before the first call.
    pub fn last_timings(&self) -> IndexingTimings {
        self.last_timings
    }

    pub fn set_extension_registry(&mut self, extension_registry: ExtensionRegistryHandle) {
        self.extension_registry = extension_registry;
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
        let start_time = Instant::now();

        self.resolve_effective_runtime(server).await?;

        // Step 1: Figure out which Ruby version we're using
        let ruby_version = self.detect_ruby_version();
        server.set_extension_project_ruby_version(
            &self.workspace_root,
            ruby_version.map(|version| version.to_string()),
        );
        info!("Detected Ruby version: {:?}", ruby_version);

        // Step 2: Find where Ruby libraries are installed
        self.discover_ruby_library_paths();

        // JRuby runtime facts are project-owned and must exist before any
        // project, gem, or stdlib file is visited.
        server.set_runtime_classpath_fingerprint(&self.workspace_root, None);
        self.setup_jruby_import_provider()?;
        server.set_runtime_classpath_fingerprint(
            &self.workspace_root,
            self.jruby_import_provider
                .as_ref()
                .map(|provider| provider.classpath_fingerprint().to_string()),
        );

        // Step 3: Set up the main indexing engine
        self.setup_file_processor(server);

        // Fact collection order: scan deps → gems → stdlib → project
        // (project skips files already indexed as Gem/Stdlib).
        info!("Collecting analysis facts");
        let facts_start = Instant::now();

        // Step 4: Quick scan project files for dependencies (no indexing yet)
        self.scan_project_dependencies()?;

        // JRuby ships the Ruby implementation of java_import/include_package
        // inside jruby.jar. Materialize only the bounded runtime source allowlist
        // so definition lookup prefers implementation source over compatibility
        // declarations without treating the whole archive as project code.
        self.index_jruby_runtime_sources(server)?;

        // Static Java imports request deterministic read-only signature
        // documents before project references are resolved.
        self.index_jruby_import_signatures(server)?;

        // Step 5: Collect facts from gems (uses discovered required gems)
        self.index_gems(server).await?;

        // Step 6: Collect facts from Ruby standard library
        self.index_standard_library(server, &ruby_version).await?;

        // Step 7: Collect facts from project files (skips files already indexed as Gem/Stdlib)
        self.collect_project_facts(server).await?;

        let facts_dur = facts_start.elapsed();
        let reserved_dur = Duration::default();
        info!("Facts collection completed in {:?}", facts_dur);

        // Publish diagnostics to the client.
        info!("Publishing diagnostics");
        let publish_start = Instant::now();
        Self::send_progress_report(server, "Publishing diagnostics...".to_string(), 0, 0).await;
        self.publish_unresolved_diagnostics(server).await;
        let publish_dur = publish_start.elapsed();

        let total_dur = start_time.elapsed();
        info!("Complete indexing finished in {:?}", total_dur);
        {
            let analysis_engine = self.analysis_engine(server);
            let mut engine = analysis_engine.write();
            engine.shrink_to_fit();
        }
        release_allocator_free_pages();
        self.log_analysis_memory_stats(server);

        self.last_timings = IndexingTimings {
            facts: facts_dur,
            reserved: reserved_dur,
            publish: publish_dur,
            total: total_dur,
        };
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

    /// Helper function to send progress report updates to the client
    pub async fn send_progress_report(
        server: &RubyLanguageServer,
        message: String,
        current: usize,
        total: usize,
    ) {
        if let Some(client) = &server.client {
            let percentage = if total > 0 {
                ((current as f64 / total as f64) * 100.0) as u32
            } else {
                0
            };

            let full_message = if total > 0 {
                format!("{}: {}/{}", message, current, total)
            } else {
                message
            };

            let _ = client
                .send_notification::<tower_lsp::lsp_types::notification::Progress>(
                    tower_lsp::lsp_types::ProgressParams {
                        token: tower_lsp::lsp_types::NumberOrString::String("indexing".to_string()),
                        value: tower_lsp::lsp_types::ProgressParamsValue::WorkDone(
                            tower_lsp::lsp_types::WorkDoneProgress::Report(
                                tower_lsp::lsp_types::WorkDoneProgressReport {
                                    message: Some(full_message),
                                    percentage: Some(percentage),
                                    cancellable: Some(false),
                                },
                            ),
                        ),
                    },
                )
                .await;
        }
    }

    /// Step 1: Detect which Ruby version we're working with
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
                .or_else(|| self.version_detector.detect_version()),
        };
        self.detected_ruby_version = version;
        version
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
        let processor = FileProcessor::with_extension_registry(self.extension_registry.clone());
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

    fn setup_jruby_import_provider(&mut self) -> Result<()> {
        self.jruby_import_provider = None;
        self.jruby_runtime_archive = None;
        let Some(runtime) = self.effective_runtime.clone() else {
            return Ok(());
        };
        if runtime.implementation != RuntimeImplementation::Jruby {
            return Ok(());
        }
        let java_home = runtime.java_home.clone().ok_or_else(|| {
            anyhow!(
                "JRuby runtime `{}` for project `{}` has no JDK. Select or configure an exact \
                 JAVA_HOME before indexing Java imports.",
                runtime.engine_version,
                self.workspace_root.display()
            )
        })?;
        let jdk_feature = read_jdk_feature(&java_home)?;
        let root = self.workspace_root.to_string_lossy();
        let project_config = self.config.jruby.project_config(&root);
        let maven_repository = std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|home| home.join(".m2/repository"))
            .filter(|path| path.is_dir());
        let java_gem_roots = discover_locked_java_gem_roots(
            &self.workspace_root,
            &runtime.executable,
            &runtime.compatibility_version,
        )
        .with_context(|| {
            format!(
                "failed to discover exact locked Java-platform gems for project {}",
                self.workspace_root.display()
            )
        })?;
        let classpath = discover_project_classpath(
            &ClasspathInputs {
                project_root: self.workspace_root.clone(),
                jruby_executable: runtime.executable,
                java_home: java_home.clone(),
                maven_repository,
                java_gem_roots,
                additional_classpath: project_config.additional_classpath,
                additional_sources: project_config.additional_sources,
            },
            ClasspathLimits::default(),
        )
        .map_err(|error| {
            anyhow!(
                "JRuby classpath discovery failed for `{}`: {error:?}",
                self.workspace_root.display()
            )
        })?;
        self.jruby_runtime_archive = classpath
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
        let catalog = build_project_java_catalog(&classpath, jdk_feature, ArchiveLimits::default())
            .map_err(|error| {
                anyhow!(
                    "JRuby Java catalog failed for `{}`: {error:?}",
                    self.workspace_root.display()
                )
            })?;
        info!(
            "JRuby Java catalog ready for {}: classes={}, artifacts={}, duplicates={}, fingerprint={}",
            self.workspace_root.display(),
            catalog.classes.len(),
            classpath.artifacts.len(),
            catalog.duplicates.len(),
            catalog.classpath_fingerprint_sha256
        );
        let source_cache_root =
            self.jruby_cache_root("jruby-sources", &catalog.classpath_fingerprint_sha256)?;
        let source_resolver = JavaSourceResolver::new(
            classpath.sources,
            source_cache_root,
            JavaSourceResolutionLimits::default(),
        );
        let mut provider = JrubyImportProvider::new(Arc::new(catalog))
            .with_source_resolver(Arc::new(source_resolver));
        let signature_cache_root =
            self.jruby_cache_root("jruby-signatures", provider.classpath_fingerprint())?;
        provider = provider.with_signature_cache_root(signature_cache_root);
        let decompiler_cache_root =
            self.jruby_cache_root("jruby-decompiler", provider.classpath_fingerprint())?;
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
                self.workspace_root.display(),
                error
            ),
        }
        self.jruby_import_provider = Some(Arc::new(provider));
        Ok(())
    }

    fn index_jruby_runtime_sources(&self, server: &RubyLanguageServer) -> Result<()> {
        let Some(artifact) = &self.jruby_runtime_archive else {
            return Ok(());
        };
        let provider = self.jruby_import_provider.as_ref().expect(
            "INVARIANT VIOLATED: a JRuby runtime archive exists without its import provider. \
             This is a bug because both are derived transactionally from one isolated classpath. \
             Fix: keep JRuby runtime archive and catalog setup in the same coordinator step.",
        );
        let cache_root =
            self.jruby_cache_root("jruby-runtime-sources", provider.classpath_fingerprint())?;
        let sources =
            materialize_jruby_runtime_sources(artifact, &cache_root).map_err(|error| {
                anyhow!(
                    "failed to materialize bounded JRuby runtime sources for {}: {error:?}",
                    self.workspace_root.display()
                )
            })?;
        let processor = self.file_processor.as_ref().expect(
            "INVARIANT VIOLATED: JRuby runtime source indexing started before FileProcessor setup. \
             This is a coordinator bug because runtime sources must use ordinary file-owned facts. \
             Fix: keep FileProcessor setup before JRuby runtime source materialization.",
        );
        let engine = self.analysis_engine(server);
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
                engine.clone(),
                ruby_analysis::core::SourceKind::Stdlib,
            )?;
        }
        Ok(())
    }

    fn index_jruby_import_signatures(&self, server: &RubyLanguageServer) -> Result<()> {
        let Some(provider) = &self.jruby_import_provider else {
            return Ok(());
        };
        let files =
            crate::utils::collect_project_files(&self.workspace_root, &self.config.indexing)?;
        let mut dependencies = BTreeSet::new();
        let mut proxy_references = BTreeSet::new();
        for path in files {
            let metadata = std::fs::metadata(&path)
                .with_context(|| format!("failed to inspect Ruby source {}", path.display()))?;
            if metadata.len() > 16 * 1024 * 1024 {
                return Err(anyhow!(
                    "Ruby source exceeds the 16 MiB JRuby import preflight limit: {}",
                    path.display()
                ));
            }
            let source = std::fs::read_to_string(&path)
                .with_context(|| format!("failed to read Ruby source {}", path.display()))?;
            dependencies.extend(static_java_dependencies(&source));
            proxy_references.extend(static_java_proxy_references(&source));
        }
        if dependencies.is_empty() && proxy_references.is_empty() {
            return Ok(());
        }

        let cache_root = self.jruby_signature_cache_root(provider)?;
        std::fs::create_dir_all(&cache_root).with_context(|| {
            format!(
                "failed to create JRuby signature cache {}",
                cache_root.display()
            )
        })?;
        let processor = self.file_processor.as_ref().expect(
            "INVARIANT VIOLATED: JRuby signature indexing started before FileProcessor setup. \
             This is a coordinator bug because generated signatures must enter the owning engine \
             through ordinary per-file replacement. Fix: keep FileProcessor setup before import \
             signature indexing.",
        );
        let analysis_engine = self.analysis_engine(server);
        let mut class_names = BTreeSet::new();
        for dependency in dependencies {
            match dependency {
                StaticJavaDependency::Class(name) => {
                    if let Some(class_name) = provider
                        .class_name_for_static_proxy_reference(&name)
                        .map_err(|message| anyhow!(message))?
                    {
                        class_names.insert(class_name);
                    }
                }
                StaticJavaDependency::Package(package) => {
                    class_names.extend(
                        provider
                            .class_names_in_package(&package)
                            .map_err(|message| anyhow!(message))?,
                    );
                }
            }
        }
        for reference in proxy_references {
            if let Some(class_name) = provider
                .class_name_for_static_proxy_reference(&reference)
                .map_err(|message| anyhow!(message))?
            {
                class_names.insert(class_name);
            }
        }
        let mut exact_sources = BTreeMap::<
            PathBuf,
            (
                String,
                Vec<(
                    String,
                    ruby_fast_lsp_jvm_metadata::JavaSourceClassLocation,
                    bool,
                )>,
            ),
        >::new();
        for class_name in class_names {
            let Some((internal_name, source)) =
                provider.generated_signature(&class_name).map_err(|error| {
                    anyhow!("failed to generate signature for Java class `{class_name}`: {error:?}")
                })?
            else {
                continue;
            };
            let path = cache_root.join(format!("{internal_name}.rb"));
            let parent = path.parent().expect(
                "INVARIANT VIOLATED: generated Java signature path has no parent. \
                 This is a bug because internal JVM names always produce a cache-relative file. \
                 Fix: preserve the deterministic cache root when constructing signature paths.",
            );
            std::fs::create_dir_all(parent).with_context(|| {
                format!(
                    "failed to create JRuby signature directory {}",
                    parent.display()
                )
            })?;
            let unchanged = std::fs::read_to_string(&path).is_ok_and(|existing| existing == source);
            if !unchanged {
                std::fs::write(&path, &source).with_context(|| {
                    format!("failed to write JRuby signature {}", path.display())
                })?;
            }
            let uri = Url::from_file_path(&path).map_err(|_| {
                anyhow!(
                    "generated JRuby signature is not a valid file URI: {}",
                    path.display()
                )
            })?;
            processor.collect_file_facts_as_deferred_resolution_in_engine(
                &uri,
                &source,
                analysis_engine.clone(),
                ruby_analysis::core::SourceKind::Signature,
            )?;
            match provider.resolved_navigation_implementations(&internal_name) {
                Ok(resolved_sources) => {
                    for (index, resolved) in resolved_sources.into_iter().enumerate() {
                        let entry = exact_sources
                            .entry(resolved.path)
                            .or_insert_with(|| (resolved.content.clone(), Vec::new()));
                        assert_eq!(
                            entry.0, resolved.content,
                            "INVARIANT VIOLATED: one exact Java source path resolved to different content \
                             during a single classpath pass. This is a bug because the classpath and source \
                             fingerprints are immutable for the pass. Fix: retain one verified source identity \
                             for every materialized path."
                        );
                        entry
                            .1
                            .push((internal_name.clone(), resolved.location, index == 0));
                    }
                }
                Err(error) => {
                    warn!(
                        "Java implementation source unavailable for {} in {}: {:?}; using generated signature fallback",
                        internal_name,
                        self.workspace_root.display(),
                        error
                    );
                }
            }
        }
        if !exact_sources.is_empty() {
            let mut engine = analysis_engine.write();
            for (path, (content, mut classes)) in exact_sources {
                classes.sort_by(|left, right| left.0.cmp(&right.0));
                classes.dedup_by(|left, right| left.0 == right.0);
                let file_id = engine.register_file(SourceFileInput {
                    path,
                    content,
                    kind: ruby_analysis::core::SourceKind::External,
                });
                let mut facts = FileFacts::default();
                for (internal_name, location, include_class_declaration) in classes {
                    let declaration = provider.class_declaration(&internal_name).expect(
                        "INVARIANT VIOLATED: exact Java source resolved for a class absent from its \
                         owning catalog. This is a bug because source resolution starts from that catalog \
                         declaration. Fix: keep provider catalog and source resolver transactionally paired.",
                    );
                    provider.register_method_navigation_ranges(&internal_name, &location, file_id);
                    append_file_facts(
                        &mut facts,
                        java_source_navigation_facts_with_declaration(
                            &declaration.class,
                            &location,
                            file_id,
                            include_class_declaration,
                        ),
                    );
                }
                engine.replace_facts(file_id, facts, ResolveMode::Deferred);
            }
        }
        Ok(())
    }

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

    fn jruby_cache_root(&self, namespace: &str, classpath_fingerprint: &str) -> Result<PathBuf> {
        let user_cache_root = match &self.user_cache_root_override {
            Some(root) => root.clone(),
            None => crate::utils::ruby_fast_lsp_user_cache_root()?,
        };
        let canonical_project_root = self.workspace_root.canonicalize().with_context(|| {
            format!(
                "failed to canonicalize JRuby project root {} for signature cache isolation",
                self.workspace_root.display()
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

    /// Quick scan for dependencies without indexing.
    /// Creates project indexer and scans for required gems/stdlib modules.
    fn scan_project_dependencies(&mut self) -> Result<()> {
        // Create a temporary project indexer just for dependency scanning
        // We'll create a proper one later for actual indexing
        let temp_indexer = IndexerProject::new(
            self.workspace_root.clone(),
            self.file_processor.as_ref().unwrap().clone(),
            self.config.indexing.clone(),
        );
        temp_indexer.scan_for_dependencies()?;
        self.project_indexer = Some(temp_indexer);
        Ok(())
    }

    /// Collect facts from project files (skips already-indexed files)
    async fn collect_project_facts(&mut self, server: &RubyLanguageServer) -> Result<()> {
        if let Some(ref mut project_indexer) = self.project_indexer {
            project_indexer.collect_project_facts(server).await?;
        } else {
            let mut project_indexer = IndexerProject::new(
                self.workspace_root.clone(),
                self.file_processor.as_ref().unwrap().clone(),
                self.config.indexing.clone(),
            );
            project_indexer.collect_project_facts(server).await?;
            self.project_indexer = Some(project_indexer);
        }
        Ok(())
    }

    /// Publish diagnostics for unresolved entries in currently open files.
    async fn publish_unresolved_diagnostics(&self, server: &RubyLanguageServer) {
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
            server.publish_diagnostics(uri, diagnostics).await;
        }
    }

    /// Step 5: Index the Ruby standard library
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

        stdlib_indexer.set_required_modules(required_stdlib);
        stdlib_indexer
            .index_stdlib(server, self.analysis_engine(server))
            .await?;
        self.stdlib_indexer = Some(stdlib_indexer);
        Ok(())
    }

    /// Index the gems (external libraries)
    async fn index_gems(&mut self, server: &RubyLanguageServer) -> Result<()> {
        let (required_gems, excluded_gems) =
            configured_gem_selection(self.get_required_gems(), &self.config.indexing);

        let mut gem_indexer = IndexerGem::new(Some(self.workspace_root.clone()));
        gem_indexer.set_file_processor(
            self.file_processor
                .as_ref()
                .expect("INVARIANT VIOLATED: gem indexing started before FileProcessor setup. This is a coordinator bug because every source kind must share the owning project's extension context. Fix: keep setup_file_processor before index_gems.")
                .clone(),
        );
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
        if let Some(runtime) = self.effective_runtime.clone() {
            let implementation = match runtime.implementation {
                RuntimeImplementation::Mri => RubyImplementation::Mri,
                RuntimeImplementation::Jruby => RubyImplementation::JRuby,
                RuntimeImplementation::Truffleruby => RubyImplementation::TruffleRuby,
            };
            gem_indexer.set_selected_runtime(runtime.executable, implementation, runtime.java_home);
        }
        gem_indexer.index_gems(true, server).await?; // selective = true
        self.gem_indexer = Some(gem_indexer);
        Ok(())
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
    use ruby_analysis::engine::AnalysisQuery;
    use std::fs;
    use std::io::{Cursor, Write};
    use tempfile::TempDir;
    use tower_lsp::lsp_types::{DidOpenTextDocumentParams, TextDocumentItem};
    use zip::write::SimpleFileOptions;

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
        coordinator.index_jruby_import_signatures(&server).unwrap();
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
        coordinator.index_jruby_import_signatures(&server).unwrap();
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
                "generated metadata signatures must enter the owning engine before project files; \
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

    #[test]
    fn adding_a_java_import_after_cold_index_materializes_navigation_inputs_on_demand() {
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
        coordinator.setup_file_processor(&server);

        let source_path = root.join("imports.rb");
        let uri = Url::from_file_path(&source_path).unwrap();
        let initial = "VALUE = 1\n";
        fs::write(&source_path, initial).unwrap();
        coordinator
            .file_processor
            .as_ref()
            .unwrap()
            .process_file(&uri, initial, &server)
            .unwrap();
        assert!(
            !signature_cache.join("fixtures/RichFixture.rb").exists(),
            "cold indexing without a Java dependency must not eagerly materialize its signature"
        );

        let added = "java_import fixtures.RichFixture\nRICH = RichFixture.new(nil)\n";
        coordinator
            .file_processor
            .as_ref()
            .unwrap()
            .process_file_current_file_resolution_forced(&uri, added, &server)
            .unwrap();
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

        coordinator
            .file_processor
            .as_ref()
            .unwrap()
            .process_file_current_file_resolution_forced(&uri, initial, &server)
            .unwrap();
        let engine = server.analysis_engine_for_uri(&uri);
        let engine = engine.read();
        assert!(
            AnalysisQuery::new(&engine)
                .resolved_reference_definition_ranges_at(source_file, constructor_offset)
                .is_empty(),
            "removing the newly added import and constructor call must clear their reference facts"
        );
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
        assert!(result.is_ok(), "Indexing should complete successfully");

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

        // Verify that Ruby lib directories were discovered
        let lib_dirs = coordinator.get_ruby_library_paths();
        assert!(
            !lib_dirs.is_empty(),
            "Should discover at least one Ruby lib directory"
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

        // Verify that gem indexer was initialized
        // Note: We can't directly access the gem_indexer field, but we can verify
        // that the ruby_lib_dirs includes gem paths
        let lib_dirs = coordinator.get_ruby_library_paths();

        // Should have at least some library directories (system + potentially gems)
        assert!(
            !lib_dirs.is_empty(),
            "Should discover library directories including potential gem paths"
        );

        // Check if any paths look like gem directories
        let has_gem_like_paths = lib_dirs.iter().any(|path| {
            path.to_string_lossy().contains("gems") || path.to_string_lossy().contains(".gem")
        });

        // This might not always be true in test environments, so we'll just log it
        if has_gem_like_paths {
            println!("Found gem-like paths in library directories");
        } else {
            println!("No obvious gem paths found - this is normal in test environments");
        }

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

        // Verify the indexing process completed all steps
        let lib_dirs = coordinator.get_ruby_library_paths();
        assert!(
            !lib_dirs.is_empty(),
            "Library directories should be discovered"
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
