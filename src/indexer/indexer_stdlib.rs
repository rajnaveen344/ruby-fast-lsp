//! Standard Library Indexing
//!
//! This module handles indexing of Ruby's standard library based on the detected
//! Ruby version and required modules from project dependencies.
//!
//! In production (VSIX), stubs are shipped as zip files and extracted by the
//! VS Code extension on first activation. The LSP server reads from the
//! extracted directories with proper file:// URIs.

use crate::indexer::coordinator::IndexingCoordinator;
use crate::indexer::file_processor::FileProcessor;
use crate::indexer::version::ruby_version::{RubyImplementation, RubyVersion};
use crate::server::RubyLanguageServer;
use crate::utils;
use crate::utils::stub_loader::find_stubs_directory;
use anyhow::Result;
use log::{debug, info, warn};
use rayon::prelude::*;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tower_lsp::lsp_types::Url;

// ============================================================================
// IndexerStdlib
// ============================================================================

/// Handles standard library indexing
pub struct IndexerStdlib {
    file_processor: FileProcessor,
    ruby_version: Option<RubyVersion>,
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
            stdlib_paths: Vec::new(),
            required_modules: HashSet::new(),
            extension_path: None,
        }
    }

    /// Set the extension path for loading zipped stubs
    pub fn set_extension_path(&mut self, path: PathBuf) {
        self.extension_path = Some(path);
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
        server: &RubyLanguageServer,
        analysis_engine: std::sync::Arc<parking_lot::RwLock<ruby_analysis::engine::AnalysisEngine>>,
    ) -> Result<()> {
        let start = Instant::now();
        info!("Starting stdlib indexing");

        self.discover_stdlib_paths();

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
        self.index_required_modules(server, analysis_engine).await?;

        info!("Stdlib indexing completed in {:?}", start.elapsed());
        Ok(())
    }

    /// Index core stubs if available
    ///
    /// Stubs are loaded from the extension's stubs directory (stubs/rubystubsXY/).
    /// In production, these are extracted from zip files by the VS Code extension
    /// on first activation.
    async fn index_core_stubs(
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
                let stub_files = utils::collect_ruby_files(&stubs_dir);
                if stub_files.is_empty() {
                    warn!("No stub files found in: {:?}", stubs_dir);
                    return Ok(());
                }

                info!(
                    "Indexing {} core stubs from: {:?}",
                    stub_files.len(),
                    stubs_dir
                );

                let processor = &self.file_processor;
                stub_files.par_iter().for_each(|path| {
                    if let Ok(content) = std::fs::read_to_string(path) {
                        if let Ok(uri) = Url::from_file_path(path) {
                            if let Err(e) = processor
                                .collect_file_facts_as_deferred_resolution_in_engine(
                                    &uri,
                                    &content,
                                    analysis_engine.clone(),
                                    ruby_analysis::core::SourceKind::Stub,
                                )
                            {
                                warn!("Failed to index stub {:?}: {}", path, e);
                            }
                        }
                    }
                });

                self.index_jruby_overlay_stubs(analysis_engine.clone());
                analysis_engine.write().resolve();

                info!("Indexed {} core stub files", stub_files.len());
                return Ok(());
            }
        }

        // Fall back to finding stubs relative to executable (development path)
        let Some(stubs_path) = self.find_core_stubs_path(version) else {
            return Ok(());
        };

        info!("Indexing core stubs from directory: {:?}", stubs_path);

        let stub_files = utils::collect_ruby_files(&stubs_path);
        if stub_files.is_empty() {
            warn!("No stub files found in: {:?}", stubs_path);
            return Ok(());
        }

        let processor = &self.file_processor;
        stub_files.par_iter().for_each(|path| {
            if let Ok(content) = std::fs::read_to_string(path) {
                if let Ok(uri) = Url::from_file_path(path) {
                    if let Err(e) = processor.collect_file_facts_as_deferred_resolution_in_engine(
                        &uri,
                        &content,
                        analysis_engine.clone(),
                        ruby_analysis::core::SourceKind::Stub,
                    ) {
                        warn!("Failed to index stub {:?}: {}", path, e);
                    }
                }
            }
        });
        self.index_jruby_overlay_stubs(analysis_engine.clone());
        analysis_engine.write().resolve();
        info!("Indexed {} core stub files", stub_files.len());

        Ok(())
    }

    fn index_jruby_overlay_stubs(
        &self,
        analysis_engine: std::sync::Arc<parking_lot::RwLock<ruby_analysis::engine::AnalysisEngine>>,
    ) {
        let Some(version) = self.ruby_version else {
            return;
        };
        if version.implementation != RubyImplementation::JRuby {
            return;
        }
        let Some(series) = jruby_series_for_compatibility(version.to_tuple()) else {
            warn!(
                "No JRuby stub overlay supports Ruby compatibility version {}.{}; JRuby-specific APIs remain unavailable",
                version.major, version.minor
            );
            return;
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

        let processor = &self.file_processor;
        let mut indexed = 0usize;
        for directory in directories {
            let files = utils::collect_ruby_files(&directory);
            files.par_iter().for_each(|path| {
                let content = std::fs::read_to_string(path).unwrap_or_else(|error| {
                    panic!(
                        "INVARIANT VIOLATED: discovered JRuby stub `{}` could not be read: {error}. \
                         This is a bug because stub discovery must only return readable regular files. \
                         Fix: validate packaged stub files before indexing.",
                        path.display()
                    )
                });
                let uri = Url::from_file_path(path).unwrap_or_else(|()| {
                    panic!(
                        "INVARIANT VIOLATED: JRuby stub path `{}` could not become a file URI. \
                         This is a bug because indexed stub paths must be absolute filesystem paths. \
                         Fix: canonicalize the JRuby stub root before discovery.",
                        path.display()
                    )
                });
                processor
                    .collect_file_facts_as_deferred_resolution_in_engine(
                        &uri,
                        &content,
                        analysis_engine.clone(),
                        ruby_analysis::core::SourceKind::Stub,
                    )
                    .unwrap_or_else(|error| {
                        panic!(
                            "INVARIANT VIOLATED: JRuby stub `{}` failed semantic indexing: {error}. \
                             This is a bug because bundled overlays must be valid Ruby source. \
                             Fix: correct the overlay or its stub composition tests.",
                            path.display()
                        )
                    });
            });
            indexed += files.len();
        }
        info!("Indexed {indexed} JRuby {series} overlay stub files");
    }

    /// Index only the required stdlib modules
    async fn index_required_modules(
        &self,
        server: &RubyLanguageServer,
        analysis_engine: std::sync::Arc<parking_lot::RwLock<ruby_analysis::engine::AnalysisEngine>>,
    ) -> Result<()> {
        if self.required_modules.is_empty() {
            debug!("No required stdlib modules to index");
            return Ok(());
        }

        let total = self.required_modules.len();
        info!("Indexing {} required stdlib modules", total);

        let mut indexed_count = 0;

        for (current, module_name) in self.required_modules.iter().enumerate() {
            IndexingCoordinator::send_progress_report(
                server,
                "Indexing Stdlib".to_string(),
                current + 1,
                total,
            )
            .await;

            let Some(files) = self.find_module_files(module_name) else {
                debug!("Stdlib module '{}' not found", module_name);
                continue;
            };

            debug!(
                "Indexing stdlib module '{}' ({} files)",
                module_name,
                files.len()
            );

            let processor = &self.file_processor;
            files.par_iter().for_each(|path| {
                if let Ok(content) = std::fs::read_to_string(path) {
                    if let Ok(uri) = Url::from_file_path(path) {
                        if let Err(e) = processor
                            .collect_file_facts_as_deferred_resolution_in_engine(
                                &uri,
                                &content,
                                analysis_engine.clone(),
                                ruby_analysis::core::SourceKind::Stdlib,
                            )
                        {
                            warn!("Failed to index stdlib file {:?}: {}", path, e);
                        }
                    }
                }
            });

            indexed_count += files.len();
        }

        if indexed_count > 0 {
            analysis_engine.write().resolve();
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
    fn discover_stdlib_paths(&mut self) {
        self.stdlib_paths.clear();

        if let Some(version) = self.ruby_version {
            self.discover_version_specific_paths(&version);
        }

        self.discover_system_stdlib_paths();
        self.discover_bundled_stubs();

        info!("Discovered {} stdlib paths", self.stdlib_paths.len());
    }

    /// Discover version-specific stdlib paths
    fn discover_version_specific_paths(&mut self, version: &RubyVersion) {
        let version_str = version.to_string();
        let home = std::env::var("HOME").unwrap_or_default();

        let potential_paths = [
            format!("/usr/lib/ruby/{}", version_str),
            format!("/usr/local/lib/ruby/{}", version_str),
            format!("/opt/ruby/{}/lib/ruby/{}", version_str, version_str),
            format!(
                "{}/.rbenv/versions/{}/lib/ruby/{}",
                home, version_str, version_str
            ),
            format!(
                "{}/.rvm/rubies/ruby-{}/lib/ruby/{}",
                home, version_str, version_str
            ),
        ];

        for path_str in potential_paths {
            let path = PathBuf::from(path_str);
            if path.exists() && path.is_dir() {
                debug!("Found version-specific stdlib path: {:?}", path);
                self.stdlib_paths.push(path);
            }
        }
    }

    /// Discover system Ruby stdlib paths
    fn discover_system_stdlib_paths(&mut self) {
        let Ok(output) = std::process::Command::new("ruby")
            .args([
                "-e",
                "puts $LOAD_PATH.select { |p| p.include?('ruby') && !p.include?('gems') }",
            ])
            .output()
        else {
            return;
        };

        if !output.status.success() {
            return;
        }

        for line in String::from_utf8_lossy(&output.stdout).lines() {
            let path = PathBuf::from(line.trim());
            if path.exists() && path.is_dir() {
                debug!("Found system stdlib path: {:?}", path);
                self.stdlib_paths.push(path);
            }
        }
    }

    /// Discover bundled stub files
    fn discover_bundled_stubs(&mut self) {
        let Some(version) = &self.ruby_version else {
            return;
        };

        if let Some(path) = self.find_core_stubs_path(version.to_tuple()) {
            if path.exists() {
                debug!("Found bundled stubs: {:?}", path);
                self.stdlib_paths.push(path);
            }
        }
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
    use ruby_analysis::core::{FullyQualifiedName, RubyConstant, RubyMethod};
    use ruby_analysis::engine::{AnalysisEngine, AnalysisQuery};
    use ruby_analysis::method_store::MethodVisibility;
    use std::fs;
    use std::sync::Arc;
    use tempfile::TempDir;

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
