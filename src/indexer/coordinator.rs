use crate::config::RubyFastLspConfig;
use crate::indexer::dependency_tracker::DependencyTracker;
use crate::indexer::indexer_core::IndexerCore;
use crate::indexer::indexer_gem::IndexerGem;
use crate::indexer::indexer_project::IndexerProject;
use crate::indexer::indexer_stdlib::IndexerStdlib;
use crate::indexer::version::version_detector::RubyVersionDetector;
use crate::server::RubyLanguageServer;
use crate::types::ruby_version::RubyVersion;
use anyhow::Result;
use log::{debug, info, warn};
use parking_lot::Mutex;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Instant;

/// Coordinates the modular indexing process following the specified order:
/// 1. version_detector
/// 2. indexer_core  
/// 3. indexer_project (tracks required stdlib and gems)
/// 4. indexer_stdlib
/// 5. indexer_gem
pub struct IndexingCoordinator {
    workspace_root: PathBuf,
    config: RubyFastLspConfig,

    // Phase 1: Version detection
    version_detector: RubyVersionDetector,
    ruby_version: Option<RubyVersion>,

    // Phase 2: Core indexing
    indexer_core: Option<IndexerCore>,

    // Phase 3: Project indexing with dependency tracking
    indexer_project: Option<IndexerProject>,
    dependency_tracker: Arc<Mutex<DependencyTracker>>,

    // Phase 4: Standard library indexing
    indexer_stdlib: Option<IndexerStdlib>,

    // Phase 5: Gem indexing
    indexer_gem: Option<IndexerGem>,

    // Ruby library directories for dependency resolution
    ruby_lib_dirs: Vec<PathBuf>,
}

impl IndexingCoordinator {
    pub fn new(workspace_root: PathBuf, config: RubyFastLspConfig) -> Self {
        let version_detector = RubyVersionDetector::from_path(workspace_root.clone());
        let dependency_tracker = Arc::new(Mutex::new(DependencyTracker::new(
            workspace_root.clone(),
            Vec::new(),
        )));

        Self {
            workspace_root,
            config,
            version_detector,
            ruby_version: None,
            indexer_core: None,
            indexer_project: None,
            dependency_tracker,
            indexer_stdlib: None,
            indexer_gem: None,
            ruby_lib_dirs: Vec::new(),
        }
    }

    /// Execute the complete indexing process in the specified order:
    /// 1. Version detection
    /// 2. Core indexing setup
    /// 3. Project indexing (with stdlib/gem tracking)
    /// 4. Standard library indexing
    /// 5. Gem indexing
    pub async fn execute_indexing(&mut self, server: &RubyLanguageServer) -> Result<()> {
        info!("Starting coordinated indexing process");
        let start_time = Instant::now();

        // 1. Detect Ruby version
        let ruby_version = self.version_detector.detect_version();
        self.ruby_version = ruby_version.clone();
        info!("Detected Ruby version: {:?}", ruby_version);

        // 1.5. Discover Ruby lib directories for dependency resolution
        self.discover_ruby_lib_dirs();

        // 2. Initialize core indexing
        self.indexer_core = Some(IndexerCore::new(server.index()));

        // 3. Initialize and execute project indexing
        let mut indexer_project = IndexerProject::new(
            self.workspace_root.clone(),
            self.indexer_core.as_ref().unwrap().clone(),
            self.dependency_tracker.clone(),
        );
        indexer_project.index_project(server).await?;
        self.indexer_project = Some(indexer_project);

        // Get tracked dependencies from project indexer
        let required_stdlib = if let Some(ref project) = self.indexer_project {
            project.get_required_stdlib()
        } else {
            Vec::new()
        };
        let required_gems = if let Some(ref project) = self.indexer_project {
            project.get_required_gems()
        } else {
            Vec::new()
        };

        // 4. Initialize and execute stdlib indexing
        let mut indexer_stdlib = IndexerStdlib::new(
            self.indexer_core.as_ref().unwrap().clone(),
            ruby_version.clone(),
        );
        indexer_stdlib.set_required_modules(required_stdlib);
        indexer_stdlib.index_stdlib(server).await?;
        self.indexer_stdlib = Some(indexer_stdlib);

        // 5. Initialize and execute gem indexing
        let mut indexer_gem = IndexerGem::new(
            Arc::new(std::sync::Mutex::new(
                self.indexer_core.as_ref().unwrap().clone(),
            )),
            Some(self.workspace_root.clone()),
        );
        indexer_gem.set_required_gems(required_gems.into_iter().collect());
        indexer_gem.index_gems(true).await?; // selective = true
        self.indexer_gem = Some(indexer_gem);

        info!(
            "Coordinated indexing completed in {:?}",
            start_time.elapsed()
        );
        Ok(())
    }

    /// Discover Ruby lib directories deterministically without hardcoding
    pub fn discover_ruby_lib_dirs(&mut self) {
        self.ruby_lib_dirs.clear();

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
                        self.ruby_lib_dirs.push(path);
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
                            self.ruby_lib_dirs.push(gems_dir.clone());
                            debug!("Found gem directory: {:?}", gems_dir);
                        }
                    }
                }
            }
        }
    }

    /// Initialize dependency tracker
    pub fn initialize_dependency_tracker(&mut self) {
        let tracker =
            DependencyTracker::new(self.workspace_root.clone(), self.ruby_lib_dirs.clone());

        self.dependency_tracker = Arc::new(Mutex::new(tracker));
    }

    /// Recursively collect Ruby files from a directory
    pub fn collect_ruby_files_recursive(&self, dir: &Path, files: &mut Vec<PathBuf>) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    // Skip common directories that don't contain Ruby source
                    if let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) {
                        if ![
                            "node_modules",
                            ".git",
                            "tmp",
                            "log",
                            "coverage",
                            ".bundle",
                            "vendor",
                        ]
                        .contains(&dir_name)
                        {
                            self.collect_ruby_files_recursive(&path, files);
                        }
                    }
                } else if self.is_ruby_file(&path) {
                    files.push(path);
                }
            }
        }
    }

    /// Check if a file is a Ruby file based on extension
    pub fn is_ruby_file(&self, path: &Path) -> bool {
        if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
            matches!(extension, "rb" | "ruby" | "rake")
        } else {
            // Check for files without extension that might be Ruby (like Rakefile, Gemfile)
            if let Some(filename) = path.file_name().and_then(|name| name.to_str()) {
                matches!(filename, "Rakefile" | "Gemfile" | "Guardfile" | "Capfile")
            } else {
                false
            }
        }
    }

    /// Get the path to core stubs for a specific Ruby version
    pub fn get_core_stubs_path(&self, version: (u8, u8)) -> Option<PathBuf> {
        // Use the config's stub path resolution which now supports extension_path
        if let Some(stubs_path_str) = self.config.get_core_stubs_path_internal(version) {
            Some(PathBuf::from(stubs_path_str))
        } else {
            // Fallback: Look for stubs in the vsix/stubs directory relative to the project root
            let stubs_dir = self.workspace_root.join("vsix").join("stubs");
            let version_dir = format!("rubystubs{}{}", version.0, version.1);
            let stubs_path = stubs_dir.join(version_dir);

            if stubs_path.exists() {
                debug!("Found core stubs in workspace at: {:?}", stubs_path);
                Some(stubs_path)
            } else {
                // Fallback to a default version if specific version not found
                let default_stubs = stubs_dir.join("rubystubs30");
                if default_stubs.exists() {
                    info!(
                        "Using default core stubs in workspace at: {:?}",
                        default_stubs
                    );
                    Some(default_stubs)
                } else {
                    warn!("No core stubs found for Ruby version {:?}", version);
                    None
                }
            }
        }
    }

    /// Get the discovered Ruby lib directories
    pub fn get_ruby_lib_dirs(&self) -> &[PathBuf] {
        &self.ruby_lib_dirs
    }
}
