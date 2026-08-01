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
use crate::extensions::{
    analysis_ruby_type_from_extension, ExtensionApplicabilitySnapshot, ExtensionRegistryHandle,
    ProjectContextSeed,
};
use crate::runtime::jruby::imports::{
    JrubyImportProvider, StaticJavaNavigationPlan, StaticJavaSourceHint,
};
use crate::runtime::jruby::source_navigation::java_source_navigation_facts_with_declaration;
use crate::server::RubyLanguageServer;
use anyhow::{anyhow, Context, Result};
use log::{debug, info, warn};
use ruby_analysis::core::{
    FullyQualifiedName, GeneratedOwnerId, GraphEdgeFact, GraphEdgeKind, GraphNodeFact,
    GraphNodeKind, MethodFact, MethodParamFact, MethodParamKind as AnalysisMethodParamKind,
    NamespaceKind as AnalysisNamespaceKind, RubyConstant, RubyMethod, RubyType, SourceKind,
    SymbolFact, SymbolKind as AnalysisSymbolKind, TextRange, TypeFact, TypeProvenance, TypeSubject,
    UnresolvedGraphEdgeFact,
};
use ruby_analysis::engine::{
    AnalysisEngine, AnalysisQuery, FileFacts, ProjectNeutralFileFactsTemplate, ResolveMode,
    SemanticChange, SourceFileInput,
};
use ruby_analysis::indexer::fact_collector::{FactCollector, FactCollectorExtensionHost};
use ruby_analysis::indexer::RubyDocument;
use ruby_analysis::indexer::{is_erb_path, mask_erb, AnalysisIndexer};
use ruby_analysis::method_store::MethodVisibility as AnalysisMethodVisibility;
use ruby_fast_lsp_extension_api::{
    IndexPatch, MixinKind, NamespaceDeclarationKind, ProjectContext, ResolvedCall, SourceRange,
};
use ruby_prism::{CallNode, Visit};
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};
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

struct CollectedFileFactsOutput {
    project_neutral_template: Option<ProjectNeutralFileFactsTemplate>,
    retained_file_facts: Option<FileFacts>,
    jruby_navigation_plan: StaticJavaNavigationPlan,
    jruby_source_hint: StaticJavaSourceHint,
    timing: ProjectFileCollectionTiming,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ProjectFileCollectionTiming {
    pub(crate) total: Duration,
    pub(crate) registration: Duration,
    pub(crate) parse: Duration,
    pub(crate) jruby_plan: Duration,
    pub(crate) semantic_seed: Duration,
    pub(crate) visitor: Duration,
    pub(crate) assembly: Duration,
    pub(crate) replacement: Duration,
}

pub(crate) struct CollectedProjectFileFacts {
    pub file_facts: FileFacts,
    pub jruby_navigation_plan: StaticJavaNavigationPlan,
    pub jruby_source_hint: StaticJavaSourceHint,
    pub timing: ProjectFileCollectionTiming,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileResolution {
    Full,
    CurrentFile,
    Deferred,
}

enum JrubyNavigationResolution {
    Immediate,
    Deferred {
        known_namespaces: Arc<HashSet<FullyQualifiedName>>,
    },
}

/// Server-owned composition of built-in runtime semantics and public Wasm
/// extensions. Runtime providers may add ordinary facts, while the extension
/// registry remains the sole owner of extension frame tracking and resolved
/// call payloads.
#[derive(Debug)]
struct ProjectFactCollectorHost {
    extension_registry: ExtensionRegistryHandle,
    extension_applicability: OnceLock<ExtensionApplicabilitySnapshot>,
    jruby_import_provider: Option<Arc<JrubyImportProvider>>,
    extensions_enabled: bool,
}

impl ProjectFactCollectorHost {
    fn extension_applicability(
        &self,
        project: Option<&ProjectContext>,
    ) -> &ExtensionApplicabilitySnapshot {
        assert!(
            self.extensions_enabled,
            "INVARIANT VIOLATED: disabled extension traversal requested an applicability snapshot. This is a bug because non-project sources must not execute extension hooks. Fix: keep all snapshot access behind the extensions_enabled gate."
        );
        self.extension_applicability
            .get_or_init(|| self.extension_registry.applicability_snapshot(project))
    }
}

impl FactCollectorExtensionHost for ProjectFactCollectorHost {
    fn process_call_node(&self, visitor: &mut FactCollector, node: &CallNode<'_>) {
        if let Some(provider) = &self.jruby_import_provider {
            provider.process_call_node(visitor, node);
        }
        if self.extensions_enabled && self.extension_registry.tracks_call(node) {
            self.extension_registry
                .process_call_node_with_applicability(
                    visitor,
                    node,
                    self.extension_applicability(visitor.extension_project_context.as_ref()),
                );
        }
    }

    fn should_track_enclosing_call(&self, visitor: &FactCollector, node: &CallNode<'_>) -> bool {
        self.extensions_enabled
            && self.extension_registry.tracks_call(node)
            && self
                .extension_registry
                .should_track_enclosing_call_with_applicability(
                    visitor,
                    node,
                    self.extension_applicability(visitor.extension_project_context.as_ref()),
                )
    }

    fn resolved_call_for_stack(
        &self,
        visitor: &FactCollector,
        node: &CallNode<'_>,
    ) -> ResolvedCall {
        assert!(
            self.extensions_enabled,
            "INVARIANT VIOLATED: an extension call frame was resolved for a source kind that disables extensions. This is a bug because disabled extension hosts must reject frame tracking before payload construction. Fix: keep should_track_enclosing_call gated by extensions_enabled."
        );
        self.extension_registry
            .resolved_call_for_stack_with_applicability(
                visitor,
                node,
                self.extension_applicability(visitor.extension_project_context.as_ref()),
            )
    }
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

fn extend_unique<T: PartialEq>(target: &mut Vec<T>, source: Vec<T>) {
    for value in source {
        if !target.contains(&value) {
            target.push(value);
        }
    }
}

// ============================================================================
// FileProcessor
// ============================================================================

/// File processor for handling parsing, indexing, and diagnostic generation
#[derive(Debug, Clone)]
pub struct FileProcessor {
    extension_registry: ExtensionRegistryHandle,
    extension_project_context_seed: Option<Arc<parking_lot::RwLock<ProjectContextSeed>>>,
    jruby_import_provider: Option<Arc<JrubyImportProvider>>,
}

impl FileProcessor {
    pub fn new() -> Self {
        Self {
            extension_registry: ExtensionRegistryHandle::from_environment(),
            extension_project_context_seed: None,
            jruby_import_provider: None,
        }
    }

    pub fn with_extension_registry(extension_registry: ExtensionRegistryHandle) -> Self {
        Self {
            extension_registry,
            extension_project_context_seed: None,
            jruby_import_provider: None,
        }
    }

    pub(crate) fn with_extension_project_context_seed(
        mut self,
        seed: Arc<parking_lot::RwLock<ProjectContextSeed>>,
    ) -> Self {
        self.extension_project_context_seed = Some(seed);
        self
    }

    pub(crate) fn with_jruby_import_provider(mut self, provider: Arc<JrubyImportProvider>) -> Self {
        self.jruby_import_provider = Some(provider);
        self
    }

    pub(crate) fn jruby_import_provider(&self) -> Option<&Arc<JrubyImportProvider>> {
        self.jruby_import_provider.as_ref()
    }

    fn fact_collector_host(&self, extensions_enabled: bool) -> Arc<dyn FactCollectorExtensionHost> {
        Arc::new(ProjectFactCollectorHost {
            extension_registry: self.extension_registry.clone(),
            extension_applicability: OnceLock::new(),
            jruby_import_provider: self.jruby_import_provider.clone(),
            extensions_enabled,
        })
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

        let total_start = Instant::now();
        let parse_start = Instant::now();
        // 1. Parse ONLY ONCE
        let analysis_source = analysis_source(uri, content);
        let analysis_engine = server.analysis_engine_for_uri(uri);
        self.ensure_jruby_navigation_inputs(content, &analysis_engine)?;
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
        let previous_export_fingerprint = analysis_engine
            .read()
            .semantic_export_fingerprint(analysis_file_id);

        // 2. Generate Syntax Diagnostics
        let diagnostics = generate_diagnostics(&parse_result, &document);
        let parse_elapsed = parse_start.elapsed();

        // If severe parse errors, skip indexing
        if parse_result.errors().count() > 10 {
            let semantic_change = replace_file_analysis(
                &analysis_engine,
                analysis_file_id,
                FileFacts::default(),
                resolution,
            );
            return Ok(ProcessResult {
                affected_uris: HashSet::new(),
                diagnostics,
                semantic_change,
            });
        }

        let affected_uris = HashSet::new();

        // 3. Collect facts.
        let direct_start = Instant::now();
        let direct_facts_seed = collect_direct_facts(
            &analysis_engine,
            &node,
            analysis_source.as_ref(),
            document.analysis_file_id(),
            None,
        );
        replace_analysis_facts_for_file(
            &analysis_engine,
            document.analysis_file_id(),
            &direct_facts_seed,
            false,
        );
        let extensions_enabled = matches!(source_kind, SourceKind::Project | SourceKind::Excluded);
        let extension_project_context_snapshot = extensions_enabled
            .then(|| server.extension_project_context_snapshot_for_uri(uri, source_kind))
            .flatten();
        let extension_project_context = extension_project_context_snapshot
            .as_ref()
            .map(|snapshot| snapshot.context.clone());
        if extensions_enabled {
            if let Some(snapshot) = extension_project_context_snapshot.as_ref() {
                self.extension_registry
                    .ensure_semantic_seed_facts_for_snapshot(&analysis_engine, snapshot);
            } else {
                self.extension_registry.ensure_semantic_seed_facts(
                    &analysis_engine,
                    extension_project_context.as_ref(),
                );
            }
        }
        let direct_elapsed = direct_start.elapsed();

        let visitor_start = Instant::now();
        let mut visitor = FactCollector::analysis_only(
            document.clone(),
            self.fact_collector_host(extensions_enabled),
            analysis_engine.clone(),
        );
        visitor.extension_project_context = extension_project_context.clone();
        visitor.visit(&node);
        let visitor_elapsed = visitor_start.elapsed();

        let extension_index_patches = visitor.extension_index_patches.clone();
        let updated_document = visitor.document.clone();
        let mut direct_facts = direct_facts_seed;
        merge_execution_context_direct_facts(&visitor.direct_facts, &mut direct_facts);
        merge_runtime_direct_facts(&visitor.direct_facts, &mut direct_facts);
        add_extension_analysis_facts(
            &analysis_engine,
            &updated_document,
            &extension_index_patches,
            extension_project_context.as_ref(),
            &mut direct_facts,
        );
        let symbol_facts = direct_facts.symbols;
        let method_facts = direct_facts.methods;
        let mut type_facts = direct_facts.types;
        let visitor_type_facts = visitor.type_store.all_facts();
        rehome_execution_context_type_facts(&visitor_type_facts, &mut type_facts);
        merge_precise_visitor_type_facts(visitor_type_facts, &mut type_facts);
        let replace_start = Instant::now();
        replace_file_analysis(
            &analysis_engine,
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
                execution_contexts: visitor.extension_execution_context_facts,
            },
            resolution,
        );
        let replace_elapsed = replace_start.elapsed();
        let current_export_fingerprint = analysis_engine
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
        info!(
            "[PERF][file_processor] file={} total={:?} parse={:?} direct={:?} visitor={:?} replace_resolve={:?}",
            uri.path(),
            total_start.elapsed(),
            parse_elapsed,
            direct_elapsed,
            visitor_elapsed,
            replace_elapsed
        );

        Ok(ProcessResult {
            affected_uris,
            diagnostics,
            semantic_change,
        })
    }

    fn ensure_jruby_navigation_inputs(
        &self,
        content: &str,
        analysis_engine: &Arc<parking_lot::RwLock<AnalysisEngine>>,
    ) -> Result<()> {
        let Some(provider) = &self.jruby_import_provider else {
            return Ok(());
        };
        let plan = provider
            .static_navigation_plan(content)
            .map_err(|message| {
                anyhow!("failed to resolve static JRuby Java dependencies: {message}")
            })?;
        self.materialize_jruby_navigation_plan(
            plan,
            analysis_engine,
            JrubyNavigationResolution::Immediate,
        )
    }

    pub(crate) fn materialize_jruby_navigation_plan_as_deferred_resolution(
        &self,
        plan: StaticJavaNavigationPlan,
        analysis_engine: &Arc<parking_lot::RwLock<AnalysisEngine>>,
        known_namespaces: Arc<HashSet<FullyQualifiedName>>,
    ) -> Result<()> {
        self.materialize_jruby_navigation_plan(
            plan,
            analysis_engine,
            JrubyNavigationResolution::Deferred { known_namespaces },
        )
    }

    fn materialize_jruby_navigation_plan(
        &self,
        plan: StaticJavaNavigationPlan,
        analysis_engine: &Arc<parking_lot::RwLock<AnalysisEngine>>,
        resolution: JrubyNavigationResolution,
    ) -> Result<()> {
        if plan.signature_class_names.is_empty() {
            return Ok(());
        }
        let provider = self.jruby_import_provider.as_ref().expect(
            "INVARIANT VIOLATED: a JRuby navigation plan was materialized without an owning \
             JRuby provider. This is a bug because plans are derived from one exact project \
             classpath catalog. Fix: keep plan collection and materialization on the same \
             project FileProcessor.",
        );
        let cache_root = provider.signature_cache_root().ok_or_else(|| {
            anyhow!(
                "JRuby provider for classpath {} has no isolated signature cache root",
                provider.classpath_fingerprint()
            )
        })?;
        let signature_class_names = plan
            .signature_class_names
            .into_iter()
            .collect::<BTreeSet<_>>();
        let implementation_class_names = plan
            .implementation_class_names
            .into_iter()
            .collect::<BTreeSet<_>>();
        let (deferred_signature_known_namespaces, file_resolution) = match resolution {
            JrubyNavigationResolution::Immediate => (None, FileResolution::Full),
            JrubyNavigationResolution::Deferred { known_namespaces } => {
                (Some(known_namespaces), FileResolution::Deferred)
            }
        };
        std::fs::create_dir_all(cache_root).with_context(|| {
            format!(
                "failed to create isolated JRuby signature cache {}",
                cache_root.display()
            )
        })?;

        let mut exact_sources = BTreeMap::<
            PathBuf,
            (
                String,
                Vec<(
                    String,
                    ruby_fast_lsp_jvm_metadata::JavaSourceClassLocation,
                    bool,
                )>,
            ),
        >::new();
        let mut signature_generation_wall = Duration::default();
        let mut signature_cache_io_wall = Duration::default();
        let mut signature_index_wall = Duration::default();
        let mut implementation_resolution_wall = Duration::default();
        let mut generated_signatures = 0usize;
        let mut indexed_signatures = 0usize;
        for class_name in signature_class_names {
            let signature_generation_started = Instant::now();
            let Some((internal_name, signature)) =
                provider.generated_signature(&class_name).map_err(|error| {
                    anyhow!("failed to generate signature for Java class `{class_name}`: {error:?}")
                })?
            else {
                signature_generation_wall += signature_generation_started.elapsed();
                continue;
            };
            signature_generation_wall += signature_generation_started.elapsed();
            generated_signatures += 1;
            let signature_cache_io_started = Instant::now();
            let signature_path = cache_root.join(format!("{internal_name}.rb"));
            let signature_parent = signature_path.parent().expect(
                "INVARIANT VIOLATED: generated JRuby signature path has no parent. \
                 This is a bug because validated JVM names always produce a cache-relative path. \
                 Fix: retain the isolated cache root and validated internal class name.",
            );
            std::fs::create_dir_all(signature_parent).with_context(|| {
                format!(
                    "failed to create JRuby signature directory {}",
                    signature_parent.display()
                )
            })?;
            if !std::fs::read_to_string(&signature_path).is_ok_and(|existing| existing == signature)
            {
                std::fs::write(&signature_path, &signature).with_context(|| {
                    format!(
                        "failed to materialize JRuby signature {}",
                        signature_path.display()
                    )
                })?;
            }
            signature_cache_io_wall += signature_cache_io_started.elapsed();
            let signature_uri = Url::from_file_path(&signature_path).map_err(|_| {
                anyhow!(
                    "generated JRuby signature is not a valid file URI: {}",
                    signature_path.display()
                )
            })?;
            let signature_already_indexed =
                analysis_engine.read().file_id(&signature_path).is_some();
            if !signature_already_indexed {
                let signature_index_started = Instant::now();
                match &deferred_signature_known_namespaces {
                    Some(known_namespaces) => {
                        self.collect_file_facts_as_deferred_resolution_with_known_namespaces_in_engine(
                            &signature_uri,
                            &signature,
                            analysis_engine.clone(),
                            SourceKind::Signature,
                            known_namespaces.clone(),
                        )?;
                    }
                    None => {
                        self.collect_file_facts_as_with_resolution(
                            &signature_uri,
                            &signature,
                            analysis_engine.clone(),
                            SourceKind::Signature,
                            true,
                            None,
                            false,
                            true,
                        )?;
                    }
                }
                signature_index_wall += signature_index_started.elapsed();
                indexed_signatures += 1;
            }

            if provider.has_registered_navigation_class(&internal_name) {
                continue;
            }
            if !implementation_class_names.contains(&internal_name) {
                continue;
            }
            let implementation_resolution_started = Instant::now();
            let resolved_sources = match provider
                .resolved_navigation_implementations(&internal_name)
            {
                Ok(resolved) => resolved,
                Err(error) => {
                    warn!(
                        "Java implementation source unavailable for {} during JRuby navigation materialization: {:?}; using generated signature fallback",
                        internal_name, error
                    );
                    Vec::new()
                }
            };
            implementation_resolution_wall += implementation_resolution_started.elapsed();
            if resolved_sources.is_empty() {
                continue;
            }
            for (index, resolved) in resolved_sources.into_iter().enumerate() {
                let entry = exact_sources
                    .entry(resolved.path)
                    .or_insert_with(|| (resolved.content.clone(), Vec::new()));
                assert_eq!(
                    entry.0, resolved.content,
                    "INVARIANT VIOLATED: one exact Java source path resolved to different content \
                     during a single classpath pass. This is a bug because the classpath and source \
                     fingerprints are immutable for the pass. Fix: retain one verified source identity \
                     for every materialized path."
                );
                entry
                    .1
                    .push((internal_name.clone(), resolved.location, index == 0));
            }
        }

        let exact_source_insertion_started = Instant::now();
        let exact_source_files = exact_sources.len();
        for (path, (content, mut classes)) in exact_sources {
            classes.sort_by(|left, right| {
                left.0
                    .cmp(&right.0)
                    .then_with(|| left.1.internal_name.cmp(&right.1.internal_name))
            });
            classes.dedup_by(|left, right| left.0 == right.0);
            let (file_id, mut facts) = {
                let mut engine = analysis_engine.write();
                let file_id = engine.register_file(SourceFileInput {
                    path,
                    content,
                    kind: SourceKind::External,
                });
                let query = AnalysisQuery::new(&engine);
                (
                    file_id,
                    FileFacts {
                        symbols: query.symbol_facts_in_file(file_id),
                        methods: query.method_facts_in_file(file_id),
                        method_visibility_overrides: query
                            .method_visibility_overrides_in_file(file_id),
                        types: query.type_facts_in_file(file_id),
                        graph_nodes: query.graph_nodes_in_file(file_id),
                        graph_edges: query.graph_edges_in_file(file_id),
                        diagnostics: query.diagnostic_facts_in_file(file_id),
                        ..FileFacts::default()
                    },
                )
            };
            for (internal_name, location, include_class_declaration) in classes {
                let declaration = provider.class_declaration(&internal_name).expect(
                    "INVARIANT VIOLATED: exact Java implementation resolved for a class absent \
                     from its owning catalog. This is a bug because resolution starts from that exact \
                     catalog declaration. Fix: keep provider catalog and resolver transactionally paired.",
                );
                provider.register_method_navigation_ranges(&internal_name, &location, file_id);
                let new_facts = java_source_navigation_facts_with_declaration(
                    &declaration.class,
                    &location,
                    file_id,
                    include_class_declaration,
                );
                extend_unique(&mut facts.symbols, new_facts.symbols);
                extend_unique(&mut facts.methods, new_facts.methods);
                extend_unique(&mut facts.types, new_facts.types);
            }
            replace_file_analysis(analysis_engine, file_id, facts, file_resolution);
        }
        info!(
            "[PERF][JRuby navigation materialization] classpath={} generated_signatures={} \
             indexed_signatures={} exact_source_files={} signature_generation={:?} \
             signature_cache_io={:?} signature_index={:?} implementation_resolution={:?} \
             exact_source_insertion={:?}",
            provider.classpath_fingerprint(),
            generated_signatures,
            indexed_signatures,
            exact_source_files,
            signature_generation_wall,
            signature_cache_io_wall,
            signature_index_wall,
            implementation_resolution_wall,
            exact_source_insertion_started.elapsed()
        );
        Ok(())
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
        let analysis_engine = server.analysis_engine_for_uri(uri);
        self.collect_file_facts_as_with_resolution(
            uri,
            content,
            analysis_engine,
            source_kind,
            true,
            None,
            false,
            true,
        )?;
        Ok(())
    }

    pub fn collect_file_facts_as_deferred_resolution(
        &self,
        uri: &Url,
        content: &str,
        server: &RubyLanguageServer,
        source_kind: SourceKind,
    ) -> Result<()> {
        let analysis_engine = server.analysis_engine_for_uri(uri);
        self.collect_file_facts_as_with_resolution(
            uri,
            content,
            analysis_engine,
            source_kind,
            false,
            None,
            false,
            true,
        )?;
        Ok(())
    }

    pub fn collect_file_facts_as_deferred_resolution_in_engine(
        &self,
        uri: &Url,
        content: &str,
        analysis_engine: Arc<parking_lot::RwLock<AnalysisEngine>>,
        source_kind: SourceKind,
    ) -> Result<()> {
        self.collect_file_facts_as_with_resolution(
            uri,
            content,
            analysis_engine,
            source_kind,
            false,
            None,
            false,
            true,
        )?;
        Ok(())
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
        let analysis_engine = server.analysis_engine_for_uri(uri);
        let analysis_file_id = server.open_or_update_analysis_file_with_kind(
            uri,
            content.to_string(),
            SourceKind::Signature,
        );
        let facts = match ruby_analysis::indexer::index_rbs(analysis_file_id, content) {
            Ok(facts) => facts,
            Err(error) => {
                replace_file_analysis(
                    &analysis_engine,
                    analysis_file_id,
                    FileFacts::default(),
                    resolution,
                );
                return Err(anyhow::anyhow!(
                    "Failed to parse RBS {}: {error}",
                    uri.path()
                ));
            }
        };
        replace_file_analysis(
            &analysis_engine,
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
        known_namespaces: Arc<HashSet<FullyQualifiedName>>,
    ) -> Result<()> {
        self.collect_file_facts_as_with_resolution(
            uri,
            content,
            server.analysis_engine_for_uri(uri),
            source_kind,
            false,
            Some(known_namespaces),
            false,
            true,
        )?;
        Ok(())
    }

    pub fn collect_file_facts_as_deferred_resolution_with_known_namespaces_in_engine(
        &self,
        uri: &Url,
        content: &str,
        analysis_engine: Arc<parking_lot::RwLock<AnalysisEngine>>,
        source_kind: SourceKind,
        known_namespaces: Arc<HashSet<FullyQualifiedName>>,
    ) -> Result<()> {
        self.collect_file_facts_as_with_resolution(
            uri,
            content,
            analysis_engine,
            source_kind,
            false,
            Some(known_namespaces),
            false,
            true,
        )?;
        Ok(())
    }

    pub(crate) fn collect_project_file_facts_and_jruby_navigation_plan_as_deferred_resolution(
        &self,
        uri: &Url,
        content: String,
        analysis_engine: Arc<parking_lot::RwLock<AnalysisEngine>>,
        known_namespaces: Arc<HashSet<FullyQualifiedName>>,
    ) -> Result<CollectedProjectFileFacts> {
        let output = self.collect_file_facts_as_with_resolution_output_owned(
            uri,
            content,
            analysis_engine,
            SourceKind::Project,
            false,
            Some(known_namespaces),
            false,
            false,
            true,
            true,
        )?;
        Ok(CollectedProjectFileFacts {
            file_facts: output.retained_file_facts.expect(
                "INVARIANT VIOLATED: project batch collection did not retain its file-owned facts. This is a bug because deterministic batch insertion requires every worker to return facts without mutating the shared engine. Fix: keep retained_file_facts enabled for the project batch path.",
            ),
            jruby_navigation_plan: output.jruby_navigation_plan,
            jruby_source_hint: output.jruby_source_hint,
            timing: output.timing,
        })
    }

    pub(crate) fn ensure_project_semantic_seed(
        &self,
        uri: &Url,
        analysis_engine: &Arc<parking_lot::RwLock<AnalysisEngine>>,
    ) {
        let project_context_snapshot = self.extension_project_context_seed.as_ref().map(|seed| {
            seed.read()
                .context_snapshot(uri.to_string(), SourceKind::Project)
        });
        if let Some(snapshot) = project_context_snapshot.as_ref() {
            self.extension_registry
                .ensure_semantic_seed_facts_for_snapshot(analysis_engine, snapshot);
        } else {
            self.extension_registry
                .ensure_semantic_seed_facts(analysis_engine, None);
        }
    }

    pub(crate) fn replace_collected_project_file_facts_as_deferred_resolution(
        &self,
        path: &Path,
        analysis_engine: &Arc<parking_lot::RwLock<AnalysisEngine>>,
        facts: FileFacts,
    ) {
        let file_id = analysis_engine.read().file_id(path).unwrap_or_else(|| {
            panic!(
                "INVARIANT VIOLATED: deterministic project fact replacement received an unregistered source {}. This is a bug because the bounded batch must be registered before semantic collection. Fix: preserve the pre-registration and ordered replacement lifecycle.",
                path.display()
            )
        });
        replace_file_analysis(analysis_engine, file_id, facts, FileResolution::Deferred);
    }

    pub fn collect_project_neutral_file_template_as_deferred_resolution_in_engine(
        &self,
        uri: &Url,
        content: &str,
        analysis_engine: Arc<parking_lot::RwLock<AnalysisEngine>>,
        source_kind: SourceKind,
        known_namespaces: Arc<HashSet<FullyQualifiedName>>,
    ) -> Result<ProjectNeutralFileFactsTemplate> {
        assert!(
            source_kind.is_external(),
            "INVARIANT VIOLATED: a project-neutral dependency template was requested for a project-owned source kind. This is a bug because project facts may contain project-specific references, diagnostics, and extension execution contexts. Fix: request templates only for validated external dependency source kinds."
        );
        self.collect_file_facts_as_with_resolution(
            uri,
            content,
            analysis_engine,
            source_kind,
            false,
            Some(known_namespaces),
            true,
            true,
        )?
        .ok_or_else(|| {
            anyhow!(
                "project-neutral template capture unexpectedly produced no template for {}",
                uri
            )
        })
    }

    pub fn collect_project_neutral_file_template_without_insertion(
        &self,
        uri: &Url,
        content: &str,
        analysis_engine: Arc<parking_lot::RwLock<AnalysisEngine>>,
        source_kind: SourceKind,
        known_namespaces: Arc<HashSet<FullyQualifiedName>>,
    ) -> Result<ProjectNeutralFileFactsTemplate> {
        assert!(
            source_kind.is_external(),
            "INVARIANT VIOLATED: a project-neutral dependency template was requested for a project-owned source kind. This is a bug because project facts may contain project-specific references, diagnostics, and extension execution contexts. Fix: request templates only for validated external dependency source kinds."
        );
        self.collect_file_facts_as_with_resolution(
            uri,
            content,
            analysis_engine,
            source_kind,
            false,
            Some(known_namespaces),
            true,
            false,
        )?
        .ok_or_else(|| {
            anyhow!(
                "project-neutral template capture unexpectedly produced no template for {}",
                uri
            )
        })
    }

    fn collect_file_facts_as_with_resolution(
        &self,
        uri: &Url,
        content: &str,
        analysis_engine: Arc<parking_lot::RwLock<AnalysisEngine>>,
        source_kind: SourceKind,
        resolve_references: bool,
        known_namespaces: Option<Arc<HashSet<FullyQualifiedName>>>,
        capture_project_neutral_template: bool,
        insert_collected_facts: bool,
    ) -> Result<Option<ProjectNeutralFileFactsTemplate>> {
        Ok(self
            .collect_file_facts_as_with_resolution_output(
                uri,
                content,
                analysis_engine,
                source_kind,
                resolve_references,
                known_namespaces,
                capture_project_neutral_template,
                insert_collected_facts,
                false,
            )?
            .project_neutral_template)
    }

    fn collect_file_facts_as_with_resolution_output(
        &self,
        uri: &Url,
        content: &str,
        analysis_engine: Arc<parking_lot::RwLock<AnalysisEngine>>,
        source_kind: SourceKind,
        resolve_references: bool,
        known_namespaces: Option<Arc<HashSet<FullyQualifiedName>>>,
        capture_project_neutral_template: bool,
        insert_collected_facts: bool,
        collect_jruby_navigation_plan: bool,
    ) -> Result<CollectedFileFactsOutput> {
        self.collect_file_facts_as_with_resolution_output_owned(
            uri,
            content.to_string(),
            analysis_engine,
            source_kind,
            resolve_references,
            known_namespaces,
            capture_project_neutral_template,
            insert_collected_facts,
            collect_jruby_navigation_plan,
            false,
        )
    }

    fn collect_file_facts_as_with_resolution_output_owned(
        &self,
        uri: &Url,
        content: String,
        analysis_engine: Arc<parking_lot::RwLock<AnalysisEngine>>,
        source_kind: SourceKind,
        resolve_references: bool,
        known_namespaces: Option<Arc<HashSet<FullyQualifiedName>>>,
        capture_project_neutral_template: bool,
        insert_collected_facts: bool,
        collect_jruby_navigation_plan: bool,
        retain_collected_facts: bool,
    ) -> Result<CollectedFileFactsOutput> {
        let collection_started = Instant::now();
        assert!(
            insert_collected_facts
                || retain_collected_facts
                || (capture_project_neutral_template && !resolve_references),
            "INVARIANT VIOLATED: FileProcessor skipped engine insertion without retaining file-owned facts or capturing a deferred project-neutral template. This is a bug because ordinary indexing must use the engine replacement lifecycle. Fix: use insertion for normal sources, retained facts for deterministic project batches, or the explicit dependency-template collection path."
        );
        assert!(
            !retain_collected_facts || (!insert_collected_facts && !capture_project_neutral_template),
            "INVARIANT VIOLATED: retained file facts were combined with insertion or project-neutral capture. This is a bug because one collection result must have exactly one owner. Fix: retain facts only for the deterministic project batch path."
        );
        debug!("Collecting facts for: {:?}", uri);

        let path = uri
            .to_file_path()
            .unwrap_or_else(|_| PathBuf::from(uri.to_string()));
        let registration_started = Instant::now();
        let analysis_file_id = if retain_collected_facts {
            analysis_engine.read().file_id(&path).unwrap_or_else(|| {
                    panic!(
                        "INVARIANT VIOLATED: deterministic project batch collection received an unregistered source {}. This is a bug because every batch file must be pre-registered before parallel semantic reads begin. Fix: register the complete bounded batch in path order before collecting facts.",
                        path.display()
                    )
                })
        } else {
            let mut engine = analysis_engine.write();
            if !insert_collected_facts {
                if let Some(file_id) = engine.file_id(&path) {
                    file_id
                } else {
                    engine.register_file(ruby_analysis::engine::SourceFileInput {
                        path,
                        content: String::new(),
                        kind: source_kind,
                    })
                }
            } else {
                engine.register_file_borrowed(path, &content, source_kind)
            }
        };
        let document =
            RubyDocument::with_analysis_file_id(uri.clone(), content, 0, analysis_file_id);
        let registration_elapsed = registration_started.elapsed();

        let parse_started = Instant::now();
        let analysis_source = analysis_source(uri, &document.content);
        let parse_result = ruby_prism::parse(analysis_source.as_bytes());
        let node = parse_result.node();
        let parse_elapsed = parse_started.elapsed();
        let jruby_plan_started = Instant::now();
        let jruby_source_hint = collect_jruby_navigation_plan
            .then(|| StaticJavaSourceHint::from_source(analysis_source.as_ref()))
            .unwrap_or_default();
        let jruby_navigation_plan = if collect_jruby_navigation_plan {
            self.jruby_import_provider
                .as_ref()
                .filter(|provider| {
                    provider.source_hint_may_reference_static_java(&jruby_source_hint)
                })
                .map(|provider| {
                    provider
                        .static_navigation_plan_for_node(&node)
                        .map_err(|message| {
                            anyhow!(
                                "failed to plan static JRuby navigation for {}: {message}",
                                uri
                            )
                        })
                })
                .transpose()?
                .unwrap_or_default()
        } else {
            StaticJavaNavigationPlan::default()
        };
        let jruby_plan_elapsed = jruby_plan_started.elapsed();

        let semantic_seed_started = Instant::now();
        let direct_facts_seed = if resolve_references {
            collect_direct_facts(
                &analysis_engine,
                &node,
                analysis_source.as_ref(),
                analysis_file_id,
                known_namespaces.as_deref(),
            )
        } else {
            ruby_analysis::indexer::AnalysisIndex::default()
        };
        if resolve_references {
            replace_analysis_facts_for_file(
                &analysis_engine,
                analysis_file_id,
                &direct_facts_seed,
                resolve_references,
            );
        }
        let extensions_enabled = matches!(source_kind, SourceKind::Project | SourceKind::Excluded);
        let extension_project_context_snapshot = extensions_enabled
            .then(|| {
                self.extension_project_context_seed
                    .as_ref()
                    .map(|seed| seed.read().context_snapshot(uri.to_string(), source_kind))
            })
            .flatten();
        let extension_project_context = extension_project_context_snapshot
            .as_ref()
            .map(|snapshot| snapshot.context.clone());
        if extensions_enabled {
            if let Some(snapshot) = extension_project_context_snapshot.as_ref() {
                self.extension_registry
                    .ensure_semantic_seed_facts_for_snapshot(&analysis_engine, snapshot);
            } else {
                self.extension_registry.ensure_semantic_seed_facts(
                    &analysis_engine,
                    extension_project_context.as_ref(),
                );
            }
        }

        let mut fact_collector = FactCollector::analysis_only(
            document.clone(),
            self.fact_collector_host(extensions_enabled),
            analysis_engine.clone(),
        );
        fact_collector.extension_project_context = extension_project_context.clone();
        let shared_direct_known_namespaces = known_namespaces
            .unwrap_or_else(|| Arc::new(collect_known_namespaces(&analysis_engine)));
        fact_collector =
            fact_collector.with_shared_direct_known_namespaces(shared_direct_known_namespaces);
        fact_collector.extend_direct_known_namespaces(
            direct_facts_seed
                .graph_nodes
                .iter()
                .map(|fact| fact.fqn.clone()),
        );
        let semantic_seed_elapsed = semantic_seed_started.elapsed();
        let visitor_started = Instant::now();
        fact_collector.visit(&node);
        let visitor_elapsed = visitor_started.elapsed();

        let assembly_started = Instant::now();
        let mut direct_facts = if resolve_references {
            direct_facts_seed
        } else {
            fact_collector.direct_facts.clone()
        };
        if resolve_references {
            merge_execution_context_direct_facts(&fact_collector.direct_facts, &mut direct_facts);
            merge_runtime_direct_facts(&fact_collector.direct_facts, &mut direct_facts);
        }
        add_extension_analysis_facts(
            &analysis_engine,
            &document,
            &fact_collector.extension_index_patches,
            extension_project_context.as_ref(),
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
        let file_facts = FileFacts {
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
            execution_contexts: fact_collector.extension_execution_context_facts,
        };
        let assembly_elapsed = assembly_started.elapsed();
        let replacement_started = Instant::now();
        let template = if capture_project_neutral_template {
            Some(
                ProjectNeutralFileFactsTemplate::try_new(analysis_file_id, file_facts.clone())
                    .with_context(|| {
                        format!(
                            "facts for {} are not safe for project-neutral dependency reuse",
                            uri
                        )
                    })?,
            )
        } else {
            None
        };
        let retained_file_facts = if retain_collected_facts {
            Some(file_facts)
        } else {
            if insert_collected_facts {
                replace_file_analysis(
                    &analysis_engine,
                    analysis_file_id,
                    file_facts,
                    if resolve_references {
                        FileResolution::Full
                    } else {
                        FileResolution::Deferred
                    },
                );
            }
            None
        };
        let replacement_elapsed = replacement_started.elapsed();
        debug!("Collected facts for {:?}", uri);
        Ok(CollectedFileFactsOutput {
            project_neutral_template: template,
            retained_file_facts,
            jruby_navigation_plan,
            jruby_source_hint,
            timing: ProjectFileCollectionTiming {
                total: collection_started.elapsed(),
                registration: registration_elapsed,
                parse: parse_elapsed,
                jruby_plan: jruby_plan_elapsed,
                semantic_seed: semantic_seed_elapsed,
                visitor: visitor_elapsed,
                assembly: assembly_elapsed,
                replacement: replacement_elapsed,
            },
        })
    }

    fn analysis_source_kind_for_uri(&self, server: &RubyLanguageServer, uri: &Url) -> SourceKind {
        let path = uri
            .to_file_path()
            .unwrap_or_else(|_| PathBuf::from(uri.to_string()));
        let analysis_engine = server.analysis_engine_for_uri(uri);
        let engine = analysis_engine.read();
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
    analysis_engine: &Arc<parking_lot::RwLock<AnalysisEngine>>,
    node: &ruby_prism::Node<'_>,
    content: &str,
    file_id: ruby_analysis::core::SourceFileId,
    known_namespaces: Option<&HashSet<FullyQualifiedName>>,
) -> ruby_analysis::indexer::AnalysisIndex {
    let known_namespaces = known_namespaces
        .cloned()
        .unwrap_or_else(|| collect_known_namespaces(analysis_engine));
    let known_constant_types = collect_known_constant_types(analysis_engine, file_id);
    AnalysisIndexer::with_known_semantics(file_id, known_namespaces, known_constant_types)
        .index_node_with_source(node, content)
}

fn merge_execution_context_direct_facts(
    extension_aware: &ruby_analysis::indexer::AnalysisIndex,
    merged: &mut ruby_analysis::indexer::AnalysisIndex,
) {
    let generated_methods = extension_aware
        .methods
        .iter()
        .filter(|fact| fact.owner.has_generated_owner())
        .cloned()
        .collect::<Vec<_>>();
    for generated in generated_methods {
        merged.methods.retain(|fact| {
            fact.owner.has_generated_owner()
                || fact.range != generated.range
                || fact.fqn.name() != generated.fqn.name()
        });
        if !merged.methods.contains(&generated) {
            merged.methods.push(generated);
        }
    }

    let generated_symbols = extension_aware
        .symbols
        .iter()
        .filter(|fact| fact.kind == AnalysisSymbolKind::Method && fact.fqn.has_generated_owner())
        .cloned()
        .collect::<Vec<_>>();
    for generated in generated_symbols {
        merged.symbols.retain(|fact| {
            fact.fqn.has_generated_owner()
                || fact.kind != AnalysisSymbolKind::Method
                || fact.range != generated.range
                || fact.fqn.name() != generated.fqn.name()
        });
        if !merged.symbols.contains(&generated) {
            merged.symbols.push(generated);
        }
    }

    for generated in extension_aware
        .method_visibility_overrides
        .iter()
        .filter(|fact| fact.owner.has_generated_owner())
    {
        merged.method_visibility_overrides.retain(|fact| {
            fact.owner.has_generated_owner()
                || fact.range != generated.range
                || fact.method != generated.method
        });
        if !merged.method_visibility_overrides.contains(generated) {
            merged.method_visibility_overrides.push(generated.clone());
        }
    }

    for generated in extension_aware
        .graph_nodes
        .iter()
        .filter(|fact| fact.fqn.has_generated_owner())
    {
        if !merged.graph_nodes.contains(generated) {
            merged.graph_nodes.push(generated.clone());
        }
    }
    for generated in extension_aware
        .graph_edges
        .iter()
        .filter(|fact| fact.source.has_generated_owner() || fact.target.has_generated_owner())
    {
        if !merged.graph_edges.contains(generated) {
            merged.graph_edges.push(generated.clone());
        }
    }
}

fn merge_runtime_direct_facts(
    runtime_aware: &ruby_analysis::indexer::AnalysisIndex,
    merged: &mut ruby_analysis::indexer::AnalysisIndex,
) {
    let runtime_types = runtime_aware
        .types
        .iter()
        .filter(|fact| fact.provenance == TypeProvenance::Runtime)
        .cloned()
        .collect::<Vec<_>>();
    let runtime_constants = runtime_types
        .iter()
        .filter_map(|fact| match &fact.subject {
            TypeSubject::Constant(fqn) => Some(fqn.clone()),
            TypeSubject::Local { .. }
            | TypeSubject::InstanceVariable { .. }
            | TypeSubject::ClassVariable { .. }
            | TypeSubject::GlobalVariable(_)
            | TypeSubject::MethodReturn(_)
            | TypeSubject::Parameter { .. }
            | TypeSubject::Expression(_) => None,
        })
        .collect::<HashSet<_>>();
    let shadowed_runtime_reopenings = merged
        .graph_nodes
        .iter()
        .filter(|fact| {
            runtime_constants
                .iter()
                .any(|constant| constant.namespace_parts() == fact.fqn.namespace_parts())
                && !runtime_aware.graph_nodes.iter().any(|runtime_fact| {
                    runtime_fact.fqn == fact.fqn && runtime_fact.range == fact.range
                })
        })
        .map(|fact| (fact.fqn.clone(), fact.range))
        .collect::<Vec<_>>();
    merged.graph_nodes.retain(|fact| {
        !shadowed_runtime_reopenings
            .iter()
            .any(|(fqn, range)| *fqn == fact.fqn && *range == fact.range)
    });
    merged.symbols.retain(|fact| {
        !shadowed_runtime_reopenings
            .iter()
            .any(|(fqn, range)| *fqn == fact.fqn && *range == fact.range)
    });
    merged.types.retain(|fact| {
        !shadowed_runtime_reopenings.iter().any(|(fqn, range)| {
            fact.range == *range
                && matches!(
                    &fact.subject,
                    TypeSubject::Constant(constant)
                        if constant.namespace_parts() == fqn.namespace_parts()
                )
        })
    });
    merged.graph_edges.retain(|edge| {
        !shadowed_runtime_reopenings.iter().any(|(fqn, range)| {
            edge.source == *fqn
                && edge.range.file_id == range.file_id
                && range.start_byte <= edge.range.start_byte
                && edge.range.end_byte <= range.end_byte
        })
    });
    merged.unresolved_graph_edges.retain(|edge| {
        !shadowed_runtime_reopenings.iter().any(|(fqn, range)| {
            edge.source == *fqn
                && edge.range.file_id == range.file_id
                && range.start_byte <= edge.range.start_byte
                && edge.range.end_byte <= range.end_byte
        })
    });

    let runtime_methods = runtime_types
        .iter()
        .filter_map(|fact| match &fact.subject {
            TypeSubject::MethodReturn(fqn) => Some(fqn.clone()),
            TypeSubject::Constant(_)
            | TypeSubject::Local { .. }
            | TypeSubject::InstanceVariable { .. }
            | TypeSubject::ClassVariable { .. }
            | TypeSubject::GlobalVariable(_)
            | TypeSubject::Parameter { .. }
            | TypeSubject::Expression(_) => None,
        })
        .collect::<HashSet<_>>();
    for method in runtime_aware
        .methods
        .iter()
        .filter(|fact| runtime_methods.contains(&fact.fqn))
    {
        merged
            .methods
            .retain(|fact| fact.range != method.range || fact.fqn.name() != method.fqn.name());
        merged.methods.push(method.clone());
    }
    for symbol in runtime_aware.symbols.iter().filter(|fact| {
        fact.kind == AnalysisSymbolKind::Method && runtime_methods.contains(&fact.fqn)
    }) {
        merged.symbols.retain(|fact| {
            fact.kind != AnalysisSymbolKind::Method
                || fact.range != symbol.range
                || fact.fqn.name() != symbol.fqn.name()
        });
        merged.symbols.push(symbol.clone());
    }
    for symbol in runtime_aware
        .symbols
        .iter()
        .filter(|fact| runtime_constants.contains(&fact.fqn))
    {
        if !merged.symbols.contains(symbol) {
            merged.symbols.push(symbol.clone());
        }
    }
    for runtime_type in runtime_types {
        merged.types.retain(|fact| {
            fact.provenance != TypeProvenance::Runtime
                || fact.range != runtime_type.range
                || fact.subject != runtime_type.subject
        });
        merged.types.push(runtime_type);
    }
}

fn rehome_execution_context_type_facts(extension_aware: &[TypeFact], merged: &mut Vec<TypeFact>) {
    for generated in extension_aware
        .iter()
        .filter(|fact| type_subject_has_generated_owner(&fact.subject))
    {
        merged.retain(|fact| {
            type_subject_has_generated_owner(&fact.subject)
                || fact.range != generated.range
                || !same_type_subject_slot(&fact.subject, &generated.subject)
        });
    }
}

fn merge_precise_visitor_type_facts(visitor_facts: Vec<TypeFact>, merged: &mut Vec<TypeFact>) {
    let existing_type_subjects = merged
        .iter()
        .map(|fact| fact.subject.clone())
        .collect::<HashSet<_>>();
    for visitor_fact in visitor_facts {
        if visitor_fact.provenance != TypeProvenance::Runtime {
            if !existing_type_subjects.contains(&visitor_fact.subject) {
                merged.push(visitor_fact);
            }
            continue;
        }
        let matching_slot_indexes = merged
            .iter()
            .enumerate()
            .filter_map(|(index, fact)| {
                (fact.subject == visitor_fact.subject
                    && assignment_ranges_identify_same_write(fact.range, visitor_fact.range))
                .then_some(index)
            })
            .collect::<Vec<_>>();
        if matching_slot_indexes.is_empty() {
            if merged
                .iter()
                .all(|fact| fact.subject != visitor_fact.subject)
            {
                merged.push(visitor_fact);
            }
            continue;
        }
        if visitor_fact.ruby_type == RubyType::Unknown {
            continue;
        }
        if matching_slot_indexes
            .iter()
            .any(|index| merged[*index].ruby_type == visitor_fact.ruby_type)
        {
            continue;
        }
        for index in matching_slot_indexes.into_iter().rev() {
            merged.remove(index);
        }
        merged.push(visitor_fact);
    }
}

fn assignment_ranges_identify_same_write(left: TextRange, right: TextRange) -> bool {
    left.file_id == right.file_id
        && ((left.start_byte <= right.start_byte && right.end_byte <= left.end_byte)
            || (right.start_byte <= left.start_byte && left.end_byte <= right.end_byte))
}

fn type_subject_has_generated_owner(subject: &TypeSubject) -> bool {
    match subject {
        TypeSubject::Constant(fqn) | TypeSubject::MethodReturn(fqn) => fqn.has_generated_owner(),
        TypeSubject::InstanceVariable { owner, .. } | TypeSubject::ClassVariable { owner, .. } => {
            owner.has_generated_owner()
        }
        TypeSubject::Parameter { method, .. } => method.has_generated_owner(),
        TypeSubject::Local { .. } | TypeSubject::GlobalVariable(_) | TypeSubject::Expression(_) => {
            false
        }
    }
}

fn same_type_subject_slot(left: &TypeSubject, right: &TypeSubject) -> bool {
    match (left, right) {
        (TypeSubject::Constant(_), TypeSubject::Constant(_))
        | (TypeSubject::MethodReturn(_), TypeSubject::MethodReturn(_)) => true,
        (
            TypeSubject::InstanceVariable { name: left, .. },
            TypeSubject::InstanceVariable { name: right, .. },
        )
        | (
            TypeSubject::ClassVariable { name: left, .. },
            TypeSubject::ClassVariable { name: right, .. },
        ) => left == right,
        (TypeSubject::Parameter { name: left, .. }, TypeSubject::Parameter { name: right, .. }) => {
            left == right
        }
        (TypeSubject::Local { .. }, TypeSubject::Local { .. })
        | (TypeSubject::GlobalVariable(_), TypeSubject::GlobalVariable(_))
        | (TypeSubject::Expression(_), TypeSubject::Expression(_)) => false,
        (TypeSubject::Constant(_), _)
        | (TypeSubject::Local { .. }, _)
        | (TypeSubject::InstanceVariable { .. }, _)
        | (TypeSubject::ClassVariable { .. }, _)
        | (TypeSubject::GlobalVariable(_), _)
        | (TypeSubject::MethodReturn(_), _)
        | (TypeSubject::Parameter { .. }, _)
        | (TypeSubject::Expression(_), _) => false,
    }
}

fn replace_analysis_facts_for_file(
    analysis_engine: &Arc<parking_lot::RwLock<AnalysisEngine>>,
    file_id: ruby_analysis::core::SourceFileId,
    facts: &ruby_analysis::indexer::AnalysisIndex,
    resolve_references: bool,
) {
    replace_file_analysis(
        analysis_engine,
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
    analysis_engine: &Arc<parking_lot::RwLock<AnalysisEngine>>,
    file_id: ruby_analysis::core::SourceFileId,
    facts: FileFacts,
    resolution: FileResolution,
) -> SemanticChange {
    let mut engine = analysis_engine.write();
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
        execution_contexts: Vec::new(),
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
    fn execution_context_merge_replaces_lexical_method_with_generated_owner() {
        let file_id = ruby_analysis::core::SourceFileId(7);
        let range = TextRange::new(file_id, 20, 44);
        let method = RubyMethod::new("helper").unwrap();
        let lexical_parts = vec![RubyConstant::new("Lexical").unwrap()];
        let generated_part = RubyConstant::generated_owner(
            ruby_analysis::core::GeneratedOwnerId::new(
                "rspec-ruby",
                "file:///workspace/spec/example_spec.rb",
                "group:1:2",
            )
            .unwrap(),
        );
        let generated_parts = vec![generated_part];
        let lexical_fqn = FullyQualifiedName::method(lexical_parts.clone(), method);
        let generated_fqn = FullyQualifiedName::method(generated_parts.clone(), method);
        let mut merged = ruby_analysis::indexer::AnalysisIndex {
            methods: vec![MethodFact::new(
                lexical_fqn.clone(),
                FullyQualifiedName::namespace(lexical_parts),
                range,
            )],
            symbols: vec![SymbolFact::new(
                lexical_fqn,
                AnalysisSymbolKind::Method,
                range,
            )],
            ..Default::default()
        };
        let extension_aware = ruby_analysis::indexer::AnalysisIndex {
            methods: vec![MethodFact::new(
                generated_fqn.clone(),
                FullyQualifiedName::namespace(generated_parts.clone()),
                range,
            )],
            symbols: vec![SymbolFact::new(
                generated_fqn,
                AnalysisSymbolKind::Method,
                range,
            )],
            graph_nodes: vec![GraphNodeFact::new(
                FullyQualifiedName::namespace(generated_parts),
                GraphNodeKind::Class,
                range,
            )],
            ..Default::default()
        };

        merge_execution_context_direct_facts(&extension_aware, &mut merged);

        assert_eq!(merged.methods.len(), 1);
        assert!(merged.methods[0].owner.has_generated_owner());
        assert_eq!(merged.symbols.len(), 1);
        assert!(merged.symbols[0].fqn.has_generated_owner());
        assert_eq!(merged.graph_nodes, extension_aware.graph_nodes);
    }

    #[test]
    fn precise_runtime_aware_assignment_replaces_the_same_less_precise_write() {
        let file_id = ruby_analysis::core::SourceFileId(8);
        let subject = TypeSubject::Constant(FullyQualifiedName::try_from("RICH").unwrap());
        let direct_name_range = TextRange::new(file_id, 0, 4);
        let visitor_assignment_range = TextRange::new(file_id, 0, 24);
        let earlier = RubyType::Class(FullyQualifiedName::try_from("RichFixture").unwrap());
        let precise =
            RubyType::Class(FullyQualifiedName::try_from("Java::Fixtures::RichFixture").unwrap());
        let mut merged = vec![TypeFact::new(
            subject.clone(),
            earlier,
            direct_name_range,
            TypeProvenance::Assignment,
        )];

        merge_precise_visitor_type_facts(
            vec![TypeFact::new(
                subject.clone(),
                precise.clone(),
                visitor_assignment_range,
                TypeProvenance::Runtime,
            )],
            &mut merged,
        );

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].subject, subject);
        assert_eq!(merged[0].ruby_type, precise);

        merge_precise_visitor_type_facts(
            vec![TypeFact::new(
                merged[0].subject.clone(),
                RubyType::Unknown,
                visitor_assignment_range,
                TypeProvenance::Runtime,
            )],
            &mut merged,
        );
        assert_eq!(
            merged[0].ruby_type, precise,
            "a later unknown fact must never erase an existing precise type"
        );
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

    #[test]
    fn file_processor_reopens_a_cross_file_class_alias_under_the_original_owner() {
        let server = RubyLanguageServer::default();
        let processor = FileProcessor::with_extension_registry(server.extension_registry.clone());
        let declaration_uri = Url::parse("file:///project/types.rb").unwrap();
        let reopening_uri = Url::parse("file:///project/reopening.rb").unwrap();

        processor
            .process_file_current_file_resolution_forced(
                &declaration_uri,
                "module Types\n  class Original\n  end\n  Alias = Original\nend\n",
                &server,
            )
            .unwrap();
        processor
            .process_file_current_file_resolution_forced(
                &reopening_uri,
                "module Types\n  class Alias\n    def from_other_file\n    end\n  end\nend\n",
                &server,
            )
            .unwrap();

        let expected = FullyQualifiedName::method(
            vec![
                RubyConstant::new("Types").unwrap(),
                RubyConstant::new("Original").unwrap(),
            ],
            RubyMethod::new("from_other_file").unwrap(),
        );
        let shadow = FullyQualifiedName::method(
            vec![
                RubyConstant::new("Types").unwrap(),
                RubyConstant::new("Alias").unwrap(),
            ],
            RubyMethod::new("from_other_file").unwrap(),
        );
        let engine = server.analysis_engine.read();
        assert_eq!(engine.method_facts_for(&expected).len(), 1);
        assert!(engine.method_facts_for(&shadow).is_empty());
    }

    #[test]
    fn explicit_project_engine_owns_external_gem_source() {
        let server = RubyLanguageServer::default();
        let project_uri = Url::parse("file:///workspace/server/").unwrap();
        let project = server.add_workspace(project_uri);
        let dependency_uri =
            Url::parse("file:///workspace/server/vendor/cache/pbkdf2/lib/pbkdf2.rb").unwrap();
        let processor = FileProcessor::with_extension_registry(server.extension_registry.clone());

        processor
            .collect_file_facts_as_deferred_resolution_in_engine(
                &dependency_uri,
                "class PBKDF2\nend\n",
                project.analysis_engine.clone(),
                SourceKind::Gem,
            )
            .unwrap();

        let engine = project.analysis_engine.read();
        let path = dependency_uri.to_file_path().unwrap();
        let file_id = engine
            .file_id(&path)
            .expect("gem source must be registered");
        assert_eq!(engine.file(file_id).unwrap().kind, SourceKind::Gem);
        assert!(server.analysis_engine.read().file_id(&path).is_none());
    }

    #[test]
    fn external_gem_collection_can_emit_a_rebindable_project_neutral_template() {
        let server = RubyLanguageServer::default();
        let processor = FileProcessor::with_extension_registry(server.extension_registry.clone());
        let producer_engine = Arc::new(parking_lot::RwLock::new(AnalysisEngine::new()));
        let dependency_uri = Url::parse("file:///shared/gems/widget/lib/widget.rb").unwrap();
        let source = "class SharedWidget\n  def value\n    'cached'\n  end\nend\n";

        let template = processor
            .collect_project_neutral_file_template_as_deferred_resolution_in_engine(
                &dependency_uri,
                source,
                producer_engine,
                SourceKind::Gem,
                Arc::new(HashSet::new()),
            )
            .unwrap();

        let mut consumer = AnalysisEngine::new();
        consumer.register_file(ruby_analysis::engine::SourceFileInput {
            path: PathBuf::from("/consumer/project.rb"),
            content: String::new(),
            kind: SourceKind::Project,
        });
        let dependency_file = consumer.register_file(ruby_analysis::engine::SourceFileInput {
            path: PathBuf::from("/consumer/cache/widget/lib/widget.rb"),
            content: source.to_string(),
            kind: SourceKind::Gem,
        });
        consumer.replace_facts(
            dependency_file,
            template.instantiate(dependency_file),
            ResolveMode::Immediate,
        );

        let definitions = AnalysisQuery::new(&consumer)
            .constant_definition_ranges(&[RubyConstant::new("SharedWidget").unwrap()], &[]);
        assert_eq!(definitions.len(), 1);
        assert_eq!(definitions[0].file_id, dependency_file);
        assert_eq!(
            consumer.file(definitions[0].file_id).unwrap().path,
            PathBuf::from("/consumer/cache/widget/lib/widget.rb")
        );
    }
}

fn collect_known_namespaces(
    analysis_engine: &Arc<parking_lot::RwLock<AnalysisEngine>>,
) -> HashSet<FullyQualifiedName> {
    let engine = analysis_engine.read();
    AnalysisQuery::new(&engine).known_namespace_fqns()
}

fn collect_known_constant_types(
    analysis_engine: &Arc<parking_lot::RwLock<AnalysisEngine>>,
    current_file: ruby_analysis::core::SourceFileId,
) -> HashMap<FullyQualifiedName, RubyType> {
    let engine = analysis_engine.read();
    let mut candidates = HashMap::<FullyQualifiedName, Option<RubyType>>::new();
    for fact in engine
        .type_store()
        .all_facts()
        .into_iter()
        .filter(|fact| fact.range.file_id != current_file)
    {
        let TypeSubject::Constant(constant) = fact.subject else {
            continue;
        };
        match candidates.entry(constant) {
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(Some(fact.ruby_type));
            }
            std::collections::hash_map::Entry::Occupied(mut entry) => {
                if entry.get().as_ref() != Some(&fact.ruby_type) {
                    entry.insert(None);
                }
            }
        }
    }
    candidates
        .into_iter()
        .filter_map(|(constant, ruby_type)| ruby_type.map(|ruby_type| (constant, ruby_type)))
        .collect()
}

fn add_extension_analysis_facts(
    analysis_engine: &Arc<parking_lot::RwLock<AnalysisEngine>>,
    document: &RubyDocument,
    patches: &[IndexPatch],
    project: Option<&ProjectContext>,
    facts: &mut ruby_analysis::indexer::AnalysisIndex,
) {
    if patches.is_empty() {
        return;
    }

    let mut known_namespaces = {
        let engine = analysis_engine.read();
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
                let (namespace, owner_kind) = analysis_patch_owner(
                    document,
                    project,
                    method.owner_target.as_ref(),
                    &method.namespace,
                    method.owner_kind,
                    &method.source.extension_id,
                    "DefineMethod owner",
                );
                let ruby_method = RubyMethod::new(&method.name).unwrap_or_else(|err| {
                    panic!(
                        "INVARIANT VIOLATED: extension emitted invalid analysis method `{}`: {}. \
                         This is a bug because extension method patches must be validated before fact conversion. \
                         Fix: reject invalid DefineMethod patches at the extension boundary.",
                        method.name, err
                    )
                });
                let fqn = FullyQualifiedName::method(namespace.clone(), ruby_method);
                let owner = FullyQualifiedName::namespace_with_kind(namespace, owner_kind);
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
                let (mut source_parts, source_kind) = analysis_patch_owner(
                    document,
                    project,
                    mixin.owner_target.as_ref(),
                    &mixin.namespace,
                    mixin.target_kind,
                    &mixin.source.extension_id,
                    "ApplyMixin owner",
                );
                if source_parts.is_empty() && mixin.owner_target.is_none() {
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

                let source =
                    FullyQualifiedName::namespace_with_kind(source_parts.clone(), source_kind);
                let kind = analysis_mixin_kind(mixin.kind);
                let range = text_range_from_source_range(document, mixin.location, "mixin");
                if let Some(target) = mixin.mixin_target.as_ref() {
                    let (target_parts, target_kind) = analysis_patch_owner(
                        document,
                        project,
                        Some(target),
                        &[],
                        ruby_fast_lsp_extension_api::NamespaceKind::Instance,
                        &mixin.source.extension_id,
                        "ApplyMixin semantic target",
                    );
                    let target = FullyQualifiedName::namespace_with_kind(target_parts, target_kind);
                    facts.graph_edges.push(GraphEdgeFact::new(
                        source.clone(),
                        target.clone(),
                        kind,
                        range,
                    ));
                    if mixin.kind == MixinKind::Extend {
                        if let Some(singleton_source) = source.to_singleton_namespace() {
                            facts.graph_edges.push(GraphEdgeFact::new(
                                singleton_source,
                                target,
                                GraphEdgeKind::Include,
                                range,
                            ));
                        }
                    }
                    continue;
                }

                let target_parts = ruby_constants(&mixin.mixin, "ApplyMixin target");
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
            IndexPatch::ConnectExecutionContext(connection) => {
                let (template_parts, template_kind) = analysis_patch_owner(
                    document,
                    project,
                    Some(&connection.template),
                    &[],
                    ruby_fast_lsp_extension_api::NamespaceKind::Instance,
                    &connection.source.extension_id,
                    "ConnectExecutionContext template",
                );
                let (application_parts, application_kind) = analysis_patch_owner(
                    document,
                    project,
                    Some(&connection.application),
                    &[],
                    ruby_fast_lsp_extension_api::NamespaceKind::Instance,
                    &connection.source.extension_id,
                    "ConnectExecutionContext application",
                );
                facts.graph_edges.push(GraphEdgeFact::new(
                    FullyQualifiedName::namespace_with_kind(template_parts, template_kind),
                    FullyQualifiedName::namespace_with_kind(application_parts, application_kind),
                    GraphEdgeKind::ExecutionContextApplication,
                    text_range_from_source_range(
                        document,
                        connection.location,
                        "execution context application",
                    ),
                ));
            }
        }
    }
}

fn analysis_patch_owner(
    document: &RubyDocument,
    project: Option<&ProjectContext>,
    target: Option<&ruby_fast_lsp_extension_api::ExecutionContextTarget>,
    fallback_namespace: &[String],
    fallback_kind: ruby_fast_lsp_extension_api::NamespaceKind,
    extension_id: &str,
    label: &str,
) -> (Vec<RubyConstant>, AnalysisNamespaceKind) {
    match target {
        None => (
            ruby_constants(fallback_namespace, label),
            analysis_namespace_kind(fallback_kind),
        ),
        Some(ruby_fast_lsp_extension_api::ExecutionContextTarget::Namespace {
            namespace,
            owner_kind,
        }) => (
            ruby_constants(namespace, label),
            analysis_namespace_kind(*owner_kind),
        ),
        Some(ruby_fast_lsp_extension_api::ExecutionContextTarget::GeneratedOwner {
            local_id,
            owner_kind,
        }) => {
            let owner = GeneratedOwnerId::new(extension_id, document.uri.as_str(), local_id)
                .expect(
                    "INVARIANT VIOLATED: invalid generated patch owner reached fact conversion. This is a bug because extension owner targets must be validated before collection. Fix: keep validate_patch_owner_target before add_extension_analysis_facts.",
                );
            (
                vec![RubyConstant::generated_owner(owner)],
                owner_kind
                    .map(analysis_namespace_kind)
                    .unwrap_or_else(|| analysis_namespace_kind(fallback_kind)),
            )
        }
        Some(ruby_fast_lsp_extension_api::ExecutionContextTarget::ProjectGeneratedOwner {
            local_id,
            owner_kind,
        }) => {
            let project_uri = project
                .map(|project| project.project_uri.as_str())
                .expect(
                    "INVARIANT VIOLATED: project-generated patch owner reached fact conversion without ProjectContext. This is a host validation bug because project-scoped targets must be rejected before collection. Fix: preserve the owning project context through extension fact conversion.",
                );
            let owner = GeneratedOwnerId::new(extension_id, project_uri, local_id).expect(
                "INVARIANT VIOLATED: invalid project-generated patch owner reached fact conversion. This is a bug because extension owner targets must be validated before collection. Fix: keep validate_patch_owner_target before add_extension_analysis_facts.",
            );
            (
                vec![RubyConstant::generated_owner(owner)],
                owner_kind
                    .map(analysis_namespace_kind)
                    .unwrap_or_else(|| analysis_namespace_kind(fallback_kind)),
            )
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
