//! File Processing Module
//!
//! This module provides the shared file processing logic used across all indexing phases.
//! It handles parsing, definition indexing (Phase 1), reference indexing (Phase 2),
//! and diagnostic generation.
//!
//! ## Key Components
//!
//! - **`FileProcessor`**: Core struct for processing individual files
//! - **`ProcessingOptions`**: Configuration for what to process (definitions, references, etc.)
//! - **`ProcessResult`**: Results of processing including diagnostics and affected URIs
//! - **`get_unresolved_diagnostics`**: Generates diagnostics for unresolved constants/methods
//!
//! ## Usage
//!
//! Each indexer (project, stdlib, gem) discovers files to process, then delegates
//! the actual processing to `FileProcessor` with appropriate options.

use crate::analyzer_prism::visitors::index_visitor::IndexVisitor;
use crate::analyzer_prism::visitors::inlay_visitor::InlayVisitor;
use crate::analyzer_prism::visitors::reference_visitor::ReferenceVisitor;
use crate::indexer::index::RubyIndex;
// use crate::indexer::utils; // Removed
use crate::capabilities::diagnostics::generate_diagnostics;
use crate::server::RubyLanguageServer;
use crate::types::ruby_document::RubyDocument;
use anyhow::Result;
use log::{debug, info, warn};
use parking_lot::Mutex;
use ruby_prism::Visit;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tower_lsp::lsp_types::{Diagnostic, Url};

// ============================================================================
// Options Structs
// ============================================================================

/// Options for processing a file
#[derive(Clone, Default)]
pub struct ProcessingOptions {
    /// Whether to index definitions (Phase 1)
    pub index_definitions: bool,
    /// Whether to index references and track unresolved (Phase 2)
    pub index_references: bool,
    /// Whether to resolve mixins immediately (can be slow)
    pub resolve_mixins: bool,
    /// Whether to include local variables in reference indexing
    pub include_local_vars: bool,
}

impl ProcessingOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn full_analysis() -> Self {
        Self {
            index_definitions: true,
            index_references: true,
            resolve_mixins: true,
            include_local_vars: true,
        }
    }

    pub fn fast_analysis() -> Self {
        Self {
            index_definitions: true,
            index_references: true,
            resolve_mixins: false,
            include_local_vars: true,
        }
    }
}

/// Result of processing a file
pub struct ProcessResult {
    /// Functionally affected URIs (files that need updated diagnostics)
    pub affected_uris: HashSet<Url>,
    /// Syntax and early validation diagnostics
    pub diagnostics: Vec<Diagnostic>,
}

// ============================================================================
// FileProcessor
// ============================================================================

/// File processor for handling parsing, indexing, and diagnostic generation
#[derive(Debug, Clone)]
pub struct FileProcessor {
    index: Arc<Mutex<RubyIndex>>,
}

impl FileProcessor {
    pub fn new(index: Arc<Mutex<RubyIndex>>) -> Self {
        Self { index }
    }

    /// Get a reference to the underlying index
    pub fn index(&self) -> &Arc<Mutex<RubyIndex>> {
        &self.index
    }

    /// Process a file: parse, index definitions, index references, and return diagnostics.
    /// This prevents double-parsing and centralizes the logic.
    pub fn process_file(
        &self,
        uri: &Url,
        content: &str,
        server: &RubyLanguageServer,
        options: ProcessingOptions,
    ) -> Result<ProcessResult> {
        let start = Instant::now();
        // Check if this version was already indexed - skip expensive re-indexing if unchanged
        // This relies on the document version being updated BEFORE calling this method
        let already_indexed = {
            let docs = server.docs.lock();
            if let Some(doc_arc) = docs.get(uri) {
                let doc = doc_arc.read();
                doc.indexed_version == Some(doc.version)
            } else {
                false
            }
        };

        if already_indexed {
            debug!(
                "Skipping re-indexing {} (version already indexed)",
                uri.path().split('/').next_back().unwrap_or("unknown")
            );
            // Still parse for syntax diagnostics
            let parse_result = ruby_prism::parse(content.as_bytes());
            let doc = RubyDocument::new(uri.clone(), content.to_string(), 0);
            let diagnostics = generate_diagnostics(&parse_result, &doc);
            return Ok(ProcessResult {
                affected_uris: HashSet::new(),
                diagnostics,
            });
        }

        // 1. Parse ONLY ONCE
        let parse_start = Instant::now();
        let parse_result = ruby_prism::parse(content.as_bytes());
        let node = parse_result.node();
        let document = RubyDocument::new(uri.clone(), content.to_string(), 0);
        info!("perf_debug: Parsing took {:?}", parse_start.elapsed());

        // 2. Generate Syntax Diagnostics
        let diag_start = Instant::now();
        let diagnostics = generate_diagnostics(&parse_result, &document);
        info!(
            "perf_debug: Syntax Diagnostics took {:?}",
            diag_start.elapsed()
        );

        // If severe parse errors, we might want to skip indexing, but usually we try best-effort.
        if parse_result.errors().count() > 10 {
            // Arbitrary threshold for "totally broken"
            return Ok(ProcessResult {
                affected_uris: HashSet::new(),
                diagnostics,
            });
        }

        let mut affected_uris = HashSet::new();

        // 3. Index Definitions (Phase 1)
        if options.index_definitions {
            let def_start = Instant::now();
            let cleanup_start = Instant::now();
            let removed_fqns = self.index.lock().remove_entries_for_uri(uri);
            let removed_fqn_set: HashSet<_> = removed_fqns.into_iter().collect();
            info!(
                "perf_debug: Cleanup (remove_entries) took {:?}",
                cleanup_start.elapsed()
            );

            let visitor_start = Instant::now();
            let mut visitor = IndexVisitor::new(server, uri.clone());
            visitor.visit(&node);
            info!(
                "perf_debug: IndexVisitor walk took {:?}",
                visitor_start.elapsed()
            );

            // Update document with visitor's state
            if let Some(doc_arc) = server.docs.lock().get(uri) {
                *doc_arc.write() = visitor.document.clone();
            }

            if options.resolve_mixins {
                let mixin_start = Instant::now();
                self.index.lock().resolve_mixins_for_uri(uri);
                info!(
                    "perf_debug: Mixin Resolution took {:?}",
                    mixin_start.elapsed()
                );
            }

            // Calculate diff for cross-file diagnostics
            let diff_start = Instant::now();
            let added_fqns: Vec<_> = {
                let index = self.index.lock();
                index
                    .file_entries
                    .get(uri)
                    .map(|entries| entries.iter().map(|e| e.fqn.clone()).collect())
                    .unwrap_or_default()
            };
            let added_fqn_set: HashSet<_> = added_fqns.iter().cloned().collect();

            let truly_removed: Vec<_> = removed_fqn_set
                .difference(&added_fqn_set)
                .cloned()
                .collect();

            if !truly_removed.is_empty() {
                let removed_affected = self
                    .index
                    .lock()
                    .mark_references_as_unresolved(&truly_removed);
                affected_uris.extend(removed_affected);
            }
            if !added_fqns.is_empty() {
                let resolved_affected = self.index.lock().clear_resolved_entries(&added_fqns);
                affected_uris.extend(resolved_affected);
            }
            info!(
                "perf_debug: Cross-file diagnostics overhead took {:?}",
                diff_start.elapsed()
            );
            info!(
                "perf_debug: Index Definitions took {:?}",
                def_start.elapsed()
            );
        }

        // 4. Index References (Phase 2) - always tracks unresolved
        if options.index_references {
            let ref_start = Instant::now();
            let mut index = self.index.lock();
            index.remove_references_for_uri(uri);
            index.remove_unresolved_entries_for_uri(uri);
            drop(index); // unlock

            // Always use with_unresolved_tracking - combined reference + unresolved indexing
            let visitor_start = Instant::now();
            let mut visitor = ReferenceVisitor::with_unresolved_tracking(
                server,
                uri.clone(),
                options.include_local_vars,
            );
            visitor.visit(&node);
            info!(
                "perf_debug: ReferenceVisitor walk took {:?}",
                visitor_start.elapsed()
            );

            // Update document with visitor's state
            if let Some(doc_arc) = server.docs.lock().get(uri) {
                *doc_arc.write() = visitor.document;
            }
            info!(
                "perf_debug: Index References took {:?}",
                ref_start.elapsed()
            );
        }
        // 5. Run InlayVisitor (Structural Hints) - usually part of Definitions or separate?
        // FileProcessor::index_definitions did this. We should probably do it here too if definitions are indexed.
        if options.index_definitions {
            let inlay_start = Instant::now();
            if let Some(doc_arc) = server.docs.lock().get(uri) {
                // InlayVisitor is lightweight
                let document = doc_arc.read();
                let mut inlay_visitor = InlayVisitor::new(&document);
                inlay_visitor.visit(&node);
                let structural_hints = inlay_visitor.inlay_hints();
                drop(document);
                doc_arc.write().set_inlay_hints(structural_hints);
            }
            info!("perf_debug: Inlay Hints took {:?}", inlay_start.elapsed());
        }

        // Mark as indexed
        if let Some(doc_arc) = server.docs.lock().get(uri) {
            let mut doc = doc_arc.write();
            doc.indexed_version = Some(doc.version);
        }

        let total_time = start.elapsed();
        debug!("Processed file {:?} in {:?}", uri, total_time);
        info!(
            "perf_debug: Total processing for {:?} took {:?}",
            uri, total_time
        );

        Ok(ProcessResult {
            affected_uris,
            diagnostics,
        })
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
    ) -> Result<()> {
        let start = Instant::now();
        debug!(
            "Indexing references for: {:?} (track_unresolved: true)",
            uri
        );

        // Remove existing unresolved entries if tracking
        self.index.lock().remove_unresolved_entries_for_uri(uri);

        // Parse and visit AST for references
        let parse_result = ruby_prism::parse(content.as_bytes());
        let node = parse_result.node();

        // Always track unresolved references now
        let mut visitor = ReferenceVisitor::with_unresolved_tracking(server, uri.clone(), true);
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
    ) -> Result<()> {
        let uri = path_to_uri(file_path)?;
        let content = read_file_async(file_path).await?;
        self.index_references(&uri, &content, server).await
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
                    .map(|uri| crate::utils::is_project_file(&uri))
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
                    core.index_file_references(&path, &server).await
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
        crate::utils::should_index_file(path)
    }

    /// Collect Ruby files recursively from a directory
    pub fn collect_ruby_files(&self, dir: &Path) -> Vec<PathBuf> {
        crate::utils::collect_ruby_files(dir)
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

/// Get diagnostics for unresolved entries (constants and methods) from the index
pub fn get_unresolved_diagnostics(server: &RubyLanguageServer, uri: &Url) -> Vec<Diagnostic> {
    use crate::indexer::index::UnresolvedEntry;
    use tower_lsp::lsp_types::{DiagnosticSeverity, NumberOrString};

    let index_arc = server.index();
    let index = index_arc.lock();
    let unresolved_list = index.get_unresolved_entries(uri);

    unresolved_list
        .iter()
        .map(|entry| match entry {
            UnresolvedEntry::Constant { name, location, .. } => Diagnostic {
                range: location.range,
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(NumberOrString::String("unresolved-constant".to_string())),
                code_description: None,
                source: Some("ruby-fast-lsp".to_string()),
                message: format!("Unresolved constant `{}`", name),
                related_information: None,
                tags: None,
                data: None,
            },
            UnresolvedEntry::Method {
                name,
                receiver,
                location,
            } => {
                let message = match receiver {
                    Some(recv) => format!("Unresolved method `{}` on `{}`", name, recv),
                    None => format!("Unresolved method `{}`", name),
                };

                Diagnostic {
                    range: location.range,
                    severity: Some(DiagnosticSeverity::WARNING),
                    code: Some(NumberOrString::String("unresolved-method".to_string())),
                    code_description: None,
                    source: Some("ruby-fast-lsp".to_string()),
                    message,
                    related_information: None,
                    tags: None,
                    data: None,
                }
            }
        })
        .collect()
}
