use crate::indexer::file_processor::FileProcessor;
use crate::server::RubyLanguageServer;
use crate::types::file_source::FileSource;
use crate::utils;
use anyhow::Result;
use log::{info, warn};
use parking_lot::Mutex;
use rayon::prelude::*;
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
    pub fn new(workspace_root: PathBuf, file_processor: FileProcessor) -> Self {
        Self {
            workspace_root,
            file_processor,
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
        utils::collect_ruby_files(&self.workspace_root)
    }

    /// Quick scan for dependencies without full indexing.
    /// This reads project files and extracts require/gem statements to determine
    /// which gems and stdlib modules are needed.
    pub fn scan_for_dependencies(&self) {
        info!("Scanning project files for dependencies...");
        self.clear_dependencies();

        let ruby_files = self.collect_project_files();
        let required_stdlib = &self.required_stdlib;
        let required_gems = &self.required_gems;

        ruby_files.par_iter().for_each(|file_path| {
            if let Ok(content) = std::fs::read_to_string(file_path) {
                Self::extract_and_track_dependencies(&content, required_stdlib, required_gems);
            }
        });

        info!(
            "Dependency scan complete: {} stdlib modules, {} gems required",
            required_stdlib.lock().len(),
            required_gems.lock().len()
        );
    }

    /// Index definitions from files and track their dependencies (Parallelized with rayon)
    async fn index_definitions_and_track_dependencies(
        &self,
        files: &[PathBuf],
        server: &RubyLanguageServer,
        total_files: usize,
    ) -> Result<()> {
        use crate::indexer::coordinator::IndexingCoordinator;

        const BATCH_SIZE: usize = 10;
        info!("Indexing definitions in parallel batches of {}", BATCH_SIZE);

        let file_processor = self.file_processor.clone();
        let required_stdlib = self.required_stdlib.clone();
        let required_gems = self.required_gems.clone();

        // Process in batches for progress reporting
        for (batch_idx, batch) in files.chunks(BATCH_SIZE).enumerate() {
            // Report progress before each batch
            let processed = batch_idx * BATCH_SIZE;
            IndexingCoordinator::send_progress_report(
                server,
                "Indexing definitions".to_string(),
                processed,
                total_files,
            )
            .await;

            // Process batch in parallel with rayon
            let file_processor_ref = &file_processor;
            let required_stdlib_ref = &required_stdlib;
            let required_gems_ref = &required_gems;

            batch.par_iter().for_each(|file_path| {
                // Read file content
                let content = match std::fs::read_to_string(file_path) {
                    Ok(c) => c,
                    Err(e) => {
                        warn!("Failed to read file {:?}: {}", file_path, e);
                        return;
                    }
                };

                // Index Definitions
                if let Ok(uri) = Url::from_file_path(file_path) {
                    // Skip files that are already indexed (e.g., gem files)
                    // This ensures gem/stdlib files don't get re-tagged as Project
                    {
                        let index = file_processor_ref.index().lock();
                        if let Some(file_id) = index.get_file_id(&uri) {
                            if index.get_file_source(file_id).is_some() {
                                // Already indexed with a source, skip
                                return;
                            }
                        }
                    }

                    // Register file as project source before indexing
                    file_processor_ref
                        .index()
                        .lock()
                        .register_file(&uri, FileSource::Project);

                    if let Err(e) =
                        file_processor_ref.index_definitions_with_analysis(&uri, &content, server)
                    {
                        warn!("Failed to index definitions {:?}: {}", file_path, e);
                    }
                } else {
                    warn!("Failed to convert path to URI: {:?}", file_path);
                }

                // Track dependencies
                Self::extract_and_track_dependencies(
                    &content,
                    required_stdlib_ref,
                    required_gems_ref,
                );
            });
        }

        // Final progress report
        IndexingCoordinator::send_progress_report(
            server,
            "Indexing definitions".to_string(),
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

    /// Index only references from files with unresolved constant tracking (Parallelized with rayon)
    async fn index_references_only(
        &self,
        files: &[PathBuf],
        server: &RubyLanguageServer,
        total_files: usize,
    ) -> Result<()> {
        use crate::indexer::coordinator::IndexingCoordinator;

        const BATCH_SIZE: usize = 10;
        info!("Indexing references in parallel batches of {}", BATCH_SIZE);

        let file_processor = self.file_processor.clone();

        // Enable per-file timing via `RUBY_FAST_LSP_TRACE=1` (env). Leaves the
        // hot path branch-free when off (checked once up front).
        let tracing_on = std::env::var("RUBY_FAST_LSP_TRACE")
            .map(|v| v != "0" && !v.is_empty())
            .unwrap_or(false);
        let per_file_timings: Arc<Mutex<Vec<(PathBuf, std::time::Duration)>>> =
            Arc::new(Mutex::new(Vec::with_capacity(if tracing_on {
                total_files
            } else {
                0
            })));

        // Process in batches for progress reporting
        for (batch_idx, batch) in files.chunks(BATCH_SIZE).enumerate() {
            // Report progress before each batch
            let processed = batch_idx * BATCH_SIZE;
            IndexingCoordinator::send_progress_report(
                server,
                "Collecting references".to_string(),
                processed,
                total_files,
            )
            .await;

            // Process batch in parallel with rayon
            let file_processor_ref = &file_processor;
            let timings_ref = if tracing_on {
                Some(&per_file_timings)
            } else {
                None
            };

            batch.par_iter().for_each(|file_path| {
                // Read file content
                let content = match std::fs::read_to_string(file_path) {
                    Ok(c) => c,
                    Err(e) => {
                        warn!("Failed to read file {:?}: {}", file_path, e);
                        return;
                    }
                };

                let start = timings_ref.map(|_| std::time::Instant::now());

                // Index references
                if let Ok(uri) = Url::from_file_path(file_path) {
                    if let Err(e) = file_processor_ref.index_references(&uri, &content) {
                        warn!("Failed to index references {:?}: {}", file_path, e);
                    }
                } else {
                    warn!("Failed to convert path to URI: {:?}", file_path);
                }

                if let (Some(start), Some(timings)) = (start, timings_ref) {
                    let elapsed = start.elapsed();
                    timings.lock().push((file_path.clone(), elapsed));
                }
            });
        }

        // Dump top slow files if tracing was on. This is the simplest
        // signal to identify pathological files that dominate Phase 2.
        if tracing_on {
            let mut timings = per_file_timings.lock();
            timings.sort_by(|a, b| b.1.cmp(&a.1));
            let total: std::time::Duration = timings.iter().map(|(_, d)| *d).sum();
            info!(
                "[trace] Phase 2 per-file sum: {:?} across {} files (wall < sum × worker count)",
                total,
                timings.len()
            );
            info!("[trace] Top 20 slowest files:");
            for (path, dur) in timings.iter().take(20) {
                info!("[trace]   {:>10.2?}  {}", dur, path.display());
            }
        }

        // Final progress report
        IndexingCoordinator::send_progress_report(
            server,
            "Collecting references".to_string(),
            total_files,
            total_files,
        )
        .await;

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
    pub fn file_processor(&self) -> &FileProcessor {
        &self.file_processor
    }
}
