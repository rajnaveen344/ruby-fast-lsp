//! Standard Library Indexing
//!
//! This module handles indexing of Ruby's standard library based on the detected
//! Ruby version and required modules from project dependencies.
//!
//! In production (VSIX), stubs are shipped as zip files and extracted by the
//! VS Code extension on first activation. The LSP server reads from the
//! extracted directories with proper file:// URIs.

use crate::indexer::file_processor::FileProcessor;
use crate::indexer::version::ruby_version::{RubyImplementation, RubyVersion};
use crate::utils;
use crate::utils::stub_loader::find_stubs_directory;
use anyhow::{anyhow, Context, Result};
use log::{debug, info, warn};
use rayon::prelude::*;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Instant;
use std::time::SystemTime;
use tower_lsp::lsp_types::Url;

const CORE_RUNTIME_CONSTANTS_RBS: &str = "constants.rbs";

// ============================================================================
// IndexerStdlib
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct RuntimeExecutableIdentity {
    byte_length: u64,
    modified: SystemTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct RuntimeStdlibPathKey {
    executable: PathBuf,
    executable_identity: RuntimeExecutableIdentity,
    java_home: Option<PathBuf>,
}

impl RuntimeStdlibPathKey {
    pub(crate) fn new(executable: &Path, java_home: Option<&Path>) -> Result<Self> {
        let executable = std::fs::canonicalize(executable).with_context(|| {
            format!(
                "failed to canonicalize selected Ruby executable {} for stdlib discovery",
                executable.display()
            )
        })?;
        let executable_identity = runtime_executable_identity(&executable)?;
        let java_home = java_home
            .map(|path| {
                let canonical = std::fs::canonicalize(path).with_context(|| {
                    format!(
                        "failed to canonicalize selected Java home {} for stdlib discovery",
                        path.display()
                    )
                })?;
                if !canonical.is_dir() {
                    return Err(anyhow!(
                        "selected Java home is not a directory: {}",
                        canonical.display()
                    ));
                }
                Ok(canonical)
            })
            .transpose()?;
        Ok(Self {
            executable,
            executable_identity,
            java_home,
        })
    }

    pub(crate) fn discover(&self) -> Result<RuntimeStdlibPaths> {
        let before = runtime_executable_identity(&self.executable)?;
        if before != self.executable_identity {
            return Err(anyhow!(
                "selected Ruby executable changed before stdlib discovery: {}",
                self.executable.display()
            ));
        }

        let mut command = std::process::Command::new(&self.executable);
        command.args([
            "--disable-gems",
            "-e",
            "STDOUT.write($LOAD_PATH.map { |path| File.expand_path(path) + \"\\0\" }.join)",
        ]);
        for name in [
            "RUBYLIB",
            "RUBYOPT",
            "GEM_HOME",
            "GEM_PATH",
            "BUNDLE_GEMFILE",
            "BUNDLE_PATH",
            "BUNDLE_BIN_PATH",
            "BUNDLE_WITH",
            "BUNDLE_WITHOUT",
        ] {
            command.env_remove(name);
        }
        if let Some(java_home) = self.java_home.as_ref() {
            command.env("JAVA_HOME", java_home);
        }
        let output = command.output().with_context(|| {
            format!(
                "failed to query exact Ruby runtime load path from {}",
                self.executable.display()
            )
        })?;
        if !output.status.success() {
            return Err(anyhow!(
                "exact Ruby runtime {} failed while reporting its stdlib load path (status {}): {}",
                self.executable.display(),
                output.status,
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        let after = runtime_executable_identity(&self.executable)?;
        if after != self.executable_identity {
            return Err(anyhow!(
                "selected Ruby executable changed during stdlib discovery: {}",
                self.executable.display()
            ));
        }

        let mut paths = Vec::new();
        let mut seen = HashSet::new();
        for encoded_path in output.stdout.split(|byte| *byte == 0) {
            if encoded_path.is_empty() {
                continue;
            }
            let path_text = std::str::from_utf8(encoded_path).with_context(|| {
                format!(
                    "exact Ruby runtime {} returned a non-UTF-8 load path",
                    self.executable.display()
                )
            })?;
            let path = PathBuf::from(path_text);
            if !path.is_dir() {
                debug!(
                    "Ignoring missing exact-runtime load path from {}: {}",
                    self.executable.display(),
                    path.display()
                );
                continue;
            }
            let path = std::fs::canonicalize(&path).with_context(|| {
                format!(
                    "failed to canonicalize exact-runtime stdlib path {} from {}",
                    path.display(),
                    self.executable.display()
                )
            })?;
            if seen.insert(path.clone()) {
                debug!("Found exact-runtime stdlib path: {:?}", path);
                paths.push(path);
            }
        }

        info!(
            "Discovered {} stdlib paths from exact runtime {}",
            paths.len(),
            self.executable.display()
        );
        Ok(RuntimeStdlibPaths { paths })
    }
}

fn runtime_executable_identity(path: &Path) -> Result<RuntimeExecutableIdentity> {
    let metadata = std::fs::metadata(path).with_context(|| {
        format!(
            "failed to read selected Ruby executable metadata from {}",
            path.display()
        )
    })?;
    if !metadata.is_file() {
        return Err(anyhow!(
            "selected Ruby executable is not a regular file: {}",
            path.display()
        ));
    }
    Ok(RuntimeExecutableIdentity {
        byte_length: metadata.len(),
        modified: metadata.modified().with_context(|| {
            format!(
                "failed to read selected Ruby executable modification time from {}",
                path.display()
            )
        })?,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuntimeStdlibPaths {
    paths: Vec<PathBuf>,
}

impl RuntimeStdlibPaths {
    pub(crate) fn paths(&self) -> &[PathBuf] {
        &self.paths
    }

    pub(crate) fn estimated_weight_bytes(&self) -> u64 {
        self.paths.iter().fold(256u64, |total, path| {
            total
                .checked_add(u64::try_from(path.as_os_str().len()).expect(
                    "INVARIANT VIOLATED: a runtime stdlib path length does not fit u64. This is a bug because an in-memory path cannot exceed the process address space. Fix: inspect runtime load-path product accounting.",
                ))
                .expect(
                    "INVARIANT VIOLATED: runtime stdlib path-product weight overflowed u64. This is a bug because retained entry count and path storage are bounded. Fix: inspect runtime load-path product accounting.",
                )
        })
    }
}

/// Handles standard library indexing
pub struct IndexerStdlib {
    file_processor: FileProcessor,
    ruby_version: Option<RubyVersion>,
    runtime_executable: Option<PathBuf>,
    runtime_java_home: Option<PathBuf>,
    runtime_stdlib_paths: Option<RuntimeStdlibPaths>,
    stdlib_paths: Vec<PathBuf>,
    required_modules: HashSet<String>,
    /// Optional path to the VS Code extension directory (for loading zipped stubs)
    extension_path: Option<PathBuf>,
}

impl IndexerStdlib {
    pub fn new(file_processor: FileProcessor, ruby_version: Option<RubyVersion>) -> Self {
        Self {
            file_processor,
            ruby_version,
            runtime_executable: None,
            runtime_java_home: None,
            runtime_stdlib_paths: None,
            stdlib_paths: Vec::new(),
            required_modules: HashSet::new(),
            extension_path: None,
        }
    }

    /// Set the extension path for loading zipped stubs
    pub fn set_extension_path(&mut self, path: PathBuf) {
        self.extension_path = Some(path);
    }

    /// Select the exact runtime whose standard-library load path is authoritative.
    pub fn set_selected_runtime(&mut self, executable: PathBuf, java_home: Option<PathBuf>) {
        assert!(
            executable.is_absolute(),
            "INVARIANT VIOLATED: selected Ruby executable is not absolute: {}. This is a bug because project runtime resolution must produce one exact executable identity. Fix: validate and canonicalize the runtime catalog entry before configuring stdlib discovery.",
            executable.display()
        );
        assert!(
            java_home.as_ref().is_none_or(|path| path.is_absolute()),
            "INVARIANT VIOLATED: selected Java home is not absolute: {:?}. This is a bug because JRuby subprocesses must inherit one exact JDK identity. Fix: validate and canonicalize Java home before configuring stdlib discovery.",
            java_home
        );
        self.runtime_executable = Some(executable);
        self.runtime_java_home = java_home;
    }

    pub(crate) fn set_runtime_stdlib_paths(&mut self, paths: RuntimeStdlibPaths) {
        assert!(
            paths.paths().iter().all(|path| path.is_absolute()),
            "INVARIANT VIOLATED: cached runtime stdlib product contains a relative path: {:?}. This is a bug because shared runtime products must retain canonical external provenance. Fix: canonicalize every exact-runtime load path before publication.",
            paths.paths()
        );
        self.runtime_stdlib_paths = Some(paths);
    }

    // ========================================================================
    // Configuration
    // ========================================================================

    /// Set the list of required stdlib modules to index
    pub fn set_required_modules(&mut self, modules: Vec<String>) {
        self.required_modules = modules.into_iter().collect();
        info!(
            "Set {} required stdlib modules",
            self.required_modules.len()
        );
    }

    /// Add a required stdlib module
    pub fn add_required_module(&mut self, module: String) {
        self.required_modules.insert(module);
    }

    // ========================================================================
    // Indexing
    // ========================================================================

    /// Index standard library based on Ruby version and required modules
    pub async fn index_stdlib(
        &mut self,
        analysis_engine: std::sync::Arc<parking_lot::RwLock<ruby_analysis::engine::AnalysisEngine>>,
    ) -> Result<()> {
        let start = Instant::now();
        info!("Starting stdlib indexing");

        self.discover_stdlib_paths()?;

        // Core Ruby classes are language semantics, not optional runtime libraries.
        // Keep them available even when the selected Ruby executable is missing or
        // its stdlib paths cannot be discovered.
        self.index_core_stubs(analysis_engine.clone()).await?;

        if self.stdlib_paths.is_empty() {
            warn!(
                "No runtime stdlib paths found; bundled core stubs remain indexed, skipping required stdlib modules"
            );
            return Ok(());
        }

        // Index required stdlib modules
        self.index_required_modules(analysis_engine).await?;

        info!("Stdlib indexing completed in {:?}", start.elapsed());
        Ok(())
    }

    /// Index core stubs if available
    ///
    /// Stubs are loaded from the extension's stubs directory (stubs/rubystubsXY/).
    /// In production, these are extracted from zip files by the VS Code extension
    /// on first activation.
    pub(crate) async fn index_core_stubs(
        &self,
        analysis_engine: std::sync::Arc<parking_lot::RwLock<ruby_analysis::engine::AnalysisEngine>>,
    ) -> Result<()> {
        self.index_core_stubs_blocking(analysis_engine)
    }

    pub(crate) fn index_core_stubs_blocking(
        &self,
        analysis_engine: std::sync::Arc<parking_lot::RwLock<ruby_analysis::engine::AnalysisEngine>>,
    ) -> Result<()> {
        let version = self
            .ruby_version
            .map(|version| version.to_tuple())
            .unwrap_or_else(|| {
                warn!(
                    "Ruby runtime version unavailable; using Ruby 3.0 core stubs as a conservative language fallback"
                );
                (3, 0)
            });

        // Try to load from extension path first
        if let Some(ref ext_path) = self.extension_path {
            if let Some(stubs_dir) = find_stubs_directory(ext_path, version) {
                let mut stub_files = utils::collect_ruby_files(&stubs_dir);
                stub_files.sort();
                if stub_files.is_empty() {
                    warn!("No stub files found in: {:?}", stubs_dir);
                    self.index_core_runtime_constants(Some(&stubs_dir), analysis_engine.clone())?;
                    analysis_engine.write().resolve();
                    return Ok(());
                }

                info!(
                    "Indexing {} core stubs from: {:?}",
                    stub_files.len(),
                    stubs_dir
                );

                self.index_core_runtime_constants(Some(&stubs_dir), analysis_engine.clone())?;
                self.index_stub_files_deterministically(&stub_files, analysis_engine.clone())?;
                self.index_jruby_overlay_stubs(analysis_engine.clone())?;

                info!("Indexed {} core stub files", stub_files.len());
                return Ok(());
            }
        }

        // Fall back to finding stubs relative to executable (development path)
        let Some(stubs_path) = self.find_core_stubs_path(version) else {
            self.index_core_runtime_constants(None, analysis_engine.clone())?;
            analysis_engine.write().resolve();
            return Ok(());
        };

        info!("Indexing core stubs from directory: {:?}", stubs_path);

        let mut stub_files = utils::collect_ruby_files(&stubs_path);
        stub_files.sort();
        if stub_files.is_empty() {
            warn!("No stub files found in: {:?}", stubs_path);
            return Ok(());
        }

        self.index_core_runtime_constants(Some(&stubs_path), analysis_engine.clone())?;
        self.index_stub_files_deterministically(&stub_files, analysis_engine.clone())?;
        self.index_jruby_overlay_stubs(analysis_engine.clone())?;
        info!("Indexed {} core stub files", stub_files.len());

        Ok(())
    }

    pub(crate) fn index_core_runtime_constants(
        &self,
        stubs_path: Option<&Path>,
        analysis_engine: std::sync::Arc<parking_lot::RwLock<ruby_analysis::engine::AnalysisEngine>>,
    ) -> Result<()> {
        let content = rbs_parser::core_rbs_file(CORE_RUNTIME_CONSTANTS_RBS).ok_or_else(|| {
            anyhow!(
                "INVARIANT VIOLATED: embedded Ruby core RBS is missing {CORE_RUNTIME_CONSTANTS_RBS}. This is a bug because universal runtime constants require a version-independent proof source. Fix: keep crates/rbs-parser/rbs_types/core/{CORE_RUNTIME_CONSTANTS_RBS} embedded and exported."
            )
        })?;
        let path = self.core_runtime_constants_path(stubs_path);
        let mut engine = analysis_engine.write();
        let file_id = engine.register_file_borrowed(
            path,
            content,
            ruby_analysis::core::SourceKind::Signature,
        );
        let facts = ruby_analysis::indexer::index_rbs(file_id, content).map_err(|error| {
            anyhow!(
                "INVARIANT VIOLATED: embedded Ruby core RBS {CORE_RUNTIME_CONSTANTS_RBS} failed to parse: {error}. This is a bug because build-time bundled language semantics must always produce valid facts. Fix: validate the vendored RBS update before embedding it."
            )
        })?;
        engine.replace_facts(
            file_id,
            ruby_analysis::engine::FileFacts {
                symbols: facts.symbols,
                methods: facts.methods,
                method_visibility_overrides: facts.method_visibility_overrides,
                types: facts.types,
                graph_nodes: facts.graph_nodes,
                graph_edges: facts.graph_edges,
                unresolved_graph_edges: facts.unresolved_graph_edges,
                ..Default::default()
            },
            ruby_analysis::engine::ResolveMode::Deferred,
        );
        Ok(())
    }

    fn core_runtime_constants_path(&self, stubs_path: Option<&Path>) -> PathBuf {
        if let Some(path) = stubs_path
            .map(|path| path.join(CORE_RUNTIME_CONSTANTS_RBS))
            .filter(|path| path.is_file())
        {
            return path;
        }

        if let Some(path) = self
            .extension_path
            .as_ref()
            .map(|path| path.join("core-rbs").join(CORE_RUNTIME_CONSTANTS_RBS))
            .filter(|path| path.is_file())
        {
            return path;
        }

        let development_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("crates")
            .join("rbs-parser")
            .join("rbs_types")
            .join("core")
            .join(CORE_RUNTIME_CONSTANTS_RBS);
        if development_path.is_file() {
            return development_path;
        }

        let executable_dir = std::env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| PathBuf::from("."));
        let adjacent = executable_dir
            .join("core-rbs")
            .join(CORE_RUNTIME_CONSTANTS_RBS);
        if adjacent.is_file() {
            return adjacent;
        }
        executable_dir
            .parent()
            .map(|parent| parent.join("core-rbs").join(CORE_RUNTIME_CONSTANTS_RBS))
            .filter(|path| path.is_file())
            .unwrap_or(adjacent)
    }

    fn index_stub_files_deterministically(
        &self,
        stub_files: &[PathBuf],
        analysis_engine: std::sync::Arc<parking_lot::RwLock<ruby_analysis::engine::AnalysisEngine>>,
    ) -> Result<()> {
        let sources = stub_files
            .par_iter()
            .map(|path| {
                let content = std::fs::read_to_string(path)
                    .with_context(|| format!("failed to read bundled stub {}", path.display()))?;
                let uri = Url::from_file_path(path).map_err(|()| {
                    anyhow::anyhow!(
                        "bundled stub path is not a valid file URI: {}",
                        path.display()
                    )
                })?;
                Ok((path.clone(), uri, content))
            })
            .collect::<Result<Vec<_>>>()?;

        // Every collector observes the same immutable pre-batch engine and
        // namespace snapshot. Core-stub files are independent declaration
        // inputs: allowing one sibling's inferred types or declarations to
        // become another sibling's input makes both the output and its cache
        // identity depend on Rayon scheduling. Declarations and aliases within
        // each file remain visible through the collector's local overlay.
        let known_namespaces = std::sync::Arc::new({
            let engine = analysis_engine.read();
            ruby_analysis::engine::AnalysisQuery::new(&engine).known_namespace_fqns()
        });
        let templates = sources
            .par_iter()
            .map(|(_, uri, content)| {
                self.file_processor
                    .collect_project_neutral_file_template_without_insertion(
                        uri,
                        content,
                        analysis_engine.clone(),
                        ruby_analysis::core::SourceKind::Stub,
                        known_namespaces.clone(),
                    )
            })
            .collect::<Result<Vec<_>>>()?;

        let mut engine = analysis_engine.write();
        for ((path, _, _), template) in sources.iter().zip(templates) {
            let file_id = engine.file_id(path).unwrap_or_else(|| {
                panic!(
                    "INVARIANT VIOLATED: deterministic stub collection lost registered file {}. This is a bug because both direct staging passes register every source before template collection. Fix: preserve the file registration lifecycle through batch commit.",
                    path.display()
                )
            });
            engine.replace_facts(
                file_id,
                template.instantiate(file_id),
                ruby_analysis::engine::ResolveMode::Deferred,
            );
        }
        engine.resolve();
        Ok(())
    }

    pub(crate) fn index_runtime_stdlib_deferred_blocking(
        &mut self,
        analysis_engine: std::sync::Arc<parking_lot::RwLock<ruby_analysis::engine::AnalysisEngine>>,
    ) -> Result<()> {
        self.index_runtime_stdlib_blocking_with_resolution(analysis_engine, false)
    }

    fn index_runtime_stdlib_blocking_with_resolution(
        &mut self,
        analysis_engine: std::sync::Arc<parking_lot::RwLock<ruby_analysis::engine::AnalysisEngine>>,
        resolve: bool,
    ) -> Result<()> {
        let start = Instant::now();
        self.discover_stdlib_paths()?;
        if self.stdlib_paths.is_empty() {
            warn!("No runtime stdlib paths found; skipping required stdlib modules");
            return Ok(());
        }
        self.index_required_modules_blocking_with_resolution(analysis_engine, resolve)?;
        info!("Runtime stdlib indexing completed in {:?}", start.elapsed());
        Ok(())
    }

    fn index_jruby_overlay_stubs(
        &self,
        analysis_engine: std::sync::Arc<parking_lot::RwLock<ruby_analysis::engine::AnalysisEngine>>,
    ) -> Result<()> {
        let Some(version) = self.ruby_version else {
            return Ok(());
        };
        if version.implementation != RubyImplementation::JRuby {
            return Ok(());
        }
        let Some(series) = jruby_series_for_compatibility(version.to_tuple()) else {
            warn!(
                "No JRuby stub overlay supports Ruby compatibility version {}.{}; JRuby-specific APIs remain unavailable",
                version.major, version.minor
            );
            return Ok(());
        };

        let packaged_root = self
            .extension_path
            .as_ref()
            .map(|extension_path| extension_path.join("jruby-stubs"));
        let development_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("support")
            .join("jruby")
            .join("stubs");
        let root_has_selected_overlay =
            |root: &Path| root.join("common").is_dir() || root.join(series).is_dir();
        let root = packaged_root
            .filter(|root| root_has_selected_overlay(root))
            .unwrap_or(development_root);

        let mut directories = Vec::new();
        for component in ["common", series] {
            let directory = root.join(component);
            if directory.is_dir() {
                directories.push(directory);
            }
        }

        let mut indexed = 0usize;
        for directory in directories {
            let mut files = utils::collect_ruby_files(&directory);
            files.sort();
            self.index_stub_files_deterministically(&files, analysis_engine.clone())?;
            indexed += files.len();
        }
        info!("Indexed {indexed} JRuby {series} overlay stub files");
        Ok(())
    }

    /// Index only the required stdlib modules
    async fn index_required_modules(
        &self,
        analysis_engine: std::sync::Arc<parking_lot::RwLock<ruby_analysis::engine::AnalysisEngine>>,
    ) -> Result<()> {
        self.index_required_modules_blocking(analysis_engine)
    }

    fn index_required_modules_blocking(
        &self,
        analysis_engine: std::sync::Arc<parking_lot::RwLock<ruby_analysis::engine::AnalysisEngine>>,
    ) -> Result<()> {
        self.index_required_modules_blocking_with_resolution(analysis_engine, true)
    }

    fn index_required_modules_blocking_with_resolution(
        &self,
        analysis_engine: std::sync::Arc<parking_lot::RwLock<ruby_analysis::engine::AnalysisEngine>>,
        resolve: bool,
    ) -> Result<()> {
        if self.required_modules.is_empty() {
            debug!("No required stdlib modules to index");
            return Ok(());
        }

        let total = self.required_modules.len();
        info!("Indexing {} required stdlib modules", total);

        let mut module_names = self.required_modules.iter().collect::<Vec<_>>();
        module_names.sort();
        let mut files = Vec::new();
        for module_name in module_names {
            let Some(module_files) = self.find_module_files(module_name) else {
                debug!("Stdlib module '{}' not found", module_name);
                continue;
            };
            debug!(
                "Indexing stdlib module '{}' ({} files)",
                module_name,
                module_files.len()
            );
            files.extend(module_files);
        }
        files.sort();
        files.dedup();

        // Core stubs are language semantics installed independently of runtime
        // discovery. A runtime probe must never change their ownership merely
        // because an injected load path aliases the packaged stub directory.
        // Other ownership collisions are invalid: one physical file cannot be
        // both project/dependency truth and runtime stdlib truth in one engine.
        files.retain(|path| {
            let engine = analysis_engine.read();
            let Some(file_id) = engine.file_id(path) else {
                return true;
            };
            let file = engine.file(file_id).unwrap_or_else(|| {
                panic!(
                    "INVARIANT VIOLATED: stdlib collision lookup found file id {:?} for {} without a registered source file. This is a bug because file-path and file-record ownership must be updated atomically. Fix: preserve the AnalysisEngine file lifecycle.",
                    file_id,
                    path.display()
                )
            });
            match file.kind {
                ruby_analysis::core::SourceKind::Stub => {
                    debug!(
                        "Skipping runtime stdlib alias of bundled core stub: {}",
                        path.display()
                    );
                    false
                }
                ruby_analysis::core::SourceKind::Stdlib => true,
                ruby_analysis::core::SourceKind::Project
                | ruby_analysis::core::SourceKind::Excluded
                | ruby_analysis::core::SourceKind::Signature
                | ruby_analysis::core::SourceKind::External
                | ruby_analysis::core::SourceKind::Gem => panic!(
                    "INVARIANT VIOLATED: runtime stdlib path {} is already owned as {:?}. This is a bug because one physical source cannot have contradictory semantic provenance in one project engine. Fix: correct exact runtime load-path discovery or source registration before stdlib collection.",
                    path.display(),
                    file.kind
                ),
            }
        });

        let sources = files
            .par_iter()
            .map(|path| {
                let content = std::fs::read_to_string(path)
                    .with_context(|| format!("failed to read runtime stdlib {}", path.display()))?;
                let uri = Url::from_file_path(path).map_err(|()| {
                    anyhow::anyhow!(
                        "runtime stdlib path is not a valid file URI: {}",
                        path.display()
                    )
                })?;
                Ok((path.clone(), uri, content))
            })
            .collect::<Result<Vec<_>>>()?;

        {
            let mut engine = analysis_engine.write();
            for (path, _, content) in &sources {
                engine.register_file(ruby_analysis::engine::SourceFileInput {
                    path: path.clone(),
                    content: content.clone(),
                    kind: ruby_analysis::core::SourceKind::Stdlib,
                });
            }
        }
        let known_namespaces = std::sync::Arc::new({
            let engine = analysis_engine.read();
            ruby_analysis::engine::AnalysisQuery::new(&engine).known_namespace_fqns()
        });
        let templates = sources
            .par_iter()
            .map(|(_, uri, content)| {
                self.file_processor
                    .collect_project_neutral_file_template_without_insertion(
                        uri,
                        content,
                        analysis_engine.clone(),
                        ruby_analysis::core::SourceKind::Stdlib,
                        known_namespaces.clone(),
                    )
            })
            .collect::<Result<Vec<_>>>()?;

        let indexed_count = sources.len();
        let mut engine = analysis_engine.write();
        for ((path, _, _), template) in sources.iter().zip(templates) {
            let file_id = engine.file_id(path).unwrap_or_else(|| {
                panic!(
                    "INVARIANT VIOLATED: deterministic stdlib collection lost registered file {}. This is a bug because the batch registers every source before template collection. Fix: preserve file registration through deterministic stdlib commit.",
                    path.display()
                )
            });
            engine.replace_facts(
                file_id,
                template.instantiate(file_id),
                ruby_analysis::engine::ResolveMode::Deferred,
            );
        }

        if resolve && indexed_count > 0 {
            engine.resolve();
        }

        info!(
            "Indexed {} stdlib files for required modules",
            indexed_count
        );
        Ok(())
    }

    // ========================================================================
    // Path Discovery
    // ========================================================================

    /// Discover standard library paths based on Ruby version
    fn discover_stdlib_paths(&mut self) -> Result<()> {
        self.stdlib_paths.clear();

        if let Some(paths) = self.runtime_stdlib_paths.as_ref() {
            self.stdlib_paths.extend(paths.paths().iter().cloned());
            return Ok(());
        }

        let Some(executable) = self.runtime_executable.as_ref() else {
            warn!(
                "Exact Ruby runtime executable unavailable; bundled core stubs remain indexed, skipping runtime-dependent stdlib discovery"
            );
            return Ok(());
        };

        let product =
            RuntimeStdlibPathKey::new(executable, self.runtime_java_home.as_deref())?.discover()?;
        self.stdlib_paths.extend(product.paths);
        Ok(())
    }

    /// Get the path to core stubs for a specific Ruby version
    fn find_core_stubs_path(&self, version: (u8, u8)) -> Option<PathBuf> {
        let stub_dir = format!("rubystubs{}{}", version.0, version.1);

        let Ok(exe_path) = std::env::current_exe() else {
            return None;
        };

        let exe_dir = exe_path.parent()?;

        // Try various relative paths
        let candidates = [
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("editors")
                .join("vscode")
                .join("vsix")
                .join("stubs")
                .join(&stub_dir),
            exe_dir.join("stubs").join(&stub_dir),
            exe_dir.parent()?.join("stubs").join(&stub_dir),
            exe_dir.parent()?.parent()?.join("stubs").join(&stub_dir),
            exe_dir
                .parent()?
                .parent()?
                .join("editors")
                .join("vscode")
                .join("vsix")
                .join("stubs")
                .join(&stub_dir),
        ];

        candidates.into_iter().find(|p| p.exists())
    }

    /// Find files for a specific stdlib module
    fn find_module_files(&self, module_name: &str) -> Option<Vec<PathBuf>> {
        let mut files = Vec::new();

        for stdlib_path in &self.stdlib_paths {
            // Try direct file match (e.g., json.rb)
            let direct_file = stdlib_path.join(format!("{}.rb", module_name));
            if direct_file.exists() {
                files.push(direct_file);
            }

            // Try directory match for nested modules (e.g., net/http)
            if module_name.contains('/') {
                let dir_file = stdlib_path.join(format!("{}.rb", module_name));
                if dir_file.exists() {
                    files.push(dir_file);
                }

                let module_dir = stdlib_path.join(module_name);
                if module_dir.exists() && module_dir.is_dir() {
                    files.extend(utils::collect_ruby_files(&module_dir));
                }
            }
        }

        files.sort();
        files.dedup();

        if files.is_empty() {
            None
        } else {
            Some(files)
        }
    }

    // ========================================================================
    // Accessors
    // ========================================================================

    pub fn get_stdlib_paths(&self) -> &[PathBuf] {
        &self.stdlib_paths
    }

    pub fn get_required_modules(&self) -> Vec<String> {
        self.required_modules.iter().cloned().collect()
    }

    pub fn is_module_required(&self, module_name: &str) -> bool {
        self.required_modules.contains(module_name)
    }

    pub fn file_processor(&self) -> &FileProcessor {
        &self.file_processor
    }
}

fn jruby_series_for_compatibility(version: (u8, u8)) -> Option<&'static str> {
    match version {
        (2, 2) => Some("9.0"),
        (2, 3) => Some("9.1"),
        (2, 5) => Some("9.2"),
        (2, 6) => Some("9.3"),
        (3, 1) => Some("9.4"),
        (3, 4) => Some("10.0"),
        (4, 0) => Some("10.1"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parking_lot::RwLock;
    use ruby_analysis::core::{
        FullyQualifiedName, MethodParamKind, NamespaceKind, RubyConstant, RubyMethod, RubyType,
    };
    use ruby_analysis::engine::{AnalysisEngine, AnalysisQuery};
    use ruby_analysis::method_store::MethodVisibility;
    use std::fs;
    use std::sync::Arc;
    use tempfile::TempDir;

    #[test]
    fn bundled_jruby_core_seed_is_cross_process_stable() {
        fn fingerprint() -> String {
            let extension_root =
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("editors/vscode/vsix");
            let mut indexer = IndexerStdlib::new(
                FileProcessor::new(),
                Some(RubyVersion::new_with_implementation(
                    2,
                    5,
                    RubyImplementation::JRuby,
                )),
            );
            indexer.set_extension_path(extension_root);
            let engine = Arc::new(RwLock::new(AnalysisEngine::new()));
            indexer
                .index_core_stubs_blocking(engine.clone())
                .expect("bundled JRuby core stubs must index");
            let fingerprint = engine
                .read()
                .semantic_context_fingerprint()
                .stable_bytes()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect();
            fingerprint
        }

        const CHILD_ENV: &str = "RUBY_FAST_LSP_JRUBY_CORE_FINGERPRINT_CHILD";
        if std::env::var_os(CHILD_ENV).is_some() {
            for index in 0..512 {
                let _ = RubyConstant::new(&format!("JrubyCoreFingerprintNoise{index}")).unwrap();
            }
            println!("RUBY_FAST_LSP_JRUBY_CORE_FINGERPRINT={}", fingerprint());
            return;
        }

        let expected = fingerprint();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "indexer::indexer_stdlib::tests::bundled_jruby_core_seed_is_cross_process_stable",
                "--nocapture",
            ])
            .env(CHILD_ENV, "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "JRuby core fingerprint child failed:\nstdout={}\nstderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let child = String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .find_map(|line| line.strip_prefix("RUBY_FAST_LSP_JRUBY_CORE_FINGERPRINT="))
            .unwrap()
            .to_string();
        assert_eq!(child, expected);
    }

    #[tokio::test]
    async fn bundled_ruby_25_signatures_match_observed_runtime_arities() {
        let extension_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("editors/vscode/vsix");
        let mut indexer = IndexerStdlib::new(FileProcessor::new(), Some(RubyVersion::new(2, 5)));
        indexer.set_extension_path(extension_root);
        let engine = Arc::new(RwLock::new(AnalysisEngine::new()));
        indexer
            .index_core_stubs(engine.clone())
            .await
            .expect("bundled Ruby 2.5 core stubs must index");

        let query_guard = engine.read();
        let query = AnalysisQuery::new(&query_guard);
        let string = RubyConstant::new("String").expect("String must be a valid constant");
        let concat = FullyQualifiedName::method(
            vec![string],
            RubyMethod::new("concat").expect("concat must be a valid method"),
        );
        assert!(
            query.methods_for_fqn(&concat).iter().any(|fact| {
                fact.owner.namespace_kind() == Some(NamespaceKind::Instance)
                    && fact
                        .param_facts
                        .iter()
                        .map(|param| param.kind)
                        .eq([MethodParamKind::Rest])
            }),
            "Ruby 2.5 String#concat must accept the runtime's zero-or-more positional shape"
        );

        let big_decimal =
            RubyConstant::new("BigDecimal").expect("BigDecimal must be a valid constant");
        let constructor = FullyQualifiedName::method(
            vec![big_decimal],
            RubyMethod::new("new").expect("new must be a valid method"),
        );
        assert!(
            query.methods_for_fqn(&constructor).iter().any(|fact| {
                fact.owner.namespace_kind() == Some(NamespaceKind::Singleton)
                    && fact
                        .param_facts
                        .iter()
                        .map(|param| param.kind)
                        .eq([MethodParamKind::Required, MethodParamKind::Optional])
            }),
            "Ruby 2.5 BigDecimal.new must accept the runtime's one-or-two positional shape"
        );
    }

    #[test]
    fn every_supported_jruby_series_has_a_parseable_explicit_overlay() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("support/jruby/stubs");
        let common = fs::read_to_string(root.join("common/runtime.rb"))
            .expect("shared JRuby runtime overlay must exist");
        assert!(
            ruby_prism::parse(common.as_bytes())
                .errors()
                .next()
                .is_none(),
            "shared JRuby runtime overlay must parse"
        );
        for series in ruby_fast_lsp_jruby_support::JrubySeries::SUPPORTED {
            let compatibility = series.ruby_compatibility();
            assert_eq!(
                jruby_series_for_compatibility((
                    u8::try_from(compatibility.major).unwrap(),
                    u8::try_from(compatibility.minor).unwrap()
                )),
                Some(series.overlay_name())
            );
            let path = root.join(series.overlay_name()).join("runtime.rb");
            let source = fs::read_to_string(&path).unwrap_or_else(|error| {
                panic!(
                    "supported {} overlay is missing at {}: {error}",
                    series.label(),
                    path.display()
                )
            });
            assert!(
                ruby_prism::parse(source.as_bytes())
                    .errors()
                    .next()
                    .is_none(),
                "{} overlay must parse",
                series.label()
            );
        }
    }

    #[tokio::test]
    async fn every_supported_jruby_series_composes_its_exact_runtime_overlay() {
        let repository_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("support/jruby/stubs");
        for series in ruby_fast_lsp_jruby_support::JrubySeries::SUPPORTED {
            let compatibility = series.ruby_compatibility();
            let major = u8::try_from(compatibility.major).unwrap();
            let minor = u8::try_from(compatibility.minor).unwrap();
            let extension = TempDir::new().unwrap();
            let core = extension
                .path()
                .join("stubs")
                .join(format!("rubystubs{major}{minor}"));
            let common = extension.path().join("jruby-stubs/common");
            let selected = extension
                .path()
                .join("jruby-stubs")
                .join(series.overlay_name());
            fs::create_dir_all(&core).unwrap();
            fs::create_dir_all(&common).unwrap();
            fs::create_dir_all(&selected).unwrap();
            fs::write(core.join("object.rb"), "class Object\nend\n").unwrap();
            fs::copy(
                repository_root.join("common/runtime.rb"),
                common.join("runtime.rb"),
            )
            .unwrap();
            fs::copy(
                repository_root
                    .join(series.overlay_name())
                    .join("runtime.rb"),
                selected.join("runtime.rb"),
            )
            .unwrap();

            let mut indexer = IndexerStdlib::new(
                FileProcessor::new(),
                Some(RubyVersion::new_with_implementation(
                    major,
                    minor,
                    RubyImplementation::JRuby,
                )),
            );
            indexer.set_extension_path(extension.path().to_path_buf());
            let engine = Arc::new(RwLock::new(AnalysisEngine::new()));
            indexer.index_core_stubs(engine.clone()).await.unwrap();

            let java_import = FullyQualifiedName::method(
                vec![RubyConstant::new("Object").unwrap()],
                RubyMethod::new("java_import").unwrap(),
            );
            let jruby_version =
                FullyQualifiedName::constant(vec![RubyConstant::new("JRUBY_VERSION").unwrap()]);
            let engine = engine.read();
            assert!(
                !AnalysisQuery::new(&engine)
                    .methods_for_fqn(&java_import)
                    .is_empty(),
                "{} must compose the shared JRuby java_import contract",
                series.label()
            );
            assert!(
                !AnalysisQuery::new(&engine)
                    .symbols_for_fqn(&jruby_version)
                    .is_empty(),
                "{} must compose JRUBY_VERSION",
                series.label()
            );
            assert!(
                engine.file_id(&selected.join("runtime.rb")).is_some(),
                "{} must index its exact selected overlay file",
                series.label()
            );
            assert!(
                engine
                    .files()
                    .filter(|file| file.path.ends_with("jruby-stubs/common/runtime.rb"))
                    .count()
                    == 1,
                "{} must compose the common overlay exactly once",
                series.label()
            );
        }
    }

    #[tokio::test]
    async fn unknown_runtime_still_loads_default_core_stubs() {
        let extension = TempDir::new().expect("test extension directory must be created");
        let stubs = extension.path().join("stubs").join("rubystubs30");
        fs::create_dir_all(&stubs).expect("test stub directory must be created");
        fs::write(
            stubs.join("thread.rb"),
            "class Thread\n  def self.new\n  end\nend\n",
        )
        .expect("Thread stub must be written");

        let mut indexer = IndexerStdlib::new(FileProcessor::new(), None);
        indexer.set_extension_path(extension.path().to_path_buf());
        let engine = Arc::new(RwLock::new(AnalysisEngine::new()));

        indexer
            .index_core_stubs(engine.clone())
            .await
            .expect("bundled core stubs must remain usable without a detected runtime");

        let thread = FullyQualifiedName::namespace(vec![
            RubyConstant::new("Thread").expect("Thread must be a valid Ruby constant")
        ]);
        assert!(
            !AnalysisQuery::new(&engine.read())
                .symbols_for_fqn(&thread)
                .is_empty(),
            "Thread must resolve from default bundled core stubs when runtime detection fails"
        );

        let argv = FullyQualifiedName::constant(vec![
            RubyConstant::new("ARGV").expect("ARGV must be a valid Ruby constant")
        ]);
        {
            let engine = engine.read();
            let query = AnalysisQuery::new(&engine);
            assert!(
                !query.symbols_for_fqn(&argv).is_empty(),
                "ARGV must resolve from embedded core RBS when runtime detection fails"
            );
            assert_eq!(
                query.constant_value_type(&argv),
                Some(RubyType::array_of(RubyType::string())),
                "ARGV must retain its proven Array[String] type from embedded core RBS"
            );
        }

        let project = extension.path().join("project.rb");
        let project_uri = Url::from_file_path(&project)
            .expect("temporary project path must convert to a file URI");
        let source = "ARGV.first.upcase\n";
        indexer
            .file_processor()
            .collect_file_facts_as_deferred_resolution_in_engine(
                &project_uri,
                source,
                engine.clone(),
                ruby_analysis::core::SourceKind::Project,
            )
            .expect("project source using ARGV must index");
        engine.write().resolve();

        let engine = engine.read();
        let file_id = engine
            .file_id(&project)
            .expect("project source must remain registered");
        let query = AnalysisQuery::new(&engine);
        assert_eq!(
            query.expression_type_at(file_id, 14),
            Some(RubyType::string()),
            "ARGV.first.upcase must preserve the proven generic String type through the chain; reason={:?}",
            query.expression_unknown_reason_at(file_id, 14)
        );
        assert!(
            query.diagnostic_facts_in_file(file_id).is_empty(),
            "a fully proven ARGV method chain must not emit semantic diagnostics: {:?}",
            query.diagnostic_facts_in_file(file_id)
        );
    }

    #[test]
    fn runtime_stdlib_discovery_without_an_exact_runtime_does_not_use_path() {
        let mut indexer = IndexerStdlib::new(FileProcessor::new(), Some(RubyVersion::new(3, 0)));

        indexer
            .discover_stdlib_paths()
            .expect("missing exact runtime must be a supported stub-only state");

        assert!(
            indexer.stdlib_paths.is_empty(),
            "runtime stdlib discovery must not borrow whichever Ruby happens to be on the server PATH: {:?}",
            indexer.stdlib_paths
        );
    }

    #[test]
    fn runtime_stdlib_path_key_changes_with_the_runtime_executable() {
        let fixture = TempDir::new().expect("runtime fixture directory must be created");
        let executable = fixture.path().join("ruby");
        fs::write(&executable, b"runtime-v1").expect("runtime fixture must be written");
        let before =
            RuntimeStdlibPathKey::new(&executable, None).expect("runtime key must be constructed");

        fs::write(&executable, b"runtime-v2-with-different-length")
            .expect("runtime fixture must be replaced");
        let after = RuntimeStdlibPathKey::new(&executable, None)
            .expect("replacement runtime key must be constructed");

        assert_ne!(
            before, after,
            "replacing a runtime in place must reserve a new immutable stdlib-path product identity"
        );
    }

    #[test]
    fn runtime_stdlib_cannot_replace_bundled_stub_ownership() {
        let fixture = TempDir::new().expect("stdlib fixture directory must be created");
        let path = fixture.path().join("runtime_probe.rb");
        fs::write(&path, "class RuntimeProbe\nend\n").expect("stdlib fixture must be written");

        let mut indexer = IndexerStdlib::new(FileProcessor::new(), Some(RubyVersion::new(3, 0)));
        let engine = Arc::new(RwLock::new(AnalysisEngine::new()));
        indexer
            .index_stub_files_deterministically(std::slice::from_ref(&path), engine.clone())
            .expect("stub fixture must index");
        indexer.stdlib_paths.push(fixture.path().to_path_buf());
        indexer.add_required_module("runtime_probe".to_string());

        indexer
            .index_required_modules_blocking_with_resolution(engine.clone(), false)
            .expect("runtime stdlib collection must succeed");

        let engine = engine.read();
        let file_id = engine
            .file_id(&path)
            .expect("stub fixture must retain a registered file");
        assert_eq!(
            engine.file(file_id).expect("stub fixture must exist").kind,
            ruby_analysis::core::SourceKind::Stub,
            "runtime stdlib discovery must never reclassify bundled language semantics"
        );
    }

    #[cfg(unix)]
    #[test]
    fn runtime_stdlib_discovery_uses_the_exact_selected_executable() {
        use std::os::unix::fs::PermissionsExt;

        let fixture = TempDir::new().expect("runtime fixture directory must be created");
        let runtime_root = fixture.path().join("exact-runtime");
        let runtime_bin = runtime_root.join("bin");
        let runtime_stdlib = runtime_root.join("lib/ruby/stdlib");
        fs::create_dir_all(&runtime_bin).expect("runtime bin directory must be created");
        fs::create_dir_all(&runtime_stdlib).expect("runtime stdlib directory must be created");
        let executable = runtime_bin.join("ruby");
        fs::write(
            &executable,
            "#!/bin/sh\nruntime_root=$(CDPATH= cd -- \"$(dirname -- \"$0\")/..\" && pwd)\nprintf '%s\\0' \"$runtime_root/lib/ruby/stdlib\"\n",
        )
        .expect("fake exact runtime must be written");
        let mut permissions = fs::metadata(&executable)
            .expect("fake exact runtime metadata must exist")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&executable, permissions)
            .expect("fake exact runtime must be executable");

        let mut indexer = IndexerStdlib::new(FileProcessor::new(), Some(RubyVersion::new(3, 0)));
        indexer.set_selected_runtime(executable, None);
        indexer
            .discover_stdlib_paths()
            .expect("exact runtime stdlib discovery must succeed");

        assert_eq!(
            indexer.stdlib_paths,
            vec![fs::canonicalize(runtime_stdlib).expect("runtime stdlib must canonicalize")],
            "stdlib discovery must use only the exact selected runtime's load path"
        );
    }

    #[test]
    fn runtime_stdlib_deferred_collection_leaves_resolution_to_the_coordinator() {
        let fixture = TempDir::new().expect("stdlib fixture directory must be created");
        let child_path = fixture.path().join("runtime_probe").join("root.rb");
        let base_dir = fixture.path().join("runtime_probe").join("root");
        let base_path = base_dir.join("base.rb");
        fs::create_dir_all(&base_dir).expect("nested stdlib fixture must be created");
        fs::write(&child_path, "class RuntimeChild < RuntimeBase\nend\n")
            .expect("stdlib child fixture must be written");
        fs::write(&base_path, "class RuntimeBase\nend\n")
            .expect("stdlib base fixture must be written");

        let mut indexer = IndexerStdlib::new(FileProcessor::new(), Some(RubyVersion::new(3, 0)));
        indexer.stdlib_paths.push(fixture.path().to_path_buf());
        indexer.add_required_module("runtime_probe/root".to_string());
        let engine = Arc::new(RwLock::new(AnalysisEngine::new()));

        indexer
            .index_required_modules_blocking_with_resolution(engine.clone(), false)
            .expect("deferred stdlib collection must succeed");

        let runtime_base =
            RubyConstant::new("RuntimeBase").expect("RuntimeBase must be a valid Ruby constant");
        assert!(
            engine
                .read()
                .unresolved_graph_edges()
                .iter()
                .any(|edge| edge.target_parts == vec![runtime_base]),
            "deferred stdlib collection must leave cross-file inheritance for the coordinator"
        );

        engine.write().resolve();
        assert!(
            engine
                .read()
                .unresolved_graph_edges()
                .iter()
                .all(|edge| edge.target_parts != vec![runtime_base]),
            "the coordinator's one final resolution must connect the deferred stdlib graph edge"
        );
        assert!(
            AnalysisQuery::new(&engine.read())
                .debug_ancestors("RuntimeChild")
                .ancestors
                .iter()
                .any(|ancestor| {
                    ancestor.name == "RuntimeBase" && ancestor.kind == "superclass"
                }),
            "the coordinator's one final resolution must materialize stdlib inheritance"
        );
    }

    #[tokio::test]
    async fn jruby_9_2_loads_jruby_overlay_without_exposing_it_to_mri() {
        let extension = TempDir::new().expect("test extension directory must be created");
        let stubs = extension.path().join("stubs").join("rubystubs25");
        let jruby_overlay = extension.path().join("jruby-stubs").join("9.2");
        let jruby_common = extension.path().join("jruby-stubs").join("common");
        fs::create_dir_all(&stubs).expect("MRI stub directory must be created");
        fs::create_dir_all(&jruby_overlay).expect("JRuby overlay directory must be created");
        fs::create_dir_all(&jruby_common).expect("JRuby common directory must be created");
        fs::write(stubs.join("object.rb"), "class Object\nend\n")
            .expect("Object stub must be written");
        fs::copy(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("support/jruby/stubs/9.2/runtime.rb"),
            jruby_overlay.join("runtime.rb"),
        )
        .expect("repository JRuby 9.2 overlay must be copied into the isolated test extension");
        fs::copy(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("support/jruby/stubs/common/runtime.rb"),
            jruby_common.join("runtime.rb"),
        )
        .expect("repository JRuby common overlay must be copied into the isolated test extension");
        fs::write(
            stubs.join("process.rb"),
            "module Process\n  def self.fork\n  end\nend\n",
        )
        .expect("Process baseline stub must be written");
        fs::write(
            stubs.join("object_space.rb"),
            "module ObjectSpace\n  def self.dump(object)\n  end\nend\n",
        )
        .expect("ObjectSpace baseline stub must be written");

        let method = FullyQualifiedName::method(
            vec![RubyConstant::new("Object").expect("Object must be a valid Ruby constant")],
            RubyMethod::new("java_import").expect("java_import must be a valid Ruby method"),
        );

        let mut jruby_indexer = IndexerStdlib::new(
            FileProcessor::new(),
            Some(RubyVersion::new_with_implementation(
                2,
                5,
                RubyImplementation::JRuby,
            )),
        );
        jruby_indexer.set_extension_path(extension.path().to_path_buf());
        let jruby_engine = Arc::new(RwLock::new(AnalysisEngine::new()));
        jruby_indexer
            .index_core_stubs(jruby_engine.clone())
            .await
            .expect("JRuby core and overlay stubs must index");
        assert!(
            !AnalysisQuery::new(&jruby_engine.read())
                .methods_for_fqn(&method)
                .is_empty(),
            "JRuby 9.2 must expose Object#java_import from its implementation overlay"
        );
        let required_instance_methods = [
            ("Object", "java_import", MethodVisibility::Private),
            ("Object", "java_kind_of?", MethodVisibility::Public),
            ("Module", "java_alias", MethodVisibility::Private),
            ("Module", "include_package", MethodVisibility::Private),
            ("Kernel", "java_package", MethodVisibility::Public),
            ("Kernel", "to_java", MethodVisibility::Public),
            ("Kernel", "java_signature", MethodVisibility::Public),
            ("Kernel", "java_implements", MethodVisibility::Public),
            ("JavaProxy", "java_send", MethodVisibility::Public),
            ("JavaProxy", "java_method", MethodVisibility::Public),
            ("JavaProxyMethods", "java_class", MethodVisibility::Public),
            ("JavaProxyMethods", "java_object", MethodVisibility::Public),
            ("JavaProxyMethods", "synchronized", MethodVisibility::Public),
            ("Class", "java_class", MethodVisibility::Public),
            ("String", "to_java_bytes", MethodVisibility::Public),
        ];
        let jruby_engine_guard = jruby_engine.read();
        let query = AnalysisQuery::new(&jruby_engine_guard);
        for (owner_name, method_name, visibility) in required_instance_methods {
            let owner_part =
                RubyConstant::new(owner_name).expect("test owner must be a valid Ruby constant");
            let owner = FullyQualifiedName::namespace(vec![owner_part]);
            let method_fqn = FullyQualifiedName::method(
                vec![owner_part],
                RubyMethod::new(method_name).expect("test method must be a valid Ruby method"),
            );
            assert!(
                query.methods_for_fqn(&method_fqn).iter().any(|fact| {
                    fact.owner == owner && fact.visibility == visibility
                }),
                "JRuby 9.2 overlay must declare {owner_name}#{method_name} with {visibility:?} visibility"
            );
        }
        for constant_name in [
            "JRUBY_VERSION",
            "JRUBY_REVISION",
            "Java",
            "JavaUtilities",
            "JavaProxyMethods",
            "JavaProxy",
            "ConcreteJavaProxy",
            "ArrayJavaProxy",
        ] {
            let constant = RubyConstant::new(constant_name)
                .expect("test constant must be a valid Ruby constant");
            let namespace = FullyQualifiedName::namespace(vec![constant]);
            let value = FullyQualifiedName::constant(vec![constant]);
            assert!(
                !query.symbols_for_fqn(&namespace).is_empty()
                    || !query.symbols_for_fqn(&value).is_empty(),
                "JRuby 9.2 overlay must declare runtime constant {constant_name}"
            );
        }
        let process = RubyConstant::new("Process").expect("Process must be a valid Ruby constant");
        let fork = RubyMethod::new("fork").expect("fork must be a valid Ruby method");
        let effective_fork_facts = jruby_engine_guard.method_facts_matching_owner_name(
            &FullyQualifiedName::singleton_namespace(vec![process]),
            &fork,
        );
        assert_eq!(
            effective_fork_facts.len(),
            1,
            "the JRuby overlay must replace the compatible baseline declaration instead of making Process.fork ambiguous: {effective_fork_facts:?}"
        );
        assert!(
            matches!(
                effective_fork_facts[0].availability,
                ruby_analysis::core::MethodAvailability::Unavailable { .. }
            ),
            "Process.fork must remain known but explicitly unavailable under JRuby 9.2"
        );
        let object_space =
            RubyConstant::new("ObjectSpace").expect("ObjectSpace must be a valid Ruby constant");
        let dump = RubyMethod::new("dump").expect("dump must be a valid Ruby method");
        assert!(
            jruby_engine_guard
                .method_facts_matching_owner_name(
                    &FullyQualifiedName::singleton_namespace(vec![object_space]),
                    &dump,
                )
                .is_empty(),
            "JRuby 9.2's absent ObjectSpace.dump marker must mask the MRI 2.5 baseline"
        );
        drop(jruby_engine_guard);

        let mut mri_indexer =
            IndexerStdlib::new(FileProcessor::new(), Some(RubyVersion::new(2, 5)));
        mri_indexer.set_extension_path(extension.path().to_path_buf());
        let mri_engine = Arc::new(RwLock::new(AnalysisEngine::new()));
        mri_indexer
            .index_core_stubs(mri_engine.clone())
            .await
            .expect("MRI core stubs must index");
        assert!(
            AnalysisQuery::new(&mri_engine.read())
                .methods_for_fqn(&method)
                .is_empty(),
            "MRI must not receive JRuby-only methods"
        );
    }
}
