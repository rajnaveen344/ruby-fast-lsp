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
    
    // Ruby version info
    version_detector: RubyVersionDetector,
    detected_ruby_version: Option<RubyVersion>,
    
    // The main indexing engine
    core_indexer: Option<IndexerCore>,
    
    // Project-specific indexer
    project_indexer: Option<IndexerProject>,
    dependency_tracker: Arc<Mutex<DependencyTracker>>,
    
    // Standard library indexer
    stdlib_indexer: Option<IndexerStdlib>,
    
    // Gem indexer
    gem_indexer: Option<IndexerGem>,
    
    // Where to find Ruby libraries on this system
    ruby_library_paths: Vec<PathBuf>,
}

impl IndexingCoordinator {
    /// Creates a new IndexingCoordinator for the given workspace.
    /// 
    /// This sets up all the basic components but doesn't start indexing yet.
    /// Call `run_complete_indexing()` to actually start the indexing process.
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
            detected_ruby_version: None,
            core_indexer: None,
            project_indexer: None,
            dependency_tracker,
            stdlib_indexer: None,
            gem_indexer: None,
            ruby_library_paths: Vec::new(),
        }
    }

    /// Runs the complete indexing process from start to finish.
    /// 
    /// This is the main method you call to index everything. It does these steps:
    /// 1. Figure out which Ruby version we're using
    /// 2. Find where Ruby libraries are installed on this system
    /// 3. Set up the main indexing engine
    /// 4. Index all the project files (and track what libraries they use)
    /// 5. Index the Ruby standard library
    /// 6. Index the gems (external libraries)
    pub async fn run_complete_indexing(&mut self, server: &RubyLanguageServer) -> Result<()> {
        info!("Starting complete indexing process");
        let start_time = Instant::now();

        // Step 1: Figure out which Ruby version we're using
        let ruby_version = self.detect_ruby_version();
        info!("Detected Ruby version: {:?}", ruby_version);

        // Step 2: Find where Ruby libraries are installed
        self.discover_ruby_library_paths();

        // Step 3: Set up the main indexing engine
        self.setup_core_indexer(server);

        // Step 4: Index the project files
        self.index_project_files(server).await?;

        // Step 5: Index the Ruby standard library
        self.index_standard_library(server, &ruby_version).await?;

        // Step 6: Index the gems
        self.index_gems().await?;

        info!(
            "Complete indexing finished in {:?}",
            start_time.elapsed()
        );
        Ok(())
    }

    /// Step 1: Detect which Ruby version we're working with
    fn detect_ruby_version(&mut self) -> Option<RubyVersion> {
        let version = self.version_detector.detect_version();
        self.detected_ruby_version = version.clone();
        version
    }

    /// Step 3: Set up the main indexing engine
    fn setup_core_indexer(&mut self, server: &RubyLanguageServer) {
        self.core_indexer = Some(IndexerCore::new(server.index()));
    }

    /// Step 4: Index all the project files and track what libraries they need
    async fn index_project_files(&mut self, server: &RubyLanguageServer) -> Result<()> {
        let mut project_indexer = IndexerProject::new(
            self.workspace_root.clone(),
            self.core_indexer.as_ref().unwrap().clone(),
            self.dependency_tracker.clone(),
        );
        
        project_indexer.index_project(server).await?;
        self.project_indexer = Some(project_indexer);
        Ok(())
    }

    /// Step 5: Index the Ruby standard library
    async fn index_standard_library(&mut self, server: &RubyLanguageServer, ruby_version: &Option<RubyVersion>) -> Result<()> {
        let required_stdlib = self.get_required_stdlib_modules();
        
        let mut stdlib_indexer = IndexerStdlib::new(
            self.core_indexer.as_ref().unwrap().clone(),
            ruby_version.clone(),
        );
        
        stdlib_indexer.set_required_modules(required_stdlib);
        stdlib_indexer.index_stdlib(server).await?;
        self.stdlib_indexer = Some(stdlib_indexer);
        Ok(())
    }

    /// Step 6: Index the gems (external libraries)
    async fn index_gems(&mut self) -> Result<()> {
        let required_gems = self.get_required_gems();
        
        let mut gem_indexer = IndexerGem::new(
            Arc::new(std::sync::Mutex::new(
                self.core_indexer.as_ref().unwrap().clone(),
            )),
            Some(self.workspace_root.clone()),
        );
        
        gem_indexer.set_required_gems(required_gems.into_iter().collect());
        gem_indexer.index_gems(true).await?; // selective = true
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

    /// Set up the dependency tracker with the Ruby library paths we found
    /// 
    /// The dependency tracker helps us figure out what external libraries
    /// the project is using so we can index them too.
    pub fn initialize_dependency_tracker(&mut self) {
        let tracker = DependencyTracker::new(
            self.workspace_root.clone(), 
            self.ruby_library_paths.clone()
        );
        self.dependency_tracker = Arc::new(Mutex::new(tracker));
    }

    /// Find all Ruby files in a directory and its subdirectories
    /// 
    /// This walks through a directory tree and collects all Ruby files,
    /// but skips common directories that usually don't contain Ruby source code
    /// (like node_modules, .git, tmp, etc.)
    pub fn find_all_ruby_files_in_directory(&self, dir: &Path, files: &mut Vec<PathBuf>) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    // Skip directories that typically don't contain Ruby source code
                    if let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) {
                        let should_skip = [
                            "node_modules", ".git", "tmp", "log", "coverage", 
                            ".bundle", "vendor"
                        ].contains(&dir_name);
                        
                        if !should_skip {
                            self.find_all_ruby_files_in_directory(&path, files);
                        }
                    }
                } else if self.is_ruby_file(&path) {
                    files.push(path);
                }
            }
        }
    }

    /// Check if a file is a Ruby file
    /// 
    /// This looks at the file extension (.rb, .ruby, .rake) and also checks
    /// for common Ruby files that don't have extensions (like Rakefile, Gemfile)
    pub fn is_ruby_file(&self, path: &Path) -> bool {
        // Check file extension first
        if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
            return matches!(extension, "rb" | "ruby" | "rake");
        }
        
        // Check for Ruby files without extensions
        if let Some(filename) = path.file_name().and_then(|name| name.to_str()) {
            matches!(filename, "Rakefile" | "Gemfile" | "Guardfile" | "Capfile")
        } else {
            false
        }
    }

    /// Find the Ruby core stubs for a specific Ruby version
    /// 
    /// Ruby core stubs are pre-written definitions of Ruby's built-in classes and methods.
    /// This helps the language server understand Ruby's core functionality.
    /// 
    /// We try to find stubs in this order:
    /// 1. Use the configured stub path
    /// 2. Look in the workspace's vsix/stubs directory
    /// 3. Fall back to Ruby 3.0 stubs if available
    pub fn find_core_stubs_for_version(&self, version: (u8, u8)) -> Option<PathBuf> {
        // First, try the configured stub path
        if let Some(stubs_path_str) = self.config.get_core_stubs_path_internal(version) {
            return Some(PathBuf::from(stubs_path_str));
        }
        
        // Look for stubs in the workspace
        let stubs_dir = self.workspace_root.join("vsix").join("stubs");
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
