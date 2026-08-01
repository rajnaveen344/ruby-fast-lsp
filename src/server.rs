use crate::capabilities::debug::{
    AncestorsParams, AncestorsResponse, ListCommandsResponse, LookupParams, LookupResponse,
    MethodsParams, MethodsResponse, StatsParams, StatsResponse,
};
use crate::config::{runtime::SelectedRuntimeDescriptor, RubyFastLspConfig};
use crate::extensions::{
    ExtensionRegistryHandle, ExtensionStatusParams, ExtensionStatusResponse, ProjectContextSeed,
    ProjectContextSnapshot,
};
use crate::handlers::{notification, request};
use crate::indexing_resources::IndexingResourceGovernor;
use crate::indexing_scheduler::IndexingScheduler;
use crate::indexing_status::{
    IndexingAggregateSnapshot, IndexingPersistentProductReuseSnapshot, IndexingPhase,
    IndexingReuseSnapshot, IndexingRun, IndexingSingleFlightReuseSnapshot,
    IndexingStatusNotification, IndexingStatusParams, IndexingStatusSnapshot,
    ProjectIndexingStatus,
};
use crate::navigation_demand::NavigationDemandController;
use crate::query::namespace_tree::{NamespaceTreeParams, NamespaceTreeResponse};
use crate::runtime::catalog::{
    DiscoveredRuntime, ProjectRuntimeStatus, RuntimeCatalog, RuntimeDiscoverParams, RuntimeStatus,
    RuntimeStatusParams,
};
use crate::runtime::jruby::imports::JrubyImportProvider;
use anyhow::Result;
use log::{debug, info, warn};
use parking_lot::{Mutex, RwLock};
use ruby_analysis::core::{SourceFileId, SourceKind};
use ruby_analysis::engine::AnalysisEngine;
use ruby_analysis::indexer::RubyDocument;
use ruby_fast_lsp_extension_api::ProjectContext;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::process::exit;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::{
    CallHierarchyIncomingCall, CallHierarchyIncomingCallsParams, CallHierarchyItem,
    CallHierarchyOutgoingCall, CallHierarchyOutgoingCallsParams, CallHierarchyPrepareParams,
    CodeActionOrCommand, CodeActionParams, CodeLens, CodeLensParams, CompletionItem,
    CompletionParams, CompletionResponse, Diagnostic, DidChangeConfigurationParams,
    DidChangeTextDocumentParams, DidChangeWatchedFilesParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DidSaveTextDocumentParams, DocumentFormattingParams,
    DocumentHighlight, DocumentHighlightParams, DocumentOnTypeFormattingParams,
    DocumentSymbolParams, DocumentSymbolResponse, FileEvent, FoldingRange, FoldingRangeParams,
    GotoDefinitionParams, GotoDefinitionResponse, InitializeParams, InitializeResult,
    InitializedParams, InlayHintParams, Location, PrepareRenameResponse, ReferenceParams,
    RenameParams, SelectionRange, SelectionRangeParams, SemanticTokensParams, SemanticTokensResult,
    SignatureHelp, SignatureHelpParams, SymbolInformation, TextDocumentPositionParams, TextEdit,
    TypeHierarchyItem, TypeHierarchyPrepareParams, TypeHierarchySubtypesParams,
    TypeHierarchySupertypesParams, Url, WorkspaceEdit, WorkspaceSymbolParams,
};
use tower_lsp::{Client, LanguageServer};

const MIB: u64 = 1024 * 1024;
const CORE_ENGINE_CACHE_MAX_ENTRIES: usize = 8;
const CORE_ENGINE_CACHE_MAX_WEIGHT_BYTES: u64 = 128 * MIB;
const RUNTIME_STDLIB_PATH_CACHE_MAX_ENTRIES: usize = 32;
const RUNTIME_STDLIB_PATH_CACHE_MAX_WEIGHT_BYTES: u64 = MIB;
const INDEXING_COUNTER_PUBLICATION_INTERVAL: Duration = Duration::from_millis(200);
pub(crate) const WATCHED_FILE_DEBOUNCE_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IndexingStatusPublicationDecision {
    Immediate,
    ScheduleCounterFlush,
    Coalesced,
}

#[derive(Debug, Default)]
struct IndexingStatusPublicationState {
    last_published: Option<IndexingStatusSnapshot>,
    /// Latest sequenced snapshot waiting for the dedicated sender task.
    /// Intermediate updates are dropped so multi-project phase storms cannot
    /// fill tower-lsp's capacity-1 client channel and backpressure stdin dispatch.
    pending_send: Option<IndexingStatusSnapshot>,
    sender_scheduled: bool,
    counter_flush_scheduled: bool,
    counter_pending: bool,
}

impl IndexingStatusPublicationState {
    fn observe(&mut self, snapshot: &IndexingStatusSnapshot) -> IndexingStatusPublicationDecision {
        let immediate = self
            .last_published
            .as_ref()
            .is_none_or(|published| !same_immediate_indexing_state(published, snapshot));
        if immediate {
            self.last_published = Some(snapshot.clone());
            self.counter_pending = false;
            return IndexingStatusPublicationDecision::Immediate;
        }

        self.counter_pending = true;
        if self.counter_flush_scheduled {
            IndexingStatusPublicationDecision::Coalesced
        } else {
            self.counter_flush_scheduled = true;
            IndexingStatusPublicationDecision::ScheduleCounterFlush
        }
    }

    fn flush_counter(&mut self, snapshot: &IndexingStatusSnapshot) -> bool {
        self.counter_flush_scheduled = false;
        if !self.counter_pending {
            return false;
        }
        self.counter_pending = false;
        self.last_published = Some(snapshot.clone());
        true
    }

    fn queue_send(&mut self, snapshot: IndexingStatusSnapshot) -> bool {
        self.pending_send = Some(snapshot);
        if self.sender_scheduled {
            return false;
        }
        self.sender_scheduled = true;
        true
    }

    fn take_pending_send(&mut self) -> Option<IndexingStatusSnapshot> {
        match self.pending_send.take() {
            Some(snapshot) => Some(snapshot),
            None => {
                self.sender_scheduled = false;
                None
            }
        }
    }
}

fn same_immediate_indexing_state(
    published: &IndexingStatusSnapshot,
    next: &IndexingStatusSnapshot,
) -> bool {
    published.aggregate == next.aggregate
        && published.projects.len() == next.projects.len()
        && published
            .projects
            .iter()
            .zip(&next.projects)
            .all(|(published, next)| {
                published.root == next.root
                    && published.generation == next.generation
                    && published.phase == next.phase
                    && published.project_navigation_ready_ms == next.project_navigation_ready_ms
                    && published.dependency_navigation_ready_ms
                        == next.dependency_navigation_ready_ms
                    && published.failure == next.failure
            })
}

#[derive(Debug, Default)]
struct WatchedFileChangeBatch {
    generation: u64,
    changes: BTreeMap<String, FileEvent>,
}

impl WatchedFileChangeBatch {
    fn queue(&mut self, changes: Vec<FileEvent>) -> u64 {
        self.generation = self.generation.checked_add(1).expect(
            "INVARIANT VIOLATED: watched-file debounce generation overflowed. This is a bug because one server cannot receive 2^64 watched-file batches. Fix: inspect the client watcher storm that exhausted the generation counter.",
        );
        for change in changes {
            self.changes.insert(change.uri.to_string(), change);
        }
        self.generation
    }

    fn take(&mut self, generation: u64) -> Option<Vec<FileEvent>> {
        if generation != self.generation || self.changes.is_empty() {
            return None;
        }
        Some(std::mem::take(&mut self.changes).into_values().collect())
    }

    fn cancel(&mut self) {
        self.generation = self.generation.checked_add(1).expect(
            "INVARIANT VIOLATED: watched-file debounce generation overflowed during cancellation. This is a bug because one server cannot cancel 2^64 watcher batches. Fix: inspect the shutdown or workspace lifecycle loop that exhausted the generation counter.",
        );
        self.changes.clear();
    }
}

fn new_core_engine_cache(
) -> crate::single_flight::BoundedSingleFlightCache<String, ruby_analysis::engine::AnalysisEngine> {
    crate::single_flight::BoundedSingleFlightCache::new(
        CORE_ENGINE_CACHE_MAX_ENTRIES,
        CORE_ENGINE_CACHE_MAX_WEIGHT_BYTES,
        |engine: &ruby_analysis::engine::AnalysisEngine| {
            u64::try_from(engine.estimated_memory_stats().total()).expect(
                "INVARIANT VIOLATED: a core template heap estimate does not fit u64. This is a bug because one in-memory engine cannot exceed the process address space. Fix: inspect engine memory estimation overflow.",
            )
        },
    )
}

fn new_runtime_stdlib_path_cache() -> crate::single_flight::BoundedSingleFlightCache<
    crate::indexer::indexer_stdlib::RuntimeStdlibPathKey,
    crate::indexer::indexer_stdlib::RuntimeStdlibPaths,
> {
    crate::single_flight::BoundedSingleFlightCache::new(
        RUNTIME_STDLIB_PATH_CACHE_MAX_ENTRIES,
        RUNTIME_STDLIB_PATH_CACHE_MAX_WEIGHT_BYTES,
        crate::indexer::indexer_stdlib::RuntimeStdlibPaths::estimated_weight_bytes,
    )
}

/// Check if a process with the given PID is still running.
/// Returns true if the process is alive, false if it has exited.
#[cfg(unix)]
fn is_process_alive(pid: u32) -> bool {
    // On Unix, sending signal 0 to a process checks if it exists without actually sending a signal
    // kill(pid, 0) returns 0 if the process exists and we have permission to send it signals
    // It returns -1 with ESRCH if the process doesn't exist
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

#[cfg(windows)]
fn is_process_alive(pid: u32) -> bool {
    use std::ptr::null_mut;

    // On Windows, we try to open the process with minimal access rights
    // If the process doesn't exist, OpenProcess returns NULL
    unsafe {
        let handle = windows_sys::Win32::System::Threading::OpenProcess(
            windows_sys::Win32::System::Threading::PROCESS_QUERY_LIMITED_INFORMATION,
            0, // bInheritHandle = FALSE
            pid,
        );

        if handle.is_null() {
            return false;
        }

        // Check if the process has exited
        let mut exit_code: u32 = 0;
        let result =
            windows_sys::Win32::System::Threading::GetExitCodeProcess(handle, &mut exit_code);

        windows_sys::Win32::Foundation::CloseHandle(handle);

        // STILL_ACTIVE (259) means the process is still running
        const STILL_ACTIVE: u32 = 259;
        result != 0 && exit_code == STILL_ACTIVE
    }
}

/// One Ruby project root. Files are routed to the workspace whose `root_path`
/// is the longest prefix of the file's path. Each project owns an isolated
/// analysis engine; external documents may retain one project's context.
#[derive(Clone)]
pub struct Workspace {
    pub root_uri: Url,
    pub root_path: PathBuf,
    pub indexing_status: Arc<ProjectIndexingStatus>,
    pub analysis_engine: Arc<RwLock<AnalysisEngine>>,
    pub effective_runtime: Arc<RwLock<Option<SelectedRuntimeDescriptor>>>,
    pub detected_ruby_version: Arc<RwLock<Option<String>>>,
    pub runtime_classpath_fingerprint_sha256: Arc<RwLock<Option<String>>>,
    pub(crate) jruby_import_provider: Arc<RwLock<Option<Arc<JrubyImportProvider>>>>,
    pub(crate) extension_project_context_seed: Arc<RwLock<ProjectContextSeed>>,
    pub(crate) navigation_demands: NavigationDemandController,
    workspace_folder_uris: Arc<RwLock<std::collections::HashSet<Url>>>,
}

impl Workspace {
    pub fn new(root_uri: Url) -> Self {
        Self::for_workspace_folder(root_uri.clone(), root_uri, false)
    }

    fn for_workspace_folder(
        root_uri: Url,
        workspace_folder_uri: Url,
        workspace_trusted: bool,
    ) -> Self {
        let root_path = root_uri
            .to_file_path()
            .unwrap_or_else(|_| PathBuf::from(root_uri.path()));
        let extension_project_context_seed = Arc::new(RwLock::new(ProjectContextSeed::detect(
            root_uri.to_string(),
            &root_path,
            workspace_trusted,
            None,
        )));
        Self {
            root_uri,
            indexing_status: Arc::new(ProjectIndexingStatus::new(root_path.clone())),
            root_path,
            analysis_engine: Arc::new(RwLock::new(AnalysisEngine::new())),
            effective_runtime: Arc::new(RwLock::new(None)),
            detected_ruby_version: Arc::new(RwLock::new(None)),
            runtime_classpath_fingerprint_sha256: Arc::new(RwLock::new(None)),
            jruby_import_provider: Arc::new(RwLock::new(None)),
            extension_project_context_seed,
            navigation_demands: NavigationDemandController::default(),
            workspace_folder_uris: Arc::new(RwLock::new(std::collections::HashSet::from([
                workspace_folder_uri,
            ]))),
        }
    }

    #[doc(hidden)]
    pub fn begin_indexing_run(&self) -> IndexingRun {
        let run = self.indexing_status.begin_run();
        self.navigation_demands.begin_generation(run.generation());
        run
    }

    pub(crate) fn cancel_current_indexing_run(&self) {
        let generation = self.indexing_status.snapshot().generation;
        let _ = self.indexing_status.cancel_current();
        self.navigation_demands.cancel_generation(generation);
    }
}

#[derive(Clone)]
pub struct RubyLanguageServer {
    pub client: Option<Client>,
    /// Registered workspace folders. Routed by longest-prefix path match in
    /// `workspace_for_uri`.
    pub workspaces: Arc<RwLock<Vec<Workspace>>>,
    pub docs: Arc<Mutex<HashMap<Url, Arc<RwLock<RubyDocument>>>>>,
    document_semantic_locks: Arc<Mutex<HashMap<Url, Weak<tokio::sync::Mutex<()>>>>>,
    /// Analysis state for open files outside every registered Ruby project.
    /// Project facts live in `Workspace::analysis_engine` and must never be
    /// written here.
    pub analysis_engine: Arc<RwLock<AnalysisEngine>>,
    /// Project context retained for external dependency documents reached from
    /// a project-owned navigation result. Values are project root URIs rather
    /// than engine handles so workspace removal cannot leave a live stale
    /// semantic owner behind.
    external_document_projects: Arc<RwLock<HashMap<Url, Url>>>,
    pub config: Arc<Mutex<RubyFastLspConfig>>,
    discovered_runtimes: Arc<tokio::sync::OnceCell<Vec<DiscoveredRuntime>>>,
    pub extension_registry: ExtensionRegistryHandle,
    pub indexing_scheduler: IndexingScheduler,
    pub indexing_resources: IndexingResourceGovernor,
    pub core_engine_cache: crate::single_flight::BoundedSingleFlightCache<
        String,
        ruby_analysis::engine::AnalysisEngine,
    >,
    pub(crate) runtime_stdlib_path_cache: crate::single_flight::BoundedSingleFlightCache<
        crate::indexer::indexer_stdlib::RuntimeStdlibPathKey,
        crate::indexer::indexer_stdlib::RuntimeStdlibPaths,
    >,
    pub gem_dependency_cache: crate::single_flight::BoundedSingleFlightCache<
        crate::dependency_product::GemDependencyProductKey,
        crate::dependency_product::GemDependencyProduct,
    >,
    pub classpath_file_product_cache: crate::runtime::jruby::classpath::ClasspathFileProductCache,
    pub java_artifact_product_cache: crate::runtime::jruby::java_catalog::JavaArtifactProductCache,
    pub persistent_derived_product_cache: crate::persistent_cache::PersistentDerivedProductCache,
    pub gem_dependency_binding_counters:
        Arc<crate::dependency_product::GemDependencyBindingCounters>,
    indexing_status_sequence: Arc<AtomicU64>,
    indexing_status_publication: Arc<tokio::sync::Mutex<IndexingStatusPublicationState>>,
    watched_file_changes: Arc<Mutex<WatchedFileChangeBatch>>,
    pub extension_watch_dynamic_registration: Arc<AtomicBool>,
    pub extension_watch_registration: Arc<tokio::sync::Mutex<Vec<String>>>,
    pub namespace_tree_cache: Arc<Mutex<Option<(u64, NamespaceTreeResponse)>>>,
    pub cache_invalidation_timer: Arc<Mutex<Option<Instant>>>,
    /// Timer for debounced reindexing on document changes
    pub reindex_timer: Arc<Mutex<Option<(Instant, Url)>>>,
    /// The process ID of the parent process (VS Code extension host).
    /// Used to detect when the parent process dies so we can exit cleanly.
    pub parent_process_id: Arc<Mutex<Option<u32>>>,
    #[cfg(test)]
    published_diagnostics: Arc<Mutex<HashMap<Url, Vec<Diagnostic>>>>,
    #[cfg(test)]
    user_cache_root_override: Arc<Mutex<Option<PathBuf>>>,
}

impl RubyLanguageServer {
    #[doc(hidden)]
    pub fn runtime_stdlib_path_cache_snapshot(&self) -> crate::single_flight::SingleFlightSnapshot {
        self.runtime_stdlib_path_cache.snapshot()
    }

    #[doc(hidden)]
    pub fn runtime_stdlib_path_cache_retained_weight(&self) -> u64 {
        self.runtime_stdlib_path_cache.retained_weight()
    }

    #[doc(hidden)]
    pub fn java_artifact_product_cache_snapshot(
        &self,
    ) -> crate::single_flight::SingleFlightSnapshot {
        self.java_artifact_product_cache.snapshot()
    }

    #[doc(hidden)]
    pub fn java_artifact_product_cache_retained_weight(&self) -> u64 {
        self.java_artifact_product_cache.retained_weight_bytes()
    }

    pub fn new(client: Client) -> Result<Self> {
        let config = RubyFastLspConfig::default();
        let user_cache_root = crate::utils::ruby_fast_lsp_user_cache_root()?;
        let persistent_cache =
            crate::persistent_cache::PersistentDerivedProductCache::new(user_cache_root);
        // Initialization owns the first extension discovery/load pass so it can
        // execute under the process resource governor off the LSP reactor.
        let extension_registry =
            ExtensionRegistryHandle::empty_with_cache(persistent_cache.clone());
        Ok(Self {
            client: Some(client),
            workspaces: Arc::new(RwLock::new(Vec::new())),
            docs: Arc::new(Mutex::new(HashMap::new())),
            document_semantic_locks: Arc::new(Mutex::new(HashMap::new())),
            analysis_engine: Arc::new(RwLock::new(AnalysisEngine::new())),
            external_document_projects: Arc::new(RwLock::new(HashMap::new())),
            config: Arc::new(Mutex::new(config)),
            discovered_runtimes: Arc::new(tokio::sync::OnceCell::new()),
            extension_registry,
            indexing_scheduler: IndexingScheduler::default(),
            indexing_resources: IndexingResourceGovernor::default(),
            core_engine_cache: new_core_engine_cache(),
            runtime_stdlib_path_cache: new_runtime_stdlib_path_cache(),
            gem_dependency_cache: crate::single_flight::BoundedSingleFlightCache::ephemeral(
                |product: &crate::dependency_product::GemDependencyProduct| {
                    product.estimated_weight_bytes()
                },
            ),
            classpath_file_product_cache:
                crate::runtime::jruby::classpath::ClasspathFileProductCache::default(),
            java_artifact_product_cache:
                crate::runtime::jruby::java_catalog::JavaArtifactProductCache::default(),
            persistent_derived_product_cache: persistent_cache,
            gem_dependency_binding_counters: Arc::new(
                crate::dependency_product::GemDependencyBindingCounters::default(),
            ),
            indexing_status_sequence: Arc::new(AtomicU64::new(0)),
            indexing_status_publication: Arc::new(tokio::sync::Mutex::new(
                IndexingStatusPublicationState::default(),
            )),
            watched_file_changes: Arc::new(Mutex::new(WatchedFileChangeBatch::default())),
            extension_watch_dynamic_registration: Arc::new(AtomicBool::new(false)),
            extension_watch_registration: Arc::new(tokio::sync::Mutex::new(Vec::new())),
            namespace_tree_cache: Arc::new(Mutex::new(None)),
            cache_invalidation_timer: Arc::new(Mutex::new(None)),
            reindex_timer: Arc::new(Mutex::new(None)),
            parent_process_id: Arc::new(Mutex::new(None)),
            #[cfg(test)]
            published_diagnostics: Arc::new(Mutex::new(HashMap::new())),
            #[cfg(test)]
            user_cache_root_override: Arc::new(Mutex::new(None)),
        })
    }

    /// Returns true once every registered workspace has finished its initial
    /// indexing pass. With no workspaces registered, returns true vacuously
    /// (orphan-only mode has no coordinator to wait on).
    pub fn is_indexing_complete(&self) -> bool {
        self.workspaces
            .read()
            .iter()
            .all(|workspace| workspace.indexing_status.snapshot().is_ready())
    }

    pub fn prioritize_indexing_project(&self, project_root: &Path) {
        let navigation_pending = self
            .workspaces
            .read()
            .iter()
            .find(|workspace| workspace.root_path == project_root)
            .is_some_and(|workspace| {
                workspace
                    .indexing_status
                    .snapshot()
                    .phase
                    .project_navigation_pending()
            });
        self.indexing_scheduler
            .prioritize_active_project(project_root);
        self.indexing_resources
            .prioritize_active_project_with_navigation_pending(project_root, navigation_pending);
    }

    pub(crate) fn queue_watched_file_changes(&self, changes: Vec<FileEvent>) -> u64 {
        self.watched_file_changes.lock().queue(changes)
    }

    pub(crate) fn take_watched_file_changes(&self, generation: u64) -> Option<Vec<FileEvent>> {
        self.watched_file_changes.lock().take(generation)
    }

    pub(crate) fn cancel_watched_file_changes(&self) {
        self.watched_file_changes.lock().cancel();
    }

    pub(crate) fn document_semantic_lock(&self, uri: &Url) -> Arc<tokio::sync::Mutex<()>> {
        let mut locks = self.document_semantic_locks.lock();
        locks.retain(|_, lock| lock.strong_count() > 0);
        if let Some(lock) = locks.get(uri).and_then(Weak::upgrade) {
            return lock;
        }
        let lock = Arc::new(tokio::sync::Mutex::new(()));
        locks.insert(uri.clone(), Arc::downgrade(&lock));
        lock
    }

    pub fn indexing_status_snapshot(&self) -> IndexingStatusSnapshot {
        let mut projects = self
            .workspaces
            .read()
            .iter()
            .map(|workspace| workspace.indexing_status.snapshot())
            .collect::<Vec<_>>();
        projects.sort_by(|left, right| left.root.cmp(&right.root));
        let scheduler = self.indexing_scheduler.snapshot();
        let mut aggregate = IndexingAggregateSnapshot {
            discovered: 0,
            queued: 0,
            active: scheduler.active,
            ready: 0,
            failed: 0,
            cancelled: 0,
            concurrency_limit: scheduler.concurrency_limit,
        };
        for project in &projects {
            match project.phase {
                IndexingPhase::Discovered => aggregate.discovered += 1,
                IndexingPhase::Queued => aggregate.queued += 1,
                IndexingPhase::Ready => aggregate.ready += 1,
                IndexingPhase::Failed => aggregate.failed += 1,
                IndexingPhase::Cancelled => aggregate.cancelled += 1,
                IndexingPhase::ResolvingRuntime
                | IndexingPhase::DiscoveringInputs
                | IndexingPhase::IndexingCore
                | IndexingPhase::IndexingProject
                | IndexingPhase::ProjectNavigationReady
                | IndexingPhase::IndexingDependencies
                | IndexingPhase::DependencyNavigationReady
                | IndexingPhase::ResolvingSemantics
                | IndexingPhase::PublishingDiagnostics => {}
            }
        }
        let persistent_gem_products = self.persistent_derived_product_cache.gem_product_snapshot();
        let persistent_java_artifacts = self
            .persistent_derived_product_cache
            .java_artifact_snapshot();
        let persistent_compiled_wasm = self
            .persistent_derived_product_cache
            .compiled_wasm_snapshot();
        let gem_single_flight = self.gem_dependency_cache.snapshot();
        let classpath_file_single_flight = self.classpath_file_product_cache.snapshot();
        let java_artifact_single_flight = self.java_artifact_product_cache.snapshot();
        IndexingStatusSnapshot {
            sequence: self.indexing_status_sequence.load(Ordering::Acquire),
            projects,
            aggregate,
            reuse: IndexingReuseSnapshot {
                persistent_gem_products: IndexingPersistentProductReuseSnapshot {
                    lookups: persistent_gem_products.lookups,
                    hits: persistent_gem_products.hits,
                    producers: persistent_gem_products.producers,
                    corruptions: persistent_gem_products.corruptions,
                },
                persistent_java_artifacts: IndexingPersistentProductReuseSnapshot {
                    lookups: persistent_java_artifacts.lookups,
                    hits: persistent_java_artifacts.hits,
                    producers: persistent_java_artifacts.producers,
                    corruptions: persistent_java_artifacts.corruptions,
                },
                persistent_compiled_wasm: IndexingPersistentProductReuseSnapshot {
                    lookups: persistent_compiled_wasm.lookups,
                    hits: persistent_compiled_wasm.hits,
                    producers: persistent_compiled_wasm.producers,
                    corruptions: persistent_compiled_wasm.corruptions,
                },
                gem_single_flight: IndexingSingleFlightReuseSnapshot {
                    lookups: gem_single_flight.lookups,
                    hits: gem_single_flight.hits,
                    joined_flights: gem_single_flight.joined_flights,
                    producers: gem_single_flight.producers,
                    failures: gem_single_flight.failures,
                },
                classpath_file_single_flight: IndexingSingleFlightReuseSnapshot {
                    lookups: classpath_file_single_flight.lookups,
                    hits: classpath_file_single_flight.hits,
                    joined_flights: classpath_file_single_flight.joined_flights,
                    producers: classpath_file_single_flight.producers,
                    failures: classpath_file_single_flight.failures,
                },
                java_artifact_single_flight: IndexingSingleFlightReuseSnapshot {
                    lookups: java_artifact_single_flight.lookups,
                    hits: java_artifact_single_flight.hits,
                    joined_flights: java_artifact_single_flight.joined_flights,
                    producers: java_artifact_single_flight.producers,
                    failures: java_artifact_single_flight.failures,
                },
            },
        }
    }

    async fn next_indexing_status_snapshot(&self) -> IndexingStatusSnapshot {
        let _publication = self.indexing_status_publication.lock().await;
        self.sequence_indexing_status_snapshot(self.indexing_status_snapshot())
    }

    fn sequence_indexing_status_snapshot(
        &self,
        mut snapshot: IndexingStatusSnapshot,
    ) -> IndexingStatusSnapshot {
        let sequence = self
            .indexing_status_sequence
            .fetch_add(1, Ordering::AcqRel)
            .checked_add(1)
            .expect(
                "INVARIANT VIOLATED: global indexing status sequence overflowed. This is a bug because one server cannot publish 2^64 snapshots. Fix: inspect the status publication loop.",
            );
        snapshot.sequence = sequence;
        snapshot
    }

    pub async fn publish_indexing_status(&self) {
        let schedule_sender;
        let schedule_counter_flush;
        {
            let mut publication = self.indexing_status_publication.lock().await;
            let snapshot = self.indexing_status_snapshot();
            match publication.observe(&snapshot) {
                IndexingStatusPublicationDecision::Immediate => {
                    let snapshot = self.sequence_indexing_status_snapshot(snapshot);
                    schedule_sender = publication.queue_send(snapshot);
                    schedule_counter_flush = false;
                }
                IndexingStatusPublicationDecision::ScheduleCounterFlush => {
                    schedule_sender = false;
                    schedule_counter_flush = true;
                }
                IndexingStatusPublicationDecision::Coalesced => {
                    return;
                }
            }
        }
        if schedule_counter_flush {
            let server = self.clone();
            tokio::spawn(async move {
                sleep(INDEXING_COUNTER_PUBLICATION_INTERVAL).await;
                server.flush_indexing_counter_status().await;
            });
        }
        if schedule_sender {
            self.spawn_indexing_status_sender();
        }
    }

    async fn flush_indexing_counter_status(&self) {
        let schedule_sender;
        {
            let mut publication = self.indexing_status_publication.lock().await;
            let snapshot = self.indexing_status_snapshot();
            if !publication.flush_counter(&snapshot) {
                return;
            }
            let snapshot = self.sequence_indexing_status_snapshot(snapshot);
            schedule_sender = publication.queue_send(snapshot);
        }
        if schedule_sender {
            self.spawn_indexing_status_sender();
        }
    }

    fn spawn_indexing_status_sender(&self) {
        let server = self.clone();
        tokio::spawn(async move {
            server.drain_indexing_status_sends().await;
        });
    }

    async fn drain_indexing_status_sends(&self) {
        loop {
            let snapshot = {
                let mut publication = self.indexing_status_publication.lock().await;
                match publication.take_pending_send() {
                    Some(snapshot) => snapshot,
                    None => return,
                }
            };
            if let Some(client) = &self.client {
                let start = Instant::now();
                let _ = client
                    .send_notification::<IndexingStatusNotification>(snapshot)
                    .await;
                let elapsed = start.elapsed();
                if elapsed >= Duration::from_millis(50) {
                    warn!(
                        "[PERF] indexing status notification send took {:?} — stdout backpressure \
                         can stall LSP request dispatch when the client falls behind",
                        elapsed
                    );
                }
            }
        }
    }

    pub async fn handle_indexing_status(
        &self,
        params: IndexingStatusParams,
    ) -> LspResult<IndexingStatusSnapshot> {
        if let Some(active_document_uri) = params.active_document_uri {
            if let Some(workspace) = self.workspace_for_uri(&active_document_uri) {
                self.prioritize_indexing_project(&workspace.root_path);
            }
        }
        Ok(self.next_indexing_status_snapshot().await)
    }

    /// Set the parent process ID and start monitoring it.
    /// If the parent process dies, the LSP server will exit.
    pub fn set_parent_process_id(&self, pid: Option<u32>) {
        *self.parent_process_id.lock() = pid;
        if let Some(pid) = pid {
            self.start_parent_process_monitor(pid);
        }
    }

    /// Optional 1s reactor heartbeat for diagnosing slow goto clicks.
    /// Enable with `RUBY_FAST_LSP_REACTOR_HEARTBEAT=1`. Continuing ticks with no
    /// request log mean the client held the request; missing ticks mean the
    /// reactor stalled. Process-wide once so FakeEditor suites do not spawn
    /// overlapping tickers.
    pub fn start_reactor_heartbeat(&self) {
        use std::sync::Once;
        static START: Once = Once::new();
        let enabled = std::env::var_os("RUBY_FAST_LSP_REACTOR_HEARTBEAT")
            .is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true"));
        if !enabled {
            return;
        }
        START.call_once(|| {
            info!(
                "[REACTOR] starting 1s heartbeat — during a slow goto: continuing ticks with no \
                 request log means the client held the request; missing ticks mean the reactor stalled"
            );
            tokio::spawn(async move {
                let mut tick = 0u64;
                let mut interval = tokio::time::interval(Duration::from_secs(1));
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                // Interval fires immediately; skip the zero-delay first tick.
                interval.tick().await;
                loop {
                    interval.tick().await;
                    tick = tick.saturating_add(1);
                    info!("[REACTOR] heartbeat tick={tick}");
                }
            });
        });
    }

    /// Start a background task that monitors the parent process.
    /// If the parent process is no longer running, exit the server.
    fn start_parent_process_monitor(&self, parent_pid: u32) {
        info!("Starting parent process monitor for PID: {}", parent_pid);

        tokio::spawn(async move {
            let check_interval = Duration::from_secs(5);

            loop {
                sleep(check_interval).await;

                if !is_process_alive(parent_pid) {
                    warn!(
                        "Parent process (PID: {}) is no longer running. Exiting LSP server.",
                        parent_pid
                    );
                    // Give a moment for any pending operations to complete
                    sleep(Duration::from_millis(100)).await;
                    exit(0);
                }
            }
        });
    }

    // ========================================================================
    // Multi-workspace routing
    // ========================================================================

    /// Find the registered workspace whose `root_path` is the longest prefix
    /// of the given URI's filesystem path. Returns `None` if the URI does not
    /// belong to any registered workspace.
    pub fn workspace_for_uri(&self, uri: &Url) -> Option<Workspace> {
        let file_path = uri.to_file_path().ok()?;
        let workspaces = self.workspaces.read();
        let mut best: Option<&Workspace> = None;
        let mut best_len = 0usize;
        for ws in workspaces.iter() {
            if file_path.starts_with(&ws.root_path) {
                let len = ws.root_path.as_os_str().len();
                if len >= best_len {
                    best_len = len;
                    best = Some(ws);
                }
            }
        }
        best.cloned()
    }

    /// Return the isolated semantic engine that owns a document URI. Files
    /// outside registered workspaces use the orphan engine.
    pub fn analysis_engine_for_uri(&self, uri: &Url) -> Arc<RwLock<AnalysisEngine>> {
        self.analysis_workspace_for_uri(uri)
            .map(|workspace| workspace.analysis_engine)
            .unwrap_or_else(|| self.analysis_engine.clone())
    }

    pub(crate) fn extension_project_context_for_uri(
        &self,
        uri: &Url,
        source_kind: SourceKind,
    ) -> Option<ProjectContext> {
        self.extension_project_context_snapshot_for_uri(uri, source_kind)
            .map(|snapshot| snapshot.context)
    }

    pub(crate) fn extension_project_context_snapshot_for_uri(
        &self,
        uri: &Url,
        source_kind: SourceKind,
    ) -> Option<ProjectContextSnapshot> {
        self.analysis_workspace_for_uri(uri).map(|workspace| {
            workspace
                .extension_project_context_seed
                .read()
                .context_snapshot(uri.to_string(), source_kind)
        })
    }

    pub(crate) fn extension_project_context_for_document(
        &self,
        uri: &Url,
    ) -> Option<ProjectContext> {
        let path = uri
            .to_file_path()
            .unwrap_or_else(|_| PathBuf::from(uri.to_string()));
        let engine = self.analysis_engine_for_uri(uri);
        let kind = {
            let engine = engine.read();
            engine
                .file_id(&path)
                .and_then(|file_id| engine.file(file_id).map(|file| file.kind))
                .unwrap_or(SourceKind::Excluded)
        };
        self.extension_project_context_for_uri(uri, kind)
    }

    pub(crate) fn extension_project_context_seed_for_root(
        &self,
        root: &PathBuf,
    ) -> Option<Arc<RwLock<ProjectContextSeed>>> {
        self.workspaces
            .read()
            .iter()
            .find(|workspace| &workspace.root_path == root)
            .map(|workspace| workspace.extension_project_context_seed.clone())
    }

    pub(crate) fn set_extension_project_ruby_version(
        &self,
        root: &PathBuf,
        ruby_version: Option<String>,
    ) {
        let Some(seed) = self.extension_project_context_seed_for_root(root) else {
            return;
        };
        seed.write().ruby_version = ruby_version.clone();
        if let Some(workspace) = self
            .workspaces
            .read()
            .iter()
            .find(|workspace| &workspace.root_path == root)
        {
            *workspace.detected_ruby_version.write() = ruby_version;
        }
    }

    pub(crate) fn set_runtime_classpath_fingerprint(
        &self,
        root: &PathBuf,
        fingerprint: Option<String>,
    ) {
        if let Some(workspace) = self
            .workspaces
            .read()
            .iter()
            .find(|workspace| &workspace.root_path == root)
        {
            *workspace.runtime_classpath_fingerprint_sha256.write() = fingerprint;
        }
    }

    pub(crate) fn set_jruby_import_provider(
        &self,
        root: &Path,
        provider: Option<Arc<JrubyImportProvider>>,
    ) {
        if let Some(workspace) = self
            .workspaces
            .read()
            .iter()
            .find(|workspace| workspace.root_path == root)
        {
            *workspace.jruby_import_provider.write() = provider;
        }
    }

    pub(crate) fn jruby_import_provider_for_uri(
        &self,
        uri: &Url,
    ) -> Option<Arc<JrubyImportProvider>> {
        self.analysis_workspace_for_uri(uri)
            .and_then(|workspace| workspace.jruby_import_provider.read().clone())
    }

    pub(crate) fn set_effective_runtime(
        &self,
        root: &PathBuf,
        runtime: Option<SelectedRuntimeDescriptor>,
    ) {
        if let Some(workspace) = self
            .workspaces
            .read()
            .iter()
            .find(|workspace| &workspace.root_path == root)
        {
            *workspace.effective_runtime.write() = runtime;
        }
    }

    pub(crate) fn refresh_extension_project_dependencies_for_uri(&self, uri: &Url) {
        let Some(workspace) = self.analysis_workspace_for_uri(uri) else {
            return;
        };
        workspace
            .extension_project_context_seed
            .write()
            .refresh_dependencies(&workspace.root_path);
    }

    /// Return the project semantic context for a URI. Project-owned paths win,
    /// followed by retained external navigation provenance, followed by a
    /// unique project engine that already owns the exact dependency path.
    pub fn analysis_workspace_for_uri(&self, uri: &Url) -> Option<Workspace> {
        if let Some(workspace) = self.workspace_for_uri(uri) {
            return Some(workspace);
        }

        if let Some(root_uri) = self.external_document_projects.read().get(uri).cloned() {
            if let Some(workspace) = self
                .workspaces
                .read()
                .iter()
                .find(|workspace| workspace.root_uri == root_uri)
                .cloned()
            {
                return Some(workspace);
            }
            self.external_document_projects.write().remove(uri);
        }

        let path = uri.to_file_path().ok()?;
        let mut owner = None;
        for workspace in self.workspaces.read().iter() {
            if workspace.analysis_engine.read().file_id(&path).is_none() {
                continue;
            }
            if owner.is_some() {
                return None;
            }
            owner = Some(workspace.clone());
        }
        owner
    }

    /// Retain the originating project for external locations returned by a
    /// semantic request. LSP follow-up requests carry only the target URI, so
    /// this provenance is required to preserve the correct bundle context.
    pub fn retain_external_document_project(&self, uri: &Url, project: &Workspace) {
        if self.workspace_for_uri(uri).is_none() {
            self.external_document_projects
                .write()
                .insert(uri.clone(), project.root_uri.clone());
        }
    }

    pub fn release_external_document_project(&self, uri: &Url) {
        self.external_document_projects.write().remove(uri);
    }

    pub(crate) fn release_external_documents_for_project(&self, project_root_uri: &Url) {
        self.external_document_projects
            .write()
            .retain(|_, retained_root| retained_root != project_root_uri);
    }

    /// Snapshot every active project engine plus the orphan engine.
    pub fn analysis_engines(&self) -> Vec<Arc<RwLock<AnalysisEngine>>> {
        let mut engines = self
            .workspaces
            .read()
            .iter()
            .map(|workspace| workspace.analysis_engine.clone())
            .collect::<Vec<_>>();
        engines.push(self.analysis_engine.clone());
        engines
    }

    pub fn clear_file_from_other_engines(&self, uri: &Url, owner: &Arc<RwLock<AnalysisEngine>>) {
        let path = uri
            .to_file_path()
            .unwrap_or_else(|_| PathBuf::from(uri.to_string()));
        for analysis_engine in self.analysis_engines() {
            if Arc::ptr_eq(&analysis_engine, owner) {
                continue;
            }
            let mut engine = analysis_engine.write();
            if let Some(file_id) = engine.file_id(&path) {
                engine.replace_facts(
                    file_id,
                    ruby_analysis::engine::FileFacts::default(),
                    ruby_analysis::engine::ResolveMode::Immediate,
                );
            }
        }
    }

    pub fn open_or_update_analysis_file(
        &self,
        uri: &Url,
        source: impl Into<String>,
    ) -> SourceFileId {
        self.open_or_update_analysis_file_with_kind(uri, source, SourceKind::Project)
    }

    pub fn open_or_update_analysis_file_with_kind(
        &self,
        uri: &Url,
        source: impl Into<String>,
        kind: SourceKind,
    ) -> SourceFileId {
        let path = uri
            .to_file_path()
            .unwrap_or_else(|_| PathBuf::from(uri.to_string()));
        self.analysis_engine_for_uri(uri).write().register_file(
            ruby_analysis::engine::SourceFileInput {
                path,
                content: source.into(),
                kind,
            },
        )
    }

    /// Register a new workspace. If a workspace with the same root URI is
    /// already registered, returns the existing one without creating a new
    /// index. Returns the (existing or newly created) `Workspace`.
    pub fn add_workspace(&self, root_uri: Url) -> Workspace {
        self.add_project(root_uri.clone(), root_uri)
    }

    /// Register an editor workspace folder, expanding a container folder into
    /// its nearest Gemfile-owned Ruby projects.
    pub fn add_workspace_folder(&self, folder_uri: Url) -> anyhow::Result<Vec<Workspace>> {
        let folder_path = folder_uri.to_file_path().map_err(|_| {
            anyhow::anyhow!(
                "Workspace folder URI is not a filesystem path: {}",
                folder_uri
            )
        })?;
        let explicit_roots = self.config.lock().indexing.project_roots.clone();
        let roots = crate::indexer::project_roots::discover_project_roots_with_explicit(
            &folder_path,
            &explicit_roots,
        )?;
        roots
            .into_iter()
            .map(|root| {
                let root_uri = Url::from_directory_path(&root).map_err(|_| {
                    anyhow::anyhow!(
                        "Ruby project root is not a valid file URI: {}",
                        root.display()
                    )
                })?;
                Ok(self.add_project(root_uri, folder_uri.clone()))
            })
            .collect()
    }

    fn add_project(&self, root_uri: Url, workspace_folder_uri: Url) -> Workspace {
        {
            let workspaces = self.workspaces.read();
            if let Some(existing) = workspaces.iter().find(|w| w.root_uri == root_uri) {
                existing
                    .workspace_folder_uris
                    .write()
                    .insert(workspace_folder_uri);
                return existing.clone();
            }
        }
        let workspace_trusted = self.config.lock().workspace_trusted;
        let ws = Workspace::for_workspace_folder(root_uri, workspace_folder_uri, workspace_trusted);
        self.workspaces.write().push(ws.clone());
        ws
    }

    pub fn remove_workspace(&self, root_uri: &Url) {
        self.workspaces.write().retain(|workspace| {
            if workspace.root_uri != *root_uri {
                return true;
            }
            workspace.cancel_current_indexing_run();
            false
        });
    }

    pub fn remove_workspace_folder(&self, folder_uri: &Url) {
        self.workspaces.write().retain(|workspace| {
            let mut owners = workspace.workspace_folder_uris.write();
            owners.remove(folder_uri);
            if owners.is_empty() {
                workspace.cancel_current_indexing_run();
                false
            } else {
                true
            }
        });
    }

    pub fn cancel_all_indexing(&self) {
        for workspace in self.workspaces.read().iter() {
            workspace.cancel_current_indexing_run();
        }
    }

    /// Snapshot of all currently registered workspaces.
    pub fn list_workspaces(&self) -> Vec<Workspace> {
        self.workspaces.read().clone()
    }

    pub fn workspace_root_paths(&self) -> Vec<PathBuf> {
        let mut roots = self
            .workspaces
            .read()
            .iter()
            .map(|workspace| workspace.root_path.clone())
            .collect::<Vec<_>>();
        roots.sort();
        roots.dedup();
        roots
    }

    pub fn get_doc(&self, uri: &Url) -> Option<RubyDocument> {
        self.docs
            .lock()
            .get(uri)
            .map(|doc_arc| doc_arc.read().clone())
    }

    /// Publish diagnostics for a document
    pub async fn publish_diagnostics(&self, uri: Url, diagnostics: Vec<Diagnostic>) {
        #[cfg(test)]
        self.published_diagnostics
            .lock()
            .insert(uri.clone(), diagnostics.clone());
        if let Some(client) = &self.client {
            let _ = client.publish_diagnostics(uri, diagnostics, None).await;
        }
    }

    #[cfg(test)]
    pub fn last_published_diagnostics(&self, uri: &Url) -> Vec<Diagnostic> {
        self.published_diagnostics
            .lock()
            .get(uri)
            .cloned()
            .unwrap_or_default()
    }

    #[cfg(test)]
    pub(crate) fn set_user_cache_root_for_tests(&self, root: PathBuf) {
        *self.user_cache_root_override.lock() = Some(root.clone());
        self.persistent_derived_product_cache
            .set_root_for_tests(root);
    }

    #[cfg(test)]
    pub(crate) fn user_cache_root_for_tests(&self) -> Option<PathBuf> {
        self.user_cache_root_override.lock().clone()
    }

    /// Request the client to refresh inlay hints
    pub async fn refresh_inlay_hints(&self) {
        if let Some(client) = &self.client {
            // Send workspace/inlayHint/refresh request to client
            let _ = client
                .send_request::<tower_lsp::lsp_types::request::InlayHintRefreshRequest>(())
                .await;
        }
    }

    pub async fn handle_namespace_tree_request(
        &self,
        params: NamespaceTreeParams,
    ) -> LspResult<NamespaceTreeResponse> {
        request::handle_namespace_tree(self, params).await
    }

    // ========================================================================
    // Debug Request Handlers
    // ========================================================================

    /// Handle `$/listCommands` - return available custom debug commands.
    pub async fn handle_list_commands(&self) -> LspResult<ListCommandsResponse> {
        request::handle_list_commands(self).await
    }

    /// Handle `ruby-fast-lsp/debug/lookup` - query index for an FQN.
    pub async fn handle_debug_lookup(&self, params: LookupParams) -> LspResult<LookupResponse> {
        request::handle_debug_lookup(self, params).await
    }

    /// Handle `ruby-fast-lsp/debug/stats` - return index statistics.
    pub async fn handle_debug_stats(&self, params: StatsParams) -> LspResult<StatsResponse> {
        request::handle_debug_stats(self, params).await
    }

    /// Handle `ruby-fast-lsp/debug/ancestors` - return inheritance chain.
    pub async fn handle_debug_ancestors(
        &self,
        params: AncestorsParams,
    ) -> LspResult<AncestorsResponse> {
        request::handle_debug_ancestors(self, params).await
    }

    /// Handle `ruby-fast-lsp/debug/methods` - list methods for a class.
    pub async fn handle_debug_methods(&self, params: MethodsParams) -> LspResult<MethodsResponse> {
        request::handle_debug_methods(self, params).await
    }

    /// Handle `ruby-fast-lsp/debug/inference-stats` - get type inference statistics.
    pub async fn handle_debug_inference_stats(
        &self,
        params: crate::capabilities::debug::InferenceStatsParams,
    ) -> LspResult<crate::capabilities::debug::InferenceStatsResponse> {
        request::handle_debug_inference_stats(self, params).await
    }

    /// Handle `ruby/exportGraph` - export the inheritance graph as JSON.
    pub async fn handle_export_graph(
        &self,
        params: crate::capabilities::debug::ExportGraphParams,
    ) -> LspResult<crate::capabilities::debug::ExportGraphResponse> {
        request::handle_export_graph(self, params).await
    }

    /// Handle `ruby-fast-lsp/extensions/status` - list loaded extension states.
    pub async fn handle_extension_status(
        &self,
        params: ExtensionStatusParams,
    ) -> LspResult<ExtensionStatusResponse> {
        request::handle_extension_status(self, params).await
    }

    /// Handle `ruby-fast-lsp/runtime/discover` without exposing editor policy
    /// or mutating any project runtime selection.
    pub async fn handle_runtime_discover(
        &self,
        _params: RuntimeDiscoverParams,
    ) -> LspResult<RuntimeCatalog> {
        Ok(crate::runtime::catalog::runtime_catalog_for_projects(
            self.workspace_root_paths(),
            self.discovered_runtimes().await,
        ))
    }

    async fn discovered_runtimes(&self) -> Vec<DiscoveredRuntime> {
        let indexing_resources = self.indexing_resources.clone();
        self.discovered_runtimes
            .get_or_init(|| crate::runtime::catalog::discover_runtimes(indexing_resources))
            .await
            .clone()
    }

    #[cfg(test)]
    pub(crate) fn set_discovered_runtimes_for_tests(&self, runtimes: Vec<DiscoveredRuntime>) {
        self.discovered_runtimes.set(runtimes).expect(
            "INVARIANT VIOLATED: test runtime catalog was initialized more than once. This is a bug because each test server must own one immutable discovery snapshot. Fix: create a fresh RubyLanguageServer per runtime test.",
        );
    }

    pub(crate) async fn resolve_auto_runtime(
        &self,
        project_root: &std::path::Path,
    ) -> Result<Option<SelectedRuntimeDescriptor>> {
        let Some(marker) = crate::runtime::catalog::project_runtime_marker(project_root)? else {
            return Ok(None);
        };
        let runtime = crate::runtime::catalog::select_runtime_for_marker(
            &marker,
            &self.discovered_runtimes().await,
        )?;
        let Some(runtime) = runtime else {
            warn!(
                "Project runtime marker `{marker}` has no exact installed runtime for {}",
                project_root.display()
            );
            return Ok(None);
        };
        if runtime.support_status != crate::runtime::catalog::RuntimeSupportStatus::Supported {
            return Err(anyhow::anyhow!(
                "project runtime marker `{marker}` selects unsupported {}",
                runtime.display_name
            ));
        }
        Ok(Some(runtime.into()))
    }

    pub async fn handle_runtime_status(
        &self,
        params: RuntimeStatusParams,
    ) -> LspResult<RuntimeStatus> {
        let config = self.config.lock().clone();
        let mut projects = self
            .list_workspaces()
            .into_iter()
            .filter(|workspace| {
                params
                    .project_root
                    .as_ref()
                    .is_none_or(|root| root == &workspace.root_path)
            })
            .map(|workspace| {
                let root = workspace.root_path.to_string_lossy();
                let selection = config
                    .runtime
                    .selection_for_project(&root, &config.ruby_version);
                let (
                    mode,
                    implementation,
                    family,
                    engine_version,
                    compatibility_version,
                    executable,
                    java_home,
                    stub_overlay,
                ) = match selection {
                    crate::config::runtime::EffectiveRuntimeSelection::Explicit(runtime) => {
                        let stub_overlay = (runtime.implementation
                            == crate::runtime::catalog::RuntimeImplementation::Jruby)
                            .then(|| runtime.family.clone());
                        (
                            "explicit".to_string(),
                            Some(runtime.implementation),
                            Some(runtime.family),
                            Some(runtime.engine_version),
                            Some(runtime.compatibility_version),
                            Some(runtime.executable),
                            runtime.java_home,
                            stub_overlay,
                        )
                    }
                    crate::config::runtime::EffectiveRuntimeSelection::Auto => {
                        if let Some(runtime) = workspace.effective_runtime.read().clone() {
                            let stub_overlay = (runtime.implementation
                                == crate::runtime::catalog::RuntimeImplementation::Jruby)
                                .then(|| runtime.family.clone());
                            (
                                "auto".to_string(),
                                Some(runtime.implementation),
                                Some(runtime.family),
                                Some(runtime.engine_version),
                                Some(runtime.compatibility_version),
                                Some(runtime.executable),
                                runtime.java_home,
                                stub_overlay,
                            )
                        } else {
                            ("auto".to_string(), None, None, None, None, None, None, None)
                        }
                    }
                    crate::config::runtime::EffectiveRuntimeSelection::LegacyMriCompatibility {
                        major,
                        minor,
                    } => {
                        let compatibility = format!("{major}.{minor}");
                        (
                            "legacy".to_string(),
                            Some(crate::runtime::catalog::RuntimeImplementation::Mri),
                            Some(compatibility.clone()),
                            None,
                            Some(compatibility),
                            None,
                            None,
                            None,
                        )
                    }
                };
                ProjectRuntimeStatus {
                    root: workspace.root_path,
                    mode,
                    implementation,
                    family,
                    engine_version,
                    compatibility_version,
                    executable,
                    java_home,
                    stub_overlay,
                    classpath_fingerprint_sha256: workspace
                        .runtime_classpath_fingerprint_sha256
                        .read()
                        .clone(),
                    indexing_complete: workspace.indexing_status.snapshot().is_ready(),
                    indexing: workspace.indexing_status.snapshot(),
                }
            })
            .collect::<Vec<_>>();
        projects.sort_by(|left, right| left.root.cmp(&right.root));
        Ok(RuntimeStatus { projects })
    }

    /// Invalidate namespace tree cache with debouncing (300ms delay)
    pub fn invalidate_namespace_tree_cache_debounced(&self) {
        let server = self.clone();
        tokio::spawn(async move {
            // Set the timer to current time
            {
                let mut timer = server.cache_invalidation_timer.lock();
                *timer = Some(Instant::now());
            }

            // Wait for the debounce period
            sleep(Duration::from_millis(300)).await;

            // Check if we should still invalidate (no newer timer was set)
            let should_invalidate = {
                let timer = server.cache_invalidation_timer.lock();
                if let Some(timer_instant) = *timer {
                    timer_instant.elapsed() >= Duration::from_millis(300)
                } else {
                    false
                }
            };

            if should_invalidate {
                *server.namespace_tree_cache.lock() = None;
                debug!("Namespace tree cache invalidated after debounce period");

                // Clear the timer
                *server.cache_invalidation_timer.lock() = None;
            }
        });
    }
}

impl Default for RubyLanguageServer {
    fn default() -> Self {
        let user_cache_root = crate::utils::ruby_fast_lsp_user_cache_root().expect(
            "INVARIANT VIOLATED: the default server could not resolve an absolute user cache root. This is a bug because test and embedded server construction still requires deterministic derived-product ownership. Fix: set HOME, XDG_CACHE_HOME, LOCALAPPDATA, or RUBY_FAST_LSP_CACHE_DIR to an absolute path.",
        );
        let persistent_cache =
            crate::persistent_cache::PersistentDerivedProductCache::new(user_cache_root);
        Self {
            client: None,
            workspaces: Arc::new(RwLock::new(Vec::new())),
            docs: Arc::new(Mutex::new(HashMap::new())),
            document_semantic_locks: Arc::new(Mutex::new(HashMap::new())),
            analysis_engine: Arc::new(RwLock::new(AnalysisEngine::new())),
            external_document_projects: Arc::new(RwLock::new(HashMap::new())),
            config: Arc::new(Mutex::new(RubyFastLspConfig::default())),
            discovered_runtimes: Arc::new(tokio::sync::OnceCell::new()),
            namespace_tree_cache: Arc::new(Mutex::new(None)),
            cache_invalidation_timer: Arc::new(Mutex::new(None)),
            reindex_timer: Arc::new(Mutex::new(None)),
            parent_process_id: Arc::new(Mutex::new(None)),
            #[cfg(test)]
            published_diagnostics: Arc::new(Mutex::new(HashMap::new())),
            #[cfg(test)]
            user_cache_root_override: Arc::new(Mutex::new(None)),
            extension_registry: ExtensionRegistryHandle::from_environment_with_cache(
                persistent_cache.clone(),
            ),
            indexing_scheduler: IndexingScheduler::default(),
            indexing_resources: IndexingResourceGovernor::default(),
            core_engine_cache: new_core_engine_cache(),
            runtime_stdlib_path_cache: new_runtime_stdlib_path_cache(),
            gem_dependency_cache: crate::single_flight::BoundedSingleFlightCache::ephemeral(
                |product: &crate::dependency_product::GemDependencyProduct| {
                    product.estimated_weight_bytes()
                },
            ),
            classpath_file_product_cache:
                crate::runtime::jruby::classpath::ClasspathFileProductCache::default(),
            java_artifact_product_cache:
                crate::runtime::jruby::java_catalog::JavaArtifactProductCache::default(),
            persistent_derived_product_cache: persistent_cache,
            gem_dependency_binding_counters: Arc::new(
                crate::dependency_product::GemDependencyBindingCounters::default(),
            ),
            indexing_status_sequence: Arc::new(AtomicU64::new(0)),
            indexing_status_publication: Arc::new(tokio::sync::Mutex::new(
                IndexingStatusPublicationState::default(),
            )),
            watched_file_changes: Arc::new(Mutex::new(WatchedFileChangeBatch::default())),
            extension_watch_dynamic_registration: Arc::new(AtomicBool::new(false)),
            extension_watch_registration: Arc::new(tokio::sync::Mutex::new(Vec::new())),
        }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for RubyLanguageServer {
    async fn initialize(&self, params: InitializeParams) -> LspResult<InitializeResult> {
        notification::handle_initialize(self, params).await
    }

    async fn initialized(&self, params: InitializedParams) {
        notification::handle_initialized(self, params).await
    }

    async fn shutdown(&self) -> LspResult<()> {
        notification::handle_shutdown(self).await
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        info!("Document opened: {}", params.text_document.uri.path());
        let start_time = Instant::now();
        if let Some(workspace) = self.workspace_for_uri(&params.text_document.uri) {
            self.prioritize_indexing_project(&workspace.root_path);
        }
        notification::handle_did_open(self, params).await;
        info!(
            "[PERF] Document open handler completed in {:?}",
            start_time.elapsed()
        );
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        info!("Document changed: {}", params.text_document.uri.path());
        let start_time = Instant::now();
        notification::handle_did_change(self, params).await;
        info!(
            "[PERF] Document change handler completed in {:?}",
            start_time.elapsed()
        );
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        info!("Document closed: {}", params.text_document.uri.path());
        let start_time = Instant::now();
        notification::handle_did_close(self, params).await;
        info!(
            "[PERF] Document close handler completed in {:?}",
            start_time.elapsed()
        );
    }

    async fn did_change_configuration(&self, params: DidChangeConfigurationParams) {
        info!("Configuration changed");
        let start_time = Instant::now();
        notification::handle_did_change_configuration(self, params).await;
        info!(
            "[PERF] Configuration change handler completed in {:?}",
            start_time.elapsed()
        );
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        info!("Document saved: {}", params.text_document.uri.path());
        let start_time = Instant::now();
        notification::handle_did_save(self, params).await;
        info!(
            "[PERF] Document save handler completed in {:?}",
            start_time.elapsed()
        );
    }

    async fn did_change_watched_files(&self, params: DidChangeWatchedFilesParams) {
        info!("Watched files changed: {} files", params.changes.len());
        let start_time = Instant::now();
        notification::handle_did_change_watched_files(self, params).await;
        info!(
            "[PERF] Watched files change handler completed in {:?}",
            start_time.elapsed()
        );
    }

    async fn did_change_workspace_folders(
        &self,
        params: tower_lsp::lsp_types::DidChangeWorkspaceFoldersParams,
    ) {
        info!(
            "Workspace folders changed: +{} -{}",
            params.event.added.len(),
            params.event.removed.len()
        );
        let start_time = Instant::now();
        notification::handle_did_change_workspace_folders(self, params).await;
        info!(
            "[PERF] Workspace folder change handler completed in {:?}",
            start_time.elapsed()
        );
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> LspResult<Option<GotoDefinitionResponse>> {
        info!(
            "Goto definition request received for {:?}",
            params
                .text_document_position_params
                .text_document
                .uri
                .path()
        );
        let start_time = Instant::now();
        let result = request::handle_goto_definition(self, params).await;

        info!(
            "[PERF] Goto definition completed in {:?}",
            start_time.elapsed()
        );

        result
    }

    async fn goto_implementation(
        &self,
        params: GotoDefinitionParams,
    ) -> LspResult<Option<GotoDefinitionResponse>> {
        info!(
            "Goto implementation request received for {:?}",
            params
                .text_document_position_params
                .text_document
                .uri
                .path()
        );
        let start_time = Instant::now();
        let result = request::handle_goto_implementation(self, params).await;

        info!(
            "[PERF] Goto implementation completed in {:?}",
            start_time.elapsed()
        );

        result
    }

    async fn references(&self, params: ReferenceParams) -> LspResult<Option<Vec<Location>>> {
        info!(
            "References request received for {:?}",
            params.text_document_position.text_document.uri.path()
        );
        let start_time = Instant::now();
        let result = request::handle_references(self, params).await;

        info!("[PERF] References completed in {:?}", start_time.elapsed());

        result
    }

    async fn document_highlight(
        &self,
        params: DocumentHighlightParams,
    ) -> LspResult<Option<Vec<DocumentHighlight>>> {
        request::handle_document_highlight(self, params).await
    }

    async fn selection_range(
        &self,
        params: SelectionRangeParams,
    ) -> LspResult<Option<Vec<SelectionRange>>> {
        request::handle_selection_ranges(self, params).await
    }

    async fn signature_help(
        &self,
        params: SignatureHelpParams,
    ) -> LspResult<Option<SignatureHelp>> {
        request::handle_signature_help(self, params).await
    }

    async fn code_action(
        &self,
        params: CodeActionParams,
    ) -> LspResult<Option<Vec<CodeActionOrCommand>>> {
        request::handle_code_actions(self, params).await
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> LspResult<Option<SemanticTokensResult>> {
        info!(
            "Semantic tokens request received for {:?}",
            params.text_document.uri.path()
        );
        let start_time = Instant::now();
        let result = request::handle_semantic_tokens_full(self, params).await;

        info!(
            "[PERF] Semantic tokens completed in {:?}",
            start_time.elapsed()
        );

        result
    }

    async fn inlay_hint(
        &self,
        params: InlayHintParams,
    ) -> LspResult<Option<Vec<tower_lsp::lsp_types::InlayHint>>> {
        info!(
            "Inlay hint request received for {:?}",
            params.text_document.uri.path()
        );

        let start_time = Instant::now();
        let result = request::handle_inlay_hints(self, params).await;

        info!("[PERF] Inlay hint completed in {:?}", start_time.elapsed());

        result
    }

    async fn completion(&self, params: CompletionParams) -> LspResult<Option<CompletionResponse>> {
        info!(
            "Completion request received for {:?}",
            params.text_document_position.text_document.uri.path()
        );
        let start_time = Instant::now();
        let result = request::handle_completion(self, params).await;

        info!("[PERF] Completion completed in {:?}", start_time.elapsed());

        result
    }

    async fn completion_resolve(&self, params: CompletionItem) -> LspResult<CompletionItem> {
        info!(
            "Completion item resolve request received for {}",
            params.label
        );
        let start_time = Instant::now();
        let result = request::handle_completion_resolve(self, params).await;

        info!(
            "[PERF] Completion item resolve completed in {:?}",
            start_time.elapsed()
        );

        result
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> LspResult<Option<DocumentSymbolResponse>> {
        info!(
            "Document symbol request received for {:?}",
            params.text_document.uri.path()
        );

        let start_time = Instant::now();
        let result = request::handle_document_symbols(self, params).await;

        info!(
            "[PERF] Document symbols completed in {:?}",
            start_time.elapsed()
        );

        Ok(result)
    }

    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> LspResult<Option<Vec<SymbolInformation>>> {
        info!(
            "Workspace symbol request received for query: '{}'",
            params.query
        );

        let start_time = Instant::now();
        let result = request::handle_workspace_symbols(self, params).await;

        info!(
            "[PERF] Workspace symbols completed in {:?}",
            start_time.elapsed()
        );

        result
    }

    async fn on_type_formatting(
        &self,
        params: DocumentOnTypeFormattingParams,
    ) -> LspResult<Option<Vec<TextEdit>>> {
        info!(
            "Document on type formatting request received for {:?}",
            params.text_document_position.text_document.uri.path()
        );

        let start_time = Instant::now();
        let result = request::handle_document_on_type_formatting(self, params).await;

        info!(
            "[PERF] Document on type formatting completed in {:?}",
            start_time.elapsed()
        );

        result
    }

    async fn formatting(
        &self,
        params: DocumentFormattingParams,
    ) -> LspResult<Option<Vec<TextEdit>>> {
        info!(
            "Document formatting request received for {:?}",
            params.text_document.uri.path()
        );
        request::handle_document_formatting(self, params).await
    }

    async fn folding_range(
        &self,
        params: FoldingRangeParams,
    ) -> LspResult<Option<Vec<FoldingRange>>> {
        info!(
            "Folding range request received for {:?}",
            params.text_document.uri.path()
        );

        let start_time = Instant::now();
        let result = request::handle_folding_range(self, params).await;

        info!(
            "[PERF] Folding range completed in {:?}",
            start_time.elapsed()
        );

        result
    }

    async fn code_lens(&self, params: CodeLensParams) -> LspResult<Option<Vec<CodeLens>>> {
        info!(
            "CodeLens request received for {:?}",
            params.text_document.uri.path()
        );

        let start_time = Instant::now();
        let result = request::handle_code_lens(self, params).await;

        info!("[PERF] CodeLens completed in {:?}", start_time.elapsed());

        result
    }

    async fn hover(
        &self,
        params: tower_lsp::lsp_types::HoverParams,
    ) -> LspResult<Option<tower_lsp::lsp_types::Hover>> {
        info!(
            "Hover request received for {:?}",
            params
                .text_document_position_params
                .text_document
                .uri
                .path()
        );

        let start_time = Instant::now();
        let result = request::handle_hover(self, params).await;

        info!("[PERF] Hover completed in {:?}", start_time.elapsed());

        result
    }

    async fn prepare_type_hierarchy(
        &self,
        params: TypeHierarchyPrepareParams,
    ) -> LspResult<Option<Vec<TypeHierarchyItem>>> {
        request::handle_prepare_type_hierarchy(self, params).await
    }

    async fn supertypes(
        &self,
        params: TypeHierarchySupertypesParams,
    ) -> LspResult<Option<Vec<TypeHierarchyItem>>> {
        request::handle_supertypes(self, params).await
    }

    async fn subtypes(
        &self,
        params: TypeHierarchySubtypesParams,
    ) -> LspResult<Option<Vec<TypeHierarchyItem>>> {
        request::handle_subtypes(self, params).await
    }

    async fn prepare_call_hierarchy(
        &self,
        params: CallHierarchyPrepareParams,
    ) -> LspResult<Option<Vec<CallHierarchyItem>>> {
        request::handle_prepare_call_hierarchy(self, params).await
    }

    async fn incoming_calls(
        &self,
        params: CallHierarchyIncomingCallsParams,
    ) -> LspResult<Option<Vec<CallHierarchyIncomingCall>>> {
        request::handle_incoming_calls(self, params).await
    }

    async fn outgoing_calls(
        &self,
        params: CallHierarchyOutgoingCallsParams,
    ) -> LspResult<Option<Vec<CallHierarchyOutgoingCall>>> {
        request::handle_outgoing_calls(self, params).await
    }

    async fn rename(&self, params: RenameParams) -> LspResult<Option<WorkspaceEdit>> {
        request::handle_rename(self, params).await
    }

    async fn prepare_rename(
        &self,
        params: TextDocumentPositionParams,
    ) -> LspResult<Option<PrepareRenameResponse>> {
        request::handle_prepare_rename(self, params).await
    }
}

#[cfg(test)]
mod runtime_status_tests {
    use super::*;
    use crate::config::runtime::{
        ProjectRuntimeSelection, RuntimeMode, RuntimeSelection, RuntimeSelectionConfig,
        SelectedRuntimeDescriptor,
    };
    use crate::runtime::catalog::{
        DiscoveredRuntime, RuntimeDiscoverySource, RuntimeImplementation, RuntimeStatusParams,
        RuntimeSupportStatus,
    };
    use tower_lsp::lsp_types::FileChangeType;

    #[tokio::test]
    async fn core_engine_template_retention_is_bounded_by_entries_and_estimated_heap() {
        let server = RubyLanguageServer::default();
        for index in 0..(CORE_ENGINE_CACHE_MAX_ENTRIES + 2) {
            server
                .core_engine_cache
                .get_or_try_init(format!("core-{index}"), || async {
                    Ok(ruby_analysis::engine::AnalysisEngine::new())
                })
                .await
                .unwrap();
        }

        assert!(
            server.core_engine_cache.len() <= CORE_ENGINE_CACHE_MAX_ENTRIES,
            "completed core templates must evict to the server-owned entry bound"
        );
        assert!(
            server.core_engine_cache.retained_weight() <= CORE_ENGINE_CACHE_MAX_WEIGHT_BYTES,
            "completed core templates must remain within the server-owned estimated-heap bound"
        );
    }

    #[test]
    fn indexing_snapshot_is_sorted_and_failure_aware() {
        let fixture = tempfile::tempdir().unwrap();
        let admin = fixture.path().join("admin");
        let server_project = fixture.path().join("server");
        std::fs::create_dir_all(&admin).unwrap();
        std::fs::create_dir_all(&server_project).unwrap();
        let language_server = RubyLanguageServer::default();
        let server_workspace =
            language_server.add_workspace(Url::from_directory_path(&server_project).unwrap());
        let admin_workspace =
            language_server.add_workspace(Url::from_directory_path(&admin).unwrap());

        let admin_generation = admin_workspace
            .indexing_status
            .begin_generation()
            .generation;
        admin_workspace
            .indexing_status
            .transition(
                admin_generation,
                IndexingPhase::IndexingProject,
                Some(1),
                Some(2),
            )
            .expect("admin generation must accept project progress");
        let server_generation = server_workspace
            .indexing_status
            .begin_generation()
            .generation;
        server_workspace
            .indexing_status
            .fail(server_generation, "lockfile failed".to_string())
            .expect("server generation must accept failure");

        let snapshot = language_server.indexing_status_snapshot();
        assert_eq!(snapshot.projects[0].root, admin);
        assert_eq!(snapshot.projects[1].root, server_project);
        assert_eq!(snapshot.aggregate.failed, 1);
        assert_eq!(snapshot.aggregate.ready, 0);
        assert_eq!(
            snapshot.reuse,
            IndexingReuseSnapshot::default(),
            "a fresh server must report exact zero process-lifetime reuse counters"
        );
        assert!(!language_server.is_indexing_complete());
    }

    #[test]
    fn indexing_snapshot_reports_process_local_classpath_file_reuse() {
        let fixture = tempfile::tempdir().unwrap();
        let project = fixture.path().join("project");
        let jruby = fixture.path().join("jruby");
        let java_home = fixture.path().join("jdk");
        for (path, bytes) in [
            (jruby.join("bin/jruby"), b"jruby".as_slice()),
            (jruby.join("lib/jruby.jar"), b"runtime".as_slice()),
            (
                java_home.join("jmods/java.base.jmod"),
                b"java base".as_slice(),
            ),
        ] {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
        }
        std::fs::create_dir_all(&project).unwrap();
        let inputs = crate::runtime::jruby::classpath::ClasspathInputs {
            project_root: project,
            jruby_executable: jruby.join("bin/jruby"),
            java_home,
            maven_repository: None,
            java_gem_roots: Vec::new(),
            additional_classpath: Vec::new(),
            additional_sources: Vec::new(),
        };
        let server = RubyLanguageServer::default();
        for _ in 0..2 {
            crate::runtime::jruby::classpath::discover_project_classpath_with_cache(
                &inputs,
                crate::runtime::jruby::classpath::ClasspathLimits::default(),
                &server.classpath_file_product_cache,
            )
            .unwrap();
        }

        assert_eq!(
            server
                .indexing_status_snapshot()
                .reuse
                .classpath_file_single_flight,
            IndexingSingleFlightReuseSnapshot {
                lookups: 4,
                hits: 2,
                joined_flights: 0,
                producers: 2,
                failures: 0,
            }
        );
    }

    #[test]
    fn indexing_status_send_queue_keeps_only_the_latest_pending_snapshot() {
        let language_server = RubyLanguageServer::default();
        let mut publication = IndexingStatusPublicationState::default();
        let first = language_server
            .sequence_indexing_status_snapshot(language_server.indexing_status_snapshot());
        assert!(
            publication.queue_send(first.clone()),
            "the first queued snapshot must schedule the sender"
        );
        let second = language_server
            .sequence_indexing_status_snapshot(language_server.indexing_status_snapshot());
        assert!(
            !publication.queue_send(second.clone()),
            "a later snapshot must replace pending state without scheduling a second sender"
        );
        let pending = publication
            .take_pending_send()
            .expect("latest snapshot must remain pending");
        assert_eq!(pending.sequence, second.sequence);
        assert!(pending.sequence > first.sequence);
        assert!(
            publication.take_pending_send().is_none(),
            "taking the pending snapshot must clear the sender schedule"
        );
    }

    #[tokio::test]
    async fn multi_project_phase_storm_publishes_latest_wins_without_blocking_callers() {
        use futures::StreamExt;
        use serde_json::json;
        use tower::{Service, ServiceExt};
        use tower_lsp::jsonrpc::Request;
        use tower_lsp::LspService;

        let (mut service, mut socket) = LspService::new(|client| {
            RubyLanguageServer::new(client).expect("test language server must initialize")
        });
        let initialize = Request::build("initialize")
            .params(json!({"capabilities": {}}))
            .id(1)
            .finish();
        service
            .ready()
            .await
            .unwrap()
            .call(initialize)
            .await
            .unwrap()
            .expect("initialize must return a response");

        let received = Arc::new(Mutex::new(Vec::<IndexingStatusSnapshot>::new()));
        let received_by_reader = received.clone();
        let socket_reader = tokio::spawn(async move {
            while let Some(request) = socket.next().await {
                if request.method() == "ruby-fast-lsp/indexing/statusChanged" {
                    received_by_reader.lock().push(
                        serde_json::from_value::<IndexingStatusSnapshot>(
                            request
                                .params()
                                .cloned()
                                .expect("status notification must carry parameters"),
                        )
                        .expect("status notification must carry a valid complete snapshot"),
                    );
                }
            }
        });

        let fixture = tempfile::tempdir().unwrap();
        let language_server = service.inner().clone();
        let mut workspaces = Vec::new();
        for index in 0..8 {
            let project = fixture.path().join(format!("project-{index}"));
            std::fs::create_dir_all(&project).unwrap();
            workspaces.push(
                language_server.add_workspace(Url::from_directory_path(&project).unwrap()),
            );
        }

        let publish_started = Instant::now();
        for workspace in &workspaces {
            let run = workspace.indexing_status.begin_run();
            for phase in [
                IndexingPhase::IndexingProject,
                IndexingPhase::ProjectNavigationReady,
                IndexingPhase::IndexingDependencies,
                IndexingPhase::DependencyNavigationReady,
                IndexingPhase::Ready,
            ] {
                workspace
                    .indexing_status
                    .transition(run.generation(), phase, None, None)
                    .unwrap();
                language_server.publish_indexing_status().await;
            }
        }
        assert!(
            publish_started.elapsed() < Duration::from_millis(200),
            "status publication must not await client IO on the caller; elapsed={:?}",
            publish_started.elapsed()
        );

        tokio::time::sleep(Duration::from_millis(150)).await;
        socket_reader.abort();
        let _ = socket_reader.await;
        let snapshots = received.lock().clone();
        assert!(
            !snapshots.is_empty(),
            "at least the latest indexing snapshot must reach the client"
        );
        assert!(
            snapshots.len() < 40,
            "latest-wins publication must drop intermediate multi-project phase storms, got {}",
            snapshots.len()
        );
        assert!(
            snapshots
                .last()
                .unwrap()
                .projects
                .iter()
                .all(|project| project.phase == IndexingPhase::Ready),
            "the final delivered snapshot must reflect the latest ready state"
        );
        assert!(
            snapshots
                .windows(2)
                .all(|pair| pair[0].sequence < pair[1].sequence),
            "client-visible status sequence must remain strictly monotonic"
        );
    }

    #[test]
    fn counter_only_status_publication_is_bounded_while_phase_changes_are_immediate() {
        let fixture = tempfile::tempdir().unwrap();
        let project = fixture.path().join("server");
        std::fs::create_dir_all(&project).unwrap();
        let language_server = RubyLanguageServer::default();
        let workspace = language_server.add_workspace(Url::from_directory_path(&project).unwrap());
        let run = workspace.indexing_status.begin_run();
        workspace
            .indexing_status
            .transition(
                run.generation(),
                IndexingPhase::IndexingProject,
                Some(0),
                Some(100),
            )
            .unwrap();

        let mut publication = IndexingStatusPublicationState::default();
        assert_eq!(
            publication.observe(&language_server.indexing_status_snapshot()),
            IndexingStatusPublicationDecision::Immediate
        );

        workspace
            .indexing_status
            .transition(
                run.generation(),
                IndexingPhase::IndexingProject,
                Some(1),
                Some(100),
            )
            .unwrap();
        assert_eq!(
            publication.observe(&language_server.indexing_status_snapshot()),
            IndexingStatusPublicationDecision::ScheduleCounterFlush
        );
        for completed in 2..=50 {
            workspace
                .indexing_status
                .transition(
                    run.generation(),
                    IndexingPhase::IndexingProject,
                    Some(completed),
                    Some(100),
                )
                .unwrap();
            assert_eq!(
                publication.observe(&language_server.indexing_status_snapshot()),
                IndexingStatusPublicationDecision::Coalesced
            );
        }
        assert!(publication.flush_counter(&language_server.indexing_status_snapshot()));
        assert!(!publication.flush_counter(&language_server.indexing_status_snapshot()));

        workspace
            .indexing_status
            .transition(
                run.generation(),
                IndexingPhase::ProjectNavigationReady,
                None,
                None,
            )
            .unwrap();
        assert_eq!(
            publication.observe(&language_server.indexing_status_snapshot()),
            IndexingStatusPublicationDecision::Immediate
        );
        assert!(
            !publication.flush_counter(&language_server.indexing_status_snapshot()),
            "an immediate phase publication must cancel the stale counter flush"
        );

        let replacement = workspace.indexing_status.begin_run();
        assert_eq!(
            publication.observe(&language_server.indexing_status_snapshot()),
            IndexingStatusPublicationDecision::Immediate,
            "a replacement generation must publish immediately even when its phase repeats"
        );
        workspace
            .indexing_status
            .fail(
                replacement.generation(),
                "runtime replacement failed".to_string(),
            )
            .unwrap();
        assert_eq!(
            publication.observe(&language_server.indexing_status_snapshot()),
            IndexingStatusPublicationDecision::Immediate,
            "terminal failures must bypass counter throttling"
        );
    }

    #[test]
    fn watched_file_batches_keep_only_the_latest_event_per_uri_and_generation() {
        let fixture = tempfile::tempdir().unwrap();
        let admin = Url::from_file_path(fixture.path().join("admin.rb")).unwrap();
        let server_file = Url::from_file_path(fixture.path().join("server.rb")).unwrap();
        let language_server = RubyLanguageServer::default();

        let first = language_server.queue_watched_file_changes(vec![
            FileEvent {
                uri: server_file.clone(),
                typ: FileChangeType::CREATED,
            },
            FileEvent {
                uri: admin.clone(),
                typ: FileChangeType::CHANGED,
            },
        ]);
        let replacement = language_server.queue_watched_file_changes(vec![
            FileEvent {
                uri: server_file.clone(),
                typ: FileChangeType::DELETED,
            },
            FileEvent {
                uri: admin.clone(),
                typ: FileChangeType::CHANGED,
            },
        ]);

        assert!(
            language_server.take_watched_file_changes(first).is_none(),
            "an older debounce generation must never process a partial filesystem state"
        );
        let changes = language_server
            .take_watched_file_changes(replacement)
            .expect("the newest debounce generation must own the complete normalized batch");
        assert_eq!(
            changes,
            vec![
                FileEvent {
                    uri: admin,
                    typ: FileChangeType::CHANGED,
                },
                FileEvent {
                    uri: server_file,
                    typ: FileChangeType::DELETED,
                },
            ]
        );
        assert!(
            language_server
                .take_watched_file_changes(replacement)
                .is_none(),
            "a normalized watcher batch must be consumed exactly once"
        );
    }

    #[tokio::test]
    async fn indexing_status_request_prioritizes_active_document_and_sequences_exact_snapshot() {
        let fixture = tempfile::tempdir().unwrap();
        let admin = fixture.path().join("admin");
        let server_project = fixture.path().join("server");
        std::fs::create_dir_all(&admin).unwrap();
        std::fs::create_dir_all(&server_project).unwrap();
        let language_server = RubyLanguageServer::default();
        language_server.add_workspace(Url::from_directory_path(&admin).unwrap());
        let server_workspace =
            language_server.add_workspace(Url::from_directory_path(&server_project).unwrap());
        let active_document_uri =
            Url::from_file_path(server_project.join("lib/active.rb")).unwrap();

        let first = language_server
            .handle_indexing_status(IndexingStatusParams {
                active_document_uri: Some(active_document_uri),
            })
            .await
            .unwrap();
        let second = language_server
            .handle_indexing_status(IndexingStatusParams::default())
            .await
            .unwrap();

        assert_eq!(first.sequence, 1);
        assert_eq!(second.sequence, 2);
        assert_eq!(
            language_server.indexing_scheduler.snapshot().active_project,
            Some(server_project.clone())
        );
        assert_eq!(
            language_server.indexing_resources.snapshot().active_project,
            Some(server_project.clone())
        );
        assert!(
            language_server
                .indexing_resources
                .snapshot()
                .active_project_navigation_pending,
            "a discovered active project must reserve its navigation-critical source pass"
        );

        let generation = server_workspace.indexing_status.begin_run().generation();
        server_workspace
            .indexing_status
            .transition(
                generation,
                IndexingPhase::ProjectNavigationReady,
                None,
                None,
            )
            .unwrap();
        language_server.prioritize_indexing_project(&server_project);
        assert!(
            !language_server
                .indexing_resources
                .snapshot()
                .active_project_navigation_pending,
            "an already navigation-ready active project must not block sibling source passes"
        );
    }

    #[tokio::test]
    async fn counter_storm_emits_one_bounded_client_flush_and_immediate_phase_transition() {
        use futures::StreamExt;
        use serde_json::json;
        use tower::{Service, ServiceExt};
        use tower_lsp::jsonrpc::Request;
        use tower_lsp::LspService;

        let (mut service, mut socket) = LspService::new(|client| {
            RubyLanguageServer::new(client).expect("test language server must initialize")
        });
        let initialize = Request::build("initialize")
            .params(json!({"capabilities": {}}))
            .id(1)
            .finish();
        service
            .ready()
            .await
            .unwrap()
            .call(initialize)
            .await
            .unwrap()
            .expect("initialize must return a response");

        let received = Arc::new(Mutex::new(Vec::<IndexingStatusSnapshot>::new()));
        let received_by_reader = received.clone();
        let socket_reader = tokio::spawn(async move {
            while let Some(request) = socket.next().await {
                if request.method() == "ruby-fast-lsp/indexing/statusChanged" {
                    received_by_reader.lock().push(
                        serde_json::from_value::<IndexingStatusSnapshot>(
                            request
                                .params()
                                .cloned()
                                .expect("status notification must carry parameters"),
                        )
                        .expect("status notification must carry a valid complete snapshot"),
                    );
                }
            }
        });

        let fixture = tempfile::tempdir().unwrap();
        let project = fixture.path().join("server");
        std::fs::create_dir_all(&project).unwrap();
        let language_server = service.inner().clone();
        let workspace = language_server.add_workspace(Url::from_directory_path(&project).unwrap());
        let run = workspace.indexing_status.begin_run();
        workspace
            .indexing_status
            .transition(
                run.generation(),
                IndexingPhase::IndexingProject,
                Some(0),
                Some(50),
            )
            .unwrap();
        language_server.publish_indexing_status().await;

        for completed in 1..=50 {
            workspace
                .indexing_status
                .transition(
                    run.generation(),
                    IndexingPhase::IndexingProject,
                    Some(completed),
                    Some(50),
                )
                .unwrap();
            language_server.publish_indexing_status().await;
        }
        tokio::time::sleep(INDEXING_COUNTER_PUBLICATION_INTERVAL + Duration::from_millis(50)).await;

        workspace
            .indexing_status
            .transition(
                run.generation(),
                IndexingPhase::ProjectNavigationReady,
                None,
                None,
            )
            .unwrap();
        language_server.publish_indexing_status().await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        socket_reader.abort();
        let _ = socket_reader.await;
        let snapshots = received.lock().clone();

        assert_eq!(
            snapshots.len(),
            3,
            "fifty counter changes must emit the initial state, one bounded counter flush, and one immediate phase transition"
        );
        assert!(
            snapshots
                .windows(2)
                .all(|pair| pair[0].sequence < pair[1].sequence),
            "client-visible status sequence must remain strictly monotonic"
        );
        assert_eq!(
            snapshots.last().unwrap().projects[0].phase,
            IndexingPhase::ProjectNavigationReady,
            "the phase transition must bypass counter throttling"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn saturated_indexing_keeps_status_switch_and_queued_cancellation_responsive() {
        let fixture = tempfile::tempdir().unwrap();
        let admin = fixture.path().join("admin");
        let server_project = fixture.path().join("server");
        std::fs::create_dir_all(&admin).unwrap();
        std::fs::create_dir_all(&server_project).unwrap();
        let mut language_server = RubyLanguageServer::default();
        language_server.indexing_resources =
            crate::indexing_resources::IndexingResourceGovernor::new(
                crate::indexing_resources::IndexingResourcePolicy::with_limits(1, 1, 100, 1),
            );
        language_server.add_workspace(Url::from_directory_path(&admin).unwrap());
        language_server.add_workspace(Url::from_directory_path(&server_project).unwrap());

        let (holder_started_tx, holder_started_rx) = tokio::sync::oneshot::channel();
        let (holder_release_tx, holder_release_rx) = std::sync::mpsc::channel();
        let holder_resources = language_server.indexing_resources.clone();
        let holder = tokio::spawn(async move {
            holder_resources
                .run_cpu("saturated status holder", move || {
                    holder_started_tx.send(()).unwrap();
                    holder_release_rx.recv().unwrap();
                })
                .await
                .unwrap();
        });
        holder_started_rx.await.unwrap();

        let cancellation = tokio_util::sync::CancellationToken::new();
        let cancelled_resources = language_server.indexing_resources.clone();
        let cancelled_token = cancellation.clone();
        let cancelled_root = admin.clone();
        let cancelled = tokio::spawn(async move {
            cancelled_resources
                .run_with_resources(
                    "cancelled saturated waiter",
                    crate::indexing_resources::IndexingWorkSpec::new(
                        Some(cancelled_root),
                        crate::indexing_resources::IndexingResourcePriority::Background,
                        1,
                        1,
                        0,
                    ),
                    Some(cancelled_token),
                    || (),
                )
                .await
        });
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while language_server.indexing_resources.snapshot().queued_tasks != 1 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("the cancellable waiter must queue behind saturated indexing");

        let active_document_uri =
            Url::from_file_path(server_project.join("lib/active.rb")).unwrap();
        let status = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            language_server.handle_indexing_status(IndexingStatusParams {
                active_document_uri: Some(active_document_uri),
            }),
        )
        .await
        .expect("status and active-editor routing must remain available within 100 ms")
        .unwrap();
        assert_eq!(status.sequence, 1);
        assert_eq!(
            language_server.indexing_resources.snapshot().active_project,
            Some(server_project)
        );

        cancellation.cancel();
        let cancellation_error =
            tokio::time::timeout(std::time::Duration::from_millis(100), cancelled)
                .await
                .expect("queued cancellation must remain available within 100 ms")
                .unwrap()
                .unwrap_err();
        assert!(cancellation_error
            .to_string()
            .contains("cancelled before entering"));
        assert_eq!(
            language_server.indexing_resources.snapshot().queued_tasks,
            0
        );

        holder_release_tx.send(()).unwrap();
        holder.await.unwrap();
    }

    #[tokio::test]
    async fn runtime_status_reports_server_owned_project_identity_and_classpath() {
        let fixture = tempfile::tempdir().unwrap();
        let admin = fixture.path().join("admin");
        let server_project = fixture.path().join("server");
        std::fs::create_dir_all(&admin).unwrap();
        std::fs::create_dir_all(&server_project).unwrap();
        let language_server = RubyLanguageServer::default();
        let admin_workspace =
            language_server.add_workspace(Url::from_directory_path(&admin).unwrap());
        language_server.add_workspace(Url::from_directory_path(&server_project).unwrap());
        *language_server.config.lock() = RubyFastLspConfig {
            runtime: RuntimeSelectionConfig {
                mode: RuntimeMode::Auto,
                projects: vec![ProjectRuntimeSelection {
                    root: admin.to_string_lossy().to_string(),
                    selection: RuntimeSelection::Explicit(SelectedRuntimeDescriptor {
                        implementation: RuntimeImplementation::Jruby,
                        family: "9.2".to_string(),
                        engine_version: "9.2.21.0".to_string(),
                        compatibility_version: "2.5".to_string(),
                        executable: fixture.path().join("jruby/bin/jruby"),
                        discovery_source: RuntimeDiscoverySource::Rvm,
                        java_home: Some(fixture.path().join("jdk")),
                    }),
                }],
            },
            ..RubyFastLspConfig::default()
        };
        language_server.set_runtime_classpath_fingerprint(&admin, Some("a".repeat(64)));
        let generation = admin_workspace
            .indexing_status
            .begin_generation()
            .generation;
        admin_workspace
            .indexing_status
            .transition(
                generation,
                crate::indexing_status::IndexingPhase::Ready,
                None,
                None,
            )
            .expect("test workspace must transition to ready");

        let status = language_server
            .handle_runtime_status(RuntimeStatusParams {
                project_root: Some(admin.clone()),
            })
            .await
            .unwrap();

        assert_eq!(status.projects.len(), 1);
        let status = &status.projects[0];
        assert_eq!(status.root, admin);
        assert_eq!(status.mode, "explicit");
        assert_eq!(status.implementation, Some(RuntimeImplementation::Jruby));
        assert_eq!(status.family.as_deref(), Some("9.2"));
        assert_eq!(status.engine_version.as_deref(), Some("9.2.21.0"));
        assert_eq!(status.compatibility_version.as_deref(), Some("2.5"));
        assert_eq!(status.stub_overlay.as_deref(), Some("9.2"));
        assert_eq!(
            status.classpath_fingerprint_sha256.as_deref(),
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        );
        assert!(status.indexing_complete);
        assert_eq!(
            status.indexing.phase,
            crate::indexing_status::IndexingPhase::Ready
        );

        let auto_runtime = SelectedRuntimeDescriptor {
            implementation: RuntimeImplementation::Mri,
            family: "3.3".to_string(),
            engine_version: "3.3.11".to_string(),
            compatibility_version: "3.3".to_string(),
            executable: fixture.path().join("ruby-3.3.11/bin/ruby"),
            discovery_source: RuntimeDiscoverySource::Rbenv,
            java_home: None,
        };
        language_server.set_effective_runtime(&server_project, Some(auto_runtime));
        let auto = language_server
            .handle_runtime_status(RuntimeStatusParams {
                project_root: Some(server_project),
            })
            .await
            .unwrap();
        let auto = &auto.projects[0];
        assert_eq!(auto.mode, "auto");
        assert_eq!(auto.implementation, Some(RuntimeImplementation::Mri));
        assert_eq!(auto.engine_version.as_deref(), Some("3.3.11"));
        assert_eq!(
            auto.executable,
            Some(fixture.path().join("ruby-3.3.11/bin/ruby"))
        );
    }

    #[tokio::test]
    async fn auto_runtime_resolves_exact_project_marker_through_server_catalog() {
        let fixture = tempfile::tempdir().unwrap();
        let admin = fixture.path().join("admin");
        std::fs::create_dir_all(&admin).unwrap();
        std::fs::write(admin.join(".ruby-version"), "jruby-9.2.21.0\n").unwrap();
        let executable = fixture.path().join("jruby-9.2.21.0/bin/jruby");
        let java_home = fixture.path().join("jdk-17");
        let language_server = RubyLanguageServer::default();
        language_server.add_workspace(Url::from_directory_path(&admin).unwrap());
        language_server.set_discovered_runtimes_for_tests(vec![DiscoveredRuntime {
            implementation: RuntimeImplementation::Jruby,
            implementation_label: "JRuby".to_string(),
            family: "9.2".to_string(),
            family_label: "JRuby 9.2 (Ruby 2.5)".to_string(),
            compatibility_version: "2.5".to_string(),
            compatibility_label: "Ruby 2.5".to_string(),
            engine_version: "9.2.21.0".to_string(),
            display_name: "JRuby 9.2.21.0 (Ruby 2.5)".to_string(),
            executable: executable.clone(),
            discovery_source: RuntimeDiscoverySource::Rvm,
            support_status: RuntimeSupportStatus::Supported,
            java_home: Some(java_home.clone()),
        }]);

        let resolved = language_server
            .resolve_auto_runtime(&admin)
            .await
            .unwrap()
            .expect("the exact installed JRuby marker must resolve");
        assert_eq!(resolved.engine_version, "9.2.21.0");
        assert_eq!(resolved.compatibility_version, "2.5");
        assert_eq!(resolved.executable, executable);
        assert_eq!(resolved.java_home, Some(java_home));
    }
}
