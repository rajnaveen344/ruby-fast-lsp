//! Content-based fact collection and file-owned fact replacement.

use super::compose::{ExtensionDocument, FileComposition, RequireDiagnosticRoots};
use super::merge::{collect_direct_facts, collect_known_namespaces};
use super::FileProcessor;
use super::{
    analysis_source, CollectedFileAnalysisOutput, CollectedProjectAnalysis, FileResolution,
    ProjectFileCollectionTiming,
};
use crate::environment::extensions::{ExtensionSemanticSeed, ProjectContextSnapshot};
use crate::environment::runtime::jruby::imports::StaticJavaNavigationPlan;
use crate::invariant::ExpectInvariant;
use crate::loader::context::LoadSink;
use anyhow::{anyhow, Context, Result};
use log::debug;
use ruby_analysis::core::{
    FileAnalysis, FullyQualifiedName, SourceKind, SymbolKind as AnalysisSymbolKind, TypeSubject,
};
use ruby_analysis::engine::{
    AnalysisEngine, ProjectNeutralFileFactsTemplate, ResolveMode, SemanticChange,
};
use ruby_analysis::indexer::fact_collector::FactCollector;
use ruby_analysis::indexer::AnalysisIndexer;
use ruby_analysis::indexer::RubyDocument;
use ruby_fast_lsp_jruby_support::StaticJavaSourceHint;
use ruby_prism::Visit;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tower_lsp::lsp_types::Url;

pub(super) fn replace_analysis_facts_for_file(
    analysis_engine: &Arc<parking_lot::RwLock<AnalysisEngine>>,
    file_id: ruby_analysis::core::SourceFileId,
    facts: &ruby_analysis::core::FileAnalysis,
    resolve_references: bool,
) {
    let mut file_facts = facts.clone();
    if file_facts.inference == ruby_analysis::core::InferenceEvidence::default() {
        if let Some(previous) = analysis_engine.read().inference_evidence_in_file(file_id) {
            file_facts.inference = previous;
        }
    }
    replace_file_analysis(
        analysis_engine,
        file_id,
        file_facts,
        if resolve_references {
            FileResolution::Full
        } else {
            FileResolution::Deferred
        },
    );
}

pub(super) fn replace_file_analysis(
    analysis_engine: &Arc<parking_lot::RwLock<AnalysisEngine>>,
    file_id: ruby_analysis::core::SourceFileId,
    facts: FileAnalysis,
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

/// Register the extension semantic seed source in `engine` and replace its
/// facts with `seed`. Resolution is deferred: the seed only has to be visible
/// to the file walk that follows it.
pub(crate) fn commit_extension_seed(
    analysis_engine: &Arc<parking_lot::RwLock<AnalysisEngine>>,
    seed: ExtensionSemanticSeed,
) {
    let mut engine = analysis_engine.write();
    let file_id = engine.register_file(seed.source());
    engine.update(file_id, seed.analysis(file_id), ResolveMode::Deferred);
}

impl FileProcessor {
    /// Commit the extension semantic seed `engine` lacks for `snapshot`'s
    /// project (or for no project) through `commit`, before a file walk
    /// consults it.
    pub(super) fn seed_extension_semantics(
        &self,
        engine: &Arc<parking_lot::RwLock<AnalysisEngine>>,
        snapshot: Option<&ProjectContextSnapshot>,
        commit: impl FnOnce(ExtensionSemanticSeed),
    ) {
        match snapshot {
            Some(snapshot) => self
                .extension_registry
                .with_semantic_seed_for_snapshot(engine, snapshot, commit),
            None => self
                .extension_registry
                .with_semantic_seed(engine, None, commit),
        }
    }

    // ========================================================================
    // Content-based Indexing (in-memory content)
    // ========================================================================

    pub(crate) fn collect_file_facts(
        &self,
        uri: &Url,
        content: &str,
        sink: &dyn LoadSink,
    ) -> Result<()> {
        self.collect_file_facts_as(uri, content, sink, SourceKind::Project)
    }

    pub(crate) fn collect_file_facts_as(
        &self,
        uri: &Url,
        content: &str,
        sink: &dyn LoadSink,
        source_kind: SourceKind,
    ) -> Result<()> {
        let analysis_engine = sink.engine_for_uri(uri);
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

    #[cfg(test)]
    pub(crate) fn collect_file_facts_as_deferred_resolution(
        &self,
        uri: &Url,
        content: &str,
        sink: &dyn LoadSink,
        source_kind: SourceKind,
    ) -> Result<()> {
        let analysis_engine = sink.engine_for_uri(uri);
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

    pub(crate) fn collect_rbs_facts_as_deferred_resolution(
        &self,
        uri: &Url,
        content: &str,
        sink: &dyn LoadSink,
    ) -> Result<()> {
        self.collect_rbs_facts_with_resolution(uri, content, sink, FileResolution::Deferred)
    }

    pub(crate) fn collect_rbs_facts(
        &self,
        uri: &Url,
        content: &str,
        sink: &dyn LoadSink,
    ) -> Result<()> {
        self.collect_rbs_facts_with_resolution(uri, content, sink, FileResolution::Full)
    }

    fn collect_rbs_facts_with_resolution(
        &self,
        uri: &Url,
        content: &str,
        sink: &dyn LoadSink,
        resolution: FileResolution,
    ) -> Result<()> {
        let analysis_engine = sink.engine_for_uri(uri);
        let analysis_file_id =
            sink.register_source(uri, content.to_string(), SourceKind::Signature);
        let facts = match ruby_analysis::indexer::index_rbs(analysis_file_id, content) {
            Ok(facts) => facts,
            Err(error) => {
                // The signature file exists but does not parse: keep it
                // registered with no facts rather than removing it.
                replace_file_analysis(
                    &analysis_engine,
                    analysis_file_id,
                    FileAnalysis::default(),
                    resolution,
                );
                return Err(anyhow::anyhow!(
                    "Failed to parse RBS {}: {error}",
                    uri.path()
                ));
            }
        };
        replace_file_analysis(&analysis_engine, analysis_file_id, facts, resolution);
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

    pub fn collect_project_file_facts_and_jruby_navigation_plan_as_deferred_resolution(
        &self,
        uri: &Url,
        content: String,
        analysis_engine: Arc<parking_lot::RwLock<AnalysisEngine>>,
        known_namespaces: Arc<HashSet<FullyQualifiedName>>,
    ) -> Result<CollectedProjectAnalysis> {
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
        Ok(CollectedProjectAnalysis {
            analysis: output.retained_analysis.expect_invariant(
                "project batch collection did not retain its file-owned facts",
                "batch workers return facts without touching the shared engine",
                "keep retained_analysis on for the project batch path",
            ),
            jruby_navigation_plan: output.jruby_navigation_plan,
            jruby_source_hint: output.jruby_source_hint,
            timing: output.timing,
        })
    }

    /// Collect the direct, extension-independent declarations that form the
    /// semantic skeleton for one deterministic project batch.
    ///
    /// Every project seeds value-constant symbols and types before body
    /// traversal. Extension-aware projects also retain their namespace and
    /// method skeleton. This keeps generic receiver/block inference independent
    /// of discovery order without broadening the extension lookup policy.
    pub fn collect_project_direct_semantic_seed(
        &self,
        uri: &Url,
        content: &str,
        analysis_engine: &Arc<parking_lot::RwLock<AnalysisEngine>>,
        known_namespaces: &HashSet<FullyQualifiedName>,
    ) -> Option<FileAnalysis> {
        let extension_namespace_seed = self.requires_extension_namespace_seed(uri);
        // The direct collector emits value-constant symbols only for Prism's
        // constant write nodes, whose assignment token contains '='. This is
        // a conservative parse prefilter; extension skeletons still run even
        // when the source contains no assignment.
        if !extension_namespace_seed && !content.contains('=') {
            return None;
        }
        let path = uri
            .to_file_path()
            .unwrap_or_else(|_| PathBuf::from(uri.to_string()));
        let file_id = analysis_engine.read().file_id(&path).unwrap_or_else(|| {
            unreachable_invariant!(
                what = "project semantic seed received an unregistered source {}",
                why = "batch file identities must be fixed before declaration collection",
                fix = "pre-register the complete project batch before collecting its direct semantic seed",
                path.display(),
            )
        });
        let source = analysis_source(uri, content);
        let parse = ruby_prism::parse(source.as_bytes());
        if !extension_namespace_seed
            && !AnalysisIndexer::has_value_constant_declarations(&parse.node())
        {
            return None;
        }
        let direct = collect_direct_facts(
            analysis_engine,
            &parse.node(),
            source.as_ref(),
            file_id,
            Some(known_namespaces),
        );
        let symbols = direct
            .symbols
            .into_iter()
            .filter(|symbol| symbol.kind == AnalysisSymbolKind::Constant)
            .collect::<Vec<_>>();
        if symbols.is_empty() && !extension_namespace_seed {
            return None;
        }
        let constants = symbols
            .iter()
            .map(|symbol| &symbol.fqn)
            .collect::<HashSet<_>>();
        let types = direct
            .types
            .into_iter()
            .filter(|fact| matches!(&fact.subject, TypeSubject::Constant(fqn) if constants.contains(fqn)))
            .collect();
        let mut facts = FileAnalysis {
            symbols,
            types,
            ..FileAnalysis::default()
        };
        if extension_namespace_seed {
            facts.methods = direct.methods;
            facts.method_visibility_overrides = direct.method_visibility_overrides;
            facts.graph_nodes = direct.graph_nodes;
            facts.graph_edges = direct.graph_edges;
            facts.unresolved_graph_edges = direct.unresolved_graph_edges;
        }
        Some(facts)
    }

    fn requires_extension_namespace_seed(&self, uri: &Url) -> bool {
        let project_context = self.extension_project_context_seed.as_ref().map(|seed| {
            seed.read()
                .context_snapshot(uri.to_string(), SourceKind::Project)
                .context
        });
        self.extension_registry
            .has_applicable_semantic_targets(project_context.as_ref())
    }

    pub(crate) fn ensure_project_semantic_seed(
        &self,
        uri: &Url,
        analysis_engine: &Arc<parking_lot::RwLock<AnalysisEngine>>,
        sink: &dyn LoadSink,
    ) {
        let project_context_snapshot = self.extension_project_context_seed.as_ref().map(|seed| {
            seed.read()
                .context_snapshot(uri.to_string(), SourceKind::Project)
        });
        self.seed_extension_semantics(analysis_engine, project_context_snapshot.as_ref(), |seed| {
            sink.commit_seed(analysis_engine, seed)
        });
    }

    pub fn replace_collected_project_file_facts_as_deferred_resolution(
        &self,
        path: &Path,
        analysis_engine: &Arc<parking_lot::RwLock<AnalysisEngine>>,
        facts: FileAnalysis,
    ) {
        let file_id = analysis_engine.read().file_id(path).unwrap_or_else(|| {
            unreachable_invariant!(
                what = "deterministic project fact replacement received an unregistered source {}",
                why = "the bounded batch must be registered before semantic collection",
                fix = "preserve the pre-registration and ordered replacement lifecycle",
                path.display(),
            )
        });
        replace_file_analysis(analysis_engine, file_id, facts, FileResolution::Deferred);
    }

    pub fn replace_collected_project_file_facts_if_source_snapshot_as_deferred_resolution(
        &self,
        path: &Path,
        analysis_engine: &Arc<parking_lot::RwLock<AnalysisEngine>>,
        source_snapshot: ruby_analysis::engine::SourceFileSnapshot,
        facts: FileAnalysis,
    ) -> bool {
        let mut engine = analysis_engine.write();
        if engine.source_snapshot_for_path(path) != Some(source_snapshot) {
            return false;
        }
        engine
            .replace_facts_if_source_snapshot(source_snapshot, facts, ResolveMode::Deferred)
            .is_some()
    }

    pub fn collect_project_neutral_file_template_as_deferred_resolution_in_engine(
        &self,
        uri: &Url,
        content: &str,
        analysis_engine: Arc<parking_lot::RwLock<AnalysisEngine>>,
        source_kind: SourceKind,
        known_namespaces: Arc<HashSet<FullyQualifiedName>>,
    ) -> Result<ProjectNeutralFileFactsTemplate> {
        invariant!(
            source_kind.is_external(),
            what = "project-neutral dependency template requested for a project-owned source",
            why = "project facts hold project-specific references, diagnostics, and contexts",
            fix = "request templates only for external dependency sources",
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
        invariant!(
            source_kind.is_external(),
            what = "project-neutral dependency template requested for a project-owned source",
            why = "project facts hold project-specific references, diagnostics, and contexts",
            fix = "request templates only for external dependency sources",
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

    pub(super) fn collect_file_facts_as_with_resolution(
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
    ) -> Result<CollectedFileAnalysisOutput> {
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
    ) -> Result<CollectedFileAnalysisOutput> {
        let collection_started = Instant::now();
        invariant!(
            insert_collected_facts
                || retain_collected_facts
                || (capture_project_neutral_template && !resolve_references),
            what =
                "FileProcessor skipped insertion without retaining facts or a dependency template",
            why = "ordinary indexing uses the engine replacement lifecycle",
            fix = "insert normal sources; retain facts only for batches or templates",
        );
        invariant!(
            !retain_collected_facts
                || (!insert_collected_facts && !capture_project_neutral_template),
            what = "retained file facts were combined with insertion or project-neutral capture",
            why = "one collection result must have exactly one owner",
            fix = "retain facts only for the deterministic project batch path",
        );
        debug!("Collecting facts for: {:?}", uri);

        let path = uri
            .to_file_path()
            .unwrap_or_else(|_| PathBuf::from(uri.to_string()));
        let registration_started = Instant::now();
        let analysis_file_id = if retain_collected_facts {
            analysis_engine.read().file_id(&path).unwrap_or_else(|| {
                    unreachable_invariant!(
                        what = "deterministic project batch collection received an unregistered source {}",
                        why = "every batch file must be pre-registered before parallel semantic reads begin",
                        fix = "register the complete bounded batch in path order before collecting facts",
                        path.display(),
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
        let direct_facts_seed = resolve_references.then(|| {
            collect_direct_facts(
                &analysis_engine,
                &node,
                analysis_source.as_ref(),
                analysis_file_id,
                known_namespaces.as_deref(),
            )
        });
        if let Some(direct_facts_seed) = direct_facts_seed.as_ref() {
            replace_analysis_facts_for_file(
                &analysis_engine,
                analysis_file_id,
                direct_facts_seed,
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
            // Collection entry points that address a caller-supplied engine
            // carry no sink; they commit the seed through the same loader
            // write the server sink applies.
            self.seed_extension_semantics(
                &analysis_engine,
                extension_project_context_snapshot.as_ref(),
                |seed| commit_extension_seed(&analysis_engine, seed),
            );
        }

        let mut fact_collector = FactCollector::analysis_only(
            document.clone(),
            self.fact_collector_host(extensions_enabled, !source_kind.is_dependency_source()),
            analysis_engine.clone(),
        );
        if source_kind.is_dependency_source() {
            fact_collector = fact_collector.without_body_inference();
        }
        fact_collector.set_extension_project_context(extension_project_context.clone());
        let shared_direct_known_namespaces = known_namespaces
            .unwrap_or_else(|| Arc::new(collect_known_namespaces(&analysis_engine)));
        fact_collector =
            fact_collector.with_shared_direct_known_namespaces(shared_direct_known_namespaces);
        fact_collector.extend_direct_known_namespaces(
            direct_facts_seed
                .iter()
                .flat_map(|seed| seed.graph_nodes.iter())
                .map(|fact| fact.fqn.clone()),
        );
        let semantic_seed_elapsed = semantic_seed_started.elapsed();
        let visitor_started = Instant::now();
        fact_collector.visit(&node);
        let visitor_elapsed = visitor_started.elapsed();
        let assembly_started = Instant::now();
        let (analysis, _) = self.compose_file_analysis(
            FileComposition {
                uri,
                content: document.content.as_str(),
                file_id: analysis_file_id,
                source_kind,
                analysis_engine: &analysis_engine,
                extension_project_context: extension_project_context.as_ref(),
                declarations: direct_facts_seed,
                extension_document: ExtensionDocument::Original(&document),
                require_roots: RequireDiagnosticRoots::Processor,
            },
            fact_collector.finish(),
        );
        let assembly_elapsed = assembly_started.elapsed();
        let replacement_started = Instant::now();
        let template = if capture_project_neutral_template {
            Some(
                ProjectNeutralFileFactsTemplate::try_new(analysis_file_id, analysis.clone())
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
        let retained_analysis = if retain_collected_facts {
            Some(analysis)
        } else {
            if insert_collected_facts {
                replace_file_analysis(
                    &analysis_engine,
                    analysis_file_id,
                    analysis,
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
        Ok(CollectedFileAnalysisOutput {
            project_neutral_template: template,
            retained_analysis,
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

    pub(super) fn analysis_source_kind_for_uri(
        &self,
        sink: &dyn LoadSink,
        uri: &Url,
    ) -> SourceKind {
        let path = uri
            .to_file_path()
            .unwrap_or_else(|_| PathBuf::from(uri.to_string()));
        let analysis_engine = sink.engine_for_uri(uri);
        let engine = analysis_engine.read();
        engine
            .file_id(&path)
            .and_then(|file_id| engine.file(file_id))
            .map(|file| file.kind)
            .unwrap_or(SourceKind::Project)
    }
}
