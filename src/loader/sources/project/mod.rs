use crate::environment::config::IndexingConfig;
use crate::environment::runtime::jruby::imports::{
    JrubyImportProvider, StaticJavaNavigationPlan, StaticJavaSourceHint,
};
use crate::loader::context::LoadContext;
use crate::loader::file_processor::FileProcessor;
use anyhow::{anyhow, Context, Result};
use navigation::project_file_matches_navigation_key;
use parking_lot::Mutex;
use ruby_analysis::core::FullyQualifiedName;
use ruby_analysis::engine::{AnalysisEngine, SourceFileSnapshot};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use std::time::Instant;
use tower_lsp::lsp_types::Url;

mod collection;
mod dependencies;
mod jruby_replay;
mod navigation;
mod semantic_context;

pub mod roots;

pub(crate) const MAX_PROJECT_NAVIGATION_DEMAND_KEYS: usize = 16;

struct ProjectFileInput {
    path: PathBuf,
    content: String,
    read_elapsed: std::time::Duration,
    dependency_elapsed: std::time::Duration,
    expected_snapshot: Option<SourceFileSnapshot>,
    open_document: bool,
}

struct RegisteredProjectFileInput {
    input: ProjectFileInput,
    source_snapshot: SourceFileSnapshot,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct ProjectNavigationDemandSelection {
    pub(crate) files: Vec<PathBuf>,
    pub(crate) completed_keys: Vec<String>,
    pub(crate) deferred_keys: Vec<String>,
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
    project_file_total: Option<u64>,
    project_progress_generation: Option<u64>,
    project_file_completed: AtomicU64,
}

impl IndexerProject {
    fn read_authoritative_project_source(ctx: &LoadContext, path: &Path) -> Result<(String, bool)> {
        let uri = Url::from_file_path(path).map_err(|_| {
            anyhow!(
                "project source path is not a valid file URI: {}",
                path.display()
            )
        })?;
        if let Some(document) = ctx.sources.open_document(&uri) {
            return Ok((document.content, true));
        }
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read project source {}", path.display()))?;
        Ok((content, false))
    }

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
            project_file_total: None,
            project_progress_generation: None,
            project_file_completed: AtomicU64::new(0),
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

    /// Bind progress to the owning coordinator before admitting its work.
    /// Reading the current workspace generation after discovery can mislabel
    /// a cancelled worker's report as belonging to its replacement.
    pub(crate) fn set_progress_generation(&mut self, generation: Option<u64>) {
        self.project_progress_generation = generation;
    }

    pub(crate) fn dependency_navigation_priority_keys(&self) -> HashSet<String> {
        self.dependency_navigation_priority_keys.clone()
    }

    pub(crate) fn has_jruby_import_provider(&self) -> bool {
        self.file_processor.jruby_import_provider().is_some()
    }

    pub(crate) fn install_jruby_import_provider(&mut self, provider: Arc<JrubyImportProvider>) {
        invariant!(
            self.pending_project_files.is_some(),
            what = "JRuby provider installed outside the retained project lifecycle",
            why = "handoff happens only between batches that own the tail and context",
            fix = "install the provider after the frontier, before finish_remaining_project_facts",
        );
        invariant!(
            self.file_processor.jruby_import_provider().is_none(),
            what = "one project indexing generation installed its exact JRuby provider twice",
            why = "runtime identity is immutable within a generation",
            fix = "cancel and replace the generation before changing its provider",
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

    /// Get the workspace root path
    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    /// Get a reference to the core indexer
    pub fn file_processor(&self) -> &FileProcessor {
        &self.file_processor
    }
}

#[cfg(test)]
mod tests;
