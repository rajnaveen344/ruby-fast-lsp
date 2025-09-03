use crate::analyzer_prism::visitors::index_visitor::IndexVisitor;
use crate::analyzer_prism::visitors::inlay_visitor::InlayVisitor;
use crate::analyzer_prism::visitors::reference_visitor::ReferenceVisitor;
use crate::indexer::index::RubyIndex;
use crate::indexer::utils;
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

    /// Phase 1: Index only definitions from Ruby content
    pub async fn index_definitions(
        &self,
        uri: &Url,
        content: &str,
        server: &RubyLanguageServer,
    ) -> Result<()> {
        let start_time = Instant::now();
        debug!("Indexing definitions for: {:?}", uri);

        // Remove existing entries for this URI
        self.index.lock().remove_entries_for_uri(uri);

        // Create a document for this URI (similar to server.rs process_file)
        let document = RubyDocument::new(uri.clone(), content.to_string(), 0);
        server
            .docs
            .lock()
            .insert(uri.clone(), Arc::new(parking_lot::RwLock::new(document)));

        // Parse Ruby code using Prism and extract definitions using IndexVisitor
        let parse_result = ruby_prism::parse(content.as_bytes());
        let node = parse_result.node();
        let mut index_visitor = IndexVisitor::new(server, uri.clone());

        // Visit the AST to extract definitions only
        use ruby_prism::Visit;
        index_visitor.visit(&node);

        // Also run InlayVisitor to populate structural hints in the document
        let document_guard = server.docs.lock();
        if let Some(doc_arc) = document_guard.get(uri) {
            let document = doc_arc.read();
            let mut inlay_visitor = InlayVisitor::new(&document);
            inlay_visitor.visit(&node);
            let structural_hints = inlay_visitor.inlay_hints();

            // Store structural hints in the document
            drop(document); // Release read lock
            let mut document_mut = doc_arc.write();
            document_mut.set_inlay_hints(structural_hints);
        }

        debug!(
            "Indexed definitions for {:?} in {:?}",
            uri,
            start_time.elapsed()
        );
        Ok(())
    }

    /// Phase 2: Index only references from Ruby content
    pub async fn index_references(
        &self,
        uri: &Url,
        content: &str,
        server: &RubyLanguageServer,
    ) -> Result<()> {
        let start_time = Instant::now();
        debug!("Indexing references for: {:?}", uri);

        // Parse Ruby code using Prism and extract references using ReferenceVisitor
        let parse_result = ruby_prism::parse(content.as_bytes());
        let node = parse_result.node();
        let mut reference_visitor = ReferenceVisitor::new(server, uri.clone());

        // Visit the AST to extract references only
        use ruby_prism::Visit;
        reference_visitor.visit(&node);

        debug!(
            "Indexed references for {:?} in {:?}",
            uri,
            start_time.elapsed()
        );
        Ok(())
    }

    /// Phase 1: Index definitions from multiple files in parallel
    pub async fn index_definitions_parallel(
        &self,
        file_paths: &[PathBuf],
        server: &RubyLanguageServer,
    ) -> Result<()> {
        let start_time = Instant::now();
        info!(
            "Indexing definitions from {} files in parallel",
            file_paths.len()
        );

        // Process files in batches to avoid overwhelming the system
        const BATCH_SIZE: usize = 50;

        for batch in file_paths.chunks(BATCH_SIZE) {
            let mut tasks = Vec::new();

            for file_path in batch {
                let core = self.clone();
                let server = server.clone();
                let path = file_path.clone();

                let task =
                    tokio::spawn(async move { core.index_file_definitions(&path, &server).await });
                tasks.push(task);
            }

            // Wait for all tasks in this batch to complete
            for task in tasks {
                if let Err(e) = task.await? {
                    log::warn!("Failed to index file definitions: {}", e);
                }
            }
        }

        info!(
            "Indexed definitions from {} files in {:?}",
            file_paths.len(),
            start_time.elapsed()
        );
        Ok(())
    }

    /// Phase 2: Index references from multiple files in parallel
    pub async fn index_references_parallel(
        &self,
        file_paths: &[PathBuf],
        server: &RubyLanguageServer,
    ) -> Result<()> {
        let start_time = Instant::now();
        info!(
            "Indexing references from {} files in parallel",
            file_paths.len()
        );

        // Filter to only project files for reference indexing
        let project_files: Vec<_> = file_paths
            .iter()
            .filter(|path| {
                if let Ok(uri) = Url::from_file_path(path) {
                    self.is_project_file(&uri, server)
                } else {
                    false
                }
            })
            .collect();

        if project_files.is_empty() {
            debug!("No project files to index references for");
            return Ok(());
        }

        // Process files in batches to avoid overwhelming the system
        const BATCH_SIZE: usize = 50;

        for batch in project_files.chunks(BATCH_SIZE) {
            let mut tasks = Vec::new();

            for file_path in batch {
                let core = self.clone();
                let server = server.clone();
                let path = (*file_path).clone();

                let task =
                    tokio::spawn(async move { core.index_file_references(&path, &server).await });
                tasks.push(task);
            }

            // Wait for all tasks in this batch to complete
            for task in tasks {
                if let Err(e) = task.await? {
                    log::warn!("Failed to index file references: {}", e);
                }
            }
        }

        info!(
            "Indexed references from {} project files in {:?}",
            project_files.len(),
            start_time.elapsed()
        );
        Ok(())
    }

    /// Index definitions from a single Ruby file
    pub async fn index_file_definitions(
        &self,
        file_path: &Path,
        server: &RubyLanguageServer,
    ) -> Result<()> {
        let start_time = Instant::now();
        debug!("Indexing definitions from file: {:?}", file_path);

        // Convert path to URI
        let uri = Url::from_file_path(file_path)
            .map_err(|_| anyhow::anyhow!("Failed to convert path to URI: {:?}", file_path))?;

        // Read file content
        let content = tokio::fs::read_to_string(file_path)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to read file {:?}: {}", file_path, e))?;

        // Index only definitions
        self.index_definitions(&uri, &content, server).await?;

        debug!(
            "Indexed definitions from file {:?} in {:?}",
            file_path,
            start_time.elapsed()
        );
        Ok(())
    }

    /// Index references from a single Ruby file
    pub async fn index_file_references(
        &self,
        file_path: &Path,
        server: &RubyLanguageServer,
    ) -> Result<()> {
        let start_time = Instant::now();
        debug!("Indexing references from file: {:?}", file_path);

        // Convert path to URI
        let uri = Url::from_file_path(file_path)
            .map_err(|_| anyhow::anyhow!("Failed to convert path to URI: {:?}", file_path))?;

        // Read file content
        let content = tokio::fs::read_to_string(file_path)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to read file {:?}: {}", file_path, e))?;

        // Index only references
        self.index_references(&uri, &content, server).await?;

        debug!(
            "Indexed references from file {:?} in {:?}",
            file_path,
            start_time.elapsed()
        );
        Ok(())
    }

    /// Check if a file should be indexed based on its extension
    pub fn should_index_file(&self, path: &Path) -> bool {
        utils::should_index_file(path)
    }

    /// Collect Ruby files recursively from a directory
    pub fn collect_ruby_files(&self, dir: &Path) -> Vec<PathBuf> {
        utils::collect_ruby_files(dir)
    }

    /// Get a reference to the underlying index
    pub fn index(&self) -> &Arc<Mutex<RubyIndex>> {
        &self.index
    }

    /// Check if a URI belongs to a project file (not stdlib or gem)
    fn is_project_file(&self, uri: &Url, _server: &RubyLanguageServer) -> bool {
        utils::is_project_file(uri)
    }
}

impl Clone for IndexerCore {
    fn clone(&self) -> Self {
        Self {
            index: Arc::clone(&self.index),
        }
    }
}
