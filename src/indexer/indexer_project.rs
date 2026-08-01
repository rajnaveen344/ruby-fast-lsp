use crate::config::IndexingConfig;
use crate::indexer::file_processor::{FileProcessor, ProjectFileCollectionTiming};
use crate::runtime::jruby::imports::{
    JrubyImportProvider, StaticJavaNavigationPlan, StaticJavaSourceHint,
};
use crate::server::RubyLanguageServer;
use crate::utils;
use anyhow::{anyhow, Context, Result};
use log::{info, warn};
use parking_lot::Mutex;
use rayon::prelude::*;
use ruby_analysis::core::{FullyQualifiedName, SourceKind};
use ruby_analysis::engine::AnalysisEngine;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tower_lsp::lsp_types::Url;

pub(crate) const MAX_PROJECT_NAVIGATION_DEMAND_KEYS: usize = 16;
const MAX_PROJECT_NAVIGATION_CANDIDATES_PER_KEY: usize = 8;
const MAX_PROJECT_NAVIGATION_DEMAND_FILES: usize = 64;
const MAX_EXHAUSTIVE_SEMANTIC_CONTEXT_BYTES: usize = 128 * 1024 * 1024;

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct ProjectNavigationDemandSelection {
    pub(crate) files: Vec<PathBuf>,
    pub(crate) completed_keys: Vec<String>,
    pub(crate) deferred_keys: Vec<String>,
}

fn map_owned_project_inputs<T, R, F>(
    mut inputs: Vec<T>,
    priority_input_count: usize,
    collect: &F,
) -> Vec<R>
where
    T: Send,
    R: Send,
    F: Fn(T) -> R + Send + Sync,
{
    assert!(
        priority_input_count <= inputs.len(),
        "INVARIANT VIOLATED: project priority input count {} exceeds the {} owned inputs. This is a bug because the priority partition must be selected from the same deterministic source vector. Fix: preserve the priority count returned with that vector.",
        priority_input_count,
        inputs.len()
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

/// Handles project-specific indexing and tracks required stdlib and gems
pub struct IndexerProject {
    workspace_root: PathBuf,
    file_processor: FileProcessor,
    required_stdlib: Arc<Mutex<HashSet<String>>>,
    required_gems: Arc<Mutex<HashSet<String>>>,
    indexing_config: IndexingConfig,
    jruby_source_hints: Vec<(PathBuf, StaticJavaSourceHint)>,
    pending_jruby_navigation_plan: StaticJavaNavigationPlan,
    project_navigation_priority_keys: HashSet<String>,
    dependency_navigation_priority_keys: HashSet<String>,
    pending_project_navigation_files: Option<Vec<PathBuf>>,
    pending_project_files: Option<Vec<PathBuf>>,
    processed_project_files: HashSet<PathBuf>,
    exhaustive_known_namespaces: Option<Arc<HashSet<FullyQualifiedName>>>,
    exhaustive_analysis_engine: Option<Arc<parking_lot::RwLock<AnalysisEngine>>>,
    exhaustive_collection_started: bool,
    jruby_replay_known_namespaces: Option<Arc<HashSet<FullyQualifiedName>>>,
    jruby_replay_analysis_engine: Option<Arc<parking_lot::RwLock<AnalysisEngine>>>,
    project_navigation_started_at: Option<Instant>,
}

impl IndexerProject {
    pub fn new(
        workspace_root: PathBuf,
        file_processor: FileProcessor,
        indexing_config: IndexingConfig,
    ) -> Self {
        Self {
            workspace_root,
            file_processor,
            required_stdlib: Arc::new(Mutex::new(HashSet::new())),
            required_gems: Arc::new(Mutex::new(HashSet::new())),
            indexing_config,
            jruby_source_hints: Vec::new(),
            pending_jruby_navigation_plan: StaticJavaNavigationPlan::default(),
            project_navigation_priority_keys: HashSet::new(),
            dependency_navigation_priority_keys: HashSet::new(),
            pending_project_navigation_files: None,
            pending_project_files: None,
            processed_project_files: HashSet::new(),
            exhaustive_known_namespaces: None,
            exhaustive_analysis_engine: None,
            exhaustive_collection_started: false,
            jruby_replay_known_namespaces: None,
            jruby_replay_analysis_engine: None,
            project_navigation_started_at: None,
        }
    }

    pub(crate) fn set_navigation_priority_keys(
        &mut self,
        project_priority_keys: HashSet<String>,
        dependency_priority_keys: HashSet<String>,
    ) {
        self.project_navigation_priority_keys = project_priority_keys;
        self.dependency_navigation_priority_keys = dependency_priority_keys;
    }

    pub(crate) fn dependency_navigation_priority_keys(&self) -> HashSet<String> {
        self.dependency_navigation_priority_keys.clone()
    }

    pub(crate) fn install_jruby_import_provider(&mut self, provider: Arc<JrubyImportProvider>) {
        assert!(
            self.pending_project_files.is_some(),
            "INVARIANT VIOLATED: the exact JRuby provider was installed outside the retained exhaustive project lifecycle. This is a bug because a generation-local handoff may occur only between bounded batches while the same pending tail and immutable semantic context are owned. Fix: install the provider after the active frontier and before finish_remaining_project_facts consumes the tail."
        );
        assert!(
            self.file_processor.jruby_import_provider().is_none(),
            "INVARIANT VIOLATED: one project indexing generation installed its exact JRuby provider twice. This is a bug because runtime identity is immutable within a generation. Fix: cancel and replace the generation before changing its provider."
        );
        self.file_processor = self
            .file_processor
            .clone()
            .with_jruby_import_provider(provider);
    }

    pub(crate) fn processed_navigation_priority_keys(&self) -> Vec<String> {
        let mut keys = self
            .project_navigation_priority_keys
            .iter()
            .filter(|key| {
                self.processed_project_files
                    .iter()
                    .any(|path| project_file_matches_navigation_key(path, key))
            })
            .cloned()
            .collect::<Vec<_>>();
        keys.sort();
        keys
    }

    /// Collect facts from project files and track dependencies
    pub fn collect_project_facts(&mut self, server: &RubyLanguageServer) -> Result<()> {
        self.collect_project_navigation_facts(server)?;
        self.collect_remaining_project_facts(server)
    }

    pub(crate) fn collect_project_navigation_facts(
        &mut self,
        server: &RubyLanguageServer,
    ) -> Result<()> {
        let selection = self.collect_initial_project_navigation_demand_facts(&[], server)?;
        assert!(
            selection == ProjectNavigationDemandSelection::default(),
            "INVARIANT VIOLATED: an empty project-demand frontier produced a non-empty selection. \
             This is a bug because demand selection must be driven only by normalized queued \
             keys. Fix: inspect the initial project frontier partitioning."
        );
        self.finish_project_navigation_facts(server)
    }

    pub(crate) fn collect_initial_project_navigation_demand_facts(
        &mut self,
        demand_keys: &[String],
        server: &RubyLanguageServer,
    ) -> Result<ProjectNavigationDemandSelection> {
        assert!(
            self.pending_project_navigation_files.is_none()
                && self.pending_project_files.is_none()
                && self.project_navigation_started_at.is_none(),
            "INVARIANT VIOLATED: a project navigation frontier started while exhaustive source \
             files from the previous frontier remained pending. This is a bug because two \
             generations could replace facts in the same isolated engine concurrently. Fix: \
             complete or discard the prior IndexerProject before starting a new frontier."
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
        self.exhaustive_known_namespaces = None;
        self.exhaustive_analysis_engine = None;
        self.exhaustive_collection_started = false;
        self.jruby_replay_known_namespaces = None;
        self.jruby_replay_analysis_engine = None;

        let mut project_files = self.collect_project_files()?;
        let total_files = project_files.len();
        let selection = select_navigation_demand_files(
            &mut project_files,
            &self.processed_project_files,
            demand_keys,
        );
        let (mut ruby_files, priority_file_count) =
            prioritize_project_files(project_files, &self.project_navigation_priority_keys);
        let exhaustive_files = ruby_files.split_off(priority_file_count);
        let signature_files =
            utils::collect_project_signature_files(&self.workspace_root, &self.indexing_config)?;
        info!(
            "Found {} Ruby files and {} RBS signature files in project; {} demanded source \
             file(s) precede {} active navigation source file(s)",
            total_files,
            signature_files.len(),
            selection.files.len(),
            priority_file_count
        );

        self.collect_signature_facts(&signature_files, server);
        self.collect_facts_and_track_dependencies(
            &selection.files,
            selection.files.len(),
            server,
            true,
            None,
            None,
        )?;
        self.record_processed_project_files(&selection.files);
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
        server: &RubyLanguageServer,
    ) -> Result<()> {
        let start_time = self.project_navigation_started_at.take().expect(
            "INVARIANT VIOLATED: project navigation completion started without a matching \
             initial frontier. This is a coordinator bug because queued demand collection and \
             the remaining active frontier are one generation-owned lifecycle. Fix: start the \
             initial project demand frontier before completing active project candidates.",
        );
        let ruby_files = self.pending_project_navigation_files.take().expect(
            "INVARIANT VIOLATED: project navigation completion has no retained active source \
             files. This is a coordinator bug because initial demand selection must retain the \
             deterministic active-file complement. Fix: preserve the same IndexerProject \
             between initial demand collection and frontier completion.",
        );
        self.collect_facts_and_track_dependencies(
            &ruby_files,
            ruby_files.len(),
            server,
            true,
            None,
            None,
        )?;
        self.record_processed_project_files(&ruby_files);
        self.refresh_exhaustive_semantic_context(server)?;

        info!(
            "Project navigation frontier completed in {:?}. Found {} stdlib deps, {} gem deps",
            start_time.elapsed(),
            self.required_stdlib.lock().len(),
            self.required_gems.lock().len()
        );
        Ok(())
    }

    pub(crate) fn collect_remaining_project_facts(
        &mut self,
        server: &RubyLanguageServer,
    ) -> Result<()> {
        assert!(
            self.pending_project_navigation_files.is_none()
                && self.project_navigation_started_at.is_none(),
            "INVARIANT VIOLATED: exhaustive project collection started before the active \
             navigation frontier completed. This is a coordinator bug because exhaustive files \
             require the immutable post-frontier namespace snapshot. Fix: finish the project \
             navigation frontier before collecting its retained tail."
        );
        let files = self.pending_project_files.take().expect(
            "INVARIANT VIOLATED: exhaustive project fact collection started without a completed \
             navigation frontier. This is a bug because the remaining file set must be the exact \
             deterministic complement discovered by that frontier. Fix: call \
             collect_project_navigation_facts first and retain the same IndexerProject.",
        );
        let started = Instant::now();
        let known_namespaces = self.exhaustive_known_namespaces.clone().expect(
            "INVARIANT VIOLATED: exhaustive project collection has no post-frontier namespace \
             snapshot. This is a bug because every tail file must be collected against one \
             immutable semantic context. Fix: capture the snapshot after the priority frontier \
             and retain it through completion.",
        );
        let semantic_context_engine = self.exhaustive_analysis_engine.clone().expect(
            "INVARIANT VIOLATED: exhaustive project collection has no immutable semantic read engine. This is a bug because arbitrary batch writes must never become inputs to later fact construction. Fix: capture the post-frontier semantic context before collecting the project tail.",
        );
        self.exhaustive_collection_started = true;
        self.collect_facts_and_track_dependencies(
            &files,
            0,
            server,
            true,
            Some(known_namespaces.clone()),
            Some(semantic_context_engine.clone()),
        )?;
        self.record_processed_project_files(&files);
        self.exhaustive_known_namespaces = None;
        assert!(
            self.jruby_replay_known_namespaces
                .replace(known_namespaces)
                .is_none(),
            "INVARIANT VIOLATED: completed project collection replaced an unconsumed JRuby \
             replay namespace snapshot. This is a bug because one IndexerProject cannot own \
             semantic context from two generations. Fix: replay or discard the completed \
             generation before starting another project pass."
        );
        assert!(
            self.jruby_replay_analysis_engine
                .replace(semantic_context_engine)
                .is_none(),
            "INVARIANT VIOLATED: completed project collection replaced an unconsumed JRuby replay semantic engine. This is a bug because one IndexerProject cannot retain read context from two generations. Fix: replay or discard the completed generation before starting another project pass."
        );
        self.exhaustive_analysis_engine = None;
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

    pub(crate) fn take_navigation_demand_files(
        &mut self,
        keys: &[String],
    ) -> ProjectNavigationDemandSelection {
        assert!(
            self.pending_project_navigation_files.is_none()
                && self.project_navigation_started_at.is_none()
                && self.exhaustive_known_namespaces.is_some(),
            "INVARIANT VIOLATED: post-frontier navigation demand selection ran before the active \
             project frontier completed. This is a coordinator bug because promoted tail files \
             require the immutable post-frontier namespace snapshot. Fix: finish the active \
             frontier before draining newly queued project demands."
        );
        let pending_files = self.pending_project_files.as_mut().expect(
            "INVARIANT VIOLATED: project navigation demand selection started without an \
             exhaustive project tail. This is a coordinator bug because request-driven \
             promotion is valid only after the deterministic project frontier discovers the \
             exact file set. Fix: retain and pass the same IndexerProject through the complete \
             project-source lifecycle.",
        );
        select_navigation_demand_files(pending_files, &self.processed_project_files, keys)
    }

    pub(crate) fn take_next_remaining_project_files(&mut self, limit: usize) -> Vec<PathBuf> {
        assert!(
            limit > 0,
            "INVARIANT VIOLATED: exhaustive project batch limit is zero. This is a bug because \
             a zero-sized batch can never make progress. Fix: configure a positive coordinator \
             batch bound."
        );
        let pending_files = self.pending_project_files.as_mut().expect(
            "INVARIANT VIOLATED: an exhaustive project batch was requested without a retained \
             project tail. This is a coordinator bug because batches must consume the exact file \
             set discovered by the navigation frontier. Fix: retain the same IndexerProject \
             until every deterministic batch is consumed.",
        );
        let take = pending_files.len().min(limit);
        pending_files.drain(..take).collect()
    }

    pub(crate) fn remaining_project_file_count(&self) -> usize {
        self.pending_project_files
            .as_ref()
            .expect(
                "INVARIANT VIOLATED: remaining project file count was requested outside the \
                 exhaustive project lifecycle. This is a coordinator bug because only a retained \
                 navigation frontier owns a pending tail. Fix: inspect the batch loop's \
                 ownership transitions.",
            )
            .len()
    }

    pub(crate) fn collect_project_file_batch(
        &mut self,
        files: &[PathBuf],
        server: &RubyLanguageServer,
        resolve_open_documents: bool,
    ) -> Result<()> {
        if files.is_empty() {
            return Ok(());
        }
        let known_namespaces = self.exhaustive_known_namespaces.clone().expect(
            "INVARIANT VIOLATED: bounded project batch has no post-frontier namespace snapshot. \
             This is a coordinator bug because every demanded and exhaustive batch belongs to \
             one retained frontier. Fix: keep the snapshot until finish_remaining_project_facts.",
        );
        let semantic_context_engine = self.exhaustive_analysis_engine.clone().expect(
            "INVARIANT VIOLATED: bounded project batch has no immutable semantic read engine. This is a coordinator bug because batch boundaries must not affect fact construction. Fix: capture and retain one post-frontier engine through finish_remaining_project_facts.",
        );
        self.exhaustive_collection_started = true;
        self.collect_facts_and_track_dependencies(
            files,
            0,
            server,
            resolve_open_documents,
            Some(known_namespaces),
            Some(semantic_context_engine),
        )?;
        self.record_processed_project_files(files);
        Ok(())
    }

    fn record_processed_project_files(&mut self, files: &[PathBuf]) {
        for file in files {
            assert!(
                self.processed_project_files.insert(file.clone()),
                "INVARIANT VIOLATED: project source {} was processed twice in one navigation \
                 frontier. This is a bug because demanded and exhaustive files must be removed \
                 from one deterministic pending set before indexing. Fix: inspect frontier \
                 partitioning and demand-file removal.",
                file.display()
            );
        }
    }

    pub(crate) fn finish_remaining_project_facts(&mut self) {
        let pending = self.pending_project_files.take().expect(
            "INVARIANT VIOLATED: exhaustive project completion has no retained tail. This is a \
             coordinator bug because completion must consume the exact frontier-owned file set. \
             Fix: call completion once after the bounded batch loop.",
        );
        assert!(
            pending.is_empty(),
            "INVARIANT VIOLATED: exhaustive project completion left {} source files unprocessed. \
             This is a coordinator bug because project-navigation readiness cannot be published \
             with omitted project truth. Fix: continue the deterministic batch loop until the \
             retained tail is empty.",
            pending.len()
        );
        assert!(
            self.pending_jruby_navigation_plan
                .signature_class_names
                .is_empty()
                && self
                    .pending_jruby_navigation_plan
                    .implementation_class_names
                    .is_empty(),
            "INVARIANT VIOLATED: exhaustive project completion retained deferred JRuby \
             navigation inputs. This is a coordinator bug because the final project batch must \
             materialize every accumulated runtime input before project readiness. Fix: mark the \
             last bounded batch as a navigation-resolution boundary."
        );
        let known_namespaces = self.exhaustive_known_namespaces.take().expect(
            "INVARIANT VIOLATED: exhaustive project completion has no post-frontier namespace \
             snapshot. This is a coordinator bug because the snapshot and pending tail have one \
             lifecycle. Fix: retain both until the deterministic batch loop finishes.",
        );
        let semantic_context_engine = self.exhaustive_analysis_engine.take().expect(
            "INVARIANT VIOLATED: exhaustive project completion has no immutable semantic read engine. This is a coordinator bug because the context and pending tail have one lifecycle. Fix: retain both until every deterministic batch is consumed.",
        );
        assert!(
            self.jruby_replay_known_namespaces
                .replace(known_namespaces)
                .is_none(),
            "INVARIANT VIOLATED: bounded project completion replaced an unconsumed JRuby replay \
             namespace snapshot. This is a bug because one IndexerProject cannot mix semantic \
             context from two indexing generations. Fix: finish the prior generation's replay \
             lifecycle before completing another bounded project pass."
        );
        assert!(
            self.jruby_replay_analysis_engine
                .replace(semantic_context_engine)
                .is_none(),
            "INVARIANT VIOLATED: bounded project completion replaced an unconsumed JRuby replay semantic engine. This is a bug because one IndexerProject cannot mix read context from two indexing generations. Fix: finish the prior generation's replay lifecycle before completing another bounded project pass."
        );
    }

    fn collect_signature_facts(&self, files: &[PathBuf], server: &RubyLanguageServer) {
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
                .collect_rbs_facts_as_deferred_resolution(&uri, &content, server)
            {
                warn!(
                    "Failed to collect RBS signature facts {:?}: {}",
                    path, error
                );
            }
        });
    }

    /// Collect all Ruby files in the project
    fn collect_project_files(&self) -> Result<Vec<PathBuf>> {
        utils::collect_project_files(&self.workspace_root, &self.indexing_config)
    }

    /// Quick scan for dependencies without full indexing.
    /// This reads project files and extracts require/gem statements to determine
    /// which gems and stdlib modules are needed.
    pub fn scan_for_dependencies(&self) -> Result<()> {
        info!("Scanning project files for dependencies...");
        self.clear_dependencies();

        let ruby_files = self.collect_project_files()?;
        let required_stdlib = &self.required_stdlib;
        let required_gems = &self.required_gems;

        ruby_files.par_iter().for_each(|file_path| {
            if let Ok(content) = std::fs::read_to_string(file_path) {
                Self::extract_and_track_dependencies(&content, required_stdlib, required_gems);
            }
        });

        info!(
            "Dependency scan complete: {} stdlib modules, {} gems required",
            required_stdlib.lock().len(),
            required_gems.lock().len()
        );
        Ok(())
    }

    pub(crate) fn refresh_exhaustive_semantic_context(
        &mut self,
        server: &RubyLanguageServer,
    ) -> Result<()> {
        assert!(
            self.pending_project_navigation_files.is_none()
                && self.pending_project_files.is_some()
                && !self.exhaustive_collection_started,
            "INVARIANT VIOLATED: the exhaustive semantic context was captured outside the idle post-frontier state. This is a bug because every background source file must read one immutable generation-owned engine before any exhaustive facts become visible. Fix: finish the active project frontier, then capture or refresh the context before taking the first tail batch."
        );
        let project_uri = Url::from_directory_path(&self.workspace_root).map_err(|_| {
            anyhow::anyhow!(
                "Project root is not a valid file URI: {}",
                self.workspace_root.display()
            )
        })?;
        let analysis_engine = server.analysis_engine_for_uri(&project_uri);
        let pending_files = self.pending_project_files.as_ref().expect(
            "INVARIANT VIOLATED: exhaustive context capture lost the retained project tail after validating it. This is a bug because one mutable IndexerProject owns both values. Fix: keep context capture inside the same exclusive IndexerProject borrow.",
        );
        if let Some(path) = pending_files.first() {
            let uri = Url::from_file_path(path).map_err(|_| {
                anyhow!(
                    "project source path is not a valid file URI: {}",
                    path.display()
                )
            })?;
            self.file_processor
                .ensure_project_semantic_seed(&uri, &analysis_engine);
        }

        let snapshot = {
            let mut engine = analysis_engine.write();
            for path in pending_files {
                if engine.file_id(path).is_none() {
                    engine.register_file_borrowed(path.clone(), "", SourceKind::Project);
                }
            }
            let estimated_bytes = engine.estimated_memory_stats().total();
            assert!(
                estimated_bytes <= MAX_EXHAUSTIVE_SEMANTIC_CONTEXT_BYTES,
                "INVARIANT VIOLATED: post-frontier semantic context for {} requires an estimated {} bytes, exceeding the bounded {}-byte clone budget. This is a bug because exhaustive parallel collection must not create an unbounded engine snapshot. Fix: reduce the active frontier/dependency seed or replace the clone with a compact immutable query projection before admitting this project.",
                self.workspace_root.display(),
                estimated_bytes,
                MAX_EXHAUSTIVE_SEMANTIC_CONTEXT_BYTES
            );
            engine.clone()
        };
        let known_namespaces =
            Arc::new(ruby_analysis::engine::AnalysisQuery::new(&snapshot).known_namespace_fqns());
        info!(
            "Captured immutable exhaustive semantic context for {}: estimated_bytes={}, namespaces={}",
            self.workspace_root.display(),
            snapshot.estimated_memory_stats().total(),
            known_namespaces.len()
        );
        self.exhaustive_known_namespaces = Some(known_namespaces);
        self.exhaustive_analysis_engine = Some(Arc::new(parking_lot::RwLock::new(snapshot)));
        Ok(())
    }

    /// Collect facts from files and track their dependencies (Parallelized with rayon)
    fn collect_facts_and_track_dependencies(
        &mut self,
        files: &[PathBuf],
        priority_file_count: usize,
        server: &RubyLanguageServer,
        resolve_open_documents: bool,
        known_namespaces: Option<Arc<HashSet<FullyQualifiedName>>>,
        semantic_context_engine: Option<Arc<parking_lot::RwLock<AnalysisEngine>>>,
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
        let analysis_engine = server.analysis_engine_for_uri(&project_uri);
        let uses_immutable_semantic_context = semantic_context_engine.is_some();
        let semantic_read_engine =
            semantic_context_engine.unwrap_or_else(|| analysis_engine.clone());
        let known_namespaces = known_namespaces.unwrap_or_else(|| {
            Arc::new({
                let engine = semantic_read_engine.read();
                ruby_analysis::engine::AnalysisQuery::new(&engine).known_namespace_fqns()
            })
        });

        let collect_start = Instant::now();
        let read_file = |file_path: &PathBuf| -> Result<(
            PathBuf,
            String,
            std::time::Duration,
            std::time::Duration,
        )> {
            let read_started = Instant::now();
            let content = std::fs::read_to_string(file_path).with_context(|| {
                format!("failed to read project source {}", file_path.display())
            })?;
            let read_elapsed = read_started.elapsed();
            let dependency_started = Instant::now();
            Self::extract_and_track_dependencies(&content, required_stdlib_ref, required_gems_ref);
            let dependency_elapsed = dependency_started.elapsed();
            Ok((
                file_path.clone(),
                content,
                read_elapsed,
                dependency_elapsed,
            ))
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
            if let Some((path, _, _, _)) = inputs.first() {
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
        if uses_immutable_semantic_context {
            let mut engine = analysis_engine.write();
            let mut semantic_engine = semantic_read_engine.write();
            for (path, content, _, _) in &inputs {
                let live_id =
                    engine.register_file_borrowed(path.clone(), content, SourceKind::Project);
                let semantic_id = semantic_engine.register_file_borrowed(
                    path.clone(),
                    content,
                    SourceKind::Project,
                );
                assert_eq!(
                    live_id,
                    semantic_id,
                    "INVARIANT VIOLATED: immutable semantic context assigned a different file id for {}. This is a bug because retained FileFacts ranges must be valid in the owning live engine. Fix: pre-register the complete tail in identical path order before either engine admits generated sources.",
                    path.display()
                );
            }
        } else {
            let mut engine = analysis_engine.write();
            for (path, content, _, _) in &inputs {
                engine.register_file_borrowed(path.clone(), content, SourceKind::Project);
            }
        }
        let batch_registration_elapsed = batch_registration_started.elapsed();

        let collect_file = |input: (PathBuf, String, std::time::Duration, std::time::Duration)| -> Result<(
            PathBuf,
            ruby_analysis::engine::FileFacts,
            StaticJavaNavigationPlan,
            StaticJavaSourceHint,
            std::time::Duration,
            std::time::Duration,
            ProjectFileCollectionTiming,
        )> {
            let (file_path, content, read_elapsed, dependency_elapsed) = input;
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
            Ok((
                file_path.clone(),
                collected.file_facts,
                collected.jruby_navigation_plan,
                collected.jruby_source_hint,
                read_elapsed,
                dependency_elapsed,
                collected.timing,
            ))
        };
        let outcomes = map_owned_project_inputs(inputs, priority_file_count, &collect_file);
        let mut jruby_navigation_plan = StaticJavaNavigationPlan::default();
        let mut jruby_source_hints = Vec::with_capacity(outcomes.len());
        let mut read_cpu = std::time::Duration::ZERO;
        let mut dependency_scan_cpu = std::time::Duration::ZERO;
        let mut timing = ProjectFileCollectionTiming::default();
        timing.total += batch_registration_elapsed;
        timing.registration += batch_registration_elapsed;
        for outcome in outcomes {
            let (path, file_facts, plan, hint, read, dependency_scan, file_timing) = outcome?;
            let replacement_started = Instant::now();
            file_processor_ref.replace_collected_project_file_facts_as_deferred_resolution(
                &path,
                &analysis_engine,
                file_facts,
            );
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
            self.resolve_open_project_files(server, &analysis_engine);
        }

        Ok(())
    }

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
        server: &RubyLanguageServer,
    ) -> Result<usize> {
        let provider = file_processor.jruby_import_provider().cloned().expect(
            "INVARIANT VIOLATED: a JRuby catalog-sensitive replay was requested without an exact \
             project import provider. This is a bug because replay selection and replacement must \
             use the same isolated classpath catalog. Fix: install the completed provider on the \
             final project FileProcessor before replay.",
        );
        let files = self.jruby_catalog_sensitive_files(&provider);
        let project_uri = Url::from_directory_path(&self.workspace_root).map_err(|_| {
            anyhow!(
                "Project root is not a valid file URI: {}",
                self.workspace_root.display()
            )
        })?;
        let analysis_engine = server.analysis_engine_for_uri(&project_uri);
        let known_namespaces = self.jruby_replay_known_namespaces.take().expect(
            "INVARIANT VIOLATED: JRuby catalog-sensitive replay has no immutable post-frontier \
             namespace snapshot. This is a bug because replayed files must use the same semantic \
             context as provider-aware project batches regardless of concurrent dependency \
             binding. Fix: retain the generation-owned snapshot through exhaustive project \
             completion and consume it exactly once during replay.",
        );
        let semantic_read_engine = self.jruby_replay_analysis_engine.take().expect(
            "INVARIANT VIOLATED: JRuby catalog-sensitive replay has no immutable post-frontier semantic engine. This is a bug because providerless and provider-aware collection must observe the same generation-owned facts. Fix: retain the exhaustive read engine through replay and consume it exactly once."
        );
        let replay_started = Instant::now();
        let outcomes = files
            .par_iter()
            .map(
                |file_path| -> Result<(
                    PathBuf,
                    ruby_analysis::engine::FileFacts,
                    StaticJavaNavigationPlan,
                )> {
                    let content = std::fs::read_to_string(file_path).with_context(|| {
                        format!(
                            "failed to reread JRuby catalog-sensitive project source {}",
                            file_path.display()
                        )
                    })?;
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
                    Ok((
                        file_path.clone(),
                        collected.file_facts,
                        collected.jruby_navigation_plan,
                    ))
                },
            )
            .collect::<Vec<_>>();
        let mut plan = StaticJavaNavigationPlan::default();
        for outcome in outcomes {
            let (path, file_facts, file_plan) = outcome?;
            file_processor.replace_collected_project_file_facts_as_deferred_resolution(
                &path,
                &analysis_engine,
                file_facts,
            );
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
        self.resolve_open_project_files(server, &analysis_engine);
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
        assert_eq!(
            known_namespaces.is_some(),
            semantic_engine.is_some(),
            "INVARIANT VIOLATED: JRuby replay namespace and semantic-engine ownership diverged. This is a bug because both snapshots are created and consumed as one generation-owned context. Fix: move or discard both fields in the same lifecycle transition."
        );
    }

    fn resolve_open_project_files(
        &self,
        server: &RubyLanguageServer,
        analysis_engine: &Arc<parking_lot::RwLock<ruby_analysis::engine::AnalysisEngine>>,
    ) {
        let resolve_start = Instant::now();
        let mut open_project_paths = server
            .docs
            .lock()
            .keys()
            .filter_map(|uri| {
                let workspace = server.workspace_for_uri(uri)?;
                (workspace.root_path == self.workspace_root)
                    .then(|| uri.to_file_path().ok())
                    .flatten()
            })
            .collect::<Vec<_>>();
        open_project_paths.sort();
        open_project_paths.dedup();
        let open_project_file_ids = {
            let engine = analysis_engine.read();
            open_project_paths
                .iter()
                .map(|path| {
                    engine.file_id(path).unwrap_or_else(|| {
                        panic!(
                            "INVARIANT VIOLATED: open project document {} has no registered \
                             analysis file after project fact collection. This is a bug because \
                             didOpen and the project pass share the owning isolated engine. Fix: \
                             keep open-document registration and project routing on the same \
                             longest-prefix workspace owner.",
                            path.display()
                        )
                    })
                })
                .collect::<Vec<_>>()
        };
        analysis_engine
            .write()
            .resolve_files(&open_project_file_ids);
        let resolve_elapsed = resolve_start.elapsed();
        info!(
            "Open project reference/diagnostic resolution completed in {:?} for {} document(s); \
             closed-file candidates remain deferred",
            resolve_elapsed,
            open_project_file_ids.len()
        );
        info!(
            "Project navigation stage completed in {:?}",
            resolve_elapsed
        );
    }

    /// Extract dependencies from content and update trackers (Static helper for parallelism)
    fn extract_and_track_dependencies(
        content: &str,
        required_stdlib: &Arc<Mutex<HashSet<String>>>,
        required_gems: &Arc<Mutex<HashSet<String>>>,
    ) {
        let mut stdlib_deps = Vec::new();
        let mut gem_deps = Vec::new();

        for line in content.lines() {
            let trimmed = line.trim();

            // Require
            if let Some(required) = Self::parse_require_statement(trimmed) {
                if Self::is_stdlib_module(&required) {
                    stdlib_deps.push(required);
                }
            }

            // Gem
            if let Some(gem_name) = Self::parse_gem_statement(trimmed) {
                gem_deps.push(gem_name);
            }
        }

        if !stdlib_deps.is_empty() {
            required_stdlib.lock().extend(stdlib_deps);
        }
        if !gem_deps.is_empty() {
            required_gems.lock().extend(gem_deps);
        }
    }

    /// Parse a require statement and extract the module name
    fn parse_require_statement(line: &str) -> Option<String> {
        // Handle various require patterns:
        // require 'module'
        // require "module"
        // require_relative 'module'

        if line.starts_with("require ") || line.starts_with("require_relative ") {
            // Find the quoted string
            if let Some(start) = line.find('"').or_else(|| line.find('\'')) {
                let quote_char = line.chars().nth(start).unwrap();
                if let Some(end) = line[start + 1..].find(quote_char) {
                    let module_name = &line[start + 1..start + 1 + end];
                    return Some(module_name.to_string());
                }
            }
        }

        None
    }

    /// Check if a module is part of Ruby's standard library
    fn is_stdlib_module(module_name: &str) -> bool {
        // Common stdlib modules
        const STDLIB_MODULES: &[&str] = &[
            "json",
            "yaml",
            "csv",
            "uri",
            "net/http",
            "net/https",
            "openssl",
            "digest",
            "base64",
            "time",
            "date",
            "fileutils",
            "pathname",
            "tempfile",
            "tmpdir",
            "logger",
            "benchmark",
            "optparse",
            "ostruct",
            "set",
            "forwardable",
            "delegate",
            "singleton",
            "observer",
            "thread",
            "mutex_m",
            "monitor",
            "sync",
            "fiber",
            "continuation",
            "english",
            "abbrev",
            "cgi",
            "erb",
            "rexml",
            "rss",
            "xmlrpc",
            "webrick",
            "socket",
            "ipaddr",
            "resolv",
            "open-uri",
            "open3",
            "pty",
            "expect",
            "readline",
            "zlib",
            "stringio",
            "strscan",
            "scanf",
            "getoptlong",
            "find",
            "ftools",
            "shell",
            "shellwords",
            "etc",
            "fcntl",
            "io/console",
            "io/nonblock",
            "io/wait",
            "dbm",
            "gdbm",
            "sdbm",
            "pstore",
            "yaml/store",
        ];

        STDLIB_MODULES.contains(&module_name)
    }

    /// Parse a gem statement from Gemfile
    fn parse_gem_statement(line: &str) -> Option<String> {
        if line.starts_with("gem ") {
            // Find the quoted gem name
            if let Some(start) = line.find('"').or_else(|| line.find('\'')) {
                let quote_char = line.chars().nth(start).unwrap();
                if let Some(end) = line[start + 1..].find(quote_char) {
                    let gem_name = &line[start + 1..start + 1 + end];
                    return Some(gem_name.to_string());
                }
            }
        }

        None
    }

    /// Clear previously tracked dependencies
    fn clear_dependencies(&self) {
        self.required_stdlib.lock().clear();
        self.required_gems.lock().clear();
    }

    /// Get the list of required stdlib modules
    pub fn get_required_stdlib(&self) -> Vec<String> {
        self.required_stdlib.lock().iter().cloned().collect()
    }

    /// Get the list of required gems
    pub fn get_required_gems(&self) -> Vec<String> {
        self.required_gems.lock().iter().cloned().collect()
    }

    /// Check if a specific stdlib module is required
    pub fn requires_stdlib(&self, module_name: &str) -> bool {
        self.required_stdlib.lock().contains(module_name)
    }

    /// Check if a specific gem is required
    pub fn requires_gem(&self, gem_name: &str) -> bool {
        self.required_gems.lock().contains(gem_name)
    }

    /// Get the workspace root path
    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    /// Get a reference to the core indexer
    pub fn file_processor(&self) -> &FileProcessor {
        &self.file_processor
    }
}

fn select_navigation_demand_files(
    files: &mut Vec<PathBuf>,
    processed_files: &HashSet<PathBuf>,
    keys: &[String],
) -> ProjectNavigationDemandSelection {
    assert!(
        keys.len() <= MAX_PROJECT_NAVIGATION_DEMAND_KEYS,
        "INVARIANT VIOLATED: one project navigation demand drain contained {} keys, above the \
         bounded maximum of {}. This is a bug because the server queue must apply backpressure \
         before the coordinator selects project files. Fix: keep demand admission and \
         coordinator drain limits identical.",
        keys.len(),
        MAX_PROJECT_NAVIGATION_DEMAND_KEYS,
    );
    let mut selected_paths = HashSet::new();
    let mut completed_keys = Vec::new();
    let mut deferred_keys = Vec::new();
    for key in keys {
        assert!(
            !key.is_empty() && key.chars().all(char::is_alphanumeric),
            "INVARIANT VIOLATED: project navigation demand key `{key}` is not a normalized \
             alphanumeric identifier. This is a bug because file selection must never \
             reinterpret arbitrary request text or paths. Fix: normalize identifiers at the \
             definition-query boundary before enqueueing a bounded demand."
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

fn prioritize_project_files(
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

fn project_file_matches_navigation_key(path: &Path, key: &str) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::IndexingConfig;
    use crate::runtime::jruby::imports::JrubyImportProvider;
    use crate::runtime::jruby::java_catalog::{JavaClassDeclaration, ProjectJavaCatalog};
    use ruby_analysis::engine::AnalysisQuery;
    use ruby_fast_lsp_jvm_metadata::ClassFile;
    use std::collections::BTreeMap;
    use tempfile::TempDir;
    use tower_lsp::lsp_types::{DidOpenTextDocumentParams, TextDocumentItem, Url};

    #[test]
    fn project_input_partitions_consume_non_clone_values_in_order() {
        struct OwnedOnly(u8);

        let values = vec![OwnedOnly(1), OwnedOnly(2), OwnedOnly(3), OwnedOnly(4)];
        let consumed = map_owned_project_inputs(values, 2, &|OwnedOnly(value)| value);

        assert_eq!(
            consumed,
            vec![1, 2, 3, 4],
            "priority and exhaustive partitions must preserve deterministic input order while \
             transferring each non-Clone source owner exactly once"
        );
    }

    #[test]
    fn active_constant_keys_prioritize_matching_project_file_stems() {
        let files = vec![
            PathBuf::from("/project/a.rb"),
            PathBuf::from("/project/user.rb"),
            PathBuf::from("/project/user_pmm.rb"),
            PathBuf::from("/project/z.rb"),
        ];
        let priority_keys = HashSet::from(["userpmm".to_string()]);

        let (files, priority_count) = prioritize_project_files(files, &priority_keys);

        assert_eq!(priority_count, 2);
        assert_eq!(
            files,
            vec![
                PathBuf::from("/project/user.rb"),
                PathBuf::from("/project/user_pmm.rb"),
                PathBuf::from("/project/a.rb"),
                PathBuf::from("/project/z.rb"),
            ],
            "active-document constant targets must move exact and conventional base filenames \
            first while preserving the exhaustive order of every nonmatching project file"
        );
    }

    #[test]
    fn exact_navigation_demand_promotes_a_late_project_file_before_the_next_batch() {
        let mut pending_files = (0..20)
            .map(|index| PathBuf::from(format!("/project/ordinary_{index:02}.rb")))
            .chain([PathBuf::from("/project/user_pmm.rb")])
            .collect::<Vec<_>>();

        let selection = select_navigation_demand_files(
            &mut pending_files,
            &HashSet::new(),
            &["userpmm".to_string()],
        );

        assert_eq!(
            selection.files,
            vec![PathBuf::from("/project/user_pmm.rb")],
            "a bounded exact request must remove its conventional definition candidate from the \
             exhaustive tail before unrelated files are selected"
        );
        assert_eq!(selection.completed_keys, vec!["userpmm".to_string()]);
        assert!(selection.deferred_keys.is_empty());
        assert!(
            pending_files
                .iter()
                .all(|path| path != Path::new("/project/user_pmm.rb")),
            "the demanded file must not be parsed again by exhaustive collection"
        );
    }

    #[test]
    fn navigation_demand_without_a_conventional_file_candidate_waits_for_project_completion() {
        let mut pending_files = vec![PathBuf::from("/project/legacy_location.rb")];

        let selection = select_navigation_demand_files(
            &mut pending_files,
            &HashSet::new(),
            &["unconventionalclass".to_string()],
        );

        assert!(selection.files.is_empty());
        assert!(selection.completed_keys.is_empty());
        assert_eq!(
            selection.deferred_keys,
            vec!["unconventionalclass".to_string()],
            "a filename heuristic cannot claim that the semantic target was processed when no \
             bounded candidate exists"
        );
    }

    #[test]
    fn exhaustive_project_tail_is_yielded_in_bounded_deterministic_batches() {
        let mut indexer = IndexerProject::new(
            PathBuf::from("/project"),
            FileProcessor::new(),
            IndexingConfig::default(),
        );
        indexer.pending_project_files = Some(
            (0..10)
                .map(|index| PathBuf::from(format!("/project/file_{index:02}.rb")))
                .collect(),
        );

        assert_eq!(
            indexer.take_next_remaining_project_files(4),
            (0..4)
                .map(|index| PathBuf::from(format!("/project/file_{index:02}.rb")))
                .collect::<Vec<_>>()
        );
        assert_eq!(indexer.remaining_project_file_count(), 6);
        assert_eq!(
            indexer.take_next_remaining_project_files(4),
            (4..8)
                .map(|index| PathBuf::from(format!("/project/file_{index:02}.rb")))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            indexer.take_next_remaining_project_files(4),
            (8..10)
                .map(|index| PathBuf::from(format!("/project/file_{index:02}.rb")))
                .collect::<Vec<_>>()
        );
        assert_eq!(indexer.remaining_project_file_count(), 0);
    }

    fn jruby_provider(class_names: &[&str]) -> JrubyImportProvider {
        jruby_provider_with_superclasses(
            &class_names
                .iter()
                .map(|name| (*name, "java/lang/Object"))
                .collect::<Vec<_>>(),
        )
    }

    fn jruby_provider_with_superclasses(
        classes_with_superclasses: &[(&str, &str)],
    ) -> JrubyImportProvider {
        let classes = classes_with_superclasses
            .iter()
            .map(|(name, superclass)| {
                (
                    (*name).to_string(),
                    JavaClassDeclaration {
                        class: Arc::new(ClassFile {
                            minor_version: 0,
                            major_version: 61,
                            access_flags: 0x0021,
                            name: (*name).to_string(),
                            super_name: Some((*superclass).to_string()),
                            interfaces: Vec::new(),
                            fields: Vec::new(),
                            methods: Vec::new(),
                            source_file: None,
                            signature: None,
                            annotations: Vec::new(),
                            inner_classes: Vec::new(),
                            record_components: Vec::new(),
                            module_name: None,
                        }),
                        artifact_path: PathBuf::from("/fixture/runtime.jar"),
                        artifact_fingerprint_sha256: "fixture".to_string(),
                        entry_name: format!("{name}.class"),
                        release: None,
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();
        JrubyImportProvider::new(Arc::new(ProjectJavaCatalog {
            classpath_fingerprint_sha256: "fixture-classpath".to_string(),
            classes,
            duplicates: Vec::new(),
        }))
    }

    #[test]
    fn configured_project_files_drive_dependency_scanning() {
        let workspace = TempDir::new().unwrap();
        let root = workspace.path();
        std::fs::create_dir_all(root.join("bin")).unwrap();
        std::fs::create_dir_all(root.join("vendor")).unwrap();
        std::fs::write(root.join("app.rb"), "gem 'rack'\n").unwrap();
        std::fs::write(root.join("bin/console"), "gem 'rails'\n").unwrap();
        std::fs::write(root.join("vendor/generated.rb"), "gem 'debug'\n").unwrap();

        let indexer = IndexerProject::new(
            root.to_path_buf(),
            FileProcessor::new(),
            IndexingConfig {
                included_patterns: vec!["bin/*".to_string()],
                excluded_patterns: vec!["vendor/**/*".to_string()],
                ..IndexingConfig::default()
            },
        );

        indexer.scan_for_dependencies().unwrap();

        assert!(indexer.requires_gem("rack"));
        assert!(indexer.requires_gem("rails"));
        assert!(!indexer.requires_gem("debug"));
    }

    #[tokio::test]
    async fn project_stage_resolves_open_documents_and_defers_closed_candidates() {
        let workspace = TempDir::new().unwrap();
        let root = workspace.path();
        let open_path = root.join("open.rb");
        let closed_path = root.join("closed.rb");
        let definition_path = root.join("user.rb");
        std::fs::write(&open_path, "User.new\n").unwrap();
        std::fs::write(&closed_path, "User.new\n").unwrap();
        std::fs::write(&definition_path, "class User\nend\n").unwrap();

        let server = RubyLanguageServer::default();
        let workspace = server.add_workspace(Url::from_directory_path(root).unwrap());
        let open_uri = Url::from_file_path(&open_path).unwrap();
        crate::capabilities::indexing::handle_did_open(
            &server,
            DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri: open_uri,
                    language_id: "ruby".to_string(),
                    version: 1,
                    text: "User.new\n".to_string(),
                },
            },
        )
        .await;

        let mut indexer = IndexerProject::new(
            root.to_path_buf(),
            FileProcessor::new(),
            IndexingConfig::default(),
        );
        indexer.collect_project_facts(&server).unwrap();

        let engine = workspace.analysis_engine.read();
        let open_file = engine.file_id(&open_path).unwrap();
        let closed_file = engine.file_id(&closed_path).unwrap();
        let query = AnalysisQuery::new(&engine);
        assert!(
            !query.references_in_file(open_file).is_empty(),
            "the open document must have its project references resolved"
        );
        assert!(
            query.references_in_file(closed_file).is_empty(),
            "closed-file candidates must remain deferred during project-navigation staging"
        );
        drop(engine);

        workspace.analysis_engine.write().resolve();
        assert!(
            !AnalysisQuery::new(&workspace.analysis_engine.read())
                .references_in_file(closed_file)
                .is_empty(),
            "the final complete resolution must materialize the deferred closed-file candidate"
        );
    }

    #[test]
    fn project_navigation_frontier_releases_before_exhaustive_source_collection() {
        let workspace = TempDir::new().unwrap();
        let root = workspace.path();
        let active_path = root.join("user.rb");
        let background_path = root.join("report.rb");
        std::fs::write(&active_path, "class User\nend\n").unwrap();
        std::fs::write(&background_path, "class Report\nend\n").unwrap();

        let server = RubyLanguageServer::default();
        let workspace_state = server.add_workspace(Url::from_directory_path(root).unwrap());
        let mut indexer = IndexerProject::new(
            root.to_path_buf(),
            FileProcessor::new(),
            IndexingConfig::default(),
        );
        indexer.set_navigation_priority_keys(HashSet::from(["user".to_string()]), HashSet::new());

        indexer.collect_project_navigation_facts(&server).unwrap();

        let user = ruby_analysis::core::RubyConstant::new("User").unwrap();
        let report = ruby_analysis::core::RubyConstant::new("Report").unwrap();
        {
            let engine = workspace_state.analysis_engine.read();
            let query = AnalysisQuery::new(&engine);
            assert!(
                !query
                    .constant_definition_ranges(&[user.clone()], &[])
                    .is_empty(),
                "the exact active target must be queryable after the navigation frontier"
            );
            assert!(
                query
                    .constant_definition_ranges(&[report.clone()], &[])
                    .is_empty(),
                "unrelated project source must remain pending until the exhaustive stage"
            );
        }

        indexer.collect_remaining_project_facts(&server).unwrap();

        let engine = workspace_state.analysis_engine.read();
        assert!(
            !AnalysisQuery::new(&engine)
                .constant_definition_ranges(&[report], &[])
                .is_empty(),
            "the exhaustive stage must complete the same isolated project engine"
        );
    }

    #[test]
    fn queued_exact_demand_is_queryable_before_unrelated_active_candidates() {
        let workspace = TempDir::new().unwrap();
        let root = workspace.path();
        std::fs::write(root.join("user_pmm.rb"), "class UserPmm\nend\n").unwrap();
        std::fs::write(root.join("report.rb"), "class Report\nend\n").unwrap();

        let server = RubyLanguageServer::default();
        let workspace_state = server.add_workspace(Url::from_directory_path(root).unwrap());
        let mut indexer = IndexerProject::new(
            root.to_path_buf(),
            FileProcessor::new(),
            IndexingConfig::default(),
        );
        indexer.set_navigation_priority_keys(HashSet::from(["report".to_string()]), HashSet::new());

        let selection = indexer
            .collect_initial_project_navigation_demand_facts(&["userpmm".to_string()], &server)
            .unwrap();

        assert_eq!(selection.completed_keys, vec!["userpmm".to_string()]);
        assert!(selection.deferred_keys.is_empty());
        let user = ruby_analysis::core::RubyConstant::new("UserPmm").unwrap();
        let report = ruby_analysis::core::RubyConstant::new("Report").unwrap();
        {
            let engine = workspace_state.analysis_engine.read();
            let query = AnalysisQuery::new(&engine);
            assert!(
                !query
                    .constant_definition_ranges(&[user.clone()], &[])
                    .is_empty(),
                "the queued exact target must be queryable before unrelated active candidates"
            );
            assert!(
                query
                    .constant_definition_ranges(&[report.clone()], &[])
                    .is_empty(),
                "an unrelated active candidate must remain pending when the exact demand wakes"
            );
        }

        indexer.finish_project_navigation_facts(&server).unwrap();
        let engine = workspace_state.analysis_engine.read();
        assert!(
            !AnalysisQuery::new(&engine)
                .constant_definition_ranges(&[report], &[])
                .is_empty(),
            "the rest of the active frontier must remain semantically complete"
        );
    }

    #[test]
    fn navigation_demand_completes_when_the_frontier_already_processed_its_file() {
        let workspace = TempDir::new().unwrap();
        let root = workspace.path();
        std::fs::write(root.join("user.rb"), "class UserPmm\nend\n").unwrap();
        std::fs::write(root.join("report.rb"), "class Report\nend\n").unwrap();

        let server = RubyLanguageServer::default();
        server.add_workspace(Url::from_directory_path(root).unwrap());
        let mut indexer = IndexerProject::new(
            root.to_path_buf(),
            FileProcessor::new(),
            IndexingConfig::default(),
        );
        indexer
            .set_navigation_priority_keys(HashSet::from(["userpmm".to_string()]), HashSet::new());

        indexer.collect_project_navigation_facts(&server).unwrap();
        let selection = indexer.take_navigation_demand_files(&["userpmm".to_string()]);

        assert!(
            selection.files.is_empty(),
            "an already indexed frontier file must never be parsed a second time"
        );
        assert_eq!(selection.completed_keys, vec!["userpmm".to_string()]);
        assert!(
            selection.deferred_keys.is_empty(),
            "a request whose matching frontier file is queryable must wake immediately"
        );
    }

    #[test]
    fn exhaustive_batches_share_one_immutable_post_frontier_namespace_context() {
        let workspace = TempDir::new().unwrap();
        let root = workspace.path();
        std::fs::write(root.join("seed.rb"), "class Seed\nend\n").unwrap();
        let parent_path = root.join("a_parent.rb");
        let child_path = root.join("b_child.rb");
        std::fs::write(&parent_path, "class Parent\nend\n").unwrap();
        std::fs::write(&child_path, "class Child < Parent\nend\n").unwrap();

        let server = RubyLanguageServer::default();
        let workspace_state = server.add_workspace(Url::from_directory_path(root).unwrap());
        let mut indexer = IndexerProject::new(
            root.to_path_buf(),
            FileProcessor::new(),
            IndexingConfig::default(),
        );
        indexer.set_navigation_priority_keys(HashSet::from(["seed".to_string()]), HashSet::new());
        indexer.collect_project_navigation_facts(&server).unwrap();

        let parent_batch = indexer.take_next_remaining_project_files(1);
        assert_eq!(parent_batch, vec![parent_path]);
        indexer
            .collect_project_file_batch(&parent_batch, &server, false)
            .unwrap();
        let child_batch = indexer.take_next_remaining_project_files(1);
        assert_eq!(child_batch, vec![child_path.clone()]);
        indexer
            .collect_project_file_batch(&child_batch, &server, false)
            .unwrap();

        let child = ruby_analysis::core::FullyQualifiedName::namespace(vec![
            ruby_analysis::core::RubyConstant::new("Child").unwrap(),
        ]);
        {
            let engine = workspace_state.analysis_engine.read();
            assert!(
                engine.unresolved_graph_edges().iter().any(|edge| {
                    edge.source == child
                        && edge.kind == ruby_analysis::core::GraphEdgeKind::Superclass
                }),
                "a later batch must not observe namespaces introduced by an arbitrary earlier \
                 exhaustive batch"
            );
        }

        workspace_state.analysis_engine.write().resolve();
        let engine = workspace_state.analysis_engine.read();
        assert!(
            engine.unresolved_graph_edges().iter().all(|edge| {
                edge.source != child || edge.kind != ruby_analysis::core::GraphEdgeKind::Superclass
            }),
            "the coordinator's final semantic resolution must resolve the deferred superclass"
        );
    }

    #[test]
    fn exhaustive_semantics_do_not_depend_on_batch_boundaries() {
        let workspace = TempDir::new().unwrap();
        let root = workspace.path();
        std::fs::write(root.join("seed.rb"), "class Seed\nend\n").unwrap();
        std::fs::write(
            root.join("a_service.rb"),
            "class Target\n  def name\n    String\n  end\nend\n\
             class Service\n  def target\n    Target.new\n  end\n\
             delegate :name, to: :target\nend\n",
        )
        .unwrap();
        std::fs::write(
            root.join("b_consumer.rb"),
            "class Consumer\n  def value\n    Service.new.name\n  end\nend\n",
        )
        .unwrap();

        let mut expected = None;
        for batch_size in [1, 2] {
            let server = RubyLanguageServer::default();
            let workspace_state = server.add_workspace(Url::from_directory_path(root).unwrap());
            let mut indexer = IndexerProject::new(
                root.to_path_buf(),
                FileProcessor::new(),
                IndexingConfig::default(),
            );
            indexer
                .set_navigation_priority_keys(HashSet::from(["seed".to_string()]), HashSet::new());
            indexer.collect_project_navigation_facts(&server).unwrap();

            rayon::ThreadPoolBuilder::new()
                .num_threads(1)
                .build()
                .unwrap()
                .install(|| {
                    while indexer.remaining_project_file_count() > 0 {
                        let batch = indexer.take_next_remaining_project_files(batch_size);
                        indexer
                            .collect_project_file_batch(&batch, &server, false)
                            .unwrap();
                    }
                });
            indexer.finish_remaining_project_facts();
            workspace_state.analysis_engine.write().resolve();
            let actual = workspace_state
                .analysis_engine
                .read()
                .semantic_result_fingerprint();

            if let Some(expected) = expected {
                assert_eq!(
                    actual, expected,
                    "exhaustive project facts must not depend on arbitrary coordinator batch \
                     boundaries"
                );
            } else {
                expected = Some(actual);
            }
        }
    }

    #[test]
    fn parallel_batch_collection_has_a_stable_semantic_result() {
        let workspace = TempDir::new().unwrap();
        let root = workspace.path();
        std::fs::write(root.join("seed.rb"), "class Seed\nend\n").unwrap();
        for index in 0..32 {
            std::fs::write(
                root.join(format!("model_{index:02}.rb")),
                format!(
                    "class Model{index:02}\n  def sibling\n    Model{:02}.new\n  end\nend\n",
                    (index + 1) % 32
                ),
            )
            .unwrap();
        }

        let mut expected = None;
        for _ in 0..4 {
            let server = RubyLanguageServer::default();
            let workspace_state = server.add_workspace(Url::from_directory_path(root).unwrap());
            let mut indexer = IndexerProject::new(
                root.to_path_buf(),
                FileProcessor::new(),
                IndexingConfig::default(),
            );
            indexer
                .set_navigation_priority_keys(HashSet::from(["seed".to_string()]), HashSet::new());
            indexer.collect_project_navigation_facts(&server).unwrap();

            let batch = indexer.take_next_remaining_project_files(64);
            assert_eq!(batch.len(), 32);
            indexer
                .collect_project_file_batch(&batch, &server, true)
                .unwrap();
            indexer.finish_remaining_project_facts();
            workspace_state.analysis_engine.write().resolve();
            let actual = workspace_state
                .analysis_engine
                .read()
                .semantic_result_fingerprint();

            if let Some(expected) = expected {
                assert_eq!(
                    actual, expected,
                    "parallel worker completion order must not change the file-owned semantic result"
                );
            } else {
                expected = Some(actual);
            }
        }
    }

    #[test]
    fn providerless_project_pass_records_exact_compact_jruby_replay_candidates() {
        let workspace = TempDir::new().unwrap();
        let root = workspace.path();
        let ordinary_path = root.join("ordinary.rb");
        let alias_path = root.join("alias.rb");
        let proxy_path = root.join("proxy.rb");
        std::fs::write(
            &ordinary_path,
            "module App\n  USER_NAME = user.profile.name\nend\n",
        )
        .unwrap();
        std::fs::write(
            &alias_path,
            "class Imported\n  java_alias :merged, :combine\nend\n",
        )
        .unwrap();
        std::fs::write(&proxy_path, "DEMO = com.example.Demo.new\n").unwrap();

        let server = RubyLanguageServer::default();
        server.add_workspace(Url::from_directory_path(root).unwrap());
        let mut indexer = IndexerProject::new(
            root.to_path_buf(),
            FileProcessor::new(),
            IndexingConfig::default(),
        );
        indexer.collect_project_facts(&server).unwrap();

        assert_eq!(
            indexer.jruby_catalog_sensitive_files(&jruby_provider(&["com/example/Demo"])),
            vec![alias_path, proxy_path],
            "the providerless pass must retain bounded source hints so only actual JRuby \
             catalog consumers are replayed after the exact project catalog arrives"
        );
    }

    #[test]
    fn exact_jruby_provider_replays_only_catalog_sensitive_project_files() {
        let workspace = TempDir::new().unwrap();
        let root = workspace.path();
        let ordinary_path = root.join("ordinary.rb");
        let import_path = root.join("import.rb");
        std::fs::write(&ordinary_path, "class User\nend\n").unwrap();
        std::fs::write(
            &import_path,
            "java_import 'com.example.Demo'\nDEMO = Demo.new\n",
        )
        .unwrap();

        let server = RubyLanguageServer::default();
        let workspace_state = server.add_workspace(Url::from_directory_path(root).unwrap());
        let mut indexer = IndexerProject::new(
            root.to_path_buf(),
            FileProcessor::new(),
            IndexingConfig::default(),
        );
        indexer.collect_project_facts(&server).unwrap();
        let imported =
            ruby_analysis::core::FullyQualifiedName::try_from("Demo").expect("valid fixture FQN");
        assert!(
            !AnalysisQuery::new(&workspace_state.analysis_engine.read())
                .all_symbol_facts()
                .iter()
                .any(|fact| fact.fqn == imported),
            "the providerless first pass must not invent a Java import alias"
        );

        let provider = Arc::new(
            jruby_provider(&["com/example/Demo"])
                .with_signature_cache_root(root.join("generated-signatures")),
        );
        let replayed = indexer
            .replay_jruby_catalog_sensitive_files(
                FileProcessor::new().with_jruby_import_provider(provider.clone()),
                &server,
            )
            .unwrap();

        assert_eq!(replayed, 1, "ordinary Ruby files must not be replayed");
        assert!(
            AnalysisQuery::new(&workspace_state.analysis_engine.read())
                .all_symbol_facts()
                .iter()
                .any(|fact| fact.fqn == imported),
            "the exact provider pass must replace the Java-sensitive file with its imported alias"
        );
    }

    #[test]
    fn exact_jruby_provider_installed_before_tail_replays_only_active_frontier_files() {
        let workspace = TempDir::new().unwrap();
        let signature_cache = TempDir::new().unwrap();
        let root = workspace.path();
        let active_path = root.join("a_active_import.rb");
        let tail_path = root.join("b_tail_import.rb");
        std::fs::write(
            &active_path,
            "java_import 'com.example.Active'\nACTIVE = Active.new\n",
        )
        .unwrap();
        std::fs::write(
            &tail_path,
            "java_import 'com.example.Tail'\nTAIL = Tail.new\n",
        )
        .unwrap();

        let server = RubyLanguageServer::default();
        let workspace_state = server.add_workspace(Url::from_directory_path(root).unwrap());
        let mut indexer = IndexerProject::new(
            root.to_path_buf(),
            FileProcessor::new(),
            IndexingConfig::default(),
        );
        indexer.set_navigation_priority_keys(
            HashSet::from(["aactiveimport".to_string()]),
            HashSet::new(),
        );
        indexer.collect_project_navigation_facts(&server).unwrap();

        let active = FullyQualifiedName::try_from("Active").unwrap();
        let tail = FullyQualifiedName::try_from("Tail").unwrap();
        assert!(
            !AnalysisQuery::new(&workspace_state.analysis_engine.read())
                .all_symbol_facts()
                .iter()
                .any(|fact| fact.fqn == active),
            "the latency frontier must remain providerless"
        );

        let provider = Arc::new(
            jruby_provider(&["com/example/Active", "com/example/Tail"])
                .with_signature_cache_root(signature_cache.path().to_path_buf()),
        );
        indexer.install_jruby_import_provider(provider.clone());
        indexer.collect_remaining_project_facts(&server).unwrap();

        {
            let engine = workspace_state.analysis_engine.read();
            let symbols = AnalysisQuery::new(&engine).all_symbol_facts();
            assert!(
                symbols.iter().any(|fact| fact.fqn == tail),
                "the exhaustive tail must be collected once with the exact provider"
            );
            assert!(
                !symbols.iter().any(|fact| fact.fqn == active),
                "the providerless active file must wait for its bounded exact replay"
            );
        }
        assert_eq!(
            indexer.jruby_catalog_sensitive_files(&provider),
            vec![active_path],
            "provider-aware tail files must not enter the replay set"
        );

        let replayed = indexer
            .replay_jruby_catalog_sensitive_files(
                FileProcessor::new().with_jruby_import_provider(provider),
                &server,
            )
            .unwrap();
        assert_eq!(replayed, 1);
        assert!(
            AnalysisQuery::new(&workspace_state.analysis_engine.read())
                .all_symbol_facts()
                .iter()
                .any(|fact| fact.fqn == active),
            "the bounded replay must replace the active file with exact Java facts"
        );
    }

    #[test]
    fn exact_jruby_provider_handoff_between_batches_replays_only_providerless_files() {
        let workspace = TempDir::new().unwrap();
        let signature_cache = TempDir::new().unwrap();
        let root = workspace.path();
        let active_path = root.join("a_active.rb");
        let first_path = root.join("b_first_import.rb");
        let second_path = root.join("c_second_import.rb");
        std::fs::write(&active_path, "class ActiveDocument\nend\n").unwrap();
        std::fs::write(
            &first_path,
            "java_import 'com.example.First'\nFIRST = First.new\n",
        )
        .unwrap();
        std::fs::write(
            &second_path,
            "java_import 'com.example.Second'\nSECOND = Second.new\n",
        )
        .unwrap();

        let server = RubyLanguageServer::default();
        let workspace_state = server.add_workspace(Url::from_directory_path(root).unwrap());
        let mut indexer = IndexerProject::new(
            root.to_path_buf(),
            FileProcessor::new(),
            IndexingConfig::default(),
        );
        indexer
            .set_navigation_priority_keys(HashSet::from(["aactive".to_string()]), HashSet::new());
        indexer.collect_project_navigation_facts(&server).unwrap();
        indexer
            .refresh_exhaustive_semantic_context(&server)
            .unwrap();

        let first_batch = indexer.take_next_remaining_project_files(1);
        assert_eq!(first_batch, vec![first_path.clone()]);
        indexer
            .collect_project_file_batch(&first_batch, &server, false)
            .unwrap();

        let provider = Arc::new(
            jruby_provider(&["com/example/First", "com/example/Second"])
                .with_signature_cache_root(signature_cache.path().to_path_buf()),
        );
        indexer.install_jruby_import_provider(provider.clone());

        let second_batch = indexer.take_next_remaining_project_files(1);
        assert_eq!(second_batch, vec![second_path.clone()]);
        indexer
            .collect_project_file_batch(&second_batch, &server, true)
            .unwrap();
        indexer.finish_remaining_project_facts();

        assert_eq!(
            indexer.jruby_catalog_sensitive_files(&provider),
            vec![first_path],
            "only files collected before the provider handoff may enter the replay set"
        );
        let second = FullyQualifiedName::try_from("Second").unwrap();
        assert!(
            AnalysisQuery::new(&workspace_state.analysis_engine.read())
                .all_symbol_facts()
                .iter()
                .any(|fact| fact.fqn == second),
            "the provider-aware batch must expose its Java import before replay"
        );

        let replayed = indexer
            .replay_jruby_catalog_sensitive_files(
                FileProcessor::new().with_jruby_import_provider(provider),
                &server,
            )
            .unwrap();
        assert_eq!(replayed, 1);
        let first = FullyQualifiedName::try_from("First").unwrap();
        assert!(
            AnalysisQuery::new(&workspace_state.analysis_engine.read())
                .all_symbol_facts()
                .iter()
                .any(|fact| fact.fqn == first),
            "the bounded replay must replace the providerless batch with exact Java facts"
        );
    }

    #[test]
    fn exact_jruby_provider_handoff_preserves_generated_signature_facts() {
        let workspace = TempDir::new().unwrap();
        let root = workspace.path();
        let active_path = root.join("a_active.rb");
        let first_path = root.join("b_first_import.rb");
        let second_path = root.join("c_second_import.rb");
        std::fs::write(&active_path, "class ActiveDocument\nend\n").unwrap();
        std::fs::write(
            &first_path,
            "java_import 'com.example.First'\nFIRST = First.new\n",
        )
        .unwrap();
        std::fs::write(
            &second_path,
            "java_import 'com.example.Second'\nSECOND = Second.new\n",
        )
        .unwrap();

        let run = |install_before_tail: bool| {
            let signature_cache = TempDir::new().unwrap();
            let server = RubyLanguageServer::default();
            let workspace_state = server.add_workspace(Url::from_directory_path(root).unwrap());
            let mut indexer = IndexerProject::new(
                root.to_path_buf(),
                FileProcessor::new(),
                IndexingConfig::default(),
            );
            indexer.set_navigation_priority_keys(
                HashSet::from(["aactive".to_string()]),
                HashSet::new(),
            );
            indexer.collect_project_navigation_facts(&server).unwrap();
            indexer
                .refresh_exhaustive_semantic_context(&server)
                .unwrap();

            let provider = Arc::new(
                jruby_provider_with_superclasses(&[
                    ("com/example/First", "com/example/Second"),
                    ("com/example/Second", "java/lang/Object"),
                ])
                .with_signature_cache_root(signature_cache.path().to_path_buf()),
            );
            if install_before_tail {
                indexer.install_jruby_import_provider(provider.clone());
            }

            let first_batch = indexer.take_next_remaining_project_files(1);
            assert_eq!(first_batch, vec![first_path.clone()]);
            indexer
                .collect_project_file_batch(&first_batch, &server, false)
                .unwrap();
            if !install_before_tail {
                indexer.install_jruby_import_provider(provider.clone());
            }

            let second_batch = indexer.take_next_remaining_project_files(1);
            assert_eq!(second_batch, vec![second_path.clone()]);
            indexer
                .collect_project_file_batch(&second_batch, &server, true)
                .unwrap();
            indexer.finish_remaining_project_facts();
            indexer
                .replay_jruby_catalog_sensitive_files(
                    FileProcessor::new().with_jruby_import_provider(provider),
                    &server,
                )
                .unwrap();
            workspace_state.analysis_engine.write().resolve();

            let engine = workspace_state.analysis_engine.read();
            let first_signature_path = signature_cache.path().join("com/example/First.rb");
            let first_signature_id = engine.file_id(&first_signature_path).expect(
                "INVARIANT VIOLATED: generated First signature was not indexed. This is a test bug because both schedules import the exact catalog class. Fix: keep the fixture import and signature cache identity aligned.",
            );
            (
                engine
                    .semantic_export_fingerprint(first_signature_id)
                    .expect(
                        "INVARIANT VIOLATED: generated First signature has no export fingerprint. This is a test bug because every indexed signature enters through replace_facts. Fix: retain the ordinary file-owned signature lifecycle in the fixture.",
                    ),
                engine.semantic_result_fingerprint(),
            )
        };

        let exact_before_tail = run(true);
        let handed_off_between_batches = run(false);
        assert_eq!(
            handed_off_between_batches, exact_before_tail,
            "exact JRuby provider readiness timing must not change generated signature facts or the final semantic result"
        );
    }

    #[test]
    fn exact_jruby_provider_handoff_preserves_ordinary_include_diagnostics() {
        let workspace = TempDir::new().unwrap();
        let root = workspace.path();
        let active_path = root.join("a_active.rb");
        let ordinary_path = root.join("b_ordinary_include.rb");
        let import_path = root.join("c_import.rb");
        std::fs::write(&active_path, "class ActiveDocument\nend\n").unwrap();
        std::fs::write(
            &ordinary_path,
            "[301, 302].should include last_response.status\n",
        )
        .unwrap();
        std::fs::write(
            &import_path,
            "java_import 'com.example.Imported'\nIMPORTED = Imported.new\n",
        )
        .unwrap();

        let run = |install_before_tail: bool| {
            let signature_cache = TempDir::new().unwrap();
            let server = RubyLanguageServer::default();
            let workspace_state = server.add_workspace(Url::from_directory_path(root).unwrap());
            let mut indexer = IndexerProject::new(
                root.to_path_buf(),
                FileProcessor::new(),
                IndexingConfig::default(),
            );
            indexer.set_navigation_priority_keys(
                HashSet::from(["aactive".to_string()]),
                HashSet::new(),
            );
            indexer.collect_project_navigation_facts(&server).unwrap();
            indexer
                .refresh_exhaustive_semantic_context(&server)
                .unwrap();

            let provider = Arc::new(
                jruby_provider(&["com/example/Imported"])
                    .with_signature_cache_root(signature_cache.path().to_path_buf()),
            );
            if install_before_tail {
                indexer.install_jruby_import_provider(provider.clone());
            }

            let ordinary_batch = indexer.take_next_remaining_project_files(1);
            assert_eq!(ordinary_batch, vec![ordinary_path.clone()]);
            indexer
                .collect_project_file_batch(&ordinary_batch, &server, false)
                .unwrap();
            if !install_before_tail {
                indexer.install_jruby_import_provider(provider.clone());
            }

            let import_batch = indexer.take_next_remaining_project_files(1);
            assert_eq!(import_batch, vec![import_path.clone()]);
            indexer
                .collect_project_file_batch(&import_batch, &server, true)
                .unwrap();
            indexer.finish_remaining_project_facts();
            let replayed = indexer
                .replay_jruby_catalog_sensitive_files(
                    FileProcessor::new().with_jruby_import_provider(provider),
                    &server,
                )
                .unwrap();
            assert_eq!(
                replayed, 0,
                "an ordinary Ruby include expression must not enter the JRuby replay set"
            );
            workspace_state.analysis_engine.write().resolve();

            let engine = workspace_state.analysis_engine.read();
            let ordinary_id = engine.file_id(&ordinary_path).expect(
                "INVARIANT VIOLATED: ordinary include fixture was not indexed. This is a test bug because the exhaustive batch must register every selected source. Fix: keep the fixture inside the project root and finish the batch.",
            );
            (
                engine.query().diagnostic_facts_in_file(ordinary_id),
                engine.semantic_result_fingerprint(),
            )
        };

        let exact_before_tail = run(true);
        let handed_off_between_batches = run(false);
        assert_eq!(
            handed_off_between_batches, exact_before_tail,
            "provider readiness timing must not reinterpret ordinary Ruby include expressions as JRuby interfaces"
        );
    }

    #[test]
    fn exact_jruby_replay_is_independent_of_exhaustive_batch_boundaries() {
        let workspace = TempDir::new().unwrap();
        let signature_cache = TempDir::new().unwrap();
        let root = workspace.path();
        let first_path = root.join("a_import.rb");
        let second_path = root.join("b_import.rb");
        std::fs::write(
            &first_path,
            "java_import 'com.example.First'\nFIRST = First.new\nFIRST_LATE = LateBound.new\n",
        )
        .unwrap();
        std::fs::write(
            &second_path,
            "java_import 'com.example.Second'\nSECOND = Second.new\nSECOND_LATE = LateBound.new\n",
        )
        .unwrap();

        let late_dependency_uri = Url::from_file_path(root.join("late_dependency.rb")).unwrap();
        let first = FullyQualifiedName::try_from("First").unwrap();
        let second = FullyQualifiedName::try_from("Second").unwrap();
        let mut expected = None;
        for batch_size in [1, 2] {
            let server = RubyLanguageServer::default();
            let workspace_state = server.add_workspace(Url::from_directory_path(root).unwrap());
            let mut indexer = IndexerProject::new(
                root.to_path_buf(),
                FileProcessor::new(),
                IndexingConfig::default(),
            );
            indexer.collect_project_navigation_facts(&server).unwrap();
            while indexer.remaining_project_file_count() > 0 {
                let batch = indexer.take_next_remaining_project_files(batch_size);
                let is_last = indexer.remaining_project_file_count() == 0;
                indexer
                    .collect_project_file_batch(&batch, &server, is_last)
                    .unwrap();
            }
            indexer.finish_remaining_project_facts();

            FileProcessor::new()
                .collect_file_facts_as_deferred_resolution(
                    &late_dependency_uri,
                    "class LateBound\nend\n",
                    &server,
                    SourceKind::External,
                )
                .unwrap();
            let provider = Arc::new(
                jruby_provider(&["com/example/First", "com/example/Second"])
                    .with_signature_cache_root(signature_cache.path().to_path_buf()),
            );
            let replayed = indexer
                .replay_jruby_catalog_sensitive_files(
                    FileProcessor::new().with_jruby_import_provider(provider),
                    &server,
                )
                .unwrap();
            assert_eq!(replayed, 2);
            workspace_state.analysis_engine.write().resolve();
            let engine = workspace_state.analysis_engine.read();
            let symbols = AnalysisQuery::new(&engine).all_symbol_facts();
            assert!(symbols.iter().any(|fact| fact.fqn == first));
            assert!(symbols.iter().any(|fact| fact.fqn == second));
            let actual = engine.semantic_result_fingerprint();
            if let Some(expected) = expected {
                assert_eq!(
                    actual, expected,
                    "exact JRuby replay must not depend on arbitrary exhaustive batch boundaries"
                );
            } else {
                expected = Some(actual);
            }
        }
    }
}
