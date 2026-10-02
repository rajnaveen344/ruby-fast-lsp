//! Standard Library Indexing
//!
//! This module handles indexing of Ruby's standard library based on the detected
//! Ruby version and required modules from project dependencies.
//!
//! In production (VSIX), stubs are shipped as zip files and extracted by the
//! VS Code extension on first activation. The LSP server reads from the
//! extracted directories with proper file:// URIs.

use crate::environment::runtime::catalog::RuntimeImplementation;
use crate::environment::runtime::version::RubyVersion;
use crate::loader::file_processor::FileProcessor;
use crate::utils;
use crate::utils::stub_loader::find_stubs_directory;
use anyhow::{anyhow, Context, Result};
use log::{debug, info, warn};
use rayon::prelude::*;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tower_lsp::lsp_types::Url;

mod runtime_paths;

pub(crate) use runtime_paths::{RuntimeStdlibPathKey, RuntimeStdlibPaths};

const CORE_RUNTIME_CONSTANTS_RBS: &str = "constants.rbs";

// ============================================================================
// IndexerStdlib
// ============================================================================

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
        invariant!(
            executable.is_absolute(),
            what = "selected Ruby executable is not absolute: {}",
            why = "runtime resolution yields one exact executable",
            fix = "canonicalize the runtime catalog entry before stdlib discovery",
            executable.display(),
        );
        invariant!(
            java_home.as_ref().is_none_or(|path| path.is_absolute()),
            what = "selected Java home is not absolute: {:?}",
            why = "JRuby subprocesses must inherit one exact JDK identity",
            fix = "validate and canonicalize Java home before configuring stdlib discovery",
            java_home,
        );
        self.runtime_executable = Some(executable);
        self.runtime_java_home = java_home;
    }

    pub(crate) fn set_runtime_stdlib_paths(&mut self, paths: RuntimeStdlibPaths) {
        invariant!(
            paths.paths().iter().all(|path| path.is_absolute()),
            what = "cached runtime stdlib product contains a relative path: {:?}",
            why = "shared runtime products must retain canonical external provenance",
            fix = "canonicalize every exact-runtime load path before publication",
            paths.paths(),
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

    /// Index core stubs if available
    ///
    /// Stubs are loaded from the extension's stubs directory (stubs/rubystubsXY/).
    /// In production, these are extracted from zip files by the VS Code extension
    /// on first activation.
    #[cfg(test)]
    async fn index_core_stubs(
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
                let mut stub_files = utils::file_ops::collect_ruby_files(&stubs_dir);
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

        let mut stub_files = utils::file_ops::collect_ruby_files(&stubs_path);
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
                "invariant violated: embedded Ruby core RBS lacks {CORE_RUNTIME_CONSTANTS_RBS} — bug: runtime constants need a version-independent proof source — fix: keep crates/rbs-parser/rbs_types/core/{CORE_RUNTIME_CONSTANTS_RBS} embedded and exported"
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
                "invariant violated: embedded Ruby core RBS {CORE_RUNTIME_CONSTANTS_RBS} failed to parse: {error} — bug: bundled language semantics must produce valid facts — fix: validate vendored RBS updates before embedding"
            )
        })?;
        engine.replace_facts(file_id, facts, ruby_analysis::engine::ResolveMode::Deferred);
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

        // Reserve stable file identities with the actual source before
        // template-only collection. Its fallback reservation has no content;
        // that would leave valid declaration ranges without an LSP position.
        {
            let mut engine = analysis_engine.write();
            for (path, _, content) in &sources {
                engine.register_file_borrowed(
                    path.clone(),
                    content,
                    ruby_analysis::core::SourceKind::Stub,
                );
            }
        }

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
                unreachable_invariant!(
                    what = "deterministic stub collection lost registered file {}",
                    why = "both direct staging passes register every source before template collection",
                    fix = "preserve the file registration lifecycle through batch commit",
                    path.display(),
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
        if version.implementation != RuntimeImplementation::Jruby {
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
            let mut files = utils::file_ops::collect_ruby_files(&directory);
            files.sort();
            self.index_stub_files_deterministically(&files, analysis_engine.clone())?;
            indexed += files.len();
        }
        info!("Indexed {indexed} JRuby {series} overlay stub files");
        Ok(())
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
                unreachable_invariant!(
                    what = "stdlib collision lookup found file id {:?} for {} without a registered source file",
                    why = "file-path and file-record ownership must be updated atomically",
                    fix = "preserve the AnalysisEngine file lifecycle",
                    file_id,
                    path.display(),
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
                | ruby_analysis::core::SourceKind::Gem => unreachable_invariant!(
                    what = "runtime stdlib path {} is already owned as {:?}",
                    why = "one source cannot have two provenances in one engine",
                    fix = "fix runtime load-path discovery or source registration",
                    path.display(),
                    file.kind,
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
                unreachable_invariant!(
                    what = "deterministic stdlib collection lost registered file {}",
                    why = "the batch registers every source before template collection",
                    fix = "preserve file registration through deterministic stdlib commit",
                    path.display(),
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
                    files.extend(utils::file_ops::collect_ruby_files(&module_dir));
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
mod tests;
