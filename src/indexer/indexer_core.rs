use crate::analyzer_prism::visitors::index_visitor::IndexVisitor;
use crate::analyzer_prism::visitors::reference_visitor::ReferenceVisitor;
use crate::indexer::index::RubyIndex;
use crate::server::RubyLanguageServer;
use crate::types::ruby_document::RubyDocument;
use anyhow::Result;
use log::{debug, info};
use parking_lot::Mutex;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tower_lsp::lsp_types::Url;

/// Core indexing functionality shared across all indexing phases
#[derive(Debug)]
pub struct IndexerCore {
    index: Arc<Mutex<RubyIndex>>,
}

impl IndexerCore {
    pub fn new(index: Arc<Mutex<RubyIndex>>) -> Self {
        Self { index }
    }

    /// Index a single Ruby file and add its symbols to the index
    pub async fn index_file(&self, file_path: &Path, server: &RubyLanguageServer) -> Result<()> {
        let start_time = Instant::now();
        debug!("Indexing file: {:?}", file_path);

        // Convert path to URI
        let uri = Url::from_file_path(file_path)
            .map_err(|_| anyhow::anyhow!("Failed to convert path to URI: {:?}", file_path))?;

        // Read file content
        let content = tokio::fs::read_to_string(file_path)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to read file {:?}: {}", file_path, e))?;

        // Index the file content
        self.index_content(&uri, &content, server).await?;

        debug!("Indexed file {:?} in {:?}", file_path, start_time.elapsed());
        Ok(())
    }

    /// Index Ruby content and extract its symbols
    pub async fn index_content(
        &self,
        uri: &Url,
        content: &str,
        server: &RubyLanguageServer,
    ) -> Result<()> {
        let start_time = Instant::now();
        debug!("Indexing content for: {:?}", uri);

        // Remove existing entries for this URI
        self.index.lock().remove_entries_for_uri(uri);

        // Create a document for this URI (similar to server.rs process_file)
        let document = RubyDocument::new(uri.clone(), content.to_string(), 0);
        server
            .docs
            .lock()
            .insert(uri.clone(), Arc::new(parking_lot::RwLock::new(document)));

        // Parse Ruby code using Prism and extract symbols using IndexVisitor
        let parse_result = ruby_prism::parse(content.as_bytes());
        let node = parse_result.node();
        let mut index_visitor = IndexVisitor::new(server, uri.clone());

        // Visit the AST to extract definitions
        use ruby_prism::Visit;
        index_visitor.visit(&node);

        // Only use ReferenceVisitor for project files, not stdlib/gem files
        if self.is_project_file(uri, server) {
            let mut reference_visitor = ReferenceVisitor::new(server, uri.clone());
            reference_visitor.visit(&node);
        }

        debug!(
            "Indexed content for {:?} in {:?}",
            uri,
            start_time.elapsed()
        );
        Ok(())
    }

    /// Index multiple files in parallel
    pub async fn index_files_parallel(
        &self,
        file_paths: &[PathBuf],
        server: &RubyLanguageServer,
    ) -> Result<()> {
        let start_time = Instant::now();
        info!("Indexing {} files in parallel", file_paths.len());

        // Process files in batches to avoid overwhelming the system
        const BATCH_SIZE: usize = 50;

        for batch in file_paths.chunks(BATCH_SIZE) {
            let mut tasks = Vec::new();

            for file_path in batch {
                let core = self.clone();
                let server = server.clone();
                let path = file_path.clone();

                let task = tokio::spawn(async move { core.index_file(&path, &server).await });
                tasks.push(task);
            }

            // Wait for all tasks in this batch to complete
            for task in tasks {
                if let Err(e) = task.await? {
                    log::warn!("Failed to index file: {}", e);
                }
            }
        }

        info!(
            "Indexed {} files in {:?}",
            file_paths.len(),
            start_time.elapsed()
        );
        Ok(())
    }

    /// Check if a file should be indexed based on its extension
    pub fn should_index_file(&self, path: &Path) -> bool {
        if let Some(extension) = path.extension() {
            matches!(extension.to_str(), Some("rb" | "rake" | "gemspec"))
        } else {
            // Check for files without extensions that might be Ruby
            if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                matches!(
                    file_name,
                    "Rakefile" | "Gemfile" | "Guardfile" | "Capfile" | "Vagrantfile"
                )
            } else {
                false
            }
        }
    }

    /// Collect Ruby files recursively from a directory
    pub fn collect_ruby_files(&self, dir: &Path) -> Vec<PathBuf> {
        let mut files = Vec::new();
        self.collect_ruby_files_recursive(dir, &mut files);
        files
    }

    /// Recursively collect Ruby files from a directory
    fn collect_ruby_files_recursive(&self, dir: &Path, files: &mut Vec<PathBuf>) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();

                if path.is_dir() {
                    // Skip common directories that don't contain indexable Ruby files
                    if let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) {
                        if !matches!(
                            dir_name,
                            ".git"
                                | ".svn"
                                | "node_modules"
                                | "tmp"
                                | "log"
                                | "coverage"
                                | ".bundle"
                        ) {
                            self.collect_ruby_files_recursive(&path, files);
                        }
                    }
                } else if self.should_index_file(&path) {
                    files.push(path);
                }
            }
        }
    }

    /// Get a reference to the underlying index
    pub fn index(&self) -> &Arc<Mutex<RubyIndex>> {
        &self.index
    }

    /// Check if a URI belongs to a project file (not stdlib or gem)
    fn is_project_file(&self, uri: &Url, _server: &RubyLanguageServer) -> bool {
        if let Ok(file_path) = uri.to_file_path() {
            let path_str = file_path.to_string_lossy();
            
            // Check if the file is in common stdlib or gem paths
            let is_stdlib_or_gem = path_str.contains("/ruby/") && 
                (path_str.contains("/lib/ruby/") || 
                 path_str.contains("/gems/") ||
                 path_str.contains("/rubystubs") ||
                 path_str.contains("/site_ruby/") ||
                 path_str.contains("/vendor_ruby/"));
            
            // If it's not in stdlib/gem paths, consider it a project file
            !is_stdlib_or_gem
        } else {
            // If we can't convert to file path, assume it's a project file
            true
        }
    }
}

impl Clone for IndexerCore {
    fn clone(&self) -> Self {
        Self {
            index: Arc::clone(&self.index),
        }
    }
}
