//! Replay of project files whose facts depend on the JRuby Java catalog.

use super::IndexerProject;
use crate::environment::runtime::jruby::imports::{JrubyImportProvider, StaticJavaNavigationPlan};
use crate::invariant::ExpectInvariant;
use crate::loader::context::LoadContext;
use crate::loader::file_processor::FileProcessor;
use anyhow::{anyhow, Context, Result};
use log::info;
use rayon::prelude::*;
use ruby_analysis::engine::SourceFileSnapshot;
use std::path::PathBuf;
use std::time::Instant;
use tower_lsp::lsp_types::Url;

impl IndexerProject {
    pub(crate) fn jruby_catalog_sensitive_files(
        &self,
        provider: &JrubyImportProvider,
    ) -> Vec<PathBuf> {
        self.jruby_source_hints
            .iter()
            .filter(|(_, hint)| provider.source_hint_may_reference_static_java(hint))
            .map(|(path, _)| path.clone())
            .collect()
    }

    pub(crate) fn replay_jruby_catalog_sensitive_files(
        &mut self,
        file_processor: FileProcessor,
        ctx: &LoadContext,
    ) -> Result<usize> {
        let provider = file_processor
            .jruby_import_provider()
            .cloned()
            .expect_invariant(
                "JRuby replay requested without a project import provider",
                "selection and replacement use one isolated classpath catalog",
                "install the completed provider on the final FileProcessor first",
            );
        let files = self.jruby_catalog_sensitive_files(&provider);
        let project_uri = Url::from_directory_path(&self.workspace_root).map_err(|_| {
            anyhow!(
                "Project root is not a valid file URI: {}",
                self.workspace_root.display()
            )
        })?;
        let analysis_engine = ctx.sink.engine_for_uri(&project_uri);
        let known_namespaces = self.jruby_replay_known_namespaces.take().expect_invariant(
            "JRuby replay has no immutable pre-collection namespace baseline",
            "replayed files use the same context as provider-aware batches",
            "keep the generation baseline through completion; consume it once in replay",
        );
        let semantic_read_engine = self.jruby_replay_analysis_engine.take().expect_invariant(
            "JRuby replay has no immutable pre-collection semantic engine",
            "providerless and provider-aware collection see the same facts",
            "keep the collection baseline through replay; consume it once",
        );
        let replay_started = Instant::now();
        let outcomes = files
            .par_iter()
            .map(
                |file_path| -> Result<Option<(
                    PathBuf,
                    SourceFileSnapshot,
                    ruby_analysis::core::FileAnalysis,
                    StaticJavaNavigationPlan,
                )>> {
                    let (content, open_document) =
                        Self::read_authoritative_project_source(ctx, file_path).with_context(|| {
                        format!(
                            "failed to reread JRuby catalog-sensitive project source {}",
                            file_path.display()
                        )
                    })?;
                    let source_snapshot = {
                        let engine = analysis_engine.read();
                        let Some(file_id) = engine.file_id(file_path) else {
                            unreachable_invariant!(
                                what = "JRuby project replay received an unregistered source {}",
                                why = "replay is selected only from the completed project pass",
                                fix = "preserve project source registration through provider materialization",
                                file_path.display(),
                            );
                        };
                        if open_document && !engine.file_content_matches(file_id, &content) {
                            info!(
                                "Skipping stale JRuby replay snapshot for open document {}",
                                file_path.display()
                            );
                            return Ok(None);
                        }
                        engine.source_snapshot_for_path(file_path).unwrap_or_else(|| {
                            unreachable_invariant!(
                                what = "JRuby project replay lost source revision for {}",
                                why = "every registered source has one monotonic revision",
                                fix = "keep source registration and revision capture atomic",
                                file_path.display(),
                            )
                        })
                    };
                    let uri = Url::from_file_path(file_path).map_err(|_| {
                        anyhow!(
                            "JRuby catalog-sensitive project source is not a valid file URI: {}",
                            file_path.display()
                        )
                    })?;
                    let collected = file_processor
                    .collect_project_file_facts_and_jruby_navigation_plan_as_deferred_resolution(
                        &uri,
                        content,
                        semantic_read_engine.clone(),
                        known_namespaces.clone(),
                    )
                    .with_context(|| {
                        format!(
                            "failed to replay JRuby catalog-sensitive project facts for {}",
                            file_path.display()
                        )
                    })?;
                    Ok(Some((
                        file_path.clone(),
                        source_snapshot,
                        collected.analysis,
                        collected.jruby_navigation_plan,
                    )))
                },
            )
            .collect::<Vec<_>>();
        let mut plan = StaticJavaNavigationPlan::default();
        for outcome in outcomes {
            let Some((path, source_snapshot, analysis, file_plan)) = outcome? else {
                continue;
            };
            let committed = file_processor
                .replace_collected_project_file_facts_if_source_snapshot_as_deferred_resolution(
                    &path,
                    &analysis_engine,
                    source_snapshot,
                    analysis,
                );
            if !committed {
                info!(
                    "Discarded JRuby project replay facts collected from a superseded source snapshot: {}",
                    path.display()
                );
                continue;
            }
            plan.signature_class_names
                .extend(file_plan.signature_class_names);
            plan.implementation_class_names
                .extend(file_plan.implementation_class_names);
        }
        let fact_replacement_elapsed = replay_started.elapsed();
        plan.signature_class_names.sort();
        plan.signature_class_names.dedup();
        plan.implementation_class_names.sort();
        plan.implementation_class_names.dedup();
        let signature_classes = plan.signature_class_names.len();
        let implementation_classes = plan.implementation_class_names.len();
        let materialization_started = Instant::now();
        file_processor.materialize_jruby_navigation_plan_as_deferred_resolution(
            plan,
            &analysis_engine,
            known_namespaces,
        )?;
        let materialization_elapsed = materialization_started.elapsed();
        self.file_processor = file_processor;
        self.resolve_open_project_files(ctx, &analysis_engine);
        info!(
            "[PERF][JRuby project replay] project={} files={} fact_replacement={:?} \
             signature_classes={} implementation_classes={} materialization={:?} total={:?}",
            self.workspace_root.display(),
            files.len(),
            fact_replacement_elapsed,
            signature_classes,
            implementation_classes,
            materialization_elapsed,
            replay_started.elapsed()
        );
        Ok(files.len())
    }

    pub(crate) fn discard_jruby_replay_semantic_context(&mut self) {
        let known_namespaces = self.jruby_replay_known_namespaces.take();
        let semantic_engine = self.jruby_replay_analysis_engine.take();
        invariant_eq!(
            known_namespaces.is_some(),
            semantic_engine.is_some(),
            what = "JRuby replay namespace and semantic-engine ownership diverged",
            why = "both snapshots are created and consumed as one generation-owned context",
            fix = "move or discard both fields in the same lifecycle transition",
        );
    }
}
