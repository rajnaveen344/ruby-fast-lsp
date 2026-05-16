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
use crate::extensions::ExtensionRegistryHandle;
use crate::indexer::analysis_facts::collect_reference_facts_from_locations;
use crate::indexer::index_ref::{Index, Unlocked};
use crate::inferrer::type_tracker::TypeTracker;
use crate::server::RubyLanguageServer;
use crate::types::file_source::FileSource;
use crate::types::ruby_document::RubyDocument;
use crate::utils::file_ops;
use anyhow::Result;
use log::{debug, warn};
use ruby_analysis_core::{
    FullyQualifiedName, GraphEdgeFact, GraphEdgeKind, GraphNodeFact, GraphNodeKind, MethodFact,
    NamespaceKind as AnalysisNamespaceKind, RubyConstant, RubyMethod, SourceKind, SymbolFact,
    SymbolKind as AnalysisSymbolKind, TextRange, UnresolvedGraphEdgeFact,
};
use ruby_analysis_indexer::AnalysisIndexer;
use ruby_fast_lsp_extension_api::{IndexPatch, MixinKind, SourceRange};
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
    extension_registry: ExtensionRegistryHandle,
}

impl FileProcessor {
    pub fn new(index: Index<Unlocked>) -> Self {
        Self {
            index,
            extension_registry: ExtensionRegistryHandle::from_environment(),
        }
    }

    pub fn with_extension_registry(
        index: Index<Unlocked>,
        extension_registry: ExtensionRegistryHandle,
    ) -> Self {
        Self {
            index,
            extension_registry,
        }
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
            let source_kind = self.analysis_source_kind_for_uri(uri);
            let analysis_file_id = server.open_or_update_analysis_file_with_kind(
                uri,
                content.to_string(),
                source_kind,
            );
            let doc = RubyDocument::with_analysis_file_id(
                uri.clone(),
                content.to_string(),
                0,
                analysis_file_id,
            );
            let diagnostics = generate_diagnostics(&parse_result, &doc);
            return Ok(ProcessResult {
                affected_uris: HashSet::new(),
                diagnostics,
            });
        }

        // 1. Parse ONLY ONCE
        let parse_result = ruby_prism::parse(content.as_bytes());
        let node = parse_result.node();
        let source_kind = self.analysis_source_kind_for_uri(uri);
        let analysis_file_id =
            server.open_or_update_analysis_file_with_kind(uri, content.to_string(), source_kind);
        let document = RubyDocument::with_analysis_file_id(
            uri.clone(),
            content.to_string(),
            0,
            analysis_file_id,
        );

        // 2. Generate Syntax Diagnostics
        let mut diagnostics = generate_diagnostics(&parse_result, &document);

        // If severe parse errors, skip indexing
        if parse_result.errors().count() > 10 {
            server
                .analysis_engine
                .lock()
                .replace_type_facts_for_file(analysis_file_id, std::iter::empty());
            server
                .analysis_engine
                .lock()
                .replace_symbol_facts_for_file(analysis_file_id, std::iter::empty());
            server
                .analysis_engine
                .lock()
                .replace_method_facts_for_file(analysis_file_id, std::iter::empty());
            server.analysis_engine.lock().replace_graph_facts_for_file(
                analysis_file_id,
                std::iter::empty(),
                std::iter::empty(),
            );
            server
                .analysis_engine
                .lock()
                .replace_reference_facts_for_file(analysis_file_id, std::iter::empty());
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
            let direct_facts_seed =
                collect_direct_facts(server, content, document.analysis_file_id());
            replace_analysis_facts_for_file(
                server,
                document.analysis_file_id(),
                &direct_facts_seed,
            );

            let mut visitor = IndexVisitor::with_extension_registry_and_analysis_engine(
                self.index.clone(),
                document.clone(),
                self.extension_registry.clone(),
                Some(server.analysis_engine.clone()),
            );
            visitor.visit(&node);
            diagnostics.extend(visitor.diagnostics);

            // Flush indexing-time type facts into the shared analysis engine.
            let extension_index_patches = visitor.extension_index_patches.clone();
            let updated_document = visitor.document.clone();
            let mut direct_facts = direct_facts_seed;
            add_extension_analysis_facts(
                server,
                &updated_document,
                &extension_index_patches,
                &mut direct_facts,
            );
            let symbol_facts = direct_facts.symbols;
            let method_facts = direct_facts.methods;
            let mut type_facts = direct_facts.types;
            type_facts.extend(visitor.type_store.all_facts());
            server
                .analysis_engine
                .lock()
                .replace_type_facts_for_file(updated_document.analysis_file_id(), type_facts);
            server
                .analysis_engine
                .lock()
                .replace_symbol_facts_for_file(updated_document.analysis_file_id(), symbol_facts);
            server
                .analysis_engine
                .lock()
                .replace_method_facts_for_file(updated_document.analysis_file_id(), method_facts);
            if let Some(program) = node.as_program_node() {
                // Run TypeTracker to infer types for all code
                // Track top-level statements (outside methods)
                let mut top_level_tracker =
                    TypeTracker::new(content.as_bytes(), self.index.clone(), uri);
                top_level_tracker.track_program(&program);

                // TypeTracker results are no longer needed — VariableScopes tree
                // already has types from IndexVisitor at assignment positions.
                let _var_types = top_level_tracker.into_var_types();

                // NOTE: Method return types are ONLY derived from YARD/RBS signatures.
                // We do NOT infer return types from method bodies to keep the system simple and fast.
                // Methods without YARD/RBS annotations will have Unknown return type.
            }

            // Update document with visitor's state (includes lvars for LocalVariable lookup)
            {
                let mut docs = server.docs.lock();
                docs.insert(
                    uri.clone(),
                    Arc::new(parking_lot::RwLock::new(updated_document.clone())),
                );
            }

            if options.resolve_mixins {
                self.index.lock().resolve_mixins_for_uri(uri);
            }

            server.analysis_engine.lock().replace_graph_update_for_file(
                updated_document.analysis_file_id(),
                direct_facts.graph_nodes,
                direct_facts.graph_edges,
                direct_facts.unresolved_graph_edges,
            );

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

                let reference_facts = collect_reference_facts_from_locations(
                    &visitor.document,
                    visitor.document.analysis_file_id(),
                    visitor.staged.references.iter(),
                );
                server
                    .analysis_engine
                    .lock()
                    .replace_reference_facts_for_file(
                        visitor.document.analysis_file_id(),
                        reference_facts,
                    );

                // Flush staged writes under a single brief write lock.
                let staged = std::mem::take(&mut visitor.staged);
                {
                    let mut index = self.index.lock();
                    staged.flush(&mut index);
                    index.clear_ancestor_chain_cache();
                }

                // Update the document with VariableScopes from visitor (includes references)
                let docs = server.docs.lock();
                if let Some(doc_arc) = docs.get(uri) {
                    let mut doc = doc_arc.write();
                    doc.variable_scopes = visitor.document.variable_scopes;
                }
            } else {
                warn!("Document not found for reference indexing: {}", uri);
            }
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
        self.index_definitions_inner(uri, content)
    }

    pub fn index_definitions_with_analysis(
        &self,
        uri: &Url,
        content: &str,
        server: &RubyLanguageServer,
    ) -> Result<()> {
        let start = Instant::now();
        debug!("Indexing definitions for: {:?}", uri);

        self.index.lock().remove_entries_for_uri(uri);

        let source_kind = self.analysis_source_kind_for_uri(uri);
        let analysis_file_id =
            server.open_or_update_analysis_file_with_kind(uri, content.to_string(), source_kind);
        let document = RubyDocument::with_analysis_file_id(
            uri.clone(),
            content.to_string(),
            0,
            analysis_file_id,
        );
        let parse_result = ruby_prism::parse(content.as_bytes());
        let node = parse_result.node();

        let direct_facts_seed = collect_direct_facts(server, content, analysis_file_id);
        replace_analysis_facts_for_file(server, analysis_file_id, &direct_facts_seed);

        let mut index_visitor = IndexVisitor::with_extension_registry_and_analysis_engine(
            self.index.clone(),
            document.clone(),
            self.extension_registry.clone(),
            Some(server.analysis_engine.clone()),
        );
        index_visitor.visit(&node);

        let mut direct_facts = direct_facts_seed;
        add_extension_analysis_facts(
            server,
            &document,
            &index_visitor.extension_index_patches,
            &mut direct_facts,
        );
        server
            .analysis_engine
            .lock()
            .replace_symbol_facts_for_file(analysis_file_id, direct_facts.symbols);
        server
            .analysis_engine
            .lock()
            .replace_method_facts_for_file(analysis_file_id, direct_facts.methods);
        server
            .analysis_engine
            .lock()
            .replace_type_facts_for_file(analysis_file_id, direct_facts.types);
        server.analysis_engine.lock().replace_graph_update_for_file(
            analysis_file_id,
            direct_facts.graph_nodes,
            direct_facts.graph_edges,
            direct_facts.unresolved_graph_edges,
        );
        debug!("Indexed definitions for {:?} in {:?}", uri, start.elapsed());
        Ok(())
    }

    fn analysis_source_kind_for_uri(&self, uri: &Url) -> SourceKind {
        let index = self.index.lock();
        index
            .get_file_id(uri)
            .and_then(|file_id| index.get_file_source(file_id))
            .map(analysis_source_kind)
            .unwrap_or(SourceKind::Project)
    }

    fn index_definitions_inner(&self, uri: &Url, content: &str) -> Result<()> {
        let start = Instant::now();
        debug!("Indexing definitions for: {:?}", uri);

        // Remove existing entries for this URI
        self.index.lock().remove_entries_for_uri(uri);

        // Parse and visit AST for definitions
        // Create a document for the visitor (needed for position conversion)
        // NOTE: We don't store lvars here - they are computed on-demand when file is opened
        let document = RubyDocument::new(uri.clone(), content.to_string(), 0);
        let parse_result = ruby_prism::parse(content.as_bytes());
        let node = parse_result.node();

        // Clone document because parse_result borrows from the original
        let mut index_visitor = IndexVisitor::with_extension_registry(
            self.index.clone(),
            document.clone(),
            self.extension_registry.clone(),
        );
        index_visitor.visit(&node);

        // NOTE: Return type inference is now done lazily when inlay hints are requested

        debug!("Indexed definitions for {:?} in {:?}", uri, start.elapsed());
        Ok(())
    }

    /// Index references and unresolved diagnostic entries from Ruby content.
    pub fn index_references(&self, uri: &Url, content: &str) -> Result<()> {
        let start = Instant::now();
        debug!(
            "Indexing references for: {:?} (track_unresolved: true)",
            uri
        );

        // Remove stale unresolved entries
        self.index.lock().remove_unresolved_entries_for_uri(uri);

        let parse_result = ruby_prism::parse(content.as_bytes());
        let node = parse_result.node();

        let document = RubyDocument::new(uri.clone(), content.to_string(), 0);
        let mut visitor =
            ReferenceVisitor::with_unresolved_tracking(self.index.clone(), document, true);
        visitor.visit(&node);

        let staged = std::mem::take(&mut visitor.staged);
        staged.flush(&mut self.index.lock());

        debug!("Indexed references for {:?} in {:?}", uri, start.elapsed());
        Ok(())
    }

    pub fn index_references_with_analysis(
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

        self.index.lock().remove_unresolved_entries_for_uri(uri);

        let source_kind = self.analysis_source_kind_for_uri(uri);
        let analysis_file_id =
            server.open_or_update_analysis_file_with_kind(uri, content.to_string(), source_kind);
        let document = RubyDocument::with_analysis_file_id(
            uri.clone(),
            content.to_string(),
            0,
            analysis_file_id,
        );
        let parse_result = ruby_prism::parse(content.as_bytes());
        let node = parse_result.node();

        let mut visitor =
            ReferenceVisitor::with_unresolved_tracking(self.index.clone(), document, true);
        visitor.visit(&node);

        let reference_facts = collect_reference_facts_from_locations(
            &visitor.document,
            analysis_file_id,
            visitor.staged.references.iter(),
        );
        server
            .analysis_engine
            .lock()
            .replace_reference_facts_for_file(analysis_file_id, reference_facts);

        let staged = std::mem::take(&mut visitor.staged);
        staged.flush(&mut self.index.lock());

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

fn collect_direct_facts(
    server: &RubyLanguageServer,
    content: &str,
    file_id: ruby_analysis_core::SourceFileId,
) -> ruby_analysis_indexer::AnalysisIndex {
    AnalysisIndexer::with_known_namespaces(file_id, collect_known_namespaces(server))
        .index_source(content)
}

fn replace_analysis_facts_for_file(
    server: &RubyLanguageServer,
    file_id: ruby_analysis_core::SourceFileId,
    facts: &ruby_analysis_indexer::AnalysisIndex,
) {
    server
        .analysis_engine
        .lock()
        .replace_symbol_facts_for_file(file_id, facts.symbols.clone());
    server
        .analysis_engine
        .lock()
        .replace_method_facts_for_file(file_id, facts.methods.clone());
    server
        .analysis_engine
        .lock()
        .replace_type_facts_for_file(file_id, facts.types.clone());
    server.analysis_engine.lock().replace_graph_update_for_file(
        file_id,
        facts.graph_nodes.clone(),
        facts.graph_edges.clone(),
        facts.unresolved_graph_edges.clone(),
    );
}

fn collect_known_namespaces(server: &RubyLanguageServer) -> HashSet<FullyQualifiedName> {
    let engine = server.analysis_engine.lock();
    engine
        .all_symbol_facts()
        .into_iter()
        .filter(|fact| {
            matches!(
                fact.kind,
                AnalysisSymbolKind::Class | AnalysisSymbolKind::Module
            )
        })
        .filter_map(|fact| fact.fqn.to_instance_namespace())
        .collect()
}

fn add_extension_analysis_facts(
    server: &RubyLanguageServer,
    document: &RubyDocument,
    patches: &[IndexPatch],
    facts: &mut ruby_analysis_indexer::AnalysisIndex,
) {
    if patches.is_empty() {
        return;
    }

    let mut known_namespaces = {
        let engine = server.analysis_engine.lock();
        engine
            .all_symbol_facts()
            .into_iter()
            .filter(|fact| {
                matches!(
                    fact.kind,
                    AnalysisSymbolKind::Class | AnalysisSymbolKind::Module
                )
            })
            .filter_map(|fact| fact.fqn.to_instance_namespace())
            .collect::<HashSet<_>>()
    };
    for node in &facts.graph_nodes {
        if let Some(namespace) = node.fqn.to_instance_namespace() {
            known_namespaces.insert(namespace);
        }
    }

    for patch in patches {
        match patch {
            IndexPatch::DefineMethod(method) => {
                let namespace = ruby_constants(&method.namespace, "DefineMethod namespace");
                let ruby_method = RubyMethod::new(&method.name).unwrap_or_else(|err| {
                    panic!(
                        "INVARIANT VIOLATED: extension emitted invalid analysis method `{}`: {}. \
                         This is a bug because extension method patches must be validated before fact conversion. \
                         Fix: reject invalid DefineMethod patches at the extension boundary.",
                        method.name, err
                    )
                });
                let fqn = FullyQualifiedName::method(namespace.clone(), ruby_method);
                let owner = FullyQualifiedName::namespace_with_kind(
                    namespace,
                    analysis_namespace_kind(method.owner_kind),
                );
                let range = text_range_from_source_range(document, method.location, "method");
                facts.symbols.push(SymbolFact::new(
                    fqn.clone(),
                    AnalysisSymbolKind::Method,
                    range,
                ));
                facts.methods.push(MethodFact::new(fqn, owner, range));
            }
            IndexPatch::ApplyMixin(mixin) => {
                let mut source_parts = ruby_constants(&mixin.namespace, "ApplyMixin namespace");
                if source_parts.is_empty() {
                    source_parts.push(RubyConstant::new("Object").expect(
                        "INVARIANT VIOLATED: Object is not a valid Ruby constant. \
                         This is a bug because root mixin patches normalize to Object. \
                         Fix: keep RubyConstant validation compatible with Ruby class names.",
                    ));
                    let object = FullyQualifiedName::namespace(source_parts.clone());
                    let range = text_range_from_source_range(document, mixin.location, "mixin");
                    facts.graph_nodes.push(GraphNodeFact::new(
                        object.clone(),
                        GraphNodeKind::Class,
                        range,
                    ));
                    facts.graph_nodes.push(GraphNodeFact::new(
                        object.to_singleton_namespace().expect(
                            "INVARIANT VIOLATED: Object namespace could not convert to singleton. \
                             This is a bug because namespace graph nodes must support singleton variants. \
                             Fix: update FullyQualifiedName singleton conversion.",
                        ),
                        GraphNodeKind::Class,
                        range,
                    ));
                    known_namespaces.insert(object);
                }

                let source = FullyQualifiedName::namespace_with_kind(
                    source_parts.clone(),
                    analysis_namespace_kind(mixin.target_kind),
                );
                let target_parts = ruby_constants(&mixin.mixin, "ApplyMixin target");
                let kind = analysis_mixin_kind(mixin.kind);
                let range = text_range_from_source_range(document, mixin.location, "mixin");
                push_extension_graph_edge(
                    facts,
                    &known_namespaces,
                    source.clone(),
                    &target_parts,
                    mixin.absolute,
                    FullyQualifiedName::namespace(source_parts.clone()),
                    kind,
                    range,
                );
                if mixin.kind == MixinKind::Extend {
                    if let Some(singleton_source) = source.to_singleton_namespace() {
                        push_extension_graph_edge(
                            facts,
                            &known_namespaces,
                            singleton_source,
                            &target_parts,
                            mixin.absolute,
                            FullyQualifiedName::namespace(source_parts),
                            GraphEdgeKind::Include,
                            range,
                        );
                    }
                }
            }
        }
    }
}

fn push_extension_graph_edge(
    facts: &mut ruby_analysis_indexer::AnalysisIndex,
    known_namespaces: &HashSet<FullyQualifiedName>,
    source: FullyQualifiedName,
    target_parts: &[RubyConstant],
    absolute: bool,
    context: FullyQualifiedName,
    kind: GraphEdgeKind,
    range: TextRange,
) {
    let Some(target) =
        resolve_extension_namespace(known_namespaces, target_parts, absolute, &context)
    else {
        facts
            .unresolved_graph_edges
            .push(UnresolvedGraphEdgeFact::new(
                source,
                target_parts.to_vec(),
                absolute,
                context,
                kind,
                range,
            ));
        return;
    };
    facts
        .graph_edges
        .push(GraphEdgeFact::new(source, target, kind, range));
}

fn resolve_extension_namespace(
    known_namespaces: &HashSet<FullyQualifiedName>,
    parts: &[RubyConstant],
    absolute: bool,
    context: &FullyQualifiedName,
) -> Option<FullyQualifiedName> {
    let mut search = if absolute {
        Vec::new()
    } else {
        context.namespace_parts()
    };

    loop {
        let mut probe = search.clone();
        probe.extend(parts.iter().cloned());
        let fqn = FullyQualifiedName::namespace(probe);
        if known_namespaces.contains(&fqn) {
            return Some(fqn);
        }
        if absolute || search.is_empty() {
            break;
        }
        search.pop();
    }

    let fqn = FullyQualifiedName::namespace(parts.to_vec());
    known_namespaces.contains(&fqn).then_some(fqn)
}

fn ruby_constants(parts: &[String], label: &str) -> Vec<RubyConstant> {
    parts
        .iter()
        .map(|part| {
            RubyConstant::new(part).unwrap_or_else(|err| {
                panic!(
                    "INVARIANT VIOLATED: extension emitted invalid {label} constant `{}`: {}. \
                     This is a bug because extension constant patches must be valid Ruby constants. \
                     Fix: validate constants before emitting extension index patches.",
                    part, err
                )
            })
        })
        .collect()
}

fn analysis_namespace_kind(
    kind: ruby_fast_lsp_extension_api::NamespaceKind,
) -> AnalysisNamespaceKind {
    match kind {
        ruby_fast_lsp_extension_api::NamespaceKind::Instance => AnalysisNamespaceKind::Instance,
        ruby_fast_lsp_extension_api::NamespaceKind::Singleton => AnalysisNamespaceKind::Singleton,
    }
}

fn analysis_mixin_kind(kind: MixinKind) -> GraphEdgeKind {
    match kind {
        MixinKind::Include => GraphEdgeKind::Include,
        MixinKind::Prepend => GraphEdgeKind::Prepend,
        MixinKind::Extend => GraphEdgeKind::Extend,
    }
}

fn analysis_source_kind(source: FileSource) -> SourceKind {
    match source {
        FileSource::Project => SourceKind::Project,
        FileSource::Stub => SourceKind::Stub,
        FileSource::Stdlib => SourceKind::Stdlib,
        FileSource::Gem => SourceKind::Gem,
    }
}

fn text_range_from_source_range(
    document: &RubyDocument,
    range: SourceRange,
    kind: &str,
) -> TextRange {
    let start = tower_lsp::lsp_types::Position {
        line: range.start.line,
        character: range.start.character,
    };
    let end = tower_lsp::lsp_types::Position {
        line: range.end.line,
        character: range.end.character,
    };
    TextRange::new(
        document.analysis_file_id(),
        byte_offset_u32(
            document.position_to_offset(start),
            &format!("extension {kind} start offset exceeded u32"),
        ),
        byte_offset_u32(
            document.position_to_offset(end),
            &format!("extension {kind} end offset exceeded u32"),
        ),
    )
}

fn byte_offset_u32(byte_offset: usize, message: &str) -> u32 {
    u32::try_from(byte_offset).unwrap_or_else(|_| {
        panic!(
            "INVARIANT VIOLATED: {message}. \
             This is a bug because ruby-analysis-core TextRange currently stores u32 offsets. \
             Fix: widen TextRange offsets before indexing files larger than u32::MAX bytes."
        )
    })
}
