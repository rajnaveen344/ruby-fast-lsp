use crate::indexer::dependency_tracker::DependencyTracker;
use crate::indexer::indexer_core::IndexerCore;
use crate::server::RubyLanguageServer;
use anyhow::Result;
use log::{debug, info, warn};
use parking_lot::Mutex;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

/// Handles project-specific indexing and tracks required stdlib and gems
pub struct IndexerProject {
    workspace_root: PathBuf,
    core: IndexerCore,
    dependency_tracker: Arc<Mutex<DependencyTracker>>,
    required_stdlib: Arc<Mutex<HashSet<String>>>,
    required_gems: Arc<Mutex<HashSet<String>>>,
}

impl IndexerProject {
    pub fn new(
        workspace_root: PathBuf,
        core: IndexerCore,
        dependency_tracker: Arc<Mutex<DependencyTracker>>,
    ) -> Self {
        Self {
            workspace_root,
            core,
            dependency_tracker,
            required_stdlib: Arc::new(Mutex::new(HashSet::new())),
            required_gems: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// Index the entire project and track dependencies
    pub async fn index_project(&mut self, server: &RubyLanguageServer) -> Result<()> {
        let start_time = Instant::now();
        info!("Starting project indexing for: {:?}", self.workspace_root);

        // Clear previous dependency tracking
        self.clear_dependencies();

        // Collect all Ruby files in the project
        let ruby_files = self.collect_project_files();
        info!("Found {} Ruby files in project", ruby_files.len());

        // Index project files and track dependencies
        self.index_and_track_dependencies(&ruby_files, server).await?;

        // Update dependency tracker with discovered dependencies
        self.update_dependency_tracker();

        info!(
            "Project indexing completed in {:?}. Found {} stdlib deps, {} gem deps",
            start_time.elapsed(),
            self.required_stdlib.lock().len(),
            self.required_gems.lock().len()
        );

        Ok(())
    }

    /// Collect all Ruby files in the project
    fn collect_project_files(&self) -> Vec<PathBuf> {
        let mut files = Vec::new();
        
        // Index main source directories
        let source_dirs = vec![
            self.workspace_root.join("app"),
            self.workspace_root.join("lib"),
            self.workspace_root.join("config"),
            self.workspace_root.join("spec"),
            self.workspace_root.join("test"),
            self.workspace_root.clone(), // Root directory for files like Rakefile, Gemfile
        ];

        for dir in source_dirs {
            if dir.exists() && dir.is_dir() {
                let dir_files = self.core.collect_ruby_files(&dir);
                files.extend(dir_files);
            }
        }

        // Remove duplicates and sort
        files.sort();
        files.dedup();
        
        files
    }

    /// Index files and track their dependencies
    async fn index_and_track_dependencies(
        &self,
        files: &[PathBuf],
        server: &RubyLanguageServer,
    ) -> Result<()> {
        for file_path in files {
            // Index the file
            if let Err(e) = self.core.index_file(file_path, server).await {
                warn!("Failed to index file {:?}: {}", file_path, e);
                continue;
            }

            // Track dependencies from this file
            self.track_file_dependencies(file_path).await;
        }

        Ok(())
    }

    /// Track dependencies from a specific file
    async fn track_file_dependencies(&self, file_path: &Path) {
        if let Ok(content) = std::fs::read_to_string(file_path) {
            self.extract_require_statements(&content);
            self.extract_gem_dependencies(&content);
        }
    }

    /// Extract require statements to identify stdlib dependencies
    fn extract_require_statements(&self, content: &str) {
        let mut stdlib_deps = self.required_stdlib.lock();
        
        for line in content.lines() {
            let trimmed = line.trim();
            
            // Match require statements
            if let Some(required) = self.parse_require_statement(trimmed) {
                // Check if it's a stdlib module
                if self.is_stdlib_module(&required) {
                    debug!("Found stdlib dependency: {}", required);
                    stdlib_deps.insert(required);
                }
            }
        }
    }

    /// Parse a require statement and extract the module name
    fn parse_require_statement(&self, line: &str) -> Option<String> {
        // Handle various require patterns:
        // require 'module'
        // require "module"
        // require_relative 'module'
        
        if line.starts_with("require ") || line.starts_with("require_relative ") {
            // Find the quoted string
            if let Some(start) = line.find('"').or_else(|| line.find('\'')) {
                let quote_char = line.chars().nth(start).unwrap();
                if let Some(end) = line[start + 1..].find(quote_char) {
                    let module_name = &line[start + 1..start + 1 + end];
                    return Some(module_name.to_string());
                }
            }
        }
        
        None
    }

    /// Check if a module is part of Ruby's standard library
    fn is_stdlib_module(&self, module_name: &str) -> bool {
        // Common stdlib modules
        const STDLIB_MODULES: &[&str] = &[
            "json", "yaml", "csv", "uri", "net/http", "net/https", "openssl",
            "digest", "base64", "time", "date", "fileutils", "pathname",
            "tempfile", "tmpdir", "logger", "benchmark", "optparse",
            "ostruct", "set", "forwardable", "delegate", "singleton",
            "observer", "thread", "mutex_m", "monitor", "sync",
            "fiber", "continuation", "english", "abbrev", "cgi",
            "erb", "rexml", "rss", "xmlrpc", "webrick", "socket",
            "ipaddr", "resolv", "open-uri", "open3", "pty", "expect",
            "readline", "zlib", "stringio", "strscan", "scanf",
            "getoptlong", "find", "ftools", "shell", "shellwords",
            "etc", "fcntl", "io/console", "io/nonblock", "io/wait",
            "dbm", "gdbm", "sdbm", "pstore", "yaml/store",
        ];
        
        STDLIB_MODULES.contains(&module_name)
    }

    /// Extract gem dependencies from various sources
    fn extract_gem_dependencies(&self, content: &str) {
        let mut gem_deps = self.required_gems.lock();
        
        for line in content.lines() {
            let trimmed = line.trim();
            
            // Match gem statements in Gemfile
            if let Some(gem_name) = self.parse_gem_statement(trimmed) {
                debug!("Found gem dependency: {}", gem_name);
                gem_deps.insert(gem_name);
            }
        }
    }

    /// Parse a gem statement from Gemfile
    fn parse_gem_statement(&self, line: &str) -> Option<String> {
        if line.starts_with("gem ") {
            // Find the quoted gem name
            if let Some(start) = line.find('"').or_else(|| line.find('\'')) {
                let quote_char = line.chars().nth(start).unwrap();
                if let Some(end) = line[start + 1..].find(quote_char) {
                    let gem_name = &line[start + 1..start + 1 + end];
                    return Some(gem_name.to_string());
                }
            }
        }
        
        None
    }

    /// Clear previously tracked dependencies
    fn clear_dependencies(&self) {
        self.required_stdlib.lock().clear();
        self.required_gems.lock().clear();
    }

    /// Update the dependency tracker with discovered dependencies
    fn update_dependency_tracker(&self) {
        let mut tracker = self.dependency_tracker.lock();
        
        // Add stdlib dependencies
        for stdlib_dep in self.required_stdlib.lock().iter() {
            tracker.add_stdlib_dependency(stdlib_dep.clone());
        }
        
        // Add gem dependencies
        for gem_dep in self.required_gems.lock().iter() {
            tracker.add_gem_dependency(gem_dep.clone());
        }
    }

    /// Get the list of required stdlib modules
    pub fn get_required_stdlib(&self) -> Vec<String> {
        self.required_stdlib.lock().iter().cloned().collect()
    }

    /// Get the list of required gems
    pub fn get_required_gems(&self) -> Vec<String> {
        self.required_gems.lock().iter().cloned().collect()
    }

    /// Check if a specific stdlib module is required
    pub fn requires_stdlib(&self, module_name: &str) -> bool {
        self.required_stdlib.lock().contains(module_name)
    }

    /// Check if a specific gem is required
    pub fn requires_gem(&self, gem_name: &str) -> bool {
        self.required_gems.lock().contains(gem_name)
    }

    /// Get the workspace root path
    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    /// Get a reference to the core indexer
    pub fn core(&self) -> &IndexerCore {
        &self.core
    }
}