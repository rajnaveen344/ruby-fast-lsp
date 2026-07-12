//! File Processing Module
//!
//! This module provides shared file processing logic. It handles parsing,
//! fact collection, reference candidates, and diagnostic generation.
//!
//! ## Key Components
//!
//! - **`FileProcessor`**: Core struct for processing individual files
//! - **`ProcessResult`**: Results of processing including diagnostics and affected URIs
//! - **`get_unresolved_diagnostics`**: Generates diagnostics for unresolved constants/methods
//!
//! ## Usage
//!
//! Each indexer (project, stdlib, gem) discovers files to process, then delegates
//! the actual processing to `FileProcessor` with appropriate options.

use crate::capabilities::diagnostics::generate_diagnostics;
use crate::extensions::{analysis_ruby_type_from_extension, ExtensionRegistryHandle};
use crate::server::RubyLanguageServer;
use anyhow::Result;
use log::debug;
use ruby_analysis::core::{
    FullyQualifiedName, GraphEdgeFact, GraphEdgeKind, GraphNodeFact, GraphNodeKind, MethodFact,
    MethodParamFact, MethodParamKind as AnalysisMethodParamKind,
    NamespaceKind as AnalysisNamespaceKind, RubyConstant, RubyMethod, SourceKind, SymbolFact,
    SymbolKind as AnalysisSymbolKind, TextRange, TypeFact, TypeProvenance, TypeSubject,
    UnresolvedGraphEdgeFact,
};
use ruby_analysis::engine::{AnalysisQuery, FileFacts, ResolveMode, SemanticChange};
use ruby_analysis::indexer::fact_collector::FactCollector;
use ruby_analysis::indexer::RubyDocument;
use ruby_analysis::indexer::{is_erb_path, mask_erb, AnalysisIndexer};
use ruby_analysis::method_store::MethodVisibility as AnalysisMethodVisibility;
use ruby_fast_lsp_extension_api::{IndexPatch, MixinKind, NamespaceDeclarationKind, SourceRange};
use ruby_prism::Visit;
use std::borrow::Cow;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use tower_lsp::lsp_types::{Diagnostic, Url};

/// Result of processing a file
pub struct ProcessResult {
    /// Functionally affected URIs (files that need updated diagnostics)
    pub affected_uris: HashSet<Url>,
    /// Syntax and early validation diagnostics
    pub diagnostics: Vec<Diagnostic>,
    /// Whether this pass changed declarations visible to other files.
    pub semantic_change: SemanticChange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileResolution {
    Full,
    CurrentFile,
    Deferred,
}

fn analysis_source<'a>(uri: &Url, content: &'a str) -> Cow<'a, str> {
    if is_erb_path(uri.path()) {
        let mut source = mask_erb(content).source().to_string();
        if source.starts_with("#!") {
            source.replace_range(1..2, "#");
        }
        Cow::Owned(source)
    } else {
        ruby_analysis::indexer::mask_shebang(content)
    }
}

// ============================================================================
// FileProcessor
// ============================================================================

/// File processor for handling parsing, indexing, and diagnostic generation
#[derive(Debug, Clone)]
pub struct FileProcessor {
    extension_registry: ExtensionRegistryHandle,
}

impl FileProcessor {
    pub fn new() -> Self {
        Self {
            extension_registry: ExtensionRegistryHandle::from_environment(),
        }
    }

    pub fn with_extension_registry(extension_registry: ExtensionRegistryHandle) -> Self {
        Self { extension_registry }
    }

    /// Process a file: parse, collect facts and reference candidates, and return diagnostics.
    /// This prevents double-parsing and centralizes the logic.
    pub fn process_file(
        &self,
        uri: &Url,
        content: &str,
        server: &RubyLanguageServer,
    ) -> Result<ProcessResult> {
        self.process_file_with_resolution(uri, content, server, FileResolution::Full)
    }

    pub fn process_file_current_file_resolution(
        &self,
        uri: &Url,
        content: &str,
        server: &RubyLanguageServer,
    ) -> Result<ProcessResult> {
        self.process_file_with_resolution(uri, content, server, FileResolution::CurrentFile)
    }

    pub fn process_file_current_file_resolution_forced(
        &self,
        uri: &Url,
        content: &str,
        server: &RubyLanguageServer,
    ) -> Result<ProcessResult> {
        self.process_file_with_resolution_forced(
            uri,
            content,
            server,
            FileResolution::CurrentFile,
            true,
        )
    }

    pub fn process_file_deferred_resolution(
        &self,
        uri: &Url,
        content: &str,
        server: &RubyLanguageServer,
    ) -> Result<ProcessResult> {
        self.process_file_with_resolution(uri, content, server, FileResolution::Deferred)
    }

    fn process_file_with_resolution(
        &self,
        uri: &Url,
        content: &str,
        server: &RubyLanguageServer,
        resolution: FileResolution,
    ) -> Result<ProcessResult> {
        self.process_file_with_resolution_forced(uri, content, server, resolution, false)
    }

    fn process_file_with_resolution_forced(
        &self,
        uri: &Url,
        content: &str,
        server: &RubyLanguageServer,
        resolution: FileResolution,
        force_reindex: bool,
    ) -> Result<ProcessResult> {
        // Check if this version was already indexed - skip expensive re-indexing if unchanged
        let already_indexed = !force_reindex && {
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
            let analysis_source = analysis_source(uri, content);
            let parse_result = ruby_prism::parse(analysis_source.as_bytes());
            let source_kind = self.analysis_source_kind_for_uri(server, uri);
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
                semantic_change: SemanticChange::BodyOnly,
            });
        }

        // 1. Parse ONLY ONCE
        let analysis_source = analysis_source(uri, content);
        let parse_result = ruby_prism::parse(analysis_source.as_bytes());
        let node = parse_result.node();
        let source_kind = self.analysis_source_kind_for_uri(server, uri);
        let analysis_file_id =
            server.open_or_update_analysis_file_with_kind(uri, content.to_string(), source_kind);
        let document_version = server
            .docs
            .lock()
            .get(uri)
            .map(|document| document.read().version)
            .unwrap_or(0);
        let document = RubyDocument::with_analysis_file_id(
            uri.clone(),
            content.to_string(),
            document_version,
            analysis_file_id,
        );
        let previous_export_fingerprint = server
            .analysis_engine
            .read()
            .semantic_export_fingerprint(analysis_file_id);

        // 2. Generate Syntax Diagnostics
        let diagnostics = generate_diagnostics(&parse_result, &document);

        // If severe parse errors, skip indexing
        if parse_result.errors().count() > 10 {
            let semantic_change =
                replace_file_analysis(server, analysis_file_id, FileFacts::default(), resolution);
            return Ok(ProcessResult {
                affected_uris: HashSet::new(),
                diagnostics,
                semantic_change,
            });
        }

        let affected_uris = HashSet::new();

        // 3. Collect facts.
        let direct_facts_seed = collect_direct_facts(
            server,
            &node,
            analysis_source.as_ref(),
            document.analysis_file_id(),
            None,
        );
        replace_analysis_facts_for_file(
            server,
            document.analysis_file_id(),
            &direct_facts_seed,
            false,
        );
        self.extension_registry
            .ensure_semantic_seed_facts(&server.analysis_engine);

        let mut visitor = FactCollector::analysis_only(
            document.clone(),
            Arc::new(self.extension_registry.clone()),
            server.analysis_engine.clone(),
        );
        visitor.visit(&node);

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
        let existing_type_subjects = type_facts
            .iter()
            .map(|fact| fact.subject.clone())
            .collect::<HashSet<_>>();
        type_facts.extend(
            visitor
                .type_store
                .all_facts()
                .into_iter()
                .filter(|fact| !existing_type_subjects.contains(&fact.subject)),
        );
        replace_file_analysis(
            server,
            updated_document.analysis_file_id(),
            FileFacts {
                symbols: symbol_facts,
                methods: method_facts,
                method_visibility_overrides: direct_facts.method_visibility_overrides,
                types: type_facts,
                graph_nodes: direct_facts.graph_nodes,
                graph_edges: direct_facts.graph_edges,
                unresolved_graph_edges: direct_facts.unresolved_graph_edges,
                reference_candidates: if source_kind.contributes_references() {
                    visitor.reference_candidates
                } else {
                    Vec::new()
                },
                diagnostic_candidates: if source_kind.contributes_project_diagnostics() {
                    visitor.diagnostic_candidates
                } else {
                    Vec::new()
                },
                diagnostics: if source_kind.contributes_project_diagnostics() {
                    visitor.analysis_diagnostics
                } else {
                    Vec::new()
                },
            },
            resolution,
        );
        let current_export_fingerprint = server
            .analysis_engine
            .read()
            .semantic_export_fingerprint(analysis_file_id)
            .expect(
                "INVARIANT VIOLATED: processed file has no semantic export fingerprint. This is a bug because every engine fact replacement must record its exported API. Fix: route final file facts through AnalysisEngine::replace_facts.",
            );
        let semantic_change =
            SemanticChange::classify(previous_export_fingerprint, current_export_fingerprint);

        {
            let mut docs = server.docs.lock();
            docs.insert(
                uri.clone(),
                Arc::new(parking_lot::RwLock::new(updated_document.clone())),
            );
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
            semantic_change,
        })
    }

    // ========================================================================
    // Content-based Indexing (in-memory content)
    // ========================================================================

    pub fn collect_file_facts(
        &self,
        uri: &Url,
        content: &str,
        server: &RubyLanguageServer,
    ) -> Result<()> {
        self.collect_file_facts_as(uri, content, server, SourceKind::Project)
    }

    pub fn collect_file_facts_as(
        &self,
        uri: &Url,
        content: &str,
        server: &RubyLanguageServer,
        source_kind: SourceKind,
    ) -> Result<()> {
        self.collect_file_facts_as_with_resolution(uri, content, server, source_kind, true, None)
    }

    pub fn collect_file_facts_as_deferred_resolution(
        &self,
        uri: &Url,
        content: &str,
        server: &RubyLanguageServer,
        source_kind: SourceKind,
    ) -> Result<()> {
        self.collect_file_facts_as_with_resolution(uri, content, server, source_kind, false, None)
    }

    pub fn collect_rbs_facts_as_deferred_resolution(
        &self,
        uri: &Url,
        content: &str,
        server: &RubyLanguageServer,
    ) -> Result<()> {
        self.collect_rbs_facts_with_resolution(uri, content, server, FileResolution::Deferred)
    }

    pub fn collect_rbs_facts(
        &self,
        uri: &Url,
        content: &str,
        server: &RubyLanguageServer,
    ) -> Result<()> {
        self.collect_rbs_facts_with_resolution(uri, content, server, FileResolution::Full)
    }

    fn collect_rbs_facts_with_resolution(
        &self,
        uri: &Url,
        content: &str,
        server: &RubyLanguageServer,
        resolution: FileResolution,
    ) -> Result<()> {
        let analysis_file_id = server.open_or_update_analysis_file_with_kind(
            uri,
            content.to_string(),
            SourceKind::Signature,
        );
        let facts = match ruby_analysis::indexer::index_rbs(analysis_file_id, content) {
            Ok(facts) => facts,
            Err(error) => {
                replace_file_analysis(server, analysis_file_id, FileFacts::default(), resolution);
                return Err(anyhow::anyhow!(
                    "Failed to parse RBS {}: {error}",
                    uri.path()
                ));
            }
        };
        replace_file_analysis(
            server,
            analysis_file_id,
            FileFacts {
                symbols: facts.symbols,
                methods: facts.methods,
                method_visibility_overrides: facts.method_visibility_overrides,
                types: facts.types,
                graph_nodes: facts.graph_nodes,
                graph_edges: facts.graph_edges,
                unresolved_graph_edges: facts.unresolved_graph_edges,
                ..Default::default()
            },
            resolution,
        );
        Ok(())
    }

    pub fn collect_file_facts_as_deferred_resolution_with_known_namespaces(
        &self,
        uri: &Url,
        content: &str,
        server: &RubyLanguageServer,
        source_kind: SourceKind,
        known_namespaces: &HashSet<FullyQualifiedName>,
    ) -> Result<()> {
        self.collect_file_facts_as_with_resolution(
            uri,
            content,
            server,
            source_kind,
            false,
            Some(known_namespaces),
        )
    }

    fn collect_file_facts_as_with_resolution(
        &self,
        uri: &Url,
        content: &str,
        server: &RubyLanguageServer,
        source_kind: SourceKind,
        resolve_references: bool,
        known_namespaces: Option<&HashSet<FullyQualifiedName>>,
    ) -> Result<()> {
        debug!("Collecting facts for: {:?}", uri);

        let analysis_file_id =
            server.open_or_update_analysis_file_with_kind(uri, content.to_string(), source_kind);
        let document = RubyDocument::with_analysis_file_id(
            uri.clone(),
            content.to_string(),
            0,
            analysis_file_id,
        );

        let analysis_source = analysis_source(uri, content);
        let parse_result = ruby_prism::parse(analysis_source.as_bytes());
        let node = parse_result.node();

        let direct_facts_seed = if resolve_references {
            collect_direct_facts(
                server,
                &node,
                analysis_source.as_ref(),
                analysis_file_id,
                known_namespaces,
            )
        } else {
            ruby_analysis::indexer::AnalysisIndex::default()
        };
        if resolve_references {
            replace_analysis_facts_for_file(
                server,
                analysis_file_id,
                &direct_facts_seed,
                resolve_references,
            );
        }
        self.extension_registry
            .ensure_semantic_seed_facts(&server.analysis_engine);

        let mut fact_collector = FactCollector::analysis_only(
            document.clone(),
            Arc::new(self.extension_registry.clone()),
            server.analysis_engine.clone(),
        );
        if !resolve_references {
            let direct_known_namespaces = known_namespaces
                .cloned()
                .unwrap_or_else(|| collect_known_namespaces(server));
            fact_collector = fact_collector.with_direct_known_namespaces(direct_known_namespaces);
        }
        fact_collector.visit(&node);

        let mut direct_facts = if resolve_references {
            direct_facts_seed
        } else {
            fact_collector.direct_facts.clone()
        };
        add_extension_analysis_facts(
            server,
            &document,
            &fact_collector.extension_index_patches,
            &mut direct_facts,
        );
        let reference_candidates = if source_kind.contributes_references() {
            fact_collector.reference_candidates
        } else {
            Vec::new()
        };
        let (diagnostic_candidates, diagnostics) = if source_kind.contributes_project_diagnostics()
        {
            (
                fact_collector.diagnostic_candidates,
                fact_collector.analysis_diagnostics,
            )
        } else {
            (Vec::new(), Vec::new())
        };
        replace_file_analysis(
            server,
            analysis_file_id,
            FileFacts {
                symbols: direct_facts.symbols,
                methods: direct_facts.methods,
                method_visibility_overrides: direct_facts.method_visibility_overrides,
                types: direct_facts.types,
                graph_nodes: direct_facts.graph_nodes,
                graph_edges: direct_facts.graph_edges,
                unresolved_graph_edges: direct_facts.unresolved_graph_edges,
                reference_candidates,
                diagnostic_candidates,
                diagnostics,
            },
            if resolve_references {
                FileResolution::Full
            } else {
                FileResolution::Deferred
            },
        );
        debug!("Collected facts for {:?}", uri);
        Ok(())
    }

    fn analysis_source_kind_for_uri(&self, server: &RubyLanguageServer, uri: &Url) -> SourceKind {
        let path = uri
            .to_file_path()
            .unwrap_or_else(|_| PathBuf::from(uri.to_string()));
        let engine = server.analysis_engine.read();
        engine
            .file_id(&path)
            .and_then(|file_id| engine.file(file_id))
            .map(|file| file.kind)
            .unwrap_or(SourceKind::Project)
    }
}

impl Default for FileProcessor {
    fn default() -> Self {
        Self::new()
    }
}

struct ExtensionGraphEdge<'a> {
    source: FullyQualifiedName,
    target_parts: &'a [RubyConstant],
    absolute: bool,
    context: FullyQualifiedName,
    kind: GraphEdgeKind,
    range: TextRange,
}

fn collect_direct_facts(
    server: &RubyLanguageServer,
    node: &ruby_prism::Node<'_>,
    content: &str,
    file_id: ruby_analysis::core::SourceFileId,
    known_namespaces: Option<&HashSet<FullyQualifiedName>>,
) -> ruby_analysis::indexer::AnalysisIndex {
    let known_namespaces = known_namespaces
        .cloned()
        .unwrap_or_else(|| collect_known_namespaces(server));
    AnalysisIndexer::with_known_namespaces(file_id, known_namespaces)
        .index_node_with_source(node, content)
}

fn replace_analysis_facts_for_file(
    server: &RubyLanguageServer,
    file_id: ruby_analysis::core::SourceFileId,
    facts: &ruby_analysis::indexer::AnalysisIndex,
    resolve_references: bool,
) {
    replace_file_analysis(
        server,
        file_id,
        file_analysis_facts_from_index(facts),
        if resolve_references {
            FileResolution::Full
        } else {
            FileResolution::Deferred
        },
    );
}

fn replace_file_analysis(
    server: &RubyLanguageServer,
    file_id: ruby_analysis::core::SourceFileId,
    facts: FileFacts,
    resolution: FileResolution,
) -> SemanticChange {
    let mut engine = server.analysis_engine.write();
    match resolution {
        FileResolution::Full => engine.replace_facts(file_id, facts, ResolveMode::Immediate),
        FileResolution::CurrentFile => {
            let semantic_change = engine.replace_facts(file_id, facts, ResolveMode::Deferred);
            engine.resolve_file(file_id);
            semantic_change
        }
        FileResolution::Deferred => engine.replace_facts(file_id, facts, ResolveMode::Deferred),
    }
}

fn file_analysis_facts_from_index(facts: &ruby_analysis::indexer::AnalysisIndex) -> FileFacts {
    FileFacts {
        symbols: facts.symbols.clone(),
        methods: facts.methods.clone(),
        method_visibility_overrides: facts.method_visibility_overrides.clone(),
        types: facts.types.clone(),
        graph_nodes: facts.graph_nodes.clone(),
        graph_edges: facts.graph_edges.clone(),
        unresolved_graph_edges: facts.unresolved_graph_edges.clone(),
        reference_candidates: Vec::new(),
        diagnostic_candidates: Vec::new(),
        diagnostics: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_processor_reports_body_only_and_exported_api_changes() {
        let server = RubyLanguageServer::default();
        let processor = FileProcessor::with_extension_registry(server.extension_registry.clone());
        let uri = Url::parse("file:///app/user.rb").unwrap();

        let initial = processor
            .process_file_current_file_resolution_forced(
                &uri,
                "class User\n  def name\n    'A'\n  end\nend\n",
                &server,
            )
            .unwrap();
        assert_eq!(initial.semantic_change, SemanticChange::InitialIndex);

        let body = processor
            .process_file_current_file_resolution_forced(
                &uri,
                "class User\n  def name\n    'B'\n  end\nend\n",
                &server,
            )
            .unwrap();
        assert_eq!(body.semantic_change, SemanticChange::BodyOnly);

        let api = processor
            .process_file_current_file_resolution_forced(
                &uri,
                "class User\n  def name(prefix)\n    prefix\n  end\nend\n",
                &server,
            )
            .unwrap();
        assert_eq!(api.semantic_change, SemanticChange::ExportsChanged);
    }

    #[test]
    fn file_processor_handles_shebang_source_without_crashing() {
        let server = RubyLanguageServer::default();
        let processor = FileProcessor::with_extension_registry(server.extension_registry.clone());
        let uri = Url::parse("file:///project/Rakefile").unwrap();
        let source = "#!/usr/bin/env rake\n# frozen_string_literal: true\nrequire File.expand_path('../config/application', __FILE__)\nDiscourse::Application.load_tasks\n";

        let result = processor
            .process_file_current_file_resolution_forced(&uri, source, &server)
            .expect("shebang-bearing Ruby entry points must index successfully");

        assert_eq!(result.semantic_change, SemanticChange::InitialIndex);
    }
}

fn collect_known_namespaces(server: &RubyLanguageServer) -> HashSet<FullyQualifiedName> {
    let engine = server.analysis_engine.read();
    AnalysisQuery::new(&engine).known_namespace_fqns()
}

fn add_extension_analysis_facts(
    server: &RubyLanguageServer,
    document: &RubyDocument,
    patches: &[IndexPatch],
    facts: &mut ruby_analysis::indexer::AnalysisIndex,
) {
    if patches.is_empty() {
        return;
    }

    let mut known_namespaces = {
        let engine = server.analysis_engine.read();
        AnalysisQuery::new(&engine).known_namespace_fqns()
    };
    for node in &facts.graph_nodes {
        if let Some(namespace) = node.fqn.to_instance_namespace() {
            known_namespaces.insert(namespace);
        }
    }

    for patch in patches {
        match patch {
            IndexPatch::DefineNamespace(namespace) => {
                let parts = ruby_constants(&namespace.namespace, "DefineNamespace namespace");
                let fqn = FullyQualifiedName::namespace(parts);
                let range = text_range_from_source_range(document, namespace.location, "namespace");
                let kind = match namespace.kind {
                    NamespaceDeclarationKind::Class => GraphNodeKind::Class,
                    NamespaceDeclarationKind::Module => GraphNodeKind::Module,
                };
                if !facts
                    .symbols
                    .iter()
                    .any(|fact| fact.fqn == fqn && fact.range == range)
                {
                    facts.symbols.push(SymbolFact::new(
                        fqn.clone(),
                        match kind {
                            GraphNodeKind::Class => AnalysisSymbolKind::Class,
                            GraphNodeKind::Module => AnalysisSymbolKind::Module,
                        },
                        range,
                    ));
                }
                if !facts
                    .graph_nodes
                    .iter()
                    .any(|fact| fact.fqn == fqn && fact.kind == kind && fact.range == range)
                {
                    facts
                        .graph_nodes
                        .push(GraphNodeFact::new(fqn.clone(), kind, range));
                    facts.graph_nodes.push(GraphNodeFact::new(
                        fqn.to_singleton_namespace().expect(
                            "INVARIANT VIOLATED: extension namespace could not convert to singleton. This is a bug because validated namespace declarations must produce namespace FQNs. Fix: construct DefineNamespace facts from FullyQualifiedName::namespace.",
                        ),
                        kind,
                        range,
                    ));
                }
                let namespace_type = TypeFact::new(
                    TypeSubject::Constant(FullyQualifiedName::constant(fqn.namespace_parts())),
                    match kind {
                        GraphNodeKind::Class => {
                            ruby_analysis::core::RubyType::ClassReference(fqn.clone())
                        }
                        GraphNodeKind::Module => {
                            ruby_analysis::core::RubyType::ModuleReference(fqn.clone())
                        }
                    },
                    range,
                    TypeProvenance::Extension,
                );
                if !facts.types.contains(&namespace_type) {
                    facts.types.push(namespace_type);
                }
                known_namespaces.insert(fqn);
            }
            IndexPatch::DefineConstant(constant) => {
                let mut parts = ruby_constants(&constant.namespace, "DefineConstant namespace");
                parts.push(RubyConstant::new(&constant.name).unwrap_or_else(|err| {
                    panic!(
                        "INVARIANT VIOLATED: extension emitted invalid constant `{}`: {}. This is a bug because constant patches must be validated before fact conversion. Fix: reject invalid DefineConstant patches at the extension boundary.",
                        constant.name, err
                    )
                }));
                let fqn = FullyQualifiedName::constant(parts);
                let range = text_range_from_source_range(document, constant.location, "constant");
                if !facts
                    .symbols
                    .iter()
                    .any(|fact| fact.fqn == fqn && fact.range == range)
                {
                    facts.symbols.push(SymbolFact::new(
                        fqn.clone(),
                        AnalysisSymbolKind::Constant,
                        range,
                    ));
                }
                if let Some(ruby_type) =
                    analysis_ruby_type_from_extension(constant.ruby_type.as_ref())
                        .expect("INVARIANT VIOLATED: extension constant type reached fact conversion without validation. This is a bug because guest patches must be validated before collection. Fix: keep extension payload validation before patch application.")
                {
                    let type_fact = TypeFact::new(
                        TypeSubject::Constant(fqn),
                        ruby_type,
                        range,
                        TypeProvenance::Extension,
                    );
                    if !facts.types.contains(&type_fact) {
                        facts.types.push(type_fact);
                    }
                }
            }
            IndexPatch::AddReference(_) => {
                // Resolved reference candidates are applied to FactCollector during
                // extension call traversal and flow through FileFacts separately
                // from direct parser/index facts.
            }
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
                if !facts
                    .symbols
                    .iter()
                    .any(|fact| fact.fqn == fqn && fact.range == range)
                {
                    facts.symbols.push(SymbolFact::new(
                        fqn.clone(),
                        AnalysisSymbolKind::Method,
                        range,
                    ));
                }
                let return_type = analysis_ruby_type_from_extension(method.return_type.as_ref())
                    .expect("INVARIANT VIOLATED: extension return type reached fact conversion without validation. This is a bug because guest patches must be validated before collection. Fix: keep extension payload validation before patch application.");
                let return_type_label = return_type.as_ref().map(ToString::to_string);
                let method_fact = MethodFact::with_param_facts(
                    fqn.clone(),
                    owner.clone(),
                    range,
                    analysis_method_params_from_extension(&method.params),
                )
                .with_visibility(analysis_method_visibility(method.visibility))
                .with_signature_metadata(None, return_type_label);
                if let Some(existing) = facts
                    .methods
                    .iter_mut()
                    .find(|fact| fact.fqn == fqn && fact.owner == owner && fact.range == range)
                {
                    *existing = method_fact;
                } else {
                    facts.methods.push(method_fact);
                }
                if let Some(return_type) = return_type {
                    facts.types.push(TypeFact::new(
                        TypeSubject::MethodReturn(fqn),
                        return_type,
                        range,
                        TypeProvenance::Extension,
                    ));
                }
            }
            IndexPatch::SetSuperclass(superclass) => {
                let source_parts = ruby_constants(&superclass.namespace, "SetSuperclass namespace");
                let source = FullyQualifiedName::namespace(source_parts.clone());
                let target_parts = ruby_constants(&superclass.superclass, "SetSuperclass target");
                let range =
                    text_range_from_source_range(document, superclass.location, "superclass");
                let context = FullyQualifiedName::namespace(source_parts.clone());
                if let Some(target) = resolve_extension_namespace(
                    &known_namespaces,
                    &target_parts,
                    superclass.absolute,
                    &context,
                ) {
                    let source_singleton = source.to_singleton_namespace().expect(
                        "INVARIANT VIOLATED: generated class namespace could not convert to singleton. This is a bug because validated class declarations must support Ruby singleton inheritance. Fix: construct SetSuperclass sources from FullyQualifiedName::namespace.",
                    );
                    if let Some(target_singleton) = target.to_singleton_namespace() {
                        facts.graph_edges.push(GraphEdgeFact::new(
                            source_singleton,
                            target_singleton,
                            GraphEdgeKind::Superclass,
                            range,
                        ));
                    }
                }
                push_extension_graph_edge(
                    facts,
                    &known_namespaces,
                    ExtensionGraphEdge {
                        source,
                        target_parts: &target_parts,
                        absolute: superclass.absolute,
                        context,
                        kind: GraphEdgeKind::Superclass,
                        range,
                    },
                );
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
                    ExtensionGraphEdge {
                        source: source.clone(),
                        target_parts: &target_parts,
                        absolute: mixin.absolute,
                        context: FullyQualifiedName::namespace(source_parts.clone()),
                        kind,
                        range,
                    },
                );
                if mixin.kind == MixinKind::Extend {
                    if let Some(singleton_source) = source.to_singleton_namespace() {
                        push_extension_graph_edge(
                            facts,
                            &known_namespaces,
                            ExtensionGraphEdge {
                                source: singleton_source,
                                target_parts: &target_parts,
                                absolute: mixin.absolute,
                                context: FullyQualifiedName::namespace(source_parts),
                                kind: GraphEdgeKind::Include,
                                range,
                            },
                        );
                    }
                }
            }
        }
    }
}

fn push_extension_graph_edge(
    facts: &mut ruby_analysis::indexer::AnalysisIndex,
    known_namespaces: &HashSet<FullyQualifiedName>,
    edge: ExtensionGraphEdge<'_>,
) {
    let Some(target) = resolve_extension_namespace(
        known_namespaces,
        edge.target_parts,
        edge.absolute,
        &edge.context,
    ) else {
        facts
            .unresolved_graph_edges
            .push(UnresolvedGraphEdgeFact::new(
                edge.source,
                edge.target_parts.to_vec(),
                edge.absolute,
                edge.context,
                edge.kind,
                edge.range,
            ));
        return;
    };
    facts.graph_edges.push(GraphEdgeFact::new(
        edge.source,
        target,
        edge.kind,
        edge.range,
    ));
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

fn analysis_method_visibility(
    visibility: ruby_fast_lsp_extension_api::MethodVisibility,
) -> AnalysisMethodVisibility {
    match visibility {
        ruby_fast_lsp_extension_api::MethodVisibility::Public => AnalysisMethodVisibility::Public,
        ruby_fast_lsp_extension_api::MethodVisibility::Protected => {
            AnalysisMethodVisibility::Protected
        }
        ruby_fast_lsp_extension_api::MethodVisibility::Private => AnalysisMethodVisibility::Private,
    }
}

fn analysis_mixin_kind(kind: MixinKind) -> GraphEdgeKind {
    match kind {
        MixinKind::Include => GraphEdgeKind::Include,
        MixinKind::Prepend => GraphEdgeKind::Prepend,
        MixinKind::Extend => GraphEdgeKind::Extend,
    }
}

fn analysis_method_params_from_extension(
    params: &[ruby_fast_lsp_extension_api::MethodParamPatch],
) -> Vec<MethodParamFact> {
    params
        .iter()
        .map(|param| {
            MethodParamFact::new(param.name.clone(), analysis_method_param_kind(param.kind))
        })
        .collect()
}

fn analysis_method_param_kind(
    kind: ruby_fast_lsp_extension_api::MethodParamKind,
) -> AnalysisMethodParamKind {
    match kind {
        ruby_fast_lsp_extension_api::MethodParamKind::Required => AnalysisMethodParamKind::Required,
        ruby_fast_lsp_extension_api::MethodParamKind::Optional => AnalysisMethodParamKind::Optional,
        ruby_fast_lsp_extension_api::MethodParamKind::Rest => AnalysisMethodParamKind::Rest,
        ruby_fast_lsp_extension_api::MethodParamKind::RequiredKeyword => {
            AnalysisMethodParamKind::RequiredKeyword
        }
        ruby_fast_lsp_extension_api::MethodParamKind::OptionalKeyword => {
            AnalysisMethodParamKind::OptionalKeyword
        }
        ruby_fast_lsp_extension_api::MethodParamKind::KeywordRest => {
            AnalysisMethodParamKind::KeywordRest
        }
        ruby_fast_lsp_extension_api::MethodParamKind::Block => AnalysisMethodParamKind::Block,
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
             This is a bug because ruby-analysis::core TextRange currently stores u32 offsets. \
             Fix: widen TextRange offsets before indexing files larger than u32::MAX bytes."
        )
    })
}
