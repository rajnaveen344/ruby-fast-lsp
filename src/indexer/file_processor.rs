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
use crate::extensions::ExtensionRegistryHandle;
use crate::server::RubyLanguageServer;
use crate::types::ruby_document::RubyDocument;
use anyhow::Result;
use log::debug;
use ruby_analysis::core::{
    FullyQualifiedName, GraphEdgeFact, GraphEdgeKind, GraphNodeFact, GraphNodeKind, MethodFact,
    MethodParamFact, MethodParamKind as AnalysisMethodParamKind,
    NamespaceKind as AnalysisNamespaceKind, RubyConstant, RubyMethod, SourceKind, SymbolFact,
    SymbolKind as AnalysisSymbolKind, TextRange, UnresolvedGraphEdgeFact,
};
use ruby_analysis::engine::{AnalysisQuery, FileAnalysisFacts};
use ruby_analysis::indexer::fact_collector::FactCollector;
use ruby_analysis::indexer::AnalysisIndexer;
use ruby_fast_lsp_extension_api::{IndexPatch, MixinKind, SourceRange};
use ruby_prism::Visit;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tower_lsp::lsp_types::{Diagnostic, Url};

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
            });
        }

        // 1. Parse ONLY ONCE
        let parse_result = ruby_prism::parse(content.as_bytes());
        let node = parse_result.node();
        let source_kind = self.analysis_source_kind_for_uri(server, uri);
        let analysis_file_id =
            server.open_or_update_analysis_file_with_kind(uri, content.to_string(), source_kind);
        let document = RubyDocument::with_analysis_file_id(
            uri.clone(),
            content.to_string(),
            0,
            analysis_file_id,
        );

        // 2. Generate Syntax Diagnostics
        let diagnostics = generate_diagnostics(&parse_result, &document);

        // If severe parse errors, skip indexing
        if parse_result.errors().count() > 10 {
            server
                .analysis_engine
                .lock()
                .replace_file_analysis(analysis_file_id, FileAnalysisFacts::default());
            return Ok(ProcessResult {
                affected_uris: HashSet::new(),
                diagnostics,
            });
        }

        let affected_uris = HashSet::new();

        // 3. Collect facts.
        let direct_facts_seed = collect_direct_facts(server, content, document.analysis_file_id());
        replace_analysis_facts_for_file(server, document.analysis_file_id(), &direct_facts_seed);

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
        server.analysis_engine.lock().replace_file_analysis(
            updated_document.analysis_file_id(),
            FileAnalysisFacts {
                symbols: symbol_facts,
                methods: method_facts,
                types: type_facts,
                graph_nodes: direct_facts.graph_nodes,
                graph_edges: direct_facts.graph_edges,
                unresolved_graph_edges: direct_facts.unresolved_graph_edges,
                reference_candidates: visitor.reference_candidates,
                diagnostic_candidates: visitor.diagnostic_candidates,
                diagnostics: visitor.analysis_diagnostics,
            },
        );

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
        let start = Instant::now();
        debug!("Collecting facts for: {:?}", uri);

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

        let mut fact_collector = FactCollector::analysis_only(
            document.clone(),
            Arc::new(self.extension_registry.clone()),
            server.analysis_engine.clone(),
        );
        fact_collector.visit(&node);

        let mut direct_facts = direct_facts_seed;
        add_extension_analysis_facts(
            server,
            &document,
            &fact_collector.extension_index_patches,
            &mut direct_facts,
        );
        server.analysis_engine.lock().replace_file_analysis(
            analysis_file_id,
            FileAnalysisFacts {
                symbols: direct_facts.symbols,
                methods: direct_facts.methods,
                types: direct_facts.types,
                graph_nodes: direct_facts.graph_nodes,
                graph_edges: direct_facts.graph_edges,
                unresolved_graph_edges: direct_facts.unresolved_graph_edges,
                reference_candidates: fact_collector.reference_candidates,
                diagnostic_candidates: fact_collector.diagnostic_candidates,
                diagnostics: fact_collector.analysis_diagnostics,
            },
        );
        debug!("Collected facts for {:?} in {:?}", uri, start.elapsed());
        Ok(())
    }

    fn analysis_source_kind_for_uri(&self, server: &RubyLanguageServer, uri: &Url) -> SourceKind {
        let path = uri
            .to_file_path()
            .unwrap_or_else(|_| PathBuf::from(uri.to_string()));
        let engine = server.analysis_engine.lock();
        engine
            .file_id(&path)
            .and_then(|file_id| engine.file(file_id))
            .map(|file| file.kind)
            .unwrap_or(SourceKind::Project)
    }
}

fn collect_direct_facts(
    server: &RubyLanguageServer,
    content: &str,
    file_id: ruby_analysis::core::SourceFileId,
) -> ruby_analysis::indexer::AnalysisIndex {
    AnalysisIndexer::with_known_namespaces(file_id, collect_known_namespaces(server))
        .index_source(content)
}

fn replace_analysis_facts_for_file(
    server: &RubyLanguageServer,
    file_id: ruby_analysis::core::SourceFileId,
    facts: &ruby_analysis::indexer::AnalysisIndex,
) {
    server
        .analysis_engine
        .lock()
        .replace_file_analysis(file_id, file_analysis_facts_from_index(facts));
}

fn file_analysis_facts_from_index(
    facts: &ruby_analysis::indexer::AnalysisIndex,
) -> FileAnalysisFacts {
    FileAnalysisFacts {
        symbols: facts.symbols.clone(),
        methods: facts.methods.clone(),
        types: facts.types.clone(),
        graph_nodes: facts.graph_nodes.clone(),
        graph_edges: facts.graph_edges.clone(),
        unresolved_graph_edges: facts.unresolved_graph_edges.clone(),
        reference_candidates: Vec::new(),
        diagnostic_candidates: Vec::new(),
        diagnostics: Vec::new(),
    }
}

fn collect_known_namespaces(server: &RubyLanguageServer) -> HashSet<FullyQualifiedName> {
    let engine = server.analysis_engine.lock();
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
        let engine = server.analysis_engine.lock();
        AnalysisQuery::new(&engine).known_namespace_fqns()
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
                facts.methods.push(MethodFact::with_param_facts(
                    fqn,
                    owner,
                    range,
                    analysis_method_params_from_extension(&method.params),
                ));
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
    facts: &mut ruby_analysis::indexer::AnalysisIndex,
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
             This is a bug because ruby-analysis-core TextRange currently stores u32 offsets. \
             Fix: widen TextRange offsets before indexing files larger than u32::MAX bytes."
        )
    })
}
