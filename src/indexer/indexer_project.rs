use crate::indexer::file_processor::FileProcessor;
use crate::server::RubyLanguageServer;
use anyhow::Result;
use log::{info, warn};
use parking_lot::Mutex;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tower_lsp::lsp_types::Url;

/// Handles project-specific indexing and tracks required stdlib and gems
pub struct IndexerProject {
    workspace_root: PathBuf,
    file_processor: FileProcessor,
    required_stdlib: Arc<Mutex<HashSet<String>>>,
    required_gems: Arc<Mutex<HashSet<String>>>,
}

impl IndexerProject {
    pub fn new(workspace_root: PathBuf, core: FileProcessor) -> Self {
        Self {
            workspace_root,
            file_processor: core,
            required_stdlib: Arc::new(Mutex::new(HashSet::new())),
            required_gems: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// Phase 1: Index only definitions from project files and track dependencies
    pub async fn index_project_definitions(&mut self, server: &RubyLanguageServer) -> Result<()> {
        let start_time = Instant::now();
        info!(
            "Starting project definitions indexing for: {:?}",
            self.workspace_root
        );

        // Clear previous dependency tracking
        self.clear_dependencies();

        // Collect all Ruby files in the project
        let ruby_files = self.collect_project_files();
        let total_files = ruby_files.len();
        info!("Found {} Ruby files in project", total_files);

        // Index definitions from project files and track dependencies
        self.index_definitions_and_track_dependencies(&ruby_files, server, total_files)
            .await?;

        // Resolve all mixin references now that all definitions are indexed
        info!("Resolving mixin references");
        server.index().lock().resolve_all_mixins();

        info!(
            "Project definitions indexing completed in {:?}. Found {} stdlib deps, {} gem deps",
            start_time.elapsed(),
            self.required_stdlib.lock().len(),
            self.required_gems.lock().len()
        );

        Ok(())
    }

    /// Phase 2: Index only references from project files
    pub async fn index_project_references(&mut self, server: &RubyLanguageServer) -> Result<()> {
        let start_time = Instant::now();
        info!(
            "Starting project references indexing for: {:?}",
            self.workspace_root
        );

        // Collect all Ruby files in the project
        let ruby_files = self.collect_project_files();
        let total_files = ruby_files.len();
        info!(
            "Indexing references from {} Ruby files in project",
            total_files
        );

        // Index references from project files
        self.index_references_only(&ruby_files, server, total_files)
            .await?;

        info!(
            "Project references indexing completed in {:?}",
            start_time.elapsed()
        );

        Ok(())
    }

    /// Collect all Ruby files in the project
    fn collect_project_files(&self) -> Vec<PathBuf> {
        // Simply collect all Ruby files from the workspace root recursively
        // This ensures we don't miss any Ruby files regardless of project structure
        self.file_processor.collect_ruby_files(&self.workspace_root)
    }

    /// Index definitions from files and track their dependencies (Parallelized)
    async fn index_definitions_and_track_dependencies(
        &self,
        files: &[PathBuf],
        server: &RubyLanguageServer,
        total_files: usize,
    ) -> Result<()> {
        use crate::indexer::coordinator::IndexingCoordinator;
        use tokio::fs;

        const BATCH_SIZE: usize = 50;

        info!("Indexing definitions in parallel batches of {}", BATCH_SIZE);

        for (batch_idx, batch) in files.chunks(BATCH_SIZE).enumerate() {
            let mut tasks = Vec::new();

            for (i, file_path) in batch.iter().enumerate() {
                let global_index = batch_idx * BATCH_SIZE + i;
                let file_path = file_path.clone();
                let server = server.clone();
                let core = self.file_processor.clone();
                let required_stdlib = self.required_stdlib.clone();
                let required_gems = self.required_gems.clone();

                tasks.push(tokio::spawn(async move {
                    // 1. Report progress (best effort)
                    if global_index.is_multiple_of(10) {
                        IndexingCoordinator::send_progress_report(
                            &server,
                            "Indexing".to_string(),
                            global_index + 1,
                            total_files,
                        )
                        .await;
                    }

                    // 2. Read file content ONCE
                    let content = match fs::read_to_string(&file_path).await {
                        Ok(c) => c,
                        Err(e) => {
                            warn!("Failed to read file {:?}: {}", file_path, e);
                            return;
                        }
                    };

                    // 3. Index Definitions
                    // In-line URL conversion to avoid visibility issues
                    match Url::from_file_path(&file_path) {
                        Ok(uri) => {
                            // core.index_definitions parses content, cpu intensive
                            if let Err(e) = core.index_definitions(&uri, &content, &server).await {
                                warn!("Failed to index definitions {:?}: {}", file_path, e);
                            }
                        }
                        Err(_) => {
                            warn!("Failed to convert path to URI: {:?}", file_path);
                        }
                    }

                    // 4. Track dependencies
                    Self::extract_and_track_dependencies(
                        &content,
                        &required_stdlib,
                        &required_gems,
                    );
                }));
            }

            // Await all tasks in this batch
            for task in tasks {
                if let Err(e) = task.await {
                    warn!("Task join error: {}", e);
                }
            }
        }

        // Final progress report
        IndexingCoordinator::send_progress_report(
            server,
            "Indexing".to_string(),
            total_files,
            total_files,
        )
        .await;

        Ok(())
    }

    /// Extract dependencies from content and update trackers (Static helper for parallelism)
    fn extract_and_track_dependencies(
        content: &str,
        required_stdlib: &Arc<Mutex<HashSet<String>>>,
        required_gems: &Arc<Mutex<HashSet<String>>>,
    ) {
        let mut stdlib_deps = required_stdlib.lock();
        let mut gem_deps = required_gems.lock();

        for line in content.lines() {
            let trimmed = line.trim();

            // Require
            if let Some(required) = Self::parse_require_statement(trimmed) {
                if Self::is_stdlib_module(&required) {
                    stdlib_deps.insert(required);
                }
            }

            // Gem
            if let Some(gem_name) = Self::parse_gem_statement(trimmed) {
                gem_deps.insert(gem_name);
            }
        }
    }

    /// Index only references from files with unresolved constant tracking
    async fn index_references_only(
        &self,
        files: &[PathBuf],
        server: &RubyLanguageServer,
        total_files: usize,
    ) -> Result<()> {
        use crate::indexer::coordinator::IndexingCoordinator;

        const BATCH_SIZE: usize = 50;

        info!("Indexing references in parallel batches of {}", BATCH_SIZE);

        for (batch_idx, batch) in files.chunks(BATCH_SIZE).enumerate() {
            let mut tasks = Vec::new();

            for (i, file_path) in batch.iter().enumerate() {
                let global_index = batch_idx * BATCH_SIZE + i;
                let file_path = file_path.clone();
                let server = server.clone();
                let core = self.file_processor.clone();

                tasks.push(tokio::spawn(async move {
                    // Report progress
                    if global_index.is_multiple_of(10) {
                        IndexingCoordinator::send_progress_report(
                            &server,
                            "Collecting References".to_string(),
                            global_index + 1,
                            total_files,
                        )
                        .await;
                    }

                    // Index only references from the file with unresolved constant tracking enabled
                    if let Err(e) = core.index_file_references(&file_path, &server).await {
                        warn!(
                            "Failed to index references from file {:?}: {}",
                            file_path, e
                        );
                    }
                }));
            }

            // Await all tasks in this batch
            for task in tasks {
                if let Err(e) = task.await {
                    warn!("Task join error: {}", e);
                }
            }
        }

        Ok(())
    }

    /// Parse a require statement and extract the module name
    fn parse_require_statement(line: &str) -> Option<String> {
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
    fn is_stdlib_module(module_name: &str) -> bool {
        // Common stdlib modules
        const STDLIB_MODULES: &[&str] = &[
            "json",
            "yaml",
            "csv",
            "uri",
            "net/http",
            "net/https",
            "openssl",
            "digest",
            "base64",
            "time",
            "date",
            "fileutils",
            "pathname",
            "tempfile",
            "tmpdir",
            "logger",
            "benchmark",
            "optparse",
            "ostruct",
            "set",
            "forwardable",
            "delegate",
            "singleton",
            "observer",
            "thread",
            "mutex_m",
            "monitor",
            "sync",
            "fiber",
            "continuation",
            "english",
            "abbrev",
            "cgi",
            "erb",
            "rexml",
            "rss",
            "xmlrpc",
            "webrick",
            "socket",
            "ipaddr",
            "resolv",
            "open-uri",
            "open3",
            "pty",
            "expect",
            "readline",
            "zlib",
            "stringio",
            "strscan",
            "scanf",
            "getoptlong",
            "find",
            "ftools",
            "shell",
            "shellwords",
            "etc",
            "fcntl",
            "io/console",
            "io/nonblock",
            "io/wait",
            "dbm",
            "gdbm",
            "sdbm",
            "pstore",
            "yaml/store",
        ];

        STDLIB_MODULES.contains(&module_name)
    }

    /// Parse a gem statement from Gemfile
    fn parse_gem_statement(line: &str) -> Option<String> {
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
    pub fn core(&self) -> &FileProcessor {
        &self.file_processor
    }
}
