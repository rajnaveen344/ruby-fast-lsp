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
use crate::analyzer_prism::visitors::reference_visitor::ReferenceVisitor;
use crate::capabilities::diagnostics::generate_diagnostics;
use crate::indexer::index_ref::{Index, Unlocked};
use crate::inferrer::type_tracker::TypeTracker;
use crate::server::RubyLanguageServer;
use crate::types::ruby_document::RubyDocument;
use crate::utils::file_ops;
use anyhow::Result;
use log::{debug, warn};
use ruby_prism::Visit;
use std::collections::HashSet;
use std::path::Path;
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
    index: Index<Unlocked>,
}

impl FileProcessor {
    pub fn new(index: Index<Unlocked>) -> Self {
        Self { index }
    }

    /// Get the index handle for creating visitors
    pub fn index(&self) -> &Index<Unlocked> {
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
        // Check if this version was already indexed - skip expensive re-indexing if unchanged
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
        let parse_result = ruby_prism::parse(content.as_bytes());
        let node = parse_result.node();
        let document = RubyDocument::new(uri.clone(), content.to_string(), 0);

        // 2. Generate Syntax Diagnostics
        let mut diagnostics = generate_diagnostics(&parse_result, &document);

        // If severe parse errors, skip indexing
        if parse_result.errors().count() > 10 {
            return Ok(ProcessResult {
                affected_uris: HashSet::new(),
                diagnostics,
            });
        }

        let mut affected_uris = HashSet::new();

        // 3. Index Definitions (Phase 1)
        if options.index_definitions {
            let removed_fqns = self.index.lock().remove_entries_for_uri(uri);
            let removed_fqn_set: HashSet<_> = removed_fqns.into_iter().collect();

            // The `document` variable from above is already initialized with content and parse_result
            // We just need to ensure it's mutable for the visitor.
            // Clone document because parse_result borrows from the original
            let mut visitor = IndexVisitor::new(self.index.clone(), document.clone());
            visitor.visit(&node);
            diagnostics.extend(visitor.diagnostics);

            // Run TypeTracker to infer types for all code
            let mut updated_document = visitor.document.clone();
            if let Some(program) = node.as_program_node() {
                let mut all_snapshots = Vec::new();

                // First, track top-level statements (outside methods)
                let mut top_level_tracker = TypeTracker::new(
                    content.as_bytes(),
                    self.index.clone(),
                    uri,
                );
                top_level_tracker.track_program(&program);
                all_snapshots.extend(top_level_tracker.snapshots().iter().cloned());

                // NOTE: Method return types are ONLY derived from YARD/RBS signatures.
                // We do NOT infer return types from method bodies to keep the system simple and fast.
                // Methods without YARD/RBS annotations will have Unknown return type.

                // Store all snapshots in document (for variable type tracking only)
                updated_document.set_type_snapshots(all_snapshots);
            }

            // Update document with visitor's state (includes lvars for LocalVariable lookup)
            {
                let mut docs = server.docs.lock();
                docs.insert(
                    uri.clone(),
                    Arc::new(parking_lot::RwLock::new(updated_document)),
                );
            }

            if options.resolve_mixins {
                self.index.lock().resolve_mixins_for_uri(uri);
            }

            // NOTE: Return type inference is now done lazily when inlay hints are requested
            // This avoids expensive CFG analysis during indexing and handles method dependencies naturally

            // Calculate diff for cross-file diagnostics
            let added_fqns: Vec<_> = {
                let index = self.index.lock();
                index
                    .file_entries(uri)
                    .iter()
                    .filter_map(|e| index.get_fqn(e.fqn_id).cloned())
                    .collect()
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
        }

        // 4. Index References (Phase 2)
        if options.index_references {
            let mut index = self.index.lock();
            index.remove_references_for_uri(uri);
            index.remove_unresolved_entries_for_uri(uri);
            drop(index);

            // Retrieve document from cache (it was inserted in step 3)
            let document = {
                let docs = server.docs.lock();
                docs.get(uri).and_then(|d| Some(d.read().clone()))
            };

            if let Some(document) = document {
                let mut visitor = ReferenceVisitor::with_unresolved_tracking(
                    self.index.clone(),
                    document,
                    options.include_local_vars,
                );
                visitor.visit(&node);

                // Merge lvar_references from visitor's document into existing document
                {
                    let docs = server.docs.lock();
                    if let Some(doc_arc) = docs.get(uri) {
                        let mut doc = doc_arc.write();
                        doc.clear_lvar_references();
                        for ((scope_id, name), locations) in
                            visitor.document.get_all_lvar_references()
                        {
                            for location in locations {
                                doc.add_lvar_reference(*scope_id, *name, location.clone());
                            }
                        }
                    }
                }
            } else {
                warn!("Document not found for reference indexing: {}", uri);
            }

            // Merge lvar_references from visitor's document into existing document
        }

        // Mark as indexed
        if let Some(doc_arc) = server.docs.lock().get(uri) {
            let mut doc = doc_arc.write();
            doc.indexed_version = Some(doc.version);
        }

        debug!("Processed file {:?}", uri);

        Ok(ProcessResult {
            affected_uris,
            diagnostics,
        })
    }

    // ========================================================================
    // Content-based Indexing (in-memory content)
    // ========================================================================

    /// Index definitions from Ruby content
    /// NOTE: This is used during global workspace indexing.
    /// It only populates the RubyIndex with definitions - it does NOT store
    /// lvars or compute inlay hints. Those are computed on-demand when a file
    /// is opened via did_open. The document is kept temporarily for the
    /// reference indexing phase and cleared by the coordinator after indexing.
    pub fn index_definitions(&self, uri: &Url, content: &str) -> Result<()> {
        let start = Instant::now();
        debug!("Indexing definitions for: {:?}", uri);

        // Remove existing entries for this URI
        self.index.lock().remove_entries_for_uri(uri);

        // Parse and visit AST for definitions
        // Create a document for the visitor (needed for position conversion)
        // NOTE: We don't store lvars here - they are computed on-demand when file is opened
        let document = RubyDocument::new(uri.clone(), content.to_string(), 0);
        let parse_result = document.parse();
        let node = parse_result.node();

        // Clone document because parse_result borrows from the original
        let mut index_visitor = IndexVisitor::new(self.index.clone(), document.clone());
        index_visitor.visit(&node);

        // NOTE: Return type inference is now done lazily when inlay hints are requested

        debug!("Indexed definitions for {:?} in {:?}", uri, start.elapsed());
        Ok(())
    }

    /// Index references from Ruby content
    pub fn index_references(&self, uri: &Url, content: &str) -> Result<()> {
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
        let document = RubyDocument::new(uri.clone(), content.to_string(), 0);
        let mut visitor =
            ReferenceVisitor::with_unresolved_tracking(self.index.clone(), document, true);
        visitor.visit(&node);

        debug!("Indexed references for {:?} in {:?}", uri, start.elapsed());
        Ok(())
    }

    // ========================================================================
    // File-based Indexing (reads from disk)
    // ========================================================================

    /// Index definitions from a single Ruby file
    pub fn index_file_definitions(&self, file_path: &Path) -> Result<()> {
        let uri = file_ops::path_to_uri(file_path)?;
        let content = std::fs::read_to_string(file_path)?;
        self.index_definitions(&uri, &content)
    }

    /// Index references from a single Ruby file
    pub fn index_file_references(&self, file_path: &Path) -> Result<()> {
        let uri = file_ops::path_to_uri(file_path)?;
        let content = std::fs::read_to_string(file_path)?;
        self.index_references(&uri, &content)
    }
}

