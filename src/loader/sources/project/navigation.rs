//! Demand-driven navigation fact collection for prioritized project files.

use super::IndexerProject;
use super::ProjectNavigationDemandSelection;
use super::MAX_PROJECT_NAVIGATION_DEMAND_KEYS;
use crate::environment::runtime::jruby::imports::StaticJavaNavigationPlan;
use crate::invariant::ExpectInvariant;
use crate::loader::context::LoadContext;
use crate::server::RubyLanguageServer;
use crate::utils;
use anyhow::Result;
use log::info;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::Instant;

const MAX_PROJECT_NAVIGATION_CANDIDATES_PER_KEY: usize = 8;

const MAX_PROJECT_NAVIGATION_DEMAND_FILES: usize = 64;

pub(super) fn select_navigation_demand_files(
    files: &mut Vec<PathBuf>,
    processed_files: &HashSet<PathBuf>,
    keys: &[String],
) -> ProjectNavigationDemandSelection {
    invariant!(
        keys.len() <= MAX_PROJECT_NAVIGATION_DEMAND_KEYS,
        what = "project demand drain held {} keys, above the maximum of {}",
        why = "the server queue applies backpressure before selection",
        fix = "keep demand admission and drain limits identical",
        keys.len(),
        MAX_PROJECT_NAVIGATION_DEMAND_KEYS,
    );
    let mut selected_paths = HashSet::new();
    let mut completed_keys = Vec::new();
    let mut deferred_keys = Vec::new();
    for key in keys {
        invariant!(
            !key.is_empty() && key.chars().all(char::is_alphanumeric),
            what = "project demand key `{key}` is not a normalized identifier",
            why = "file selection must not reinterpret request text or paths",
            fix = "normalize identifiers at the definition-query boundary",
            key = key,
        );
        let candidates = files
            .iter()
            .filter(|path| project_file_matches_navigation_key(path, key))
            .cloned()
            .collect::<Vec<_>>();
        let candidate_count = candidates.len();
        if candidate_count == 0 {
            if processed_files
                .iter()
                .any(|path| project_file_matches_navigation_key(path, key))
            {
                completed_keys.push(key.clone());
            } else {
                deferred_keys.push(key.clone());
            }
            continue;
        }
        if candidate_count > MAX_PROJECT_NAVIGATION_CANDIDATES_PER_KEY
            || selected_paths.len().saturating_add(candidate_count)
                > MAX_PROJECT_NAVIGATION_DEMAND_FILES
        {
            deferred_keys.push(key.clone());
            continue;
        }
        selected_paths.extend(candidates);
        completed_keys.push(key.clone());
    }

    let mut selected_files = Vec::with_capacity(selected_paths.len());
    files.retain(|path| {
        if selected_paths.contains(path) {
            selected_files.push(path.clone());
            false
        } else {
            true
        }
    });
    ProjectNavigationDemandSelection {
        files: selected_files,
        completed_keys,
        deferred_keys,
    }
}

pub(super) fn prioritize_project_files(
    files: Vec<PathBuf>,
    priority_keys: &HashSet<String>,
) -> (Vec<PathBuf>, usize) {
    let (prioritized, exhaustive): (Vec<_>, Vec<_>) = files.into_iter().partition(|path| {
        priority_keys
            .iter()
            .any(|terminal| project_file_matches_navigation_key(path, terminal))
    });
    let priority_count = prioritized.len();
    (
        prioritized.into_iter().chain(exhaustive).collect(),
        priority_count,
    )
}

pub(super) fn project_file_matches_navigation_key(path: &Path, key: &str) -> bool {
    path.file_stem()
        .map(|stem| {
            stem.to_string_lossy()
                .chars()
                .filter(|character| character.is_alphanumeric())
                .flat_map(char::to_lowercase)
                .collect::<String>()
        })
        .is_some_and(|stem| {
            key == stem
                || (stem.len() >= 4 && key.starts_with(&stem))
                || (key.len() >= 4 && stem.starts_with(key))
        })
}

impl IndexerProject {
    pub(crate) fn collect_project_navigation_facts(
        &mut self,
        ctx: &LoadContext,
        server: &RubyLanguageServer,
    ) -> Result<()> {
        let selection = self.collect_initial_project_navigation_demand_facts(&[], ctx, server)?;
        invariant!(
            selection == ProjectNavigationDemandSelection::default(),
            what = "an empty project-demand frontier produced a non-empty selection",
            why = "demand selection must be driven only by normalized queued keys",
            fix = "inspect the initial project frontier partitioning",
        );
        self.finish_project_navigation_facts(ctx, server)
    }

    pub(crate) fn collect_initial_project_navigation_demand_facts(
        &mut self,
        demand_keys: &[String],
        ctx: &LoadContext,
        server: &RubyLanguageServer,
    ) -> Result<ProjectNavigationDemandSelection> {
        invariant!(
            self.pending_project_navigation_files.is_none()
                && self.pending_project_files.is_none()
                && self.project_navigation_started_at.is_none(),
            what = "navigation frontier started while the previous frontier's files were pending",
            why = "two generations could replace facts in one engine concurrently",
            fix = "complete or discard the prior IndexerProject first",
        );
        let start_time = Instant::now();
        self.project_navigation_started_at = Some(start_time);
        info!(
            "Starting project navigation fact frontier for: {:?}",
            self.workspace_root
        );

        self.clear_dependencies();
        self.jruby_source_hints.clear();
        self.pending_jruby_navigation_plan = StaticJavaNavigationPlan::default();
        self.processed_project_files.clear();
        self.project_file_total = None;
        self.project_file_completed.store(0, Ordering::Relaxed);
        self.exhaustive_known_namespaces = None;
        self.exhaustive_analysis_engine = None;
        self.exhaustive_collection_started = false;
        self.jruby_replay_known_namespaces = None;
        self.jruby_replay_analysis_engine = None;

        let mut project_files = self.collect_project_files()?;
        let all_project_files = project_files.clone();
        let total_files = project_files.len();
        let selection = select_navigation_demand_files(
            &mut project_files,
            &self.processed_project_files,
            demand_keys,
        );
        let (mut ruby_files, priority_file_count) =
            prioritize_project_files(project_files, &self.project_navigation_priority_keys);
        let exhaustive_files = ruby_files.split_off(priority_file_count);
        let signature_files = utils::file_ops::collect_project_signature_files(
            &self.workspace_root,
            &self.indexing_config,
        )?;
        info!(
            "Found {} Ruby files and {} RBS signature files in project; {} demanded source \
             file(s) precede {} active navigation source file(s)",
            total_files,
            signature_files.len(),
            selection.files.len(),
            priority_file_count
        );

        self.begin_project_file_progress(total_files, server);

        self.collect_signature_facts(&signature_files, server);
        self.initialize_project_collection_semantic_context(ctx, server, &all_project_files)?;
        let collection_known_namespaces =
            self.exhaustive_known_namespaces.clone().expect_invariant(
                "project collection baseline has no namespace set after initialization",
                "every project file must use one generation-owned semantic universe",
                "initialize the baseline before collecting the first project batch",
            );
        let collection_analysis_engine = self.exhaustive_analysis_engine.clone().expect_invariant(
            "project collection baseline has no analysis engine after initialization",
            "every project file must use one generation-owned semantic universe",
            "initialize the baseline before collecting the first project batch",
        );
        self.collect_facts_and_track_dependencies(
            &selection.files,
            selection.files.len(),
            ctx,
            server,
            true,
            Some(collection_known_namespaces),
            Some(collection_analysis_engine),
        )?;
        self.record_processed_project_files(&selection.files, server);
        self.pending_project_navigation_files = Some(ruby_files);
        self.pending_project_files = Some(exhaustive_files);

        if !demand_keys.is_empty() {
            info!(
                "[PERF][initial project demand frontier] project={} keys={} files={} elapsed={:?}",
                self.workspace_root.display(),
                demand_keys.len(),
                selection.files.len(),
                start_time.elapsed()
            );
        }
        Ok(selection)
    }

    pub(crate) fn finish_project_navigation_facts(
        &mut self,
        ctx: &LoadContext,
        server: &RubyLanguageServer,
    ) -> Result<()> {
        let start_time = self.project_navigation_started_at.take().expect_invariant(
            "navigation completion started without a matching initial frontier",
            "demand collection and the active frontier are one lifecycle",
            "start the initial demand frontier first",
        );
        let ruby_files = self
            .pending_project_navigation_files
            .take()
            .expect_invariant(
                "navigation completion has no retained active source files",
                "initial selection retains the active-file complement",
                "keep the same IndexerProject between demand collection and completion",
            );
        self.collect_facts_and_track_dependencies(
            &ruby_files,
            ruby_files.len(),
            ctx,
            server,
            true,
            Some(self.exhaustive_known_namespaces.clone().expect_invariant(
                "active project frontier lost the generation-owned namespace baseline",
                "frontier ordering must not change collected facts",
                "retain the baseline through project completion",
            )),
            Some(self.exhaustive_analysis_engine.clone().expect_invariant(
                "active project frontier lost the generation-owned analysis baseline",
                "frontier ordering must not change collected facts",
                "retain the baseline through project completion",
            )),
        )?;
        self.record_processed_project_files(&ruby_files, server);
        self.refresh_exhaustive_semantic_context(server)?;

        info!(
            "Project navigation frontier completed in {:?}. Found {} stdlib deps, {} gem deps",
            start_time.elapsed(),
            self.required_stdlib.lock().len(),
            self.required_gems.lock().len()
        );
        Ok(())
    }

    pub(crate) fn take_navigation_demand_files(
        &mut self,
        keys: &[String],
    ) -> ProjectNavigationDemandSelection {
        invariant!(
            self.pending_project_navigation_files.is_none()
                && self.project_navigation_started_at.is_none()
                && self.exhaustive_known_namespaces.is_some(),
            what = "post-frontier demand selection ran before the active frontier completed",
            why = "promoted tail files need the pre-collection namespace baseline",
            fix = "finish the frontier and keep the baseline before draining demands",
        );
        let pending_files = self.pending_project_files.as_mut().expect_invariant(
            "project demand selection started without an exhaustive project tail",
            "promotion is valid only after the frontier finds the file set",
            "pass the same IndexerProject through the project-source lifecycle",
        );
        select_navigation_demand_files(pending_files, &self.processed_project_files, keys)
    }
}
