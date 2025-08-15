use crate::config::RubyFastLspConfig;
use crate::handlers::helpers::{process_files_parallel, ProcessingMode};
use crate::indexer::version::version_detector::RubyVersionDetector;
use crate::server::RubyLanguageServer;
use crate::types::ruby_version::RubyVersion;
use anyhow::Result;
use log::{debug, info, warn};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;
use tower_lsp::lsp_types::Url;

/// Coordinates the simplified indexing process
pub struct IndexingCoordinator {
    workspace_root: PathBuf,
    ruby_version: Option<RubyVersion>,
    ruby_lib_dirs: Vec<PathBuf>,
    config: RubyFastLspConfig,
}

impl IndexingCoordinator {
    pub fn new(workspace_root: PathBuf, config: RubyFastLspConfig) -> Self {
        Self {
            workspace_root,
            ruby_version: None,
            ruby_lib_dirs: Vec::new(),
            config,
        }
    }

    /// Execute the simplified indexing workflow
    pub async fn execute_indexing(&mut self, server: &RubyLanguageServer) -> Result<()> {
        let start_time = Instant::now();
        info!("Starting simplified workspace indexing");

        // Step 1: Detect Ruby version
        self.detect_ruby_version();
        info!("Detected Ruby version: {:?}", self.ruby_version);

        // Step 2: Index corresponding core stubs
        self.index_core_stubs(server).await?;

        // Step 3: Get current Ruby's lib dirs deterministically
        self.discover_ruby_lib_dirs();
        info!(
            "Discovered {} Ruby lib directories",
            self.ruby_lib_dirs.len()
        );

        // Step 4: Index project's root directory
        self.index_project_root(server).await?;

        let duration = start_time.elapsed();
        info!("Simplified workspace indexing completed in {:?}", duration);
        Ok(())
    }

    /// Detect Ruby version for the workspace
    fn detect_ruby_version(&mut self) {
        if let Ok(workspace_uri) = Url::from_file_path(&self.workspace_root) {
            if let Some(detector) = RubyVersionDetector::new(&workspace_uri) {
                if let Some(version) = detector.detect_version() {
                    info!("Detected Ruby version: {}", version);
                    self.ruby_version = Some(version);
                    return;
                }
            }
        }

        // Fallback to system version detection
        if let Some(system_version) = self.detect_system_ruby_version() {
            let version = RubyVersion::new(system_version.0, system_version.1);
            info!("Using system Ruby version: {}", version);
            self.ruby_version = Some(version);
        } else {
            info!("Could not detect Ruby version, using default 3.0.0");
            self.ruby_version = Some(RubyVersion::new(3, 0));
        }
    }

    /// Index core Ruby stubs based on detected version
    async fn index_core_stubs(&self, server: &RubyLanguageServer) -> Result<()> {
        if let Some(version) = &self.ruby_version {
            let version_tuple = (version.major, version.minor);
            if let Some(stubs_path) = self.get_core_stubs_path(version_tuple) {
                info!("Indexing core stubs from: {:?}", stubs_path);
                let mut files = Vec::new();
                if stubs_path.is_file() && self.is_ruby_file(&stubs_path) {
                    files.push(stubs_path);
                } else if stubs_path.is_dir() {
                    self.collect_ruby_files_recursive(&stubs_path, &mut files);
                }
                if !files.is_empty() {
                    let file_count = files.len();
                    process_files_parallel(server, files, ProcessingMode::Definitions).await?;
                    info!("Indexed {} core stub files", file_count);
                }
            } else {
                debug!("No core stubs found for Ruby version {:?}", version_tuple);
            }
        }
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
                        info!("Found Ruby lib directory: {:?}", path_str.trim());
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
                            info!("Found gem directory: {:?}", gems_dir);
                        }
                    }
                }
            }
        }
    }

    /// Index project's root directory
    async fn index_project_root(&self, server: &RubyLanguageServer) -> Result<()> {
        info!("Indexing project root directory: {:?}", self.workspace_root);

        let mut project_files = Vec::new();
        self.collect_ruby_files_recursive(&self.workspace_root, &mut project_files);

        if !project_files.is_empty() {
            info!("Found {} Ruby files in project", project_files.len());

            // Index definitions first
            process_files_parallel(server, project_files.clone(), ProcessingMode::Definitions)
                .await?;

            // Then collect references
            process_files_parallel(
                server,
                project_files,
                ProcessingMode::References {
                    include_local_vars: false,
                },
            )
            .await?;

            info!("Completed indexing project files");
        } else {
            info!("No Ruby files found in project root");
        }

        Ok(())
    }

    /// Recursively collect Ruby files from a directory
    pub fn collect_ruby_files_recursive(&self, dir: &Path, files: &mut Vec<PathBuf>) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    // Skip common directories that don't contain Ruby source
                    if let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) {
                        if !["node_modules", ".git", "tmp", "log", "coverage", ".bundle"]
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
                info!("Found core stubs in workspace at: {:?}", stubs_path);
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

    /// Detect system Ruby version as fallback
    fn detect_system_ruby_version(&self) -> Option<(u8, u8)> {
        if let Ok(output) = Command::new("ruby").args(["-v"]).output() {
            if output.status.success() {
                let version_str = String::from_utf8_lossy(&output.stdout);
                // Parse version string like "ruby 3.1.0p0 (2021-12-25 revision fb4df44d16) [arm64-darwin21]"
                if let Some(version_part) = version_str.split_whitespace().nth(1) {
                    let version_nums: Vec<&str> = version_part.split('.').collect();
                    if version_nums.len() >= 2 {
                        if let (Ok(major), Ok(minor)) =
                            (version_nums[0].parse::<u8>(), version_nums[1].parse::<u8>())
                        {
                            return Some((major, minor));
                        }
                    }
                }
            }
        }

        // Fallback to default version
        Some((3, 0))
    }

    /// Get the discovered Ruby lib directories
    pub fn get_ruby_lib_dirs(&self) -> &[PathBuf] {
        &self.ruby_lib_dirs
    }
}
