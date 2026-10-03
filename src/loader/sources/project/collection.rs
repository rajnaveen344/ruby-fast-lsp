//! Project file discovery, batched fact collection, progress, and dependency tracking.

use super::IndexerProject;
use super::ProjectFileInput;
use super::RegisteredProjectFileInput;
use crate::environment::runtime::jruby::imports::StaticJavaNavigationPlan;
use crate::invariant::ExpectInvariant;
use crate::loader::context::{
    LoadContext, LoadSink, LoadTarget, PathFacts, ProjectSourceCandidate,
};
use crate::loader::file_processor::ProjectFileCollectionTiming;
use anyhow::{anyhow, Context, Result};
use log::{info, warn};
use rayon::prelude::*;
use ruby_analysis::core::{FileAnalysis, FullyQualifiedName};
use ruby_analysis::engine::{Project, ResolveMode, SourceFileSnapshot};
use ruby_fast_lsp_jruby_support::StaticJavaSourceHint;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;
use tower_lsp::lsp_types::Url;

pub(super) fn map_owned_project_inputs<T, R, F>(
    mut inputs: Vec<T>,
    priority_input_count: usize,
    collect: &F,
) -> Vec<R>
where
    T: Send,
    R: Send,
    F: Fn(T) -> R + Send + Sync,
{
    invariant!(
        priority_input_count <= inputs.len(),
        what = "project priority input count {} exceeds the {} owned inputs",
        why = "the priority partition must be selected from the same deterministic source vector",
        fix = "preserve the priority count returned with that vector",
        priority_input_count,
        inputs.len(),
    );
    let exhaustive_inputs = inputs.split_off(priority_input_count);
    let mut outcomes = inputs.into_par_iter().map(collect).collect::<Vec<_>>();
    outcomes.extend(
        exhaustive_inputs
            .into_par_iter()
            .map(collect)
            .collect::<Vec<_>>(),
    );
    outcomes
}

impl IndexerProject {
    /// Collect facts from project files and track dependencies
    pub fn collect_project_facts(&mut self, ctx: &LoadContext) -> Result<()> {
        self.collect_project_navigation_facts(ctx)?;
        self.collect_remaining_project_facts(ctx)
    }

    pub(crate) fn collect_remaining_project_facts(&mut self, ctx: &LoadContext) -> Result<()> {
        invariant!(
            self.pending_project_navigation_files.is_none()
                && self.project_navigation_started_at.is_none(),
            what = "exhaustive project collection started before the active navigation frontier completed",
            why = "exhaustive files require the retained immutable pre-collection namespace baseline",
            fix = "finish the project navigation frontier before collecting its retained tail",
        );
        let files = self.pending_project_files.take().expect_invariant(
            "exhaustive project collection started without a completed navigation frontier",
            "remaining files are the frontier's deterministic complement",
            "call collect_project_navigation_facts first; keep the same IndexerProject",
        );
        let started = Instant::now();
        let known_namespaces = self.exhaustive_known_namespaces.clone().expect_invariant(
            "exhaustive project collection has no pre-collection namespace baseline",
            "all project files read one immutable semantic context",
            "initialize the baseline before the first file; keep it to completion",
        );
        let semantic_context_engine = self.exhaustive_semantic_context.clone().expect_invariant(
            "exhaustive project collection has no immutable pre-collection read engine",
            "project writes must not feed later fact construction",
            "initialize the baseline before the first file; keep it to completion",
        );
        self.exhaustive_collection_started = true;
        self.collect_facts_and_track_dependencies(
            &files,
            0,
            ctx,
            true,
            Some(known_namespaces.clone()),
            Some(semantic_context_engine.clone()),
        )?;
        self.record_processed_project_files(&files, ctx.sink.as_ref());
        self.exhaustive_known_namespaces = None;
        invariant!(
            self.jruby_replay_known_namespaces
                .replace(known_namespaces)
                .is_none(),
            what = "completed project collection replaced an unconsumed JRuby replay namespace snapshot",
            why = "one IndexerProject cannot own semantic context from two generations",
            fix = "replay or discard the completed generation before starting another project pass",
        );
        invariant!(
            self.jruby_replay_semantic_context
                .replace(semantic_context_engine)
                .is_none(),
            what =
                "completed project collection replaced an unconsumed JRuby replay semantic engine",
            why = "one IndexerProject cannot retain read context from two generations",
            fix = "replay or discard the completed generation before starting another project pass",
        );
        self.exhaustive_semantic_context = None;
        info!(
            "Exhaustive project fact collection completed in {:?} for {} file(s). Found {} stdlib \
             deps, {} gem deps",
            started.elapsed(),
            files.len(),
            self.required_stdlib.lock().len(),
            self.required_gems.lock().len()
        );
        Ok(())
    }

    pub(crate) fn take_next_remaining_project_files(&mut self, limit: usize) -> Vec<PathBuf> {
        invariant!(
            limit > 0,
            what = "exhaustive project batch limit is zero",
            why = "a zero-sized batch can never make progress",
            fix = "configure a positive coordinator batch bound",
        );
        let pending_files = self.pending_project_files.as_mut().expect_invariant(
            "an exhaustive project batch was requested without a retained project tail",
            "batches must consume the exact file set discovered by the navigation frontier",
            "retain the same IndexerProject until every deterministic batch is consumed",
        );
        let take = pending_files.len().min(limit);
        pending_files.drain(..take).collect()
    }

    pub(crate) fn remaining_project_file_count(&self) -> usize {
        self.pending_project_files
            .as_ref()
            .expect_invariant(
                "remaining project file count was requested outside the exhaustive project lifecycle",
                "only a retained navigation frontier owns a pending tail",
                "inspect the batch loop's ownership transitions",
            )
            .len()
    }

    pub(crate) fn collect_project_file_batch(
        &mut self,
        files: &[PathBuf],
        ctx: &LoadContext,
        resolve_open_documents: bool,
    ) -> Result<()> {
        if files.is_empty() {
            return Ok(());
        }
        let known_namespaces = self.exhaustive_known_namespaces.clone().expect_invariant(
            "bounded project batch has no pre-collection namespace baseline",
            "every demanded and exhaustive batch belongs to one project generation",
            "keep the baseline until finish_remaining_project_facts",
        );
        let semantic_context_engine = self.exhaustive_semantic_context.clone().expect_invariant(
            "bounded project batch has no immutable pre-collection read engine",
            "demand and batch boundaries must not affect facts",
            "initialize the baseline first; keep it through finish_remaining_project_facts",
        );
        self.exhaustive_collection_started = true;
        self.collect_facts_and_track_dependencies(
            files,
            0,
            ctx,
            resolve_open_documents,
            Some(known_namespaces),
            Some(semantic_context_engine),
        )?;
        self.record_processed_project_files(files, ctx.sink.as_ref());
        Ok(())
    }

    pub(super) fn begin_project_file_progress(&mut self, total_files: usize, sink: &dyn LoadSink) {
        self.project_file_total = Some(u64::try_from(total_files).expect_invariant(
            "project file count exceeds u64",
            "a filesystem cannot contain that many paths",
            "inspect collect_project_files",
        ));
        self.report_project_file_progress(sink);
    }

    fn report_project_file_progress(&self, sink: &dyn LoadSink) {
        let Some(total) = self.project_file_total else {
            return;
        };
        if total == 0 {
            return;
        }
        let completed = u64::try_from(self.processed_project_files.len()).expect_invariant(
            "processed project file count exceeds u64",
            "processed files are a subset of the discovered set",
            "inspect record_processed_project_files",
        );
        self.project_file_completed
            .store(completed, Ordering::Relaxed);
        sink.report_project_progress(
            &self.workspace_root,
            self.project_progress_generation,
            completed,
            total,
        );
    }

    pub(super) fn record_processed_project_files(
        &mut self,
        files: &[PathBuf],
        sink: &dyn LoadSink,
    ) {
        for file in files {
            invariant!(
                self.processed_project_files.insert(file.clone()),
                what = "project source {} was processed twice in one navigation frontier",
                why = "demanded and exhaustive files leave one pending set before indexing",
                fix = "inspect frontier partitioning and demand-file removal",
                file.display(),
            );
        }
        if !files.is_empty() {
            self.report_project_file_progress(sink);
        }
    }

    pub(crate) fn finish_remaining_project_facts(&mut self) {
        let pending = self.pending_project_files.take().expect_invariant(
            "exhaustive project completion has no retained tail",
            "completion must consume the exact frontier-owned file set",
            "call completion once after the bounded batch loop",
        );
        invariant!(
            pending.is_empty(),
            what = "exhaustive project completion left {} source files unprocessed",
            why = "project-navigation readiness cannot be published with omitted project truth",
            fix = "continue the deterministic batch loop until the retained tail is empty",
            pending.len(),
        );
        invariant!(
            self.pending_jruby_navigation_plan
                .signature_class_names
                .is_empty()
                && self
                    .pending_jruby_navigation_plan
                    .implementation_class_names
                    .is_empty(),
            what = "exhaustive project completion retained deferred JRuby navigation inputs",
            why = "the final batch materializes all runtime inputs before readiness",
            fix = "mark the last batch as a navigation-resolution boundary",
        );
        let known_namespaces = self.exhaustive_known_namespaces.take().expect_invariant(
            "exhaustive project completion has no pre-collection namespace baseline",
            "the baseline and pending tail have one lifecycle",
            "retain both until the deterministic batch loop finishes",
        );
        let semantic_context_engine = self.exhaustive_semantic_context.take().expect_invariant(
            "exhaustive project completion has no immutable semantic read engine",
            "the context and pending tail have one lifecycle",
            "retain both until every deterministic batch is consumed",
        );
        invariant!(
            self.jruby_replay_known_namespaces
                .replace(known_namespaces)
                .is_none(),
            what = "project completion replaced an unconsumed JRuby replay namespace snapshot",
            why = "an IndexerProject cannot mix two indexing generations",
            fix = "finish the prior replay before completing another project pass",
        );
        invariant!(
            self.jruby_replay_semantic_context
                .replace(semantic_context_engine)
                .is_none(),
            what = "project completion replaced an unconsumed JRuby replay semantic engine",
            why = "an IndexerProject cannot mix two indexing generations",
            fix = "finish the prior replay before completing another project pass",
        );
    }

    pub(super) fn collect_signature_facts(&self, files: &[PathBuf], sink: &dyn LoadSink) {
        files.par_iter().for_each(|path| {
            let content = match std::fs::read_to_string(path) {
                Ok(content) => content,
                Err(error) => {
                    warn!("Failed to read RBS signature {:?}: {}", path, error);
                    return;
                }
            };
            let Ok(uri) = Url::from_file_path(path) else {
                warn!("Failed to convert RBS signature path to URI: {:?}", path);
                return;
            };
            if let Err(error) = self
                .file_processor
                .collect_rbs_facts_as_deferred_resolution(&uri, &content, sink)
            {
                warn!(
                    "Failed to collect RBS signature facts {:?}: {}",
                    path, error
                );
            }
        });
    }

    /// Collect all Ruby files in the project
    pub(super) fn collect_project_files(&self) -> Result<Vec<PathBuf>> {
        super::files::collect_project_files(&self.workspace_root, &self.indexing_config)
    }

    /// Collect facts from files and track their dependencies (Parallelized with rayon)
    pub(super) fn collect_facts_and_track_dependencies(
        &mut self,
        files: &[PathBuf],
        priority_file_count: usize,
        ctx: &LoadContext,
        resolve_open_documents: bool,
        known_namespaces: Option<Arc<HashSet<FullyQualifiedName>>>,
        semantic_context_engine: Option<Arc<parking_lot::RwLock<Project>>>,
    ) -> Result<()> {
        info!("Collecting facts in one parallel pass");

        let file_processor = self.file_processor.clone();
        let providerless_collection = file_processor.jruby_import_provider().is_none();
        let required_stdlib = self.required_stdlib.clone();
        let required_gems = self.required_gems.clone();

        let file_processor_ref = &file_processor;
        let required_stdlib_ref = &required_stdlib;
        let required_gems_ref = &required_gems;
        let project_uri = Url::from_directory_path(&self.workspace_root).map_err(|_| {
            anyhow::anyhow!(
                "Project root is not a valid file URI: {}",
                self.workspace_root.display()
            )
        })?;
        let analysis_engine = ctx.sink.target_for_uri(&project_uri);
        let uses_immutable_semantic_context = semantic_context_engine.is_some();
        let base_semantic_read_engine: Arc<dyn LoadTarget> = match &semantic_context_engine {
            Some(engine) => engine.clone(),
            None => analysis_engine.clone(),
        };

        let collect_start = Instant::now();
        let read_file = |file_path: &PathBuf| -> Result<ProjectFileInput> {
            let expected_snapshot =
                analysis_engine.view(|view| view.source_snapshot_for_path(file_path));
            let read_started = Instant::now();
            let (content, open_document) = Self::read_authoritative_project_source(ctx, file_path)?;
            let read_elapsed = read_started.elapsed();
            let dependency_started = Instant::now();
            Self::extract_and_track_dependencies(&content, required_stdlib_ref, required_gems_ref);
            let dependency_elapsed = dependency_started.elapsed();
            Ok(ProjectFileInput {
                path: file_path.clone(),
                content,
                read_elapsed,
                dependency_elapsed,
                expected_snapshot,
                open_document,
            })
        };
        let mut input_results = files[..priority_file_count]
            .par_iter()
            .map(&read_file)
            .collect::<Vec<_>>();
        input_results.extend(
            files[priority_file_count..]
                .par_iter()
                .map(&read_file)
                .collect::<Vec<_>>(),
        );
        let inputs = input_results.into_iter().collect::<Result<Vec<_>>>()?;

        if !uses_immutable_semantic_context {
            if let Some(input) = inputs.first() {
                let path = &input.path;
                let uri = Url::from_file_path(path).map_err(|_| {
                    anyhow!(
                        "project source path is not a valid file URI: {}",
                        path.display()
                    )
                })?;
                file_processor_ref.ensure_project_semantic_seed(&uri, &analysis_engine);
            }
        }
        let batch_registration_started = Instant::now();
        let mut registered_inputs = Vec::with_capacity(inputs.len());
        let mut registered_priority_file_count = 0usize;
        // In an immutable semantic context every registration is mirrored
        // into the context so retained ranges keep one file identity.
        let candidates = inputs
            .iter()
            .map(|input| ProjectSourceCandidate {
                path: &input.path,
                content: &input.content,
                open_document: input.open_document,
                expected_snapshot: input.expected_snapshot,
            })
            .collect::<Vec<_>>();
        let snapshots = analysis_engine
            .register_project_sources_if_snapshot(&candidates, semantic_context_engine.as_deref());
        drop(candidates);
        for (index, (input, source_snapshot)) in inputs.into_iter().zip(snapshots).enumerate() {
            let Some(source_snapshot) = source_snapshot else {
                continue;
            };
            registered_priority_file_count += usize::from(index < priority_file_count);
            registered_inputs.push(RegisteredProjectFileInput {
                input,
                source_snapshot,
            });
        }
        let batch_registration_elapsed = batch_registration_started.elapsed();

        // Project collection must be independent of whether an identical
        // document was indexed interactively before the cold pass. The live
        // engine may already contain facts for one or more input files; using
        // those facts while rebuilding the same files creates a second-pass
        // fixed point that a clean cold index cannot observe. Sanitize the
        // whole batch in one bounded snapshot so parallel workers share the
        // same file-order-independent semantic universe.
        let (semantic_read_engine, reads_base_engine): (Arc<dyn LoadTarget>, bool) =
            if uses_immutable_semantic_context {
                // This generation-owned context was sanitized before the project
                // skeleton was installed. Clearing one batch here would remove
                // exact cross-batch inheritance and mixin edges and restore
                // traversal-order-dependent extension dispatch.
                (base_semantic_read_engine.clone(), true)
            } else {
                let stale = base_semantic_read_engine.snapshot_with(|view| {
                    let stale_file_ids = registered_inputs
                        .iter()
                        .filter_map(|registered| {
                            let file_id = view.file_id(&registered.input.path)?;
                            view.semantic_export_fingerprint(file_id).map(|_| file_id)
                        })
                        .collect::<Vec<_>>();
                    (!stale_file_ids.is_empty()).then_some(stale_file_ids)
                });
                match stale {
                    None => (base_semantic_read_engine.clone(), true),
                    Some((mut snapshot, stale_file_ids)) => {
                        // These files still exist and are about to be collected;
                        // only their prior facts are withheld. They keep their
                        // identity because the seed below addresses them by
                        // path, so this is an empty update rather than a removal.
                        for file_id in stale_file_ids {
                            snapshot.update(
                                file_id,
                                FileAnalysis::default(),
                                ResolveMode::Deferred,
                            );
                        }
                        (Arc::new(parking_lot::RwLock::new(snapshot)), false)
                    }
                }
            };
        let baseline_known_namespaces = if reads_base_engine {
            known_namespaces.unwrap_or_else(|| {
                Arc::new(semantic_read_engine.view(|view| view.known_namespace_fqns()))
            })
        } else {
            Arc::new(semantic_read_engine.view(|view| view.known_namespace_fqns()))
        };

        // Before body inference, publish the same direct declaration skeleton
        // for every worker. Value-constant receiver and block types must not
        // depend on file traversal, editor-open order, or enabled extensions.
        let requires_direct_semantic_seed =
            !uses_immutable_semantic_context && !registered_inputs.is_empty();
        let semantic_seed_started = Instant::now();
        if requires_direct_semantic_seed {
            let semantic_seed_outcomes = registered_inputs
                .par_iter()
                .map(|registered| -> Result<(PathBuf, Option<FileAnalysis>)> {
                    let uri = Url::from_file_path(&registered.input.path).map_err(|_| {
                        anyhow!(
                            "project source path is not a valid file URI: {}",
                            registered.input.path.display()
                        )
                    })?;
                    Ok((
                        registered.input.path.clone(),
                        file_processor_ref.collect_project_direct_semantic_seed(
                            &uri,
                            &registered.input.content,
                            &semantic_read_engine,
                            baseline_known_namespaces.as_ref(),
                        ),
                    ))
                })
                .collect::<Vec<_>>();
            // Commit the seeds that precede the first failure, and resolve
            // only a complete batch that seeded at least one file.
            let mut seed_paths = Vec::with_capacity(semantic_seed_outcomes.len());
            let mut seed_facts = Vec::with_capacity(semantic_seed_outcomes.len());
            let mut failure = None;
            for outcome in semantic_seed_outcomes {
                match outcome {
                    Ok((path, Some(facts))) => {
                        seed_paths.push(path);
                        seed_facts.push(PathFacts::Facts(facts));
                    }
                    Ok((_, None)) => {}
                    Err(error) => {
                        failure = Some(error);
                        break;
                    }
                }
            }
            let resolve = failure.is_none() && !seed_paths.is_empty();
            semantic_read_engine.replace_facts_by_path(
                seed_paths.iter().map(PathBuf::as_path).zip(seed_facts),
                resolve,
            );
            if let Some(error) = failure {
                return Err(error);
            }
        }
        let semantic_seed_elapsed = requires_direct_semantic_seed
            .then(|| semantic_seed_started.elapsed())
            .unwrap_or_default();
        let known_namespaces = if requires_direct_semantic_seed {
            Arc::new(semantic_read_engine.view(|view| view.known_namespace_fqns()))
        } else {
            baseline_known_namespaces
        };

        let project_file_completed = &self.project_file_completed;
        let project_file_total = self.project_file_total;
        let project_progress_generation = self.project_progress_generation;
        let workspace_root = &self.workspace_root;
        let collect_file = |registered: RegisteredProjectFileInput| -> Result<(
            PathBuf,
            SourceFileSnapshot,
            FileAnalysis,
            StaticJavaNavigationPlan,
            StaticJavaSourceHint,
            std::time::Duration,
            std::time::Duration,
            ProjectFileCollectionTiming,
        )> {
            let ProjectFileInput {
                path: file_path,
                content,
                read_elapsed,
                dependency_elapsed,
                expected_snapshot: _,
                open_document: _,
            } = registered.input;
            let uri = Url::from_file_path(&file_path).map_err(|_| {
                anyhow!(
                    "project source path is not a valid file URI: {}",
                    file_path.display()
                )
            })?;
            let collected = file_processor_ref
                .collect_project_file_facts_and_jruby_navigation_plan_as_deferred_resolution(
                    &uri,
                    content,
                    semantic_read_engine.clone(),
                    known_namespaces.clone(),
                )
                .with_context(|| {
                    format!(
                        "failed to collect project facts for {}",
                        file_path.display()
                    )
                })?;
            if let Some(total) = project_file_total {
                let completed = project_file_completed.fetch_add(1, Ordering::Relaxed) + 1;
                ctx.sink.report_project_progress(
                    workspace_root,
                    project_progress_generation,
                    completed,
                    total,
                );
            }
            Ok((
                file_path.clone(),
                registered.source_snapshot,
                collected.analysis,
                collected.jruby_navigation_plan,
                collected.jruby_source_hint,
                read_elapsed,
                dependency_elapsed,
                collected.timing,
            ))
        };
        let outcomes = map_owned_project_inputs(
            registered_inputs,
            registered_priority_file_count,
            &collect_file,
        );
        let mut jruby_navigation_plan = StaticJavaNavigationPlan::default();
        let mut jruby_source_hints = Vec::with_capacity(outcomes.len());
        let mut read_cpu = std::time::Duration::ZERO;
        let mut dependency_scan_cpu = std::time::Duration::ZERO;
        let mut timing = ProjectFileCollectionTiming::default();
        timing.semantic_seed += semantic_seed_elapsed;
        timing.total += semantic_seed_elapsed;
        timing.total += batch_registration_elapsed;
        timing.registration += batch_registration_elapsed;
        for outcome in outcomes {
            let (path, source_snapshot, analysis, plan, hint, read, dependency_scan, file_timing) =
                outcome?;
            #[cfg(test)]
            ctx.sink.test_schedule().checkpoint_blocking(
                crate::loader::scheduling::test_schedule::Point::ProjectFactsCollected,
                &path,
            );
            let replacement_started = Instant::now();
            let committed = file_processor_ref
                .replace_collected_project_file_facts_if_source_snapshot_as_deferred_resolution(
                    &path,
                    &analysis_engine,
                    source_snapshot,
                    analysis,
                );
            #[cfg(test)]
            ctx.sink.test_schedule().checkpoint_blocking(
                crate::loader::scheduling::test_schedule::Point::ProjectCommitAttempted,
                &path,
            );
            if !committed {
                info!(
                    "Discarded project facts collected from a superseded source snapshot: {}",
                    path.display()
                );
            }
            let replacement_elapsed = replacement_started.elapsed();
            if providerless_collection {
                jruby_source_hints.push((path, hint));
            }
            jruby_navigation_plan
                .signature_class_names
                .extend(plan.signature_class_names);
            jruby_navigation_plan
                .implementation_class_names
                .extend(plan.implementation_class_names);
            read_cpu += read;
            dependency_scan_cpu += dependency_scan;
            timing.total += file_timing.total;
            timing.registration += file_timing.registration;
            timing.parse += file_timing.parse;
            timing.jruby_plan += file_timing.jruby_plan;
            timing.semantic_seed += file_timing.semantic_seed;
            timing.visitor += file_timing.visitor;
            timing.assembly += file_timing.assembly;
            timing.replacement += file_timing.replacement + replacement_elapsed;
            timing.total += replacement_elapsed;
        }
        jruby_navigation_plan.signature_class_names.sort();
        jruby_navigation_plan.signature_class_names.dedup();
        jruby_navigation_plan.implementation_class_names.sort();
        jruby_navigation_plan.implementation_class_names.dedup();
        self.pending_jruby_navigation_plan
            .signature_class_names
            .extend(jruby_navigation_plan.signature_class_names);
        self.pending_jruby_navigation_plan
            .implementation_class_names
            .extend(jruby_navigation_plan.implementation_class_names);
        if resolve_open_documents {
            self.pending_jruby_navigation_plan
                .signature_class_names
                .sort();
            self.pending_jruby_navigation_plan
                .signature_class_names
                .dedup();
            self.pending_jruby_navigation_plan
                .implementation_class_names
                .sort();
            self.pending_jruby_navigation_plan
                .implementation_class_names
                .dedup();
            let navigation_plan = std::mem::take(&mut self.pending_jruby_navigation_plan);
            file_processor_ref.materialize_jruby_navigation_plan_as_deferred_resolution(
                navigation_plan,
                &analysis_engine,
                known_namespaces.clone(),
            )?;
        }
        jruby_source_hints.sort_by(|left, right| left.0.cmp(&right.0));
        self.jruby_source_hints.extend(jruby_source_hints);
        self.jruby_source_hints
            .sort_by(|left, right| left.0.cmp(&right.0));
        self.jruby_source_hints
            .dedup_by(|left, right| left.0 == right.0);
        let collect_elapsed = collect_start.elapsed();
        info!(
            "Project parallel file fact pass completed in {:?}; {} active-target file(s) were \
             completed before the exhaustive pass",
            collect_elapsed, priority_file_count
        );
        if !files.is_empty() {
            info!(
                "[PERF][project file CPU] files={} total={:?} read={:?} dependency_scan={:?} \
                 registration={:?} parse={:?} jruby_plan={:?} semantic_seed={:?} visitor={:?} \
                 assembly={:?} replacement={:?}",
                files.len(),
                timing.total,
                read_cpu,
                dependency_scan_cpu,
                timing.registration,
                timing.parse,
                timing.jruby_plan,
                timing.semantic_seed,
                timing.visitor,
                timing.assembly,
                timing.replacement
            );
        }

        if resolve_open_documents {
            self.resolve_open_project_files(ctx, &analysis_engine);
        }

        Ok(())
    }
}
