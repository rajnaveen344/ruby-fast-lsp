//! Core Indexing Functionality
//!
//! This module provides the core indexing engine shared across all indexing phases.
//! It handles both definition indexing (Phase 1) and reference indexing (Phase 2).

use crate::analyzer_prism::visitors::index_visitor::IndexVisitor;
use crate::analyzer_prism::visitors::inlay_visitor::InlayVisitor;
use crate::analyzer_prism::visitors::reference_visitor::ReferenceVisitor;
use crate::indexer::index::RubyIndex;
use crate::indexer::utils;
use crate::server::RubyLanguageServer;
use crate::types::ruby_document::RubyDocument;
use anyhow::Result;
use log::{debug, info, warn};
use parking_lot::Mutex;
use ruby_prism::Visit;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tower_lsp::lsp_types::Url;

// ============================================================================
// Options Structs
// ============================================================================

/// Options for indexing references
#[derive(Clone, Copy, Default)]
pub struct ReferenceIndexOptions {
    /// When true, track unresolved constants in the index for diagnostics
    pub track_unresolved: bool,
}

impl ReferenceIndexOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_unresolved_tracking(mut self, track: bool) -> Self {
        self.track_unresolved = track;
        self
    }
}

// ============================================================================
// IndexerCore
// ============================================================================

/// Core indexing functionality shared across all indexing phases
#[derive(Debug, Clone)]
pub struct IndexerCore {
    index: Arc<Mutex<RubyIndex>>,
}

impl IndexerCore {
    pub fn new(index: Arc<Mutex<RubyIndex>>) -> Self {
        Self { index }
    }

    /// Get a reference to the underlying index
    pub fn index(&self) -> &Arc<Mutex<RubyIndex>> {
        &self.index
    }

    // ========================================================================
    // Content-based Indexing (in-memory content)
    // ========================================================================

    /// Index definitions from Ruby content
    pub async fn index_definitions(
        &self,
        uri: &Url,
        content: &str,
        server: &RubyLanguageServer,
    ) -> Result<()> {
        let start = Instant::now();
        debug!("Indexing definitions for: {:?}", uri);

        // Remove existing entries for this URI
        self.index.lock().remove_entries_for_uri(uri);

        // Create and cache the document
        let document = RubyDocument::new(uri.clone(), content.to_string(), 0);
        server
            .docs
            .lock()
            .insert(uri.clone(), Arc::new(parking_lot::RwLock::new(document)));

        // Parse and visit AST for definitions
        let parse_result = ruby_prism::parse(content.as_bytes());
        let node = parse_result.node();

        let mut index_visitor = IndexVisitor::new(server, uri.clone());
        index_visitor.visit(&node);

        // Run InlayVisitor to populate structural hints
        if let Some(doc_arc) = server.docs.lock().get(uri) {
            let document = doc_arc.read();
            let mut inlay_visitor = InlayVisitor::new(&document);
            inlay_visitor.visit(&node);
            let structural_hints = inlay_visitor.inlay_hints();

            drop(document);
            doc_arc.write().set_inlay_hints(structural_hints);
        }

        debug!("Indexed definitions for {:?} in {:?}", uri, start.elapsed());
        Ok(())
    }

    /// Index references from Ruby content
    pub async fn index_references(
        &self,
        uri: &Url,
        content: &str,
        server: &RubyLanguageServer,
        options: ReferenceIndexOptions,
    ) -> Result<()> {
        let start = Instant::now();
        debug!(
            "Indexing references for: {:?} (track_unresolved: {})",
            uri, options.track_unresolved
        );

        // Remove existing unresolved entries if tracking
        if options.track_unresolved {
            self.index.lock().remove_unresolved_entries_for_uri(uri);
        }

        // Parse and visit AST for references
        let parse_result = ruby_prism::parse(content.as_bytes());
        let node = parse_result.node();

        let mut visitor = if options.track_unresolved {
            ReferenceVisitor::with_unresolved_tracking(server, uri.clone(), true)
        } else {
            ReferenceVisitor::new(server, uri.clone())
        };
        visitor.visit(&node);

        debug!("Indexed references for {:?} in {:?}", uri, start.elapsed());
        Ok(())
    }

    // ========================================================================
    // File-based Indexing (reads from disk)
    // ========================================================================

    /// Index definitions from a single Ruby file
    pub async fn index_file_definitions(
        &self,
        file_path: &Path,
        server: &RubyLanguageServer,
    ) -> Result<()> {
        let uri = path_to_uri(file_path)?;
        let content = read_file_async(file_path).await?;
        self.index_definitions(&uri, &content, server).await
    }

    /// Index references from a single Ruby file
    pub async fn index_file_references(
        &self,
        file_path: &Path,
        server: &RubyLanguageServer,
        options: ReferenceIndexOptions,
    ) -> Result<()> {
        let uri = path_to_uri(file_path)?;
        let content = read_file_async(file_path).await?;
        self.index_references(&uri, &content, server, options).await
    }

    // ========================================================================
    // Parallel Indexing
    // ========================================================================

    /// Index definitions from multiple files in parallel
    pub async fn index_definitions_parallel(
        &self,
        file_paths: &[PathBuf],
        server: &RubyLanguageServer,
    ) -> Result<()> {
        let start = Instant::now();
        info!(
            "Indexing definitions from {} files in parallel",
            file_paths.len()
        );

        const BATCH_SIZE: usize = 50;

        for batch in file_paths.chunks(BATCH_SIZE) {
            let mut tasks = Vec::new();

            for file_path in batch {
                let core = self.clone();
                let server = server.clone();
                let path = file_path.clone();

                tasks.push(tokio::spawn(async move {
                    core.index_file_definitions(&path, &server).await
                }));
            }

            for task in tasks {
                if let Err(e) = task.await? {
                    warn!("Failed to index file definitions: {}", e);
                }
            }
        }

        info!(
            "Indexed definitions from {} files in {:?}",
            file_paths.len(),
            start.elapsed()
        );
        Ok(())
    }

    /// Index references from multiple files in parallel
    pub async fn index_references_parallel(
        &self,
        file_paths: &[PathBuf],
        server: &RubyLanguageServer,
        options: ReferenceIndexOptions,
    ) -> Result<()> {
        let start = Instant::now();
        info!(
            "Indexing references from {} files in parallel",
            file_paths.len()
        );

        // Filter to only project files for reference indexing
        let project_files: Vec<_> = file_paths
            .iter()
            .filter(|path| {
                Url::from_file_path(path)
                    .map(|uri| utils::is_project_file(&uri))
                    .unwrap_or(false)
            })
            .cloned()
            .collect();

        if project_files.is_empty() {
            debug!("No project files to index references for");
            return Ok(());
        }

        const BATCH_SIZE: usize = 50;

        for batch in project_files.chunks(BATCH_SIZE) {
            let mut tasks = Vec::new();

            for file_path in batch {
                let core = self.clone();
                let server = server.clone();
                let path = file_path.clone();

                tasks.push(tokio::spawn(async move {
                    core.index_file_references(&path, &server, options).await
                }));
            }

            for task in tasks {
                if let Err(e) = task.await? {
                    warn!("Failed to index file references: {}", e);
                }
            }
        }

        info!(
            "Indexed references from {} project files in {:?}",
            project_files.len(),
            start.elapsed()
        );
        Ok(())
    }

    // ========================================================================
    // Utility Methods
    // ========================================================================

    /// Check if a file should be indexed based on its extension
    pub fn should_index_file(&self, path: &Path) -> bool {
        utils::should_index_file(path)
    }

    /// Collect Ruby files recursively from a directory
    pub fn collect_ruby_files(&self, dir: &Path) -> Vec<PathBuf> {
        utils::collect_ruby_files(dir)
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Convert a file path to a URI
fn path_to_uri(path: &Path) -> Result<Url> {
    Url::from_file_path(path)
        .map_err(|_| anyhow::anyhow!("Failed to convert path to URI: {:?}", path))
}

/// Read file content asynchronously
async fn read_file_async(path: &Path) -> Result<String> {
    tokio::fs::read_to_string(path)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to read file {:?}: {}", path, e))
}
