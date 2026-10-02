//! File Processing Module
//!
//! This module provides shared file processing logic. It handles parsing,
//! fact collection, reference candidates, and diagnostic generation.
//!
//! ## Key Components
//!
//! - **`FileProcessor`**: Core struct for processing individual files
//! - **`ProcessResult`**: Results of processing including diagnostics and affected URIs
//! - **`syntax_diagnostics`**: Parser-derived diagnostics for one parsed file
//!
//! ## Usage
//!
//! Each indexer (project, stdlib, gem) discovers files to process, then delegates
//! the actual processing to `FileProcessor` with appropriate options.

use crate::environment::extensions::{ExtensionRegistryHandle, ProjectContextSeed};
use crate::environment::runtime::jruby::imports::{
    JrubyImportProvider, StaticJavaNavigationPlan, StaticJavaSourceHint,
};
use crate::invariant::ExpectInvariant;
use crate::loader::context::LoadContext;
use crate::loader::jruby_add_on::JrubyAddOn;
use crate::loader::require_paths::RequireFeatureIndex;
use anyhow::Result;
pub(crate) use collection::commit_extension_seed;
use collection::{replace_analysis_facts_for_file, replace_file_analysis};
use compose::{ExtensionDocument, FileComposition, RequireDiagnosticRoots};
use log::{debug, info};
use merge::collect_direct_facts;
use ruby_analysis::core::{FileAnalysis, FullyQualifiedName, SourceKind};
use ruby_analysis::engine::{ProjectNeutralFileFactsTemplate, SemanticChange};
use ruby_analysis::indexer::fact_collector::FactCollector;
use ruby_analysis::indexer::RubyDocument;
use ruby_analysis::indexer::{is_erb_path, mask_erb};
use ruby_prism::Visit;
use std::borrow::Cow;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use syntax_diagnostics::generate_diagnostics;
use tower_lsp::lsp_types::{Diagnostic, Url};

mod collection;
mod compose;
mod extension_facts;
mod extension_host;
mod jruby_navigation;
mod merge;
pub mod syntax_diagnostics;

/// Result of processing a file
pub struct ProcessResult {
    /// Functionally affected URIs (files that need updated diagnostics)
    pub affected_uris: HashSet<Url>,
    /// Syntax and early validation diagnostics
    pub diagnostics: Vec<Diagnostic>,
    /// Whether this pass changed declarations visible to other files.
    pub semantic_change: SemanticChange,
}

struct CollectedFileAnalysisOutput {
    project_neutral_template: Option<ProjectNeutralFileFactsTemplate>,
    retained_analysis: Option<FileAnalysis>,
    jruby_navigation_plan: StaticJavaNavigationPlan,
    jruby_source_hint: StaticJavaSourceHint,
    timing: ProjectFileCollectionTiming,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ProjectFileCollectionTiming {
    pub total: Duration,
    pub registration: Duration,
    pub parse: Duration,
    pub jruby_plan: Duration,
    pub semantic_seed: Duration,
    pub visitor: Duration,
    pub assembly: Duration,
    pub replacement: Duration,
}

pub struct CollectedProjectAnalysis {
    pub analysis: FileAnalysis,
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

pub(crate) fn analysis_source<'a>(uri: &Url, content: &'a str) -> Cow<'a, str> {
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
    extension_project_context_seed: Option<Arc<parking_lot::RwLock<ProjectContextSeed>>>,
    jruby_import_provider: Option<Arc<JrubyImportProvider>>,
    /// Owning project root used for require-path diagnostics during batch collection.
    require_project_root: Option<PathBuf>,
    /// Per-project configured load paths for require-path diagnostics.
    require_load_paths: Vec<String>,
    /// Absolute gem/stdlib require roots for require-path diagnostics.
    require_dependency_roots: Vec<PathBuf>,
    /// Published gem/stdlib feature map shared across cloned processors.
    require_feature_index: Arc<RequireFeatureIndex>,
}

impl FileProcessor {
    pub fn new() -> Self {
        Self {
            extension_registry: ExtensionRegistryHandle::from_environment(),
            extension_project_context_seed: None,
            jruby_import_provider: None,
            require_project_root: None,
            require_load_paths: Vec::new(),
            require_dependency_roots: Vec::new(),
            require_feature_index: Arc::new(RequireFeatureIndex::empty()),
        }
    }

    pub fn with_extension_registry(extension_registry: ExtensionRegistryHandle) -> Self {
        Self {
            extension_registry,
            extension_project_context_seed: None,
            jruby_import_provider: None,
            require_project_root: None,
            require_load_paths: Vec::new(),
            require_dependency_roots: Vec::new(),
            require_feature_index: Arc::new(RequireFeatureIndex::empty()),
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

    pub(crate) fn with_require_resolve_context(
        mut self,
        project_root: PathBuf,
        load_paths: Vec<String>,
        dependency_roots: Vec<PathBuf>,
    ) -> Self {
        self.require_project_root = Some(project_root);
        self.require_load_paths = load_paths;
        self.require_feature_index = Arc::new(RequireFeatureIndex::build(&dependency_roots, None));
        self.require_dependency_roots = dependency_roots;
        self
    }

    pub(crate) fn set_require_dependency_roots(&mut self, dependency_roots: Vec<PathBuf>) {
        self.require_dependency_roots = dependency_roots;
    }

    pub(crate) fn set_require_feature_index(&mut self, index: Arc<RequireFeatureIndex>) {
        self.require_feature_index = index;
    }

    /// Collect catalog-sensitive JRuby facts through the project's add-on.
    pub(crate) fn with_jruby_add_on(self, add_on: &JrubyAddOn) -> Self {
        self.with_jruby_import_provider(add_on.import_provider().clone())
    }

    pub(crate) fn jruby_import_provider(&self) -> Option<&Arc<JrubyImportProvider>> {
        self.jruby_import_provider.as_ref()
    }

    /// Process a file: parse, collect facts and reference candidates, and return diagnostics.
    /// This prevents double-parsing and centralizes the logic.
    pub fn process_file(
        &self,
        uri: &Url,
        content: &str,
        ctx: &LoadContext,
    ) -> Result<ProcessResult> {
        self.process_file_with_resolution(uri, content, ctx, FileResolution::Full)
    }

    pub fn process_file_current_file_resolution(
        &self,
        uri: &Url,
        content: &str,
        ctx: &LoadContext,
    ) -> Result<ProcessResult> {
        self.process_file_with_resolution(uri, content, ctx, FileResolution::CurrentFile)
    }

    pub fn process_file_current_file_resolution_forced(
        &self,
        uri: &Url,
        content: &str,
        ctx: &LoadContext,
    ) -> Result<ProcessResult> {
        self.process_file_with_resolution_forced(
            uri,
            content,
            ctx,
            FileResolution::CurrentFile,
            true,
        )
    }

    fn process_file_with_resolution(
        &self,
        uri: &Url,
        content: &str,
        ctx: &LoadContext,
        resolution: FileResolution,
    ) -> Result<ProcessResult> {
        self.process_file_with_resolution_forced(uri, content, ctx, resolution, false)
    }

    fn process_file_with_resolution_forced(
        &self,
        uri: &Url,
        content: &str,
        ctx: &LoadContext,
        resolution: FileResolution,
        force_reindex: bool,
    ) -> Result<ProcessResult> {
        // Check if this version was already indexed - skip expensive re-indexing if unchanged
        let already_indexed = !force_reindex && {
            ctx.sources
                .open_document_version(uri)
                .is_some_and(|document| document.indexed_version == Some(document.version))
        };

        if already_indexed {
            debug!(
                "Skipping re-indexing {} (version already indexed)",
                uri.path().split('/').next_back().unwrap_or("unknown")
            );
            // Still parse for syntax diagnostics
            let analysis_source = analysis_source(uri, content);
            let parse_result = ruby_prism::parse(analysis_source.as_bytes());
            let source_kind = self.analysis_source_kind_for_uri(ctx.sink.as_ref(), uri);
            let analysis_file_id = ctx
                .sink
                .register_source(uri, content.to_string(), source_kind);
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
        let analysis_engine = ctx.sink.engine_for_uri(uri);
        self.ensure_jruby_navigation_inputs(content, &analysis_engine)?;
        let parse_result = ruby_prism::parse(analysis_source.as_bytes());
        let node = parse_result.node();
        let source_kind = self.analysis_source_kind_for_uri(ctx.sink.as_ref(), uri);
        let analysis_file_id = ctx
            .sink
            .register_source(uri, content.to_string(), source_kind);
        let document_version = ctx
            .sources
            .open_document_version(uri)
            .map(|document| document.version)
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
            // The file exists but is too broken to analyze: keep it
            // registered with no facts rather than removing it.
            let semantic_change = replace_file_analysis(
                &analysis_engine,
                analysis_file_id,
                FileAnalysis::default(),
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
            .then(|| ctx.sink.extension_context_snapshot(uri, source_kind))
            .flatten();
        let extension_project_context = extension_project_context_snapshot
            .as_ref()
            .map(|snapshot| snapshot.context.clone());
        if extensions_enabled {
            self.seed_extension_semantics(
                &analysis_engine,
                extension_project_context_snapshot.as_ref(),
                |seed| ctx.sink.commit_seed(&analysis_engine, seed),
            );
        }
        let direct_elapsed = direct_start.elapsed();

        let visitor_start = Instant::now();
        let mut visitor = FactCollector::analysis_only(
            document.clone(),
            self.fact_collector_host(extensions_enabled, !source_kind.is_dependency_source()),
            analysis_engine.clone(),
        );
        if source_kind.is_dependency_source() {
            visitor = visitor.without_body_inference();
        }
        visitor.set_extension_project_context(extension_project_context.clone());
        visitor.visit(&node);
        let visitor_elapsed = visitor_start.elapsed();

        let (analysis, updated_document) = self.compose_file_analysis(
            FileComposition {
                uri,
                content,
                file_id: analysis_file_id,
                source_kind,
                analysis_engine: &analysis_engine,
                extension_project_context: extension_project_context.as_ref(),
                declarations: Some(direct_facts_seed),
                extension_document: ExtensionDocument::Collected,
                require_roots: RequireDiagnosticRoots::Context(ctx),
            },
            visitor.finish(),
        );
        let replace_start = Instant::now();
        replace_file_analysis(
            &analysis_engine,
            updated_document.analysis_file_id(),
            analysis,
            resolution,
        );
        let replace_elapsed = replace_start.elapsed();
        let current_export_fingerprint = analysis_engine
            .read()
            .semantic_export_fingerprint(analysis_file_id)
            .expect_invariant(
                "processed file has no semantic export fingerprint",
                "every engine fact replacement must record its exported API",
                "route final file facts through AnalysisEngine::replace_facts",
            );
        let semantic_change =
            SemanticChange::classify(previous_export_fingerprint, current_export_fingerprint);

        // Retain the processed document and mark its version indexed.
        ctx.sink.mark_document_indexed(uri, updated_document);

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
}

impl Default for FileProcessor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
