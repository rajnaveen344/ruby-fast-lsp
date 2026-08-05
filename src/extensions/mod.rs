use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

use globset::{Glob, GlobSet, GlobSetBuilder};
use log::warn;
use once_cell::sync::Lazy;
use parking_lot::{Mutex, RwLock};
use ruby_fast_lsp_extension_api::{
    Argument, ArgumentValue, BlockExecutionContextPatch, CallContext, DocumentContext,
    ExecutionContextTarget, Extension, ExtensionEvent, GeneratedOwnerScope, IndexPatch, Keyword,
    NamespaceKind as AbiNamespaceKind, ProcessRequest, ProcessResult, ProcessResultStatus,
    Receiver, ResolvedCall, ResolvedCallee, ResponsePatch, SourcePosition, SourceRange,
    WatchedFileChange, WatchedFileChangeKind,
};

mod project_context;
pub(crate) use project_context::{
    ExtensionApplicabilityFingerprint, ProjectContextSeed, ProjectContextSnapshot,
};
use ruby_prism::{CallNode, Node};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::process::Command as ProcessCommand;
use tower_lsp::lsp_types::{
    CodeLens, Command, DocumentSymbol, FileChangeType, FileEvent, Position, Range, SymbolKind, Url,
};
use walkdir::WalkDir;

use crate::config::RubyFastLspConfig;
use crate::indexing_resources::{
    IndexingResourceGovernor, IndexingResourcePriority, IndexingWorkSpec,
};
use crate::persistent_cache::{
    CompiledWasmProductKey, PersistentCompiledWasmLookup, PersistentDerivedProductCache,
};
use ruby_analysis::core::{
    ExecutionContextFact, ExecutionScopeMode, FullyQualifiedName, GeneratedOwnerId, GraphEdgeKind,
    GraphNodeFact, GraphNodeKind, MethodCalleeResolution, MethodFact, NamespaceKind,
    ReferenceCandidate, RubyConstant, RubyMethod, RubyType as AnalysisRubyType, SourceKind,
    SymbolFact, SymbolKind as AnalysisSymbolKind, TextRange, TypeFact, TypeProvenance, TypeSubject,
};
use ruby_analysis::engine::{FileFacts, ResolveMode, SourceFileInput};
use ruby_analysis::indexer as utils;
use ruby_analysis::indexer::fact_collector::{
    BlockExecutionContext, FactCollector, FactCollectorExtensionHost,
};
use ruby_analysis::indexer::MethodReceiver as CoreMethodReceiver;
use ruby_analysis::method_store::MethodVisibility as AnalysisMethodVisibility;

static EXTENSION_REGISTRY: Lazy<ExtensionRegistryHandle> =
    Lazy::new(ExtensionRegistryHandle::from_environment);

const MAX_PROCESS_REQUESTS_PER_EVENT: usize = 16;
const MAX_PROCESS_ARGUMENTS: usize = 64;
const MAX_PROCESS_ARGUMENT_BYTES: usize = 8 * 1024;
const MAX_PROCESS_STDIN_BYTES: usize = 64 * 1024;
const MAX_PROCESS_OUTPUT_BYTES: usize = 256 * 1024;
const DEFAULT_PROCESS_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_PROCESS_TIMEOUT: Duration = Duration::from_secs(10);
const EXTENSION_PROCESS_TRANSIENT_MEMORY_BYTES: usize = 128 * 1024 * 1024;
const EXTENSION_LOAD_TRANSIENT_MEMORY_BYTES: usize = 256 * 1024 * 1024;
const EXTENSION_RESPONSE_TRANSIENT_MEMORY_BYTES: usize = 128 * 1024 * 1024;
const MAX_EXTENSION_WASM_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone)]
pub struct ExtensionRegistryHandle {
    inner: Arc<RwLock<ExtensionRegistry>>,
    reconfiguration: Arc<tokio::sync::Mutex<()>>,
    persistent_cache: Option<PersistentDerivedProductCache>,
}

impl std::fmt::Debug for ExtensionRegistryHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExtensionRegistryHandle")
            .field("extension_count", &self.inner.read().extensions.len())
            .finish()
    }
}

struct ExtensionRegistry {
    extensions: Vec<Arc<LoadedWasmExtension>>,
    tracked_call_names: BTreeSet<String>,
    semantic_seeded_engines: Mutex<Vec<SeededExtensionEngine>>,
    load_config: ExtensionLoadConfig,
    discovery_fingerprint: [u8; 32],
}

/// File-traversal-scoped extension applicability decisions.
///
/// This intentionally retains only one bit per extension plus the immutable
/// registry identity. It must never retain extension instances or semantic
/// engine state across project generations.
#[derive(Clone, Debug)]
pub(crate) struct ExtensionApplicabilitySnapshot {
    registry_fingerprint: [u8; 32],
    applies_to_source: Vec<bool>,
}

struct SeededExtensionEngine {
    engine: Weak<RwLock<ruby_analysis::engine::AnalysisEngine>>,
    applicability_fingerprint: ExtensionApplicabilityFingerprint,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionStatusReport {
    pub id: String,
    pub name: Option<String>,
    pub version: Option<String>,
    pub status: String,
    pub last_error: Option<String>,
    pub capabilities: Vec<String>,
    pub permissions: Vec<String>,
    pub watched_files: Vec<String>,
    pub process_commands: Vec<String>,
    pub indexed_call_names: Vec<String>,
    #[serde(default)]
    pub telemetry: ExtensionTelemetryReport,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionTelemetryReport {
    pub guest_calls: u64,
    pub lifecycle_calls: u64,
    pub index_calls: u64,
    pub event_calls: u64,
    pub guest_failures: u64,
    pub guest_traps: u64,
    pub resource_limit_failures: u64,
    pub disablements: u64,
    pub rejected_outputs: u64,
    pub patch_conflicts: u64,
    pub emitted_index_patches: u64,
    pub emitted_execution_contexts: u64,
    pub emitted_response_patches: u64,
    pub emitted_command_patches: u64,
    pub emitted_process_requests: u64,
    pub requested_reindex_files: u64,
    pub total_guest_time_ns: u64,
    pub max_guest_time_ns: u64,
    pub project_instance_creations: u64,
    pub project_instance_failures: u64,
    pub total_project_instance_time_ns: u64,
    pub max_project_instance_time_ns: u64,
    pub project_instances: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ExtensionStatusParams {}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExtensionStatusResponse {
    pub extensions: Vec<ExtensionStatusReport>,
}

struct LoadedWasmExtension {
    metadata: ExtensionMetadata,
    extension: Mutex<ruby_fast_lsp_extension_wasm_host::WasmExtension>,
    project_extensions: Mutex<BTreeMap<String, ruby_fast_lsp_extension_wasm_host::WasmExtension>>,
    compiled_extension: ruby_fast_lsp_extension_wasm_host::CompiledWasmExtension,
    activation_settings: Mutex<Option<serde_json::Value>>,
    status: Mutex<ExtensionStatus>,
    indexed_call_names: BTreeSet<String>,
    frame_call_names: BTreeSet<String>,
    semantic_targets: Vec<ExtensionMethodTarget>,
    watched_file_matcher: GlobSet,
    applicability: Vec<ExtensionGemRequirement>,
    project_context_delivery: ExtensionProjectContextDelivery,
    telemetry: ExtensionTelemetry,
    #[cfg(test)]
    applicability_evaluations: AtomicU64,
}

#[derive(Debug, Default)]
struct ExtensionTelemetry {
    guest_calls: AtomicU64,
    lifecycle_calls: AtomicU64,
    index_calls: AtomicU64,
    event_calls: AtomicU64,
    guest_failures: AtomicU64,
    guest_traps: AtomicU64,
    resource_limit_failures: AtomicU64,
    disablements: AtomicU64,
    rejected_outputs: AtomicU64,
    patch_conflicts: AtomicU64,
    emitted_index_patches: AtomicU64,
    emitted_execution_contexts: AtomicU64,
    emitted_response_patches: AtomicU64,
    emitted_command_patches: AtomicU64,
    emitted_process_requests: AtomicU64,
    requested_reindex_files: AtomicU64,
    total_guest_time_ns: AtomicU64,
    max_guest_time_ns: AtomicU64,
    project_instance_creations: AtomicU64,
    project_instance_failures: AtomicU64,
    total_project_instance_time_ns: AtomicU64,
    max_project_instance_time_ns: AtomicU64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GuestCallKind {
    Lifecycle,
    Index,
    Event,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ExtensionMethodTarget {
    owner: Vec<RubyConstant>,
    owner_kind: NamespaceKind,
    method: RubyMethod,
    frame: bool,
}

#[derive(Clone, Debug)]
struct ExtensionMetadata {
    id: String,
    name: Option<String>,
    version: Option<String>,
    capabilities: Vec<String>,
    permissions: Vec<String>,
    watched_files: Vec<String>,
    process_commands: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ExtensionStatus {
    Discovered,
    Loaded,
    Deactivated,
    Slow { reason: String },
    Failed { reason: String },
}

impl ExtensionStatus {
    fn from_failure(reason: impl Into<String>) -> Self {
        let reason = reason.into();
        if reason.contains("wall-clock deadline") {
            Self::Slow { reason }
        } else {
            Self::Failed { reason }
        }
    }
}

impl ExtensionTelemetry {
    fn record_call(
        &self,
        kind: GuestCallKind,
        elapsed: Duration,
        output: Option<&ruby_fast_lsp_extension_api::ExtensionOutput>,
        failure: Option<&str>,
    ) {
        saturating_increment(&self.guest_calls, 1);
        match kind {
            GuestCallKind::Lifecycle => saturating_increment(&self.lifecycle_calls, 1),
            GuestCallKind::Index => saturating_increment(&self.index_calls, 1),
            GuestCallKind::Event => saturating_increment(&self.event_calls, 1),
        }
        if let Some(reason) = failure {
            self.record_guest_failure(reason);
        }
        if let Some(output) = output {
            saturating_increment(
                &self.emitted_index_patches,
                u64::try_from(output.index_patches.len()).expect(
                    "INVARIANT VIOLATED: index patch count does not fit in u64. This is a bug because extension output is bounded far below u64::MAX. Fix: enforce output bounds before telemetry recording.",
                ),
            );
            saturating_increment(
                &self.emitted_execution_contexts,
                u64::try_from(output.execution_contexts.len()).expect(
                    "INVARIANT VIOLATED: execution-context count does not fit in u64. This is a bug because extension output is bounded far below u64::MAX. Fix: enforce output bounds before telemetry recording.",
                ),
            );
            saturating_increment(
                &self.emitted_response_patches,
                u64::try_from(output.response_patches.len()).expect(
                    "INVARIANT VIOLATED: response patch count does not fit in u64. This is a bug because extension output is bounded far below u64::MAX. Fix: enforce output bounds before telemetry recording.",
                ),
            );
            saturating_increment(
                &self.emitted_command_patches,
                u64::try_from(output.command_patches.len()).expect(
                    "INVARIANT VIOLATED: command patch count does not fit in u64. This is a bug because extension output is bounded far below u64::MAX. Fix: enforce output bounds before telemetry recording.",
                ),
            );
            saturating_increment(
                &self.emitted_process_requests,
                u64::try_from(output.process_requests.len()).expect(
                    "INVARIANT VIOLATED: process request count does not fit in u64. This is a bug because extension output is bounded far below u64::MAX. Fix: enforce output bounds before telemetry recording.",
                ),
            );
            saturating_increment(
                &self.requested_reindex_files,
                u64::try_from(output.reindex_files.len()).expect(
                    "INVARIANT VIOLATED: reindex file count does not fit in u64. This is a bug because extension output is bounded far below u64::MAX. Fix: enforce output bounds before telemetry recording.",
                ),
            );
        }
        let elapsed_ns = u64::try_from(elapsed.as_nanos()).unwrap_or(u64::MAX);
        saturating_increment(&self.total_guest_time_ns, elapsed_ns);
        self.max_guest_time_ns
            .fetch_max(elapsed_ns, Ordering::Relaxed);
    }

    fn record_disablement(&self) {
        saturating_increment(&self.disablements, 1);
    }

    fn record_rejected_output(&self) {
        saturating_increment(&self.rejected_outputs, 1);
    }

    fn record_patch_conflict(&self) {
        saturating_increment(&self.patch_conflicts, 1);
    }

    fn record_guest_failure(&self, reason: &str) {
        saturating_increment(&self.guest_failures, 1);
        let reason = reason.to_ascii_lowercase();
        let trapped = reason.contains("wasm trap")
            || reason.contains("unreachable")
            || reason.contains("fuel")
            || reason.contains("wall-clock deadline");
        if trapped {
            saturating_increment(&self.guest_traps, 1);
        }
        let resource_limited = reason.contains("fuel")
            || reason.contains("wall-clock deadline")
            || (reason.contains("payload") && reason.contains("exceeds max"))
            || (reason.contains("memory")
                && (reason.contains("limit")
                    || reason.contains("grow")
                    || reason.contains("out of bounds")));
        if resource_limited {
            saturating_increment(&self.resource_limit_failures, 1);
        }
    }

    fn record_project_instance_creation(&self, elapsed: Duration, failure: Option<&str>) {
        saturating_increment(&self.project_instance_creations, 1);
        if let Some(reason) = failure {
            saturating_increment(&self.project_instance_failures, 1);
            self.record_guest_failure(reason);
        }
        let elapsed_ns = u64::try_from(elapsed.as_nanos()).unwrap_or(u64::MAX);
        saturating_increment(&self.total_project_instance_time_ns, elapsed_ns);
        self.max_project_instance_time_ns
            .fetch_max(elapsed_ns, Ordering::Relaxed);
    }

    fn report(&self, project_instances: usize) -> ExtensionTelemetryReport {
        ExtensionTelemetryReport {
            guest_calls: self.guest_calls.load(Ordering::Relaxed),
            lifecycle_calls: self.lifecycle_calls.load(Ordering::Relaxed),
            index_calls: self.index_calls.load(Ordering::Relaxed),
            event_calls: self.event_calls.load(Ordering::Relaxed),
            guest_failures: self.guest_failures.load(Ordering::Relaxed),
            guest_traps: self.guest_traps.load(Ordering::Relaxed),
            resource_limit_failures: self.resource_limit_failures.load(Ordering::Relaxed),
            disablements: self.disablements.load(Ordering::Relaxed),
            rejected_outputs: self.rejected_outputs.load(Ordering::Relaxed),
            patch_conflicts: self.patch_conflicts.load(Ordering::Relaxed),
            emitted_index_patches: self.emitted_index_patches.load(Ordering::Relaxed),
            emitted_execution_contexts: self.emitted_execution_contexts.load(Ordering::Relaxed),
            emitted_response_patches: self.emitted_response_patches.load(Ordering::Relaxed),
            emitted_command_patches: self.emitted_command_patches.load(Ordering::Relaxed),
            emitted_process_requests: self.emitted_process_requests.load(Ordering::Relaxed),
            requested_reindex_files: self.requested_reindex_files.load(Ordering::Relaxed),
            total_guest_time_ns: self.total_guest_time_ns.load(Ordering::Relaxed),
            max_guest_time_ns: self.max_guest_time_ns.load(Ordering::Relaxed),
            project_instance_creations: self
                .project_instance_creations
                .load(Ordering::Relaxed),
            project_instance_failures: self.project_instance_failures.load(Ordering::Relaxed),
            total_project_instance_time_ns: self
                .total_project_instance_time_ns
                .load(Ordering::Relaxed),
            max_project_instance_time_ns: self
                .max_project_instance_time_ns
                .load(Ordering::Relaxed),
            project_instances: u64::try_from(project_instances).expect(
                "INVARIANT VIOLATED: project extension instance count does not fit in u64. This is a bug because process address space cannot contain that many Wasm instances. Fix: keep project instance accounting bounded by host memory limits.",
            ),
        }
    }
}

fn saturating_increment(counter: &AtomicU64, amount: u64) {
    let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
        Some(current.saturating_add(amount))
    });
}

#[derive(Clone, Debug, Default, PartialEq)]
struct ExtensionLoadConfig {
    package_paths: Vec<ConfiguredExtensionPath>,
    directory_paths: Vec<ConfiguredExtensionPath>,
    project_package_paths: Vec<ConfiguredExtensionPath>,
    settings: BTreeMap<String, serde_json::Value>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExtensionPathSource {
    Environment,
    ProjectLocal,
    InitializationOptions,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ConfiguredExtensionPath {
    path: PathBuf,
    source: ExtensionPathSource,
}

#[derive(Debug)]
struct ExtensionLoadError {
    message: String,
}

#[derive(Clone, Debug, Deserialize)]
struct ExtensionManifest {
    id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    version: Option<String>,
    abi_version: u32,
    #[serde(default)]
    server_version: Option<String>,
    runtime: String,
    wasm: Option<String>,
    #[serde(default)]
    checksum_sha256: Option<String>,
    #[serde(default)]
    capabilities: Vec<String>,
    #[serde(default)]
    permissions: Vec<String>,
    #[serde(default)]
    settings_schema: Option<toml::Value>,
    build: Option<ExtensionBuildManifest>,
    indexing: Option<ExtensionIndexingManifest>,
    watching: Option<ExtensionWatchingManifest>,
    process: Option<ExtensionProcessManifest>,
    applicability: Option<ExtensionApplicabilityManifest>,
}

#[derive(Clone, Debug, Deserialize)]
struct ExtensionApplicabilityManifest {
    locked_gems: Vec<ExtensionGemRequirementManifest>,
}

#[derive(Clone, Debug, Deserialize)]
struct ExtensionGemRequirementManifest {
    name: String,
    version: String,
}

#[derive(Clone, Debug)]
struct ExtensionGemRequirement {
    name: String,
    version: VersionReq,
}

#[derive(Clone, Debug, Deserialize)]
struct ExtensionBuildManifest {
    output: String,
}

#[derive(Clone, Debug, Deserialize)]
struct ExtensionIndexingManifest {
    call_names: Vec<String>,
    #[serde(default)]
    project_context: ExtensionProjectContextDelivery,
    #[serde(default)]
    frame_call_names: Vec<String>,
    #[serde(default)]
    targets: Vec<ExtensionMethodTargetManifest>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum ExtensionProjectContextDelivery {
    #[default]
    PerCall,
    Activation,
}

fn guest_call_context<'a>(
    delivery: ExtensionProjectContextDelivery,
    project: &'a ruby_fast_lsp_extension_api::ProjectContext,
    context: &'a CallContext,
) -> Cow<'a, CallContext> {
    match delivery {
        ExtensionProjectContextDelivery::Activation if context.project.is_none() => {
            Cow::Borrowed(context)
        }
        ExtensionProjectContextDelivery::Activation => {
            let mut compact = context.clone();
            compact.project = None;
            Cow::Owned(compact)
        }
        ExtensionProjectContextDelivery::PerCall if context.project.is_some() => {
            Cow::Borrowed(context)
        }
        ExtensionProjectContextDelivery::PerCall => {
            let mut complete = context.clone();
            complete.project = Some(project.clone());
            Cow::Owned(complete)
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
struct ExtensionMethodTargetManifest {
    owner: Vec<String>,
    owner_kind: String,
    method: String,
    #[serde(default)]
    frame: bool,
}

#[derive(Clone, Debug, Deserialize)]
struct ExtensionWatchingManifest {
    globs: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct ExtensionProcessManifest {
    commands: Vec<String>,
}

impl ExtensionLoadConfig {
    fn from_config(config: &RubyFastLspConfig) -> Self {
        Self::from_config_and_workspace_roots(config, &[])
    }

    fn from_config_and_workspace_roots(
        config: &RubyFastLspConfig,
        workspace_roots: &[PathBuf],
    ) -> Self {
        let mut load_config = Self::from_environment();
        load_config
            .package_paths
            .extend(
                config
                    .extension_packages
                    .iter()
                    .map(|path| ConfiguredExtensionPath {
                        path: PathBuf::from(path),
                        source: ExtensionPathSource::InitializationOptions,
                    }),
            );
        load_config.settings = config.extension_settings.clone();
        load_config
            .directory_paths
            .extend(
                config
                    .extension_dirs
                    .iter()
                    .map(|path| ConfiguredExtensionPath {
                        path: PathBuf::from(path),
                        source: ExtensionPathSource::InitializationOptions,
                    }),
            );
        if config.workspace_trusted && config.project_extensions_enabled {
            let mut roots = workspace_roots.to_vec();
            roots.sort();
            roots.dedup();
            for root in roots {
                load_config.project_package_paths.extend(
                    discover_project_extension_packages(&root)
                        .into_iter()
                        .map(|path| ConfiguredExtensionPath {
                            path,
                            source: ExtensionPathSource::ProjectLocal,
                        }),
                );
            }
        }
        load_config
    }

    fn from_environment() -> Self {
        let mut config = Self::default();
        if let Some(paths) = std::env::var_os("RUBY_FAST_LSP_EXTENSION_PATHS") {
            for path in std::env::split_paths(&paths) {
                config.package_paths.push(ConfiguredExtensionPath {
                    path,
                    source: ExtensionPathSource::Environment,
                });
            }
        }
        if let Some(paths) = std::env::var_os("RUBY_FAST_LSP_EXTENSION_DIRS") {
            for path in std::env::split_paths(&paths) {
                config.directory_paths.push(ConfiguredExtensionPath {
                    path,
                    source: ExtensionPathSource::Environment,
                });
            }
        }
        config
    }
}

fn discover_project_extension_packages(workspace_root: &Path) -> Vec<PathBuf> {
    let mut packages = Vec::new();
    let hidden_root = workspace_root.join(".ruby-fast-lsp/extensions");
    if hidden_root.is_dir() {
        for entry in WalkDir::new(&hidden_root)
            .min_depth(2)
            .max_depth(2)
            .follow_links(false)
        {
            match entry {
                Ok(entry)
                    if entry.file_type().is_file() && entry.file_name() == "extension.toml" =>
                {
                    packages.push(
                        entry
                            .path()
                            .parent()
                            .expect("INVARIANT VIOLATED: extension.toml discovered without a parent directory. This is a bug because WalkDir entries below a workspace root must have a parent. Fix: preserve the package-directory discovery depth.")
                            .to_path_buf(),
                    );
                }
                Ok(_) => {}
                Err(err) => warn!(
                    "Skipping unreadable project extension entry under `{}`: {}",
                    hidden_root.display(),
                    err
                ),
            }
        }
    }

    let conventional_root = workspace_root.join("ruby_fast_lsp");
    if conventional_root.is_dir() {
        for entry in WalkDir::new(&conventional_root)
            .min_depth(1)
            .follow_links(false)
        {
            match entry {
                Ok(entry)
                    if entry.file_type().is_file() && entry.file_name() == "extension.toml" =>
                {
                    packages.push(
                        entry
                            .path()
                            .parent()
                            .expect("INVARIANT VIOLATED: extension.toml discovered without a parent directory. This is a bug because WalkDir entries below a workspace root must have a parent. Fix: preserve manifest-only project discovery.")
                            .to_path_buf(),
                    );
                }
                Ok(_) => {}
                Err(err) => warn!(
                    "Skipping unreadable project extension entry under `{}`: {}",
                    conventional_root.display(),
                    err
                ),
            }
        }
    }

    packages.sort();
    packages.dedup();
    packages
}

impl ExtensionRegistryHandle {
    pub fn empty() -> Self {
        Self::empty_with_persistent_cache(None)
    }

    pub fn empty_with_cache(persistent_cache: PersistentDerivedProductCache) -> Self {
        Self::empty_with_persistent_cache(Some(persistent_cache))
    }

    fn empty_with_persistent_cache(
        persistent_cache: Option<PersistentDerivedProductCache>,
    ) -> Self {
        Self {
            inner: Arc::new(RwLock::new(ExtensionRegistry::empty())),
            reconfiguration: Arc::new(tokio::sync::Mutex::new(())),
            persistent_cache,
        }
    }

    pub fn from_environment() -> Self {
        Self::from_environment_with_persistent_cache(None)
    }

    pub fn from_environment_with_cache(persistent_cache: PersistentDerivedProductCache) -> Self {
        Self::from_environment_with_persistent_cache(Some(persistent_cache))
    }

    fn from_environment_with_persistent_cache(
        persistent_cache: Option<PersistentDerivedProductCache>,
    ) -> Self {
        let config = ExtensionLoadConfig::from_environment();
        let registry =
            ExtensionRegistry::load_with_persistent_cache(&config, persistent_cache.as_ref());
        Self {
            inner: Arc::new(RwLock::new(registry)),
            reconfiguration: Arc::new(tokio::sync::Mutex::new(())),
            persistent_cache,
        }
    }

    pub fn from_config(config: &RubyFastLspConfig) -> Self {
        Self {
            inner: Arc::new(RwLock::new(ExtensionRegistry::load(
                &ExtensionLoadConfig::from_config(config),
            ))),
            reconfiguration: Arc::new(tokio::sync::Mutex::new(())),
            persistent_cache: None,
        }
    }

    pub fn configure_from_config(&self, config: &RubyFastLspConfig) {
        self.configure_from_config_and_workspace_roots(config, &[]);
    }

    pub fn configure_from_config_and_workspace_roots(
        &self,
        config: &RubyFastLspConfig,
        workspace_roots: &[PathBuf],
    ) {
        let load_config =
            ExtensionLoadConfig::from_config_and_workspace_roots(config, workspace_roots);
        self.configure_from_load_config(load_config);
    }

    pub async fn configure_from_config_and_workspace_roots_governed(
        &self,
        config: &RubyFastLspConfig,
        workspace_roots: &[PathBuf],
        indexing_resources: IndexingResourceGovernor,
    ) -> anyhow::Result<()> {
        let _reconfiguration = self.reconfiguration.lock().await;
        let config = config.clone();
        let workspace_roots = workspace_roots.to_vec();
        let registry = self.clone();
        indexing_resources
            .run_with_resources(
                "extension registry reconfiguration",
                IndexingWorkSpec::new(
                    None,
                    IndexingResourcePriority::Background,
                    1,
                    EXTENSION_LOAD_TRANSIENT_MEMORY_BYTES,
                    1,
                ),
                None,
                move || {
                    let load_config = ExtensionLoadConfig::from_config_and_workspace_roots(
                        &config,
                        &workspace_roots,
                    );
                    registry.configure_from_load_config(load_config);
                },
            )
            .await
    }

    fn configure_from_load_config(&self, load_config: ExtensionLoadConfig) {
        let settings_only = {
            let registry = self.inner.read();
            if !registry.same_discovery(&load_config) || !registry.all_extensions_loaded() {
                false
            } else {
                registry.update_settings(load_config.settings.clone());
                true
            }
        };
        if settings_only {
            let mut registry = self.inner.write();
            registry.load_config.settings = load_config.settings;
            return;
        }

        let replacement = ExtensionRegistry::load_with_persistent_cache(
            &load_config,
            self.persistent_cache.as_ref(),
        );
        let mut registry = self.inner.write();
        let previous = std::mem::replace(&mut *registry, replacement);
        drop(registry);
        previous.deactivate();
    }

    pub fn shutdown(&self) {
        self.inner.read().deactivate();
    }

    pub fn status_reports(&self) -> Vec<ExtensionStatusReport> {
        self.inner.read().status_reports()
    }

    pub fn ensure_semantic_seed_facts(
        &self,
        engine: &Arc<RwLock<ruby_analysis::engine::AnalysisEngine>>,
        project: Option<&ruby_fast_lsp_extension_api::ProjectContext>,
    ) {
        let applicability_fingerprint = extension_applicability_fingerprint(project);
        self.inner
            .read()
            .ensure_semantic_seed_facts(engine, project, applicability_fingerprint);
    }

    pub(crate) fn ensure_semantic_seed_facts_for_snapshot(
        &self,
        engine: &Arc<RwLock<ruby_analysis::engine::AnalysisEngine>>,
        snapshot: &ProjectContextSnapshot,
    ) {
        self.inner.read().ensure_semantic_seed_facts(
            engine,
            Some(&snapshot.context),
            snapshot.applicability_fingerprint,
        );
    }

    pub fn process_call_node(&self, visitor: &mut FactCollector, node: &CallNode) {
        process_call_node_with_registry(self, visitor, node, None, false);
    }

    pub(crate) fn applicability_snapshot(
        &self,
        project: Option<&ruby_fast_lsp_extension_api::ProjectContext>,
    ) -> ExtensionApplicabilitySnapshot {
        self.inner.read().applicability_snapshot(project)
    }

    pub(crate) fn tracks_call(&self, node: &CallNode) -> bool {
        self.inner
            .read()
            .tracked_call_names
            .contains(utils::utf8_str(node.name().as_slice()))
    }

    pub(crate) fn process_call_node_with_applicability(
        &self,
        visitor: &mut FactCollector,
        node: &CallNode,
        applicability: &ExtensionApplicabilitySnapshot,
    ) {
        process_call_node_with_registry(self, visitor, node, Some(applicability), true);
    }

    pub(crate) fn should_track_enclosing_call_with_applicability(
        &self,
        visitor: &FactCollector,
        node: &CallNode,
        applicability: &ExtensionApplicabilitySnapshot,
    ) -> bool {
        self.inner
            .read()
            .should_track_enclosing_call(visitor, node, Some(applicability), true)
    }

    pub(crate) fn resolved_call_for_stack_with_applicability(
        &self,
        visitor: &FactCollector,
        node: &CallNode,
        applicability: &ExtensionApplicabilitySnapshot,
    ) -> ResolvedCall {
        let mut call = resolved_call_for_stack(visitor, node);
        call.frame_extension_ids =
            self.inner
                .read()
                .frame_extension_ids(visitor, node, Some(applicability));
        call
    }

    pub fn document_symbols(
        &self,
        uri: &str,
        text: &str,
        project: Option<ruby_fast_lsp_extension_api::ProjectContext>,
    ) -> Vec<DocumentSymbol> {
        document_symbols_with_registry(self, uri, text, project)
    }

    pub async fn document_symbols_governed(
        &self,
        indexing_resources: IndexingResourceGovernor,
        project_root: Option<PathBuf>,
        uri: String,
        text: String,
        project: Option<ruby_fast_lsp_extension_api::ProjectContext>,
    ) -> anyhow::Result<Vec<DocumentSymbol>> {
        if !self.has_loaded_capability("document_symbol") {
            return Ok(Vec::new());
        }
        let registry = self.clone();
        indexing_resources
            .run_with_resources(
                "extension document symbols",
                IndexingWorkSpec::new(
                    project_root,
                    IndexingResourcePriority::OpenDocument,
                    1,
                    EXTENSION_RESPONSE_TRANSIENT_MEMORY_BYTES,
                    0,
                ),
                None,
                move || registry.document_symbols(&uri, &text, project),
            )
            .await
    }

    pub fn code_lenses(
        &self,
        uri: &str,
        text: &str,
        project: Option<ruby_fast_lsp_extension_api::ProjectContext>,
    ) -> Vec<CodeLens> {
        code_lenses_with_registry(self, uri, text, project)
    }

    pub async fn code_lenses_governed(
        &self,
        indexing_resources: IndexingResourceGovernor,
        project_root: Option<PathBuf>,
        uri: String,
        text: String,
        project: Option<ruby_fast_lsp_extension_api::ProjectContext>,
    ) -> anyhow::Result<Vec<CodeLens>> {
        if !self.has_loaded_capability("code_lens") {
            return Ok(Vec::new());
        }
        let registry = self.clone();
        indexing_resources
            .run_with_resources(
                "extension code lenses",
                IndexingWorkSpec::new(
                    project_root,
                    IndexingResourcePriority::OpenDocument,
                    1,
                    EXTENSION_RESPONSE_TRANSIENT_MEMORY_BYTES,
                    0,
                ),
                None,
                move || registry.code_lenses(&uri, &text, project),
            )
            .await
    }

    fn has_loaded_capability(&self, capability: &str) -> bool {
        self.inner.read().extensions.iter().any(|extension| {
            extension.is_loaded()
                && extension
                    .metadata
                    .capabilities
                    .iter()
                    .any(|candidate| candidate == capability)
        })
    }

    pub fn watcher_globs(&self) -> Vec<String> {
        self.inner.read().watcher_globs()
    }

    pub async fn handle_watched_file_changes(
        &self,
        workspace_trusted: bool,
        workspace_roots: &[PathBuf],
        changes: &[FileEvent],
        indexing_resources: IndexingResourceGovernor,
    ) -> Vec<Url> {
        let pending = handle_watched_file_changes_with_registry(self, workspace_roots, changes);
        let mut reindex_uris = BTreeSet::new();
        for pending in pending {
            if !pending.loaded.is_loaded() {
                continue;
            }
            let validated = match validate_extension_process_request(
                &pending.loaded.metadata.id,
                workspace_trusted,
                &pending.loaded.metadata.permissions,
                &pending.loaded.metadata.process_commands,
                workspace_roots,
                &pending.event_roots,
                &pending.request,
            ) {
                Ok(validated) => validated,
                Err(err) => {
                    pending.loaded.reject(err.to_string());
                    continue;
                }
            };
            let result = run_extension_process(validated, indexing_resources.clone()).await;
            let event = ExtensionEvent {
                event: "process.completed".to_string(),
                call: None,
                document: None,
                project: None,
                settings: None,
                files: None,
                process_results: Some(vec![result]),
            };
            match pending.loaded.handle_event_for_project(&event, None) {
                Ok(output)
                    if output.index_patches.is_empty()
                        && output.execution_contexts.is_empty()
                        && output.response_patches.is_empty()
                        && output.command_patches.is_empty()
                        && output.process_requests.is_empty() =>
                {
                    match validate_extension_reindex_files(
                        &pending.loaded.metadata.id,
                        workspace_roots,
                        &pending.event_roots,
                        &output.reindex_files,
                    ) {
                        Ok(uris) => reindex_uris.extend(uris),
                        Err(err) => {
                            pending.loaded.reject(err.to_string());
                        }
                    }
                }
                Ok(_) => {
                    pending.loaded.reject(format!(
                        "extension `{}` returned output from `process.completed`; process completion callbacks may update private extension state only",
                        pending.loaded.metadata.id
                    ));
                }
                Err(err) => {
                    pending.loaded.fail(format!(
                        "extension `{}` process.completed failed: {err}",
                        pending.loaded.metadata.id
                    ));
                }
            }
        }
        reindex_uris.into_iter().collect()
    }

    fn extensions(&self) -> Vec<Arc<LoadedWasmExtension>> {
        self.inner.read().extensions()
    }

    fn extensions_with_applicability(
        &self,
        project: Option<&ruby_fast_lsp_extension_api::ProjectContext>,
        applicability: Option<&ExtensionApplicabilitySnapshot>,
    ) -> Vec<(Arc<LoadedWasmExtension>, bool)> {
        let registry = self.inner.read();
        registry
            .extensions
            .iter()
            .enumerate()
            .map(|(index, extension)| {
                (
                    extension.clone(),
                    registry.extension_applies_to_source(index, extension, project, applicability),
                )
            })
            .collect()
    }
}

impl FactCollectorExtensionHost for ExtensionRegistryHandle {
    fn process_call_node(&self, visitor: &mut FactCollector, node: &CallNode) {
        ExtensionRegistryHandle::process_call_node(self, visitor, node);
    }

    fn should_track_enclosing_call(&self, visitor: &FactCollector, node: &CallNode) -> bool {
        self.inner
            .read()
            .should_track_enclosing_call(visitor, node, None, false)
    }

    fn resolved_call_for_stack(&self, visitor: &FactCollector, node: &CallNode) -> ResolvedCall {
        let mut call = resolved_call_for_stack(visitor, node);
        call.frame_extension_ids = self.inner.read().frame_extension_ids(visitor, node, None);
        call
    }
}

impl ExtensionLoadError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for ExtensionLoadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl ExtensionRegistry {
    fn empty() -> Self {
        let load_config = ExtensionLoadConfig::default();
        Self {
            extensions: Vec::new(),
            tracked_call_names: tracked_call_names(&[]),
            semantic_seeded_engines: Mutex::new(Vec::new()),
            discovery_fingerprint: extension_packages_fingerprint(&[]),
            load_config,
        }
    }

    fn load(config: &ExtensionLoadConfig) -> Self {
        Self::load_with_persistent_cache(config, None)
    }

    fn load_with_persistent_cache(
        config: &ExtensionLoadConfig,
        persistent_cache: Option<&PersistentDerivedProductCache>,
    ) -> Self {
        let packages = discover_extension_packages(config);
        let discovery_fingerprint = extension_packages_fingerprint(&packages);
        let extensions = load_wasm_extensions_from_packages_with_cache(packages, persistent_cache);
        let tracked_call_names = tracked_call_names(&extensions);
        let registry = Self {
            extensions,
            tracked_call_names,
            semantic_seeded_engines: Mutex::new(Vec::new()),
            load_config: config.clone(),
            discovery_fingerprint,
        };
        registry.activate();
        registry
    }

    fn same_discovery(&self, config: &ExtensionLoadConfig) -> bool {
        self.load_config.package_paths == config.package_paths
            && self.load_config.directory_paths == config.directory_paths
            && self.load_config.project_package_paths == config.project_package_paths
            && self.discovery_fingerprint
                == extension_packages_fingerprint(&discover_extension_packages(config))
    }

    fn all_extensions_loaded(&self) -> bool {
        self.extensions
            .iter()
            .all(|extension| extension.is_loaded())
    }

    fn activate(&self) {
        for extension in &self.extensions {
            let settings = self
                .load_config
                .settings
                .get(&extension.metadata.id)
                .cloned();
            extension.handle_lifecycle_event("lifecycle.activate", settings);
        }
    }

    fn update_settings(&self, settings: BTreeMap<String, serde_json::Value>) {
        if self.load_config.settings == settings {
            return;
        }
        for extension in &self.extensions {
            if !extension.is_loaded() {
                continue;
            }
            let previous = self.load_config.settings.get(&extension.metadata.id);
            let current = settings.get(&extension.metadata.id);
            if previous != current {
                extension.handle_lifecycle_event("settings.changed", current.cloned());
            }
        }
    }

    fn deactivate(&self) {
        for extension in &self.extensions {
            if extension.is_loaded() {
                extension.handle_lifecycle_event("lifecycle.deactivate", None);
            }
        }
    }

    fn extensions(&self) -> Vec<Arc<LoadedWasmExtension>> {
        self.extensions.clone()
    }

    fn applicability_snapshot(
        &self,
        project: Option<&ruby_fast_lsp_extension_api::ProjectContext>,
    ) -> ExtensionApplicabilitySnapshot {
        ExtensionApplicabilitySnapshot {
            registry_fingerprint: self.discovery_fingerprint,
            applies_to_source: self
                .extensions
                .iter()
                .map(|extension| extension.applies_to_source(project))
                .collect(),
        }
    }

    fn extension_applies_to_source(
        &self,
        extension_index: usize,
        extension: &LoadedWasmExtension,
        project: Option<&ruby_fast_lsp_extension_api::ProjectContext>,
        applicability: Option<&ExtensionApplicabilitySnapshot>,
    ) -> bool {
        let Some(applicability) = applicability else {
            return extension.applies_to_source(project);
        };
        if applicability.registry_fingerprint != self.discovery_fingerprint
            || applicability.applies_to_source.len() != self.extensions.len()
        {
            // Registry replacement is rare and may race an already-running file
            // traversal. Preserve exact current-registry semantics in that case;
            // the owning indexing generation will replace the file normally.
            return extension.applies_to_source(project);
        }
        applicability.applies_to_source[extension_index]
    }

    fn should_track_enclosing_call(
        &self,
        visitor: &FactCollector,
        node: &CallNode,
        applicability: Option<&ExtensionApplicabilitySnapshot>,
        tracked_call_prechecked: bool,
    ) -> bool {
        let method_name = utils::utf8_str(node.name().as_slice());
        if !tracked_call_prechecked && !self.tracked_call_names.contains(method_name) {
            return false;
        }

        if self
            .extensions
            .iter()
            .enumerate()
            .any(|(index, extension)| {
                extension.is_loaded()
                    && self.extension_applies_to_source(
                        index,
                        extension,
                        visitor.extension_project_context.as_ref(),
                        applicability,
                    )
                    && extension
                        .semantic_targets
                        .iter()
                        .any(|target| target.frame && target.method.as_str() == method_name)
                    && extension.semantically_matches_call(visitor, node)
            })
        {
            return true;
        }

        if self
            .extensions
            .iter()
            .enumerate()
            .any(|(index, extension)| {
                extension.is_loaded()
                    && self.extension_applies_to_source(
                        index,
                        extension,
                        visitor.extension_project_context.as_ref(),
                        applicability,
                    )
                    && extension.frame_call_names.contains(method_name)
            })
        {
            return true;
        }

        if !visitor.extension_call_stack.is_empty()
            && self
                .extensions
                .iter()
                .enumerate()
                .any(|(index, extension)| {
                    extension.is_loaded()
                        && self.extension_applies_to_source(
                            index,
                            extension,
                            visitor.extension_project_context.as_ref(),
                            applicability,
                        )
                        && extension.can_run_inside_extension_frame(visitor, node)
                })
        {
            return true;
        }

        !self.has_loaded_wasm_for_call(method_name)
            && ruby_fast_lsp_extension_rspec::extension()
                .indexed_call_names()
                .contains(&method_name)
    }

    fn frame_extension_ids(
        &self,
        visitor: &FactCollector,
        node: &CallNode,
        applicability: Option<&ExtensionApplicabilitySnapshot>,
    ) -> Vec<String> {
        let method_name = utils::utf8_str(node.name().as_slice());
        let active_frame_ids = visitor
            .extension_call_stack
            .iter()
            .flat_map(|call| call.frame_extension_ids.iter())
            .collect::<BTreeSet<_>>();
        let explicitly_switches_receiver = node
            .receiver()
            .is_some_and(|receiver| receiver.as_self_node().is_none());
        self.extensions
            .iter()
            .enumerate()
            .filter(|(index, extension)| {
                if !extension.is_loaded()
                    || !self.extension_applies_to_source(
                        *index,
                        extension,
                        visitor.extension_project_context.as_ref(),
                        applicability,
                    )
                {
                    return false;
                }
                let inherits_frame = visitor.extension_call_stack.iter().any(|call| {
                    call.frame_extension_ids
                        .iter()
                        .any(|id| id == &extension.metadata.id)
                });
                if !active_frame_ids.is_empty() && !inherits_frame && !explicitly_switches_receiver
                {
                    return false;
                }
                extension.semantically_matches_frame_call(visitor, node)
                    || (inherits_frame
                        && (extension.handles_call(method_name)
                            || extension.frame_call_names.contains(method_name)))
                    || (!extension.has_semantic_targets()
                        && extension.frame_call_names.contains(method_name))
            })
            .map(|(_, extension)| extension.metadata.id.clone())
            .collect()
    }

    fn status_reports(&self) -> Vec<ExtensionStatusReport> {
        self.extensions
            .iter()
            .map(|extension| extension.status_report())
            .collect()
    }

    fn watcher_globs(&self) -> Vec<String> {
        self.extensions
            .iter()
            .filter(|extension| extension.is_loaded())
            .flat_map(|extension| extension.metadata.watched_files.iter().cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    fn has_loaded_wasm_for_call(&self, method_name: &str) -> bool {
        self.extensions
            .iter()
            .any(|extension| extension.is_loaded() && extension.handles_call(method_name))
    }

    fn ensure_semantic_seed_facts(
        &self,
        engine: &Arc<RwLock<ruby_analysis::engine::AnalysisEngine>>,
        project: Option<&ruby_fast_lsp_extension_api::ProjectContext>,
        applicability_fingerprint: ExtensionApplicabilityFingerprint,
    ) {
        let mut seeded_engines = self.semantic_seeded_engines.lock();
        seeded_engines.retain(|seeded| seeded.engine.strong_count() > 0);
        if let Some(seeded) = seeded_engines.iter_mut().find(|seeded| {
            seeded
                .engine
                .upgrade()
                .is_some_and(|seeded_engine| Arc::ptr_eq(&seeded_engine, engine))
        }) {
            if seeded.applicability_fingerprint == applicability_fingerprint {
                return;
            }
            seeded.applicability_fingerprint = applicability_fingerprint;
        }

        let mut engine_guard = engine.write();
        let file_id = engine_guard.register_file(SourceFileInput {
            path: PathBuf::from("/__ruby_fast_lsp_extension__/semantic_targets.rb"),
            content: String::new(),
            kind: SourceKind::Stub,
        });
        let range = TextRange::new(file_id, 0, 0);
        let mut facts = FileFacts::default();
        for extension in &self.extensions {
            if !extension.is_loaded() || !extension.applies_to(project) {
                continue;
            }
            for target in &extension.semantic_targets {
                let owner = FullyQualifiedName::namespace_with_kind(
                    target.owner.clone(),
                    target.owner_kind,
                );
                let fqn = FullyQualifiedName::method(target.owner.clone(), target.method);
                facts.symbols.push(SymbolFact::new(
                    fqn.clone(),
                    AnalysisSymbolKind::Method,
                    range,
                ));
                facts.methods.push(MethodFact::new(fqn, owner, range));
            }
        }
        engine_guard.replace_facts(file_id, facts, ResolveMode::Deferred);
        drop(engine_guard);
        if !seeded_engines.iter().any(|seeded| {
            seeded
                .engine
                .upgrade()
                .is_some_and(|seeded_engine| Arc::ptr_eq(&seeded_engine, engine))
        }) {
            seeded_engines.push(SeededExtensionEngine {
                engine: Arc::downgrade(engine),
                applicability_fingerprint,
            });
        }
    }
}

#[cfg(test)]
impl ExtensionApplicabilitySnapshot {
    fn applies_to_extension(
        &self,
        registry: &ExtensionRegistryHandle,
        extension_id: &str,
        project: Option<&ruby_fast_lsp_extension_api::ProjectContext>,
    ) -> bool {
        let registry = registry.inner.read();
        let (index, extension) = registry
            .extensions
            .iter()
            .enumerate()
            .find(|(_, extension)| extension.metadata.id == extension_id)
            .expect(
                "INVARIANT VIOLATED: applicability test requested an unknown extension. This is a broken test because snapshots contain one decision per loaded registry extension. Fix: load the extension before querying its decision.",
            );
        registry.extension_applies_to_source(index, extension, project, Some(self))
    }
}

fn extension_applicability_fingerprint(
    project: Option<&ruby_fast_lsp_extension_api::ProjectContext>,
) -> ExtensionApplicabilityFingerprint {
    ExtensionApplicabilityFingerprint::from_project_context(project)
}

impl LoadedWasmExtension {
    fn new(
        metadata: ExtensionMetadata,
        extension: ruby_fast_lsp_extension_wasm_host::WasmExtension,
        compiled_extension: ruby_fast_lsp_extension_wasm_host::CompiledWasmExtension,
        semantic_targets: Vec<ExtensionMethodTarget>,
        frame_call_names: BTreeSet<String>,
        applicability: Vec<ExtensionGemRequirement>,
        project_context_delivery: ExtensionProjectContextDelivery,
    ) -> Self {
        let indexed_call_names = extension
            .indexed_call_names()
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        let watched_file_matcher = build_watched_file_matcher(
            &metadata.id,
            &metadata.watched_files,
        )
        .expect("INVARIANT VIOLATED: validated extension watcher globs failed to compile while constructing a loaded extension. This is a bug because manifest validation and runtime matching use the same compiler. Fix: keep watcher validation before Wasm instantiation.");
        Self {
            metadata,
            extension: Mutex::new(extension),
            project_extensions: Mutex::new(BTreeMap::new()),
            compiled_extension,
            activation_settings: Mutex::new(None),
            status: Mutex::new(ExtensionStatus::Discovered),
            indexed_call_names,
            frame_call_names,
            semantic_targets,
            watched_file_matcher,
            applicability,
            project_context_delivery,
            telemetry: ExtensionTelemetry::default(),
            #[cfg(test)]
            applicability_evaluations: AtomicU64::new(0),
        }
    }

    fn is_loaded(&self) -> bool {
        *self.status.lock() == ExtensionStatus::Loaded
    }

    fn handle_lifecycle_event(&self, event_name: &str, settings: Option<serde_json::Value>) {
        let started = Instant::now();
        if matches!(event_name, "lifecycle.activate" | "settings.changed") {
            *self.activation_settings.lock() = settings.clone();
        }
        let event = ExtensionEvent {
            event: event_name.to_string(),
            call: None,
            document: None,
            project: None,
            settings,
            files: None,
            process_results: None,
        };
        let base_result = self.extension.lock().handle_event(&event);
        let project_result = if lifecycle_output_is_empty(&base_result) {
            let mut projects = self.project_extensions.lock();
            projects.values_mut().try_for_each(|extension| {
                extension
                    .handle_event(&event)
                    .and_then(require_empty_lifecycle_output)
            })
        } else {
            Ok(())
        };
        let failure = base_result
            .as_ref()
            .err()
            .map(ToString::to_string)
            .or_else(|| project_result.as_ref().err().map(ToString::to_string));
        self.telemetry.record_call(
            GuestCallKind::Lifecycle,
            started.elapsed(),
            base_result.as_ref().ok(),
            failure.as_deref(),
        );
        match (base_result, project_result) {
            (Ok(output), Ok(()))
                if output.index_patches.is_empty()
                    && output.execution_contexts.is_empty()
                    && output.response_patches.is_empty()
                    && output.command_patches.is_empty()
                    && output.process_requests.is_empty()
                    && output.reindex_files.is_empty() =>
            {
                let mut status = self.status.lock();
                *status = match event_name {
                    "lifecycle.activate" | "settings.changed" => ExtensionStatus::Loaded,
                    "lifecycle.deactivate" => ExtensionStatus::Deactivated,
                    other => panic!(
                        "INVARIANT VIOLATED: unsupported extension lifecycle event `{other}`. This is a bug because lifecycle state transitions must be explicit. Fix: add the event and its resulting state to handle_lifecycle_event."
                    ),
                };
            }
            (Ok(_), Ok(())) => self.reject(format!(
                "extension `{}` returned patches from `{event_name}`; lifecycle events must not mutate semantic or editor state",
                self.metadata.id
            )),
            (Err(err), _) | (_, Err(err)) => self.fail(format!(
                "extension `{}` {event_name} failed: {err}",
                self.metadata.id
            )),
        }
        if event_name == "lifecycle.deactivate" {
            self.project_extensions.lock().clear();
        }
    }

    #[cfg(test)]
    fn index_call_output(
        &self,
        context: &CallContext,
    ) -> anyhow::Result<ruby_fast_lsp_extension_api::ExtensionOutput> {
        self.index_call_output_for_project(context.project.as_ref(), context)
    }

    fn index_call_output_for_project(
        &self,
        project: Option<&ruby_fast_lsp_extension_api::ProjectContext>,
        context: &CallContext,
    ) -> anyhow::Result<ruby_fast_lsp_extension_api::ExtensionOutput> {
        assert!(
            context
                .project
                .as_ref()
                .is_none_or(|context_project| Some(context_project) == project),
            "INVARIANT VIOLATED: an extension call payload disagrees with its explicit owning project. This is a host bug because project instance selection and guest-visible context must describe one source owner. Fix: construct the call context from the same FactCollector project passed to index_call_output_for_project."
        );
        let (result, elapsed) = if let Some(project) = project {
            let mut extensions = self.project_extensions.lock();
            if !extensions.contains_key(&project.project_uri) {
                let creation_started = Instant::now();
                let created = (|| {
                    let mut extension =
                        ruby_fast_lsp_extension_wasm_host::WasmExtension::from_compiled(
                            self.metadata.id.clone(),
                            self.compiled_extension.clone(),
                        )?;
                    let activation = ExtensionEvent {
                        event: "lifecycle.activate".to_string(),
                        call: None,
                        document: None,
                        project: Some(project.clone()),
                        settings: self.activation_settings.lock().clone(),
                        files: None,
                        process_results: None,
                    };
                    require_empty_lifecycle_output(extension.handle_event(&activation)?)?;
                    Ok::<_, anyhow::Error>(extension)
                })();
                let creation_failure = created.as_ref().err().map(ToString::to_string);
                self.telemetry.record_project_instance_creation(
                    creation_started.elapsed(),
                    creation_failure.as_deref(),
                );
                extensions.insert(project.project_uri.clone(), created?);
            }
            let started = Instant::now();
            let guest_context = guest_call_context(self.project_context_delivery, project, context);
            let result = extensions
                .get_mut(&project.project_uri)
                .expect(
                    "INVARIANT VIOLATED: project Wasm instance disappeared immediately after insertion. This is a host registry bug because the instance map is locked for the entire operation. Fix: keep lookup and insertion under one project-extension lock.",
                )
                .index_call_output(guest_context.as_ref());
            (result, started.elapsed())
        } else {
            let started = Instant::now();
            let result = self.extension.lock().index_call_output(context);
            (result, started.elapsed())
        };
        let failure = result.as_ref().err().map(ToString::to_string);
        self.telemetry.record_call(
            GuestCallKind::Index,
            elapsed,
            result.as_ref().ok(),
            failure.as_deref(),
        );
        result
    }

    fn handle_event_for_project(
        &self,
        event: &ExtensionEvent,
        project: Option<&ruby_fast_lsp_extension_api::ProjectContext>,
    ) -> anyhow::Result<ruby_fast_lsp_extension_api::ExtensionOutput> {
        let (result, elapsed) = if let Some(project) = project {
            let mut extensions = self.project_extensions.lock();
            if !extensions.contains_key(&project.project_uri) {
                let creation_started = Instant::now();
                let created = (|| {
                    let mut extension =
                        ruby_fast_lsp_extension_wasm_host::WasmExtension::from_compiled(
                            self.metadata.id.clone(),
                            self.compiled_extension.clone(),
                        )?;
                    let activation = ExtensionEvent {
                        event: "lifecycle.activate".to_string(),
                        call: None,
                        document: None,
                        project: Some(project.clone()),
                        settings: self.activation_settings.lock().clone(),
                        files: None,
                        process_results: None,
                    };
                    require_empty_lifecycle_output(extension.handle_event(&activation)?)?;
                    Ok::<_, anyhow::Error>(extension)
                })();
                let creation_failure = created.as_ref().err().map(ToString::to_string);
                self.telemetry.record_project_instance_creation(
                    creation_started.elapsed(),
                    creation_failure.as_deref(),
                );
                extensions.insert(project.project_uri.clone(), created?);
            }
            let started = Instant::now();
            let result = extensions
                .get_mut(&project.project_uri)
                .expect(
                    "INVARIANT VIOLATED: project Wasm instance disappeared immediately after insertion. This is a host registry bug because the instance map is locked for the entire event. Fix: keep lookup and insertion under one project-extension lock.",
                )
                .handle_event(event);
            (result, started.elapsed())
        } else {
            let started = Instant::now();
            let result = self.extension.lock().handle_event(event);
            (result, started.elapsed())
        };
        let failure = result.as_ref().err().map(ToString::to_string);
        self.telemetry.record_call(
            GuestCallKind::Event,
            elapsed,
            result.as_ref().ok(),
            failure.as_deref(),
        );
        result
    }

    fn fail(&self, reason: impl Into<String>) {
        let failure = ExtensionStatus::from_failure(reason);
        let mut status = self.status.lock();
        if matches!(
            *status,
            ExtensionStatus::Discovered | ExtensionStatus::Loaded
        ) {
            self.telemetry.record_disablement();
            *status = failure;
        }
    }

    fn reject(&self, reason: impl Into<String>) {
        self.telemetry.record_rejected_output();
        self.fail(reason);
    }

    fn reject_conflict(&self, reason: impl Into<String>) {
        self.telemetry.record_rejected_output();
        self.telemetry.record_patch_conflict();
        self.fail(reason);
    }

    fn status_report(&self) -> ExtensionStatusReport {
        let project_instances = self.project_extensions.lock().len();
        let status_guard = self.status.lock();
        let (status, last_error) = match &*status_guard {
            ExtensionStatus::Discovered => ("discovered", None),
            ExtensionStatus::Loaded => ("loaded", None),
            ExtensionStatus::Deactivated => ("deactivated", None),
            ExtensionStatus::Slow { reason } => ("slow", Some(reason.clone())),
            ExtensionStatus::Failed { reason } => ("failed", Some(reason.clone())),
        };
        ExtensionStatusReport {
            id: self.metadata.id.clone(),
            name: self.metadata.name.clone(),
            version: self.metadata.version.clone(),
            status: status.to_string(),
            last_error,
            capabilities: self.metadata.capabilities.clone(),
            permissions: self.metadata.permissions.clone(),
            watched_files: self.metadata.watched_files.clone(),
            process_commands: self.metadata.process_commands.clone(),
            indexed_call_names: self.indexed_call_names.iter().cloned().collect(),
            telemetry: self.telemetry.report(project_instances),
        }
    }

    fn handles_call(&self, method_name: &str) -> bool {
        self.indexed_call_names.contains(method_name)
    }

    fn applies_to(&self, project: Option<&ruby_fast_lsp_extension_api::ProjectContext>) -> bool {
        #[cfg(test)]
        self.applicability_evaluations
            .fetch_add(1, Ordering::Relaxed);
        if self.applicability.is_empty() {
            return true;
        }
        let Some(project) = project else {
            return false;
        };
        if !project.lockfile_present || !project.locked_gems_complete {
            return false;
        }
        self.applicability.iter().all(|required| {
            project.locked_gems.iter().any(|locked| {
                locked.name == required.name
                    && Version::parse(&locked.version)
                        .ok()
                        .is_some_and(|version| required.version.matches(&version))
            })
        })
    }

    #[cfg(test)]
    fn test_applicability_evaluations(&self) -> u64 {
        self.applicability_evaluations.load(Ordering::Relaxed)
    }

    fn applies_to_source(
        &self,
        project: Option<&ruby_fast_lsp_extension_api::ProjectContext>,
    ) -> bool {
        if !self.applies_to(project) {
            return false;
        }
        project.is_none_or(|project| {
            matches!(
                project.source_kind,
                ruby_fast_lsp_extension_api::ProjectSourceKind::Project
                    | ruby_fast_lsp_extension_api::ProjectSourceKind::Excluded
            )
        })
    }

    fn has_semantic_targets(&self) -> bool {
        !self.semantic_targets.is_empty()
    }

    fn semantically_matches_call(&self, visitor: &FactCollector, node: &CallNode) -> bool {
        if !self.has_semantic_targets() {
            return self.handles_call(utils::utf8_str(node.name().as_slice()));
        }

        let method_name = utils::utf8_str(node.name().as_slice());
        if !self.handles_call(method_name) {
            return false;
        }

        let Ok(method) = RubyMethod::new(method_name) else {
            return false;
        };
        let callees = resolved_core_callees_for_call(visitor, node);
        self.semantic_targets.iter().any(|target| {
            extension_target_owner_exists(visitor, target)
                && target.method == method
                && callees.iter().any(|callee| {
                    callee.resolution != MethodCalleeResolution::ReceiverOnly
                        && target.owner == callee.owner.namespace_parts()
                        && Some(target.owner_kind) == callee.owner.namespace_kind()
                        && target.method == callee.method
                })
        })
    }

    fn semantically_matches_frame_call(&self, visitor: &FactCollector, node: &CallNode) -> bool {
        if !self.has_semantic_targets() {
            return self
                .frame_call_names
                .contains(utils::utf8_str(node.name().as_slice()));
        }

        let method_name = utils::utf8_str(node.name().as_slice());
        let Ok(method) = RubyMethod::new(method_name) else {
            return false;
        };
        let callees = resolved_core_callees_for_call(visitor, node);
        self.semantic_targets.iter().any(|target| {
            target.frame
                && extension_target_owner_exists(visitor, target)
                && target.method == method
                && callees.iter().any(|callee| {
                    callee.resolution != MethodCalleeResolution::ReceiverOnly
                        && target.owner == callee.owner.namespace_parts()
                        && Some(target.owner_kind) == callee.owner.namespace_kind()
                        && target.method == callee.method
                })
        })
    }

    fn can_run_inside_extension_frame(&self, visitor: &FactCollector, node: &CallNode) -> bool {
        self.handles_call(utils::utf8_str(node.name().as_slice()))
            && visitor.extension_call_stack.iter().any(|call| {
                call.frame_extension_ids
                    .iter()
                    .any(|id| id == &self.metadata.id)
            })
    }
}

fn tracked_call_names(extensions: &[Arc<LoadedWasmExtension>]) -> BTreeSet<String> {
    let mut names = ruby_fast_lsp_extension_rspec::extension()
        .indexed_call_names()
        .iter()
        .map(|name| (*name).to_string())
        .collect::<BTreeSet<_>>();
    for extension in extensions {
        names.extend(extension.indexed_call_names.iter().cloned());
        names.extend(extension.frame_call_names.iter().cloned());
    }
    names
}

fn extension_target_owner_exists(visitor: &FactCollector, target: &ExtensionMethodTarget) -> bool {
    let required_owner = FullyQualifiedName::namespace(target.owner.clone());
    let engine = visitor.analysis_engine.read();
    ruby_analysis::engine::AnalysisQuery::new(&engine).namespace_exists(&required_owner)
}

pub fn configure_from_config(config: &RubyFastLspConfig) {
    EXTENSION_REGISTRY.configure_from_config(config);
}

pub fn extension_status_reports() -> Vec<ExtensionStatusReport> {
    EXTENSION_REGISTRY.status_reports()
}

pub fn extension_status_response() -> ExtensionStatusResponse {
    ExtensionStatusResponse {
        extensions: extension_status_reports(),
    }
}

pub fn validate_extension_package(path: &Path) -> Result<ExtensionStatusReport, String> {
    let mut packages = Vec::new();
    collect_extension_package(
        &ConfiguredExtensionPath {
            path: path.to_path_buf(),
            source: ExtensionPathSource::InitializationOptions,
        },
        true,
        &mut packages,
    )
    .map_err(|err| err.to_string())?;
    if packages.len() != 1 {
        return Err(format!(
            "extension package `{}` resolved to {} packages; expected exactly 1",
            path.display(),
            packages.len()
        ));
    }
    let extension = load_wasm_extension(
        packages
            .pop()
            .expect("INVARIANT VIOLATED: package length checked above"),
    )
    .map_err(|err| err.to_string())?;
    Ok(extension.status_report())
}

pub fn process_call_node(visitor: &mut FactCollector, node: &CallNode) {
    process_call_node_with_registry(&EXTENSION_REGISTRY, visitor, node, None, false);
}

fn process_call_node_with_registry(
    registry: &ExtensionRegistryHandle,
    visitor: &mut FactCollector,
    node: &CallNode,
    applicability: Option<&ExtensionApplicabilitySnapshot>,
    tracked_call_prechecked: bool,
) {
    let method_name = utils::utf8_str(node.name().as_slice());
    if !tracked_call_prechecked
        && !registry
            .inner
            .read()
            .tracked_call_names
            .contains(method_name)
    {
        return;
    }
    if process_wasm_call_node(registry, visitor, node, applicability) {
        return;
    }
    if registry.inner.read().has_loaded_wasm_for_call(method_name) {
        return;
    }

    let rspec = ruby_fast_lsp_extension_rspec::extension();

    assert!(
        rspec.abi_version() == ruby_fast_lsp_extension_api::ABI_VERSION,
        "INVARIANT VIOLATED: extension ABI version mismatch for {}. \
         This is a bug because extension patches cannot be safely interpreted across ABI versions. \
         Fix: rebuild extension against current ruby-fast-lsp-extension-api.",
        rspec.id()
    );

    if !rspec.indexed_call_names().contains(&method_name) {
        return;
    }

    let ctx = call_context(visitor, node, true);
    let output = rspec.index_call_output(&ctx);
    validate_index_patch_provenance(rspec.id(), &output.index_patches).expect(
        "INVARIANT VIOLATED: bundled native extension spoofed index patch provenance. This is a bug because bundled and Wasm extensions must obey the same public trust contract. Fix: emit the compiled extension ID in every PatchSource.",
    );
    validate_index_patch_payloads(&output.index_patches).expect(
        "INVARIANT VIOLATED: bundled native extension emitted an invalid index patch. This is a bug because native adapters must use the same validated ABI as Wasm guests. Fix: correct the extension payload.",
    );
    assert!(
        ctx.project.is_some()
            || !output
                .index_patches
                .iter()
                .any(index_patch_requires_project_context),
        "INVARIANT VIOLATED: bundled native extension emitted a project-generated owner without an owning ProjectContext. This is a guest bug because project-scoped semantic identity cannot be constructed outside a project. Fix: emit source-scoped owners or require project context."
    );
    validate_execution_contexts(rspec.id(), &ctx, &output.execution_contexts).expect(
        "INVARIANT VIOLATED: bundled native extension emitted an invalid execution context. This is a bug because native adapters must use the same validated ABI as Wasm guests. Fix: correct the context ranges, owners, targets, or provenance.",
    );
    for patch in output.index_patches {
        apply_patch(visitor, node, patch);
    }
    for context in output.execution_contexts {
        apply_execution_context(visitor, context);
    }
}

pub fn document_symbols(uri: &str, text: &str) -> Vec<DocumentSymbol> {
    document_symbols_with_registry(&EXTENSION_REGISTRY, uri, text, None)
}

fn document_symbols_with_registry(
    registry: &ExtensionRegistryHandle,
    uri: &str,
    text: &str,
    project: Option<ruby_fast_lsp_extension_api::ProjectContext>,
) -> Vec<DocumentSymbol> {
    let mut symbols = Vec::new();
    handle_response_event(
        registry,
        "request.document_symbol",
        uri,
        text,
        project,
        |patch| match response_patch_to_document_symbol(patch) {
            Ok(Some(symbol)) => {
                symbols.push(symbol);
                Ok(())
            }
            Ok(None) => Ok(()),
            Err(err) => Err(err),
        },
    );
    symbols
}

pub fn code_lenses(uri: &str, text: &str) -> Vec<CodeLens> {
    code_lenses_with_registry(&EXTENSION_REGISTRY, uri, text, None)
}

fn code_lenses_with_registry(
    registry: &ExtensionRegistryHandle,
    uri: &str,
    text: &str,
    project: Option<ruby_fast_lsp_extension_api::ProjectContext>,
) -> Vec<CodeLens> {
    let mut lenses = Vec::new();
    handle_response_event(registry, "request.code_lens", uri, text, project, |patch| {
        match response_patch_to_code_lens(patch) {
            Ok(Some(lens)) => {
                lenses.push(lens);
                Ok(())
            }
            Ok(None) => Ok(()),
            Err(err) => Err(err),
        }
    });
    lenses
}

fn handle_response_event(
    registry: &ExtensionRegistryHandle,
    event_name: &str,
    uri: &str,
    text: &str,
    project: Option<ruby_fast_lsp_extension_api::ProjectContext>,
    mut handle_patch: impl FnMut(ResponsePatch) -> Result<(), String>,
) {
    let required_capability = match event_name {
        "request.document_symbol" => "document_symbol",
        "request.code_lens" => "code_lens",
        other => panic!(
            "INVARIANT VIOLATED: unsupported response event `{other}` reached extension dispatch. This is a host bug because response events must map to an explicit manifest capability. Fix: add the event-to-capability mapping before dispatching it."
        ),
    };
    let event = ExtensionEvent {
        event: event_name.to_string(),
        call: None,
        document: Some(DocumentContext {
            uri: uri.to_string(),
            text: text.to_string(),
            project: project.clone(),
        }),
        project: None,
        settings: None,
        files: None,
        process_results: None,
    };
    let extensions = registry.extensions();

    for loaded in extensions {
        if !loaded.is_loaded() {
            continue;
        }
        if !loaded
            .metadata
            .capabilities
            .iter()
            .any(|capability| capability == required_capability)
        {
            continue;
        }
        if !loaded.applies_to_source(project.as_ref()) {
            continue;
        }

        let extension_output = match loaded.handle_event_for_project(&event, project.as_ref()) {
            Ok(extension_output) => extension_output,
            Err(err) => {
                warn!(
                    "Disabling Ruby Fast LSP extension `{}` after event `{}` failure: {}",
                    loaded.metadata.id, event_name, err
                );
                let reason = err.to_string();
                loaded.fail(reason);
                continue;
            }
        };
        if let Err(spoofed_id) = validate_response_patch_provenance(
            &loaded.metadata.id,
            &extension_output.response_patches,
        ) {
            warn!(
                "Disabling Ruby Fast LSP extension `{}` after response patch provenance spoofed `{}` for `{}`",
                loaded.metadata.id, spoofed_id, event_name
            );
            loaded.reject(format!(
                "extension `{}` emitted response patch provenance for `{spoofed_id}`",
                loaded.metadata.id
            ));
            continue;
        }
        for patch in extension_output.response_patches {
            if let Err(err) = handle_patch(patch) {
                warn!(
                    "Disabling Ruby Fast LSP extension `{}` after invalid response patch for `{}`: {}",
                    loaded.metadata.id, event_name, err
                );
                loaded.reject(err);
                break;
            }
        }
    }
}

fn handle_watched_file_changes_with_registry(
    registry: &ExtensionRegistryHandle,
    workspace_roots: &[PathBuf],
    changes: &[FileEvent],
) -> Vec<PendingExtensionProcessRequest> {
    let mut pending = Vec::new();
    let candidates = watched_file_candidates(workspace_roots, changes);
    if candidates.is_empty() {
        return pending;
    }

    for loaded in registry.extensions() {
        if !loaded.is_loaded() || loaded.metadata.watched_files.is_empty() {
            continue;
        }
        let matched = candidates
            .iter()
            .filter(|change| loaded.watched_file_matcher.is_match(&change.path))
            .cloned()
            .collect::<Vec<_>>();
        if matched.is_empty() {
            continue;
        }
        let event_roots = matched
            .iter()
            .map(|change| PathBuf::from(&change.workspace_root))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();

        let event = ExtensionEvent {
            event: "files.changed".to_string(),
            call: None,
            document: None,
            project: None,
            settings: None,
            files: Some(matched),
            process_results: None,
        };
        match loaded.handle_event_for_project(&event, None) {
            Ok(output)
                if output.index_patches.is_empty()
                    && output.execution_contexts.is_empty()
                    && output.response_patches.is_empty()
                    && output.command_patches.is_empty() =>
            {
                if output.process_requests.len() > MAX_PROCESS_REQUESTS_PER_EVENT {
                    loaded.reject(format!(
                        "extension `{}` returned {} process requests from `files.changed`, exceeding the limit of {MAX_PROCESS_REQUESTS_PER_EVENT}",
                        loaded.metadata.id,
                        output.process_requests.len()
                    ));
                    continue;
                }
                let mut request_ids = BTreeSet::new();
                if output
                    .process_requests
                    .iter()
                    .any(|request| !request_ids.insert(request.request_id.clone()))
                {
                    loaded.reject(format!(
                        "extension `{}` returned duplicate process request ids from `files.changed`",
                        loaded.metadata.id
                    ));
                    continue;
                }
                pending.extend(output.process_requests.into_iter().map(|request| {
                    PendingExtensionProcessRequest {
                        loaded: Arc::clone(&loaded),
                        event_roots: event_roots.clone(),
                        request,
                    }
                }));
            }
            Ok(_) => {
                loaded.reject(format!(
                    "extension `{}` returned patches from `files.changed`; watched-file events may update private extension state only",
                    loaded.metadata.id
                ));
            }
            Err(err) => {
                loaded.fail(format!(
                    "extension `{}` files.changed failed: {err}",
                    loaded.metadata.id
                ));
            }
        }
    }
    pending
}

struct PendingExtensionProcessRequest {
    loaded: Arc<LoadedWasmExtension>,
    event_roots: Vec<PathBuf>,
    request: ProcessRequest,
}

fn watched_file_candidates(
    workspace_roots: &[PathBuf],
    changes: &[FileEvent],
) -> Vec<WatchedFileChange> {
    let mut roots = workspace_roots.to_vec();
    roots.sort_by(|left, right| {
        right
            .as_os_str()
            .len()
            .cmp(&left.as_os_str().len())
            .then_with(|| left.cmp(right))
    });
    roots.dedup();

    let mut candidates = BTreeSet::new();
    for change in changes {
        let Ok(file_path) = change.uri.to_file_path() else {
            warn!(
                "Ignoring extension watched-file event with non-file URI `{}`",
                change.uri
            );
            continue;
        };
        let Some(root) = roots.iter().find(|root| file_path.starts_with(root)) else {
            continue;
        };
        let relative = file_path.strip_prefix(root).expect(
            "INVARIANT VIOLATED: watched file selected a workspace root that is not its prefix. This is a bug because the root was chosen with starts_with. Fix: keep root selection and strip_prefix adjacent.",
        );
        let kind = if change.typ == FileChangeType::CREATED {
            WatchedFileChangeKind::Created
        } else if change.typ == FileChangeType::CHANGED {
            WatchedFileChangeKind::Changed
        } else if change.typ == FileChangeType::DELETED {
            WatchedFileChangeKind::Deleted
        } else {
            warn!(
                "Ignoring extension watched-file event with unsupported change type for `{}`",
                change.uri
            );
            continue;
        };
        candidates.insert(WatchedFileChange {
            workspace_root: root.to_string_lossy().replace('\\', "/"),
            path: relative.to_string_lossy().replace('\\', "/"),
            uri: change.uri.to_string(),
            kind,
        });
    }
    candidates.into_iter().collect()
}

#[derive(Debug)]
struct ValidatedExtensionProcessRequest {
    request_id: String,
    program: PathBuf,
    arguments: Vec<String>,
    stdin: Option<String>,
    workspace_root: PathBuf,
    timeout: Duration,
}

fn validate_extension_process_request(
    extension_id: &str,
    workspace_trusted: bool,
    permissions: &[String],
    allowed_commands: &[String],
    workspace_roots: &[PathBuf],
    event_roots: &[PathBuf],
    request: &ProcessRequest,
) -> Result<ValidatedExtensionProcessRequest, ExtensionLoadError> {
    if !workspace_trusted {
        return Err(ExtensionLoadError::new(format!(
            "extension `{extension_id}` requested a process outside a trusted workspace"
        )));
    }
    if !permissions
        .iter()
        .any(|permission| permission == "process.exec")
    {
        return Err(ExtensionLoadError::new(format!(
            "extension `{extension_id}` requested a process without `process.exec` permission"
        )));
    }
    if !allowed_commands
        .iter()
        .any(|command| command == &request.command)
    {
        return Err(ExtensionLoadError::new(format!(
            "extension `{extension_id}` requested command `{}` which is not allowlisted by its manifest",
            request.command
        )));
    }
    if request.request_id.is_empty() || request.request_id.len() > 128 {
        return Err(ExtensionLoadError::new(format!(
            "extension `{extension_id}` process request id must contain 1..=128 bytes"
        )));
    }
    if request.arguments.len() > MAX_PROCESS_ARGUMENTS
        || request
            .arguments
            .iter()
            .any(|argument| argument.len() > MAX_PROCESS_ARGUMENT_BYTES)
    {
        return Err(ExtensionLoadError::new(format!(
            "extension `{extension_id}` process request exceeds argument count or size limits"
        )));
    }
    if request
        .stdin
        .as_ref()
        .is_some_and(|stdin| stdin.len() > MAX_PROCESS_STDIN_BYTES)
    {
        return Err(ExtensionLoadError::new(format!(
            "extension `{extension_id}` process stdin exceeds {MAX_PROCESS_STDIN_BYTES} bytes"
        )));
    }

    let mut roots = workspace_roots.to_vec();
    roots.sort();
    roots.dedup();
    let mut allowed_event_roots = event_roots.to_vec();
    allowed_event_roots.sort();
    allowed_event_roots.dedup();
    let workspace_root = match &request.workspace_root {
        Some(requested) => roots
            .iter()
            .find(|root| normalized_path(root) == *requested)
            .cloned()
            .filter(|root| allowed_event_roots.contains(root))
            .ok_or_else(|| {
                ExtensionLoadError::new(format!(
                    "extension `{extension_id}` requested unregistered or unrelated workspace root `{requested}`"
                ))
            })?,
        None if allowed_event_roots.len() == 1 => allowed_event_roots[0].clone(),
        None => {
            return Err(ExtensionLoadError::new(format!(
                "extension `{extension_id}` process request must select one workspace root when an event spans multiple roots"
            )))
        }
    };
    if !roots.contains(&workspace_root) {
        return Err(ExtensionLoadError::new(format!(
            "extension `{extension_id}` process request resolved outside registered workspace roots"
        )));
    }

    let command_path = Path::new(&request.command);
    let program = if command_path.components().count() == 1 {
        command_path.to_path_buf()
    } else {
        if command_path.is_absolute()
            || command_path.components().any(|component| {
                matches!(
                    component,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
        {
            return Err(ExtensionLoadError::new(format!(
                "extension `{extension_id}` process command `{}` must be a bare executable or workspace-relative path without traversal",
                request.command
            )));
        }
        workspace_root.join(command_path)
    };
    let requested_timeout = Duration::from_millis(
        request
            .timeout_ms
            .unwrap_or(DEFAULT_PROCESS_TIMEOUT.as_millis() as u64),
    );
    let timeout = requested_timeout.min(MAX_PROCESS_TIMEOUT);
    if timeout.is_zero() {
        return Err(ExtensionLoadError::new(format!(
            "extension `{extension_id}` process timeout must be greater than zero"
        )));
    }

    Ok(ValidatedExtensionProcessRequest {
        request_id: request.request_id.clone(),
        program,
        arguments: request.arguments.clone(),
        stdin: request.stdin.clone(),
        workspace_root,
        timeout,
    })
}

const MAX_RUNTIME_REINDEX_FILES: usize = 256;

fn validate_extension_reindex_files(
    extension_id: &str,
    workspace_roots: &[PathBuf],
    event_roots: &[PathBuf],
    requests: &[ruby_fast_lsp_extension_api::ReindexFile],
) -> Result<Vec<Url>, ExtensionLoadError> {
    if requests.len() > MAX_RUNTIME_REINDEX_FILES {
        return Err(ExtensionLoadError::new(format!(
            "extension `{extension_id}` requested {} runtime reindex files, exceeding the limit of {MAX_RUNTIME_REINDEX_FILES}",
            requests.len()
        )));
    }
    let roots = workspace_roots.iter().cloned().collect::<BTreeSet<_>>();
    let event_roots = event_roots.iter().cloned().collect::<BTreeSet<_>>();
    let mut uris = BTreeSet::new();
    for request in requests {
        let root = roots
            .iter()
            .find(|root| normalized_path(root) == request.workspace_root)
            .filter(|root| event_roots.contains(*root))
            .ok_or_else(|| {
                ExtensionLoadError::new(format!(
                    "extension `{extension_id}` requested runtime reindex outside event-related workspace root `{}`",
                    request.workspace_root
                ))
            })?;
        let relative = Path::new(&request.path);
        if relative.as_os_str().is_empty()
            || relative.is_absolute()
            || relative.components().any(|component| {
                matches!(
                    component,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
        {
            return Err(ExtensionLoadError::new(format!(
                "extension `{extension_id}` runtime reindex path `{}` must be workspace-relative without traversal",
                request.path
            )));
        }
        let canonical_root = fs::canonicalize(root).map_err(|err| {
            ExtensionLoadError::new(format!(
                "extension `{extension_id}` runtime reindex workspace root `{}` could not be canonicalized: {err}",
                request.workspace_root
            ))
        })?;
        let requested_path = root.join(relative);
        let canonical_path = fs::canonicalize(&requested_path).map_err(|err| {
            ExtensionLoadError::new(format!(
                "extension `{extension_id}` runtime reindex path `{}` is not an existing file: {err}",
                request.path
            ))
        })?;
        if !canonical_path.starts_with(&canonical_root) || !canonical_path.is_file() {
            return Err(ExtensionLoadError::new(format!(
                "extension `{extension_id}` runtime reindex path `{}` resolves outside its workspace root or is not a file",
                request.path
            )));
        }
        let uri = Url::from_file_path(canonical_path).map_err(|_| {
            ExtensionLoadError::new(format!(
                "extension `{extension_id}` runtime reindex path `{}` could not convert to a file URI",
                request.path
            ))
        })?;
        uris.insert(uri);
    }
    Ok(uris.into_iter().collect())
}

fn normalized_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

async fn run_extension_process(
    request: ValidatedExtensionProcessRequest,
    indexing_resources: IndexingResourceGovernor,
) -> ProcessResult {
    let spec = IndexingWorkSpec::new(
        Some(request.workspace_root.clone()),
        IndexingResourcePriority::Background,
        1,
        EXTENSION_PROCESS_TRANSIENT_MEMORY_BYTES,
        1,
    );
    indexing_resources
        .run_async_with_resources(
            "extension child process",
            spec,
            None,
            run_extension_process_admitted(request),
        )
        .await
        .expect(
            "INVARIANT VIOLATED: a non-cancellable extension process failed resource admission. \
             This is a bug because its fixed positive claim must fit the server-owned policy. \
             Fix: keep the extension process claim within the configured production budget.",
        )
}

async fn run_extension_process_admitted(
    request: ValidatedExtensionProcessRequest,
) -> ProcessResult {
    let mut command = ProcessCommand::new(&request.program);
    command
        .args(&request.arguments)
        .current_dir(&request.workspace_root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(err) => {
            return ProcessResult {
                request_id: request.request_id,
                status: ProcessResultStatus::Failed,
                exit_code: None,
                stdout: String::new(),
                stderr: format!(
                    "failed to start extension process `{}` in {}: {err}",
                    request.program.display(),
                    request.workspace_root.display()
                ),
                stdout_truncated: false,
                stderr_truncated: false,
            }
        }
    };
    let stdout = child.stdout.take().expect(
        "INVARIANT VIOLATED: extension process has no piped stdout. This is a bug because stdout is configured before spawning. Fix: keep stdout piped before taking the child handle.",
    );
    let stderr = child.stderr.take().expect(
        "INVARIANT VIOLATED: extension process has no piped stderr. This is a bug because stderr is configured before spawning. Fix: keep stderr piped before taking the child handle.",
    );
    let stdout_task = tokio::spawn(read_bounded_process_output(stdout));
    let stderr_task = tokio::spawn(read_bounded_process_output(stderr));
    let mut stdin = child.stdin.take().expect(
        "INVARIANT VIOLATED: extension process has no piped stdin. This is a bug because stdin is configured before spawning. Fix: keep stdin piped before taking the child handle.",
    );
    let stdin_content = request.stdin.unwrap_or_default();
    let stdin_task = tokio::spawn(async move {
        let result = stdin.write_all(stdin_content.as_bytes()).await;
        drop(stdin);
        result
    });

    let (status, exit_code, wait_error) =
        match tokio::time::timeout(request.timeout, child.wait()).await {
            Ok(Ok(status)) => (ProcessResultStatus::Exited, status.code(), None),
            Ok(Err(err)) => (ProcessResultStatus::Failed, None, Some(err.to_string())),
            Err(_) => {
                let kill_error = child.kill().await.err().map(|err| err.to_string());
                let _ = child.wait().await;
                (ProcessResultStatus::TimedOut, None, kill_error)
            }
        };
    let stdin_error = stdin_task
        .await
        .expect("INVARIANT VIOLATED: extension process stdin task panicked. This is a bug because the task only writes bounded bytes. Fix: keep panicking work out of the stdin task.")
        .err()
        .map(|err| err.to_string());
    let (stdout, stdout_truncated) = stdout_task.await.expect(
        "INVARIANT VIOLATED: extension process stdout task panicked. This is a bug because the task only drains bounded process output. Fix: keep panicking work out of the output task.",
    );
    let (mut stderr, stderr_truncated) = stderr_task.await.expect(
        "INVARIANT VIOLATED: extension process stderr task panicked. This is a bug because the task only drains bounded process output. Fix: keep panicking work out of the output task.",
    );
    for error in [wait_error, stdin_error].into_iter().flatten() {
        if !stderr.is_empty() {
            stderr.push(b'\n');
        }
        stderr.extend_from_slice(error.as_bytes());
    }

    ProcessResult {
        request_id: request.request_id,
        status,
        exit_code,
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
        stdout_truncated,
        stderr_truncated,
    }
}

async fn read_bounded_process_output(mut reader: impl AsyncRead + Unpin) -> (Vec<u8>, bool) {
    let mut retained = Vec::new();
    let mut buffer = [0_u8; 8192];
    let mut truncated = false;
    loop {
        let read = match reader.read(&mut buffer).await {
            Ok(0) => break,
            Ok(read) => read,
            Err(err) => {
                let message = format!("failed to read extension process output: {err}");
                let remaining = MAX_PROCESS_OUTPUT_BYTES.saturating_sub(retained.len());
                retained.extend_from_slice(&message.as_bytes()[..message.len().min(remaining)]);
                truncated |= message.len() > remaining;
                break;
            }
        };
        let remaining = MAX_PROCESS_OUTPUT_BYTES.saturating_sub(retained.len());
        let keep = read.min(remaining);
        retained.extend_from_slice(&buffer[..keep]);
        truncated |= keep < read;
    }
    (retained, truncated)
}

fn process_wasm_call_node(
    registry: &ExtensionRegistryHandle,
    visitor: &mut FactCollector,
    node: &CallNode,
    applicability: Option<&ExtensionApplicabilitySnapshot>,
) -> bool {
    let method_name = utils::utf8_str(node.name().as_slice());
    let project = visitor.extension_project_context.as_ref();
    let extensions = registry.extensions_with_applicability(project, applicability);
    let mut emitted = Vec::new();
    let mut emitted_contexts = Vec::new();
    let mut emitters = BTreeMap::new();
    let active_frame_ids = visitor
        .extension_call_stack
        .iter()
        .flat_map(|call| call.frame_extension_ids.iter())
        .collect::<BTreeSet<_>>();
    let explicitly_switches_receiver = node
        .receiver()
        .is_some_and(|receiver| receiver.as_self_node().is_none());
    let mut shared_call_context = None;
    let mut shared_project_call_context = None;

    for (loaded, applies_to_source) in extensions {
        if !loaded.is_loaded() {
            continue;
        }
        if !loaded.handles_call(method_name) {
            continue;
        }
        if !applies_to_source {
            continue;
        }
        let owns_active_frame = active_frame_ids.contains(&loaded.metadata.id);
        if !active_frame_ids.is_empty() && !owns_active_frame && !explicitly_switches_receiver {
            continue;
        }
        if loaded.has_semantic_targets()
            && !loaded.semantically_matches_call(visitor, node)
            && !loaded.can_run_inside_extension_frame(visitor, node)
        {
            continue;
        }
        let compact_ctx =
            shared_call_context.get_or_insert_with(|| call_context(visitor, node, false));
        let ctx = match (loaded.project_context_delivery, project) {
            (ExtensionProjectContextDelivery::PerCall, Some(project)) => {
                shared_project_call_context.get_or_insert_with(|| {
                    let mut complete = compact_ctx.clone();
                    complete.project = Some(project.clone());
                    complete
                })
            }
            (
                ExtensionProjectContextDelivery::Activation
                | ExtensionProjectContextDelivery::PerCall,
                None,
            )
            | (ExtensionProjectContextDelivery::Activation, Some(_)) => compact_ctx,
        };
        let output = match loaded.index_call_output_for_project(project, ctx) {
            Ok(output) => output,
            Err(err) => {
                warn!(
                    "Disabling Ruby Fast LSP extension `{}` after indexing failure on `{}`: {}",
                    loaded.metadata.id, method_name, err
                );
                let reason = err.to_string();
                loaded.fail(reason);
                continue;
            }
        };
        if output.index_patches.is_empty() && output.execution_contexts.is_empty() {
            continue;
        }
        if let Err(spoofed_id) =
            validate_index_patch_provenance(&loaded.metadata.id, &output.index_patches)
        {
            loaded.reject(format!(
                "extension `{}` emitted an index patch attributed to `{spoofed_id}`; patch provenance must match the loaded manifest id",
                loaded.metadata.id
            ));
            continue;
        }
        if let Err(err) = validate_index_patch_payloads(&output.index_patches) {
            loaded.reject(format!(
                "extension `{}` emitted an invalid index patch: {err}",
                loaded.metadata.id
            ));
            continue;
        }
        if project.is_none()
            && output
                .index_patches
                .iter()
                .any(index_patch_requires_project_context)
        {
            loaded.reject(format!(
                "extension `{}` emitted a project-generated owner without an owning ProjectContext",
                loaded.metadata.id
            ));
            continue;
        }
        if let Err(err) = validate_execution_contexts_for_project(
            &loaded.metadata.id,
            ctx,
            project.is_some(),
            &output.execution_contexts,
        ) {
            loaded.reject(format!(
                "extension `{}` emitted an invalid block execution context: {err}",
                loaded.metadata.id
            ));
            continue;
        }
        emitters.insert(loaded.metadata.id.clone(), Arc::clone(&loaded));
        emitted.extend(output.index_patches);
        emitted_contexts.extend(output.execution_contexts);
    }

    if emitted.is_empty() && emitted_contexts.is_empty() {
        return false;
    }
    let mut pending = emitted;
    let mut pending_contexts = emitted_contexts;
    let (patches, contexts) = loop {
        let conflict = match resolve_index_patch_conflicts(pending.clone()) {
            Ok(patches) => match resolve_execution_context_conflicts(pending_contexts.clone()) {
                Ok(contexts) => break (patches, contexts),
                Err(conflict) => conflict,
            },
            Err(conflict) => conflict,
        };
        let rejected_ids = conflict
            .extension_ids
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        for extension_id in &conflict.extension_ids {
            let loaded = emitters.get(extension_id).expect(
                        "INVARIANT VIOLATED: conflicting patch source has no emitting extension. This is a bug because provenance is validated before conflict resolution. Fix: keep emitter registration adjacent to accepted patch collection.",
                    );
            loaded.reject_conflict(conflict.message.clone());
        }
        warn!(
            "Rejecting conflicting extension index patches: {}",
            conflict.message
        );
        pending.retain(|patch| !rejected_ids.contains(index_patch_extension_id(patch)));
        pending_contexts
            .retain(|context| !rejected_ids.contains(context.source.extension_id.as_str()));
        if pending.is_empty() && pending_contexts.is_empty() {
            return false;
        }
    };
    for patch in patches {
        apply_patch(visitor, node, patch);
    }
    for context in contexts {
        apply_execution_context(visitor, context);
    }
    true
}

fn apply_execution_context(visitor: &mut FactCollector, context: BlockExecutionContextPatch) {
    let source_identity = visitor.document.uri.as_str();
    let project_identity = visitor
        .extension_project_context
        .as_ref()
        .map(|project| project.project_uri.as_str());
    let range = visitor
        .document
        .lsp_range_to_text_range(range_from_abi(context.block_range));
    let mut owners = BTreeMap::new();

    for owner in &context.generated_owners {
        let identity = generated_owner_scope_identity(
            owner.scope,
            source_identity,
            project_identity,
            "execution-context owner",
        );
        let generated = GeneratedOwnerId::new(
            &context.source.extension_id,
            identity,
            &owner.local_id,
        )
        .expect(
            "INVARIANT VIOLATED: invalid generated owner reached extension context application. This is a bug because execution contexts must be validated before fact conversion. Fix: keep validation before apply_execution_context.",
        );
        let namespace = vec![RubyConstant::generated_owner(generated)];
        let previous = owners.insert(
            (owner.scope, owner.local_id.clone()),
            (namespace, namespace_kind_from_abi(owner.owner_kind)),
        );
        assert!(
            previous.is_none(),
            "INVARIANT VIOLATED: duplicate generated owner reached extension context application. This is a bug because duplicate local identities must be rejected at the extension boundary. Fix: keep owner uniqueness validation before fact conversion."
        );
    }

    for owner in &context.generated_owners {
        let (namespace, owner_kind) = owners.get(&(owner.scope, owner.local_id.clone())).expect(
            "INVARIANT VIOLATED: validated generated owner is absent during context application. This is a bug because the owner map is built from the same context. Fix: keep context conversion atomic.",
        );
        let instance_fqn = FullyQualifiedName::namespace(namespace.clone());
        let graph_kind = match owner.declaration_kind {
            ruby_fast_lsp_extension_api::NamespaceDeclarationKind::Class => GraphNodeKind::Class,
            ruby_fast_lsp_extension_api::NamespaceDeclarationKind::Module => GraphNodeKind::Module,
        };
        let singleton_fqn = instance_fqn.to_singleton_namespace().expect(
            "INVARIANT VIOLATED: generated owner could not convert to a singleton namespace. This is a bug because generated owners are namespace segments. Fix: construct generated owner graph nodes through FullyQualifiedName::namespace.",
        );
        for node in [
            GraphNodeFact::new(instance_fqn.clone(), graph_kind, range),
            GraphNodeFact::new(singleton_fqn, graph_kind, range),
        ] {
            if !visitor.direct_facts.graph_nodes.contains(&node) {
                visitor.direct_facts.graph_nodes.push(node);
            }
        }
        if let Some(parent) = &owner.parent {
            let (parent_namespace, parent_kind) = resolve_execution_context_target(parent, &owners);
            let source = FullyQualifiedName::namespace_with_kind(namespace.clone(), *owner_kind);
            let target = FullyQualifiedName::namespace_with_kind(parent_namespace, parent_kind);
            visitor.direct_push_resolved_edge(source, target, GraphEdgeKind::Superclass, range);
        }
    }

    let (implicit_receiver, implicit_receiver_kind) =
        resolve_execution_context_target(&context.implicit_receiver, &owners);
    let (method_definition_owner, method_definition_kind) =
        resolve_execution_context_target(&context.method_definition_owner, &owners);
    let implicit_receiver_fqn =
        FullyQualifiedName::namespace_with_kind(implicit_receiver.clone(), implicit_receiver_kind);
    let method_definition_owner_fqn = FullyQualifiedName::namespace_with_kind(
        method_definition_owner.clone(),
        method_definition_kind,
    );
    visitor
        .extension_execution_context_facts
        .push(ExecutionContextFact {
            range,
            lexical_namespace: FullyQualifiedName::namespace(visitor.scope_tracker.get_ns_stack()),
            implicit_receiver: implicit_receiver_fqn,
            method_definition_owner: method_definition_owner_fqn,
            lexical_scope: ExecutionScopeMode::Preserve,
            local_scope: ExecutionScopeMode::Preserve,
            extension_id: context.source.extension_id,
        });
    visitor.set_pending_block_execution_context(BlockExecutionContext {
        block_range: range,
        implicit_receiver,
        implicit_receiver_kind,
        method_definition_owner,
        method_definition_kind,
    });
}

fn resolve_execution_context_target(
    target: &ExecutionContextTarget,
    owners: &BTreeMap<(GeneratedOwnerScope, String), (Vec<RubyConstant>, NamespaceKind)>,
) -> (Vec<RubyConstant>, NamespaceKind) {
    match target {
        ExecutionContextTarget::Namespace {
            namespace,
            owner_kind,
        } => (
            extension_ruby_constants(namespace, "execution context namespace target"),
            namespace_kind_from_abi(*owner_kind),
        ),
        ExecutionContextTarget::GeneratedOwner {
            local_id,
            owner_kind,
        } => {
            let (namespace, declared_kind) = owners
                .get(&(GeneratedOwnerScope::Source, local_id.clone()))
                .cloned()
                .expect(
                "INVARIANT VIOLATED: undeclared generated target reached context application. This is a bug because every context target must be validated before conversion. Fix: reject undeclared local IDs at the extension boundary.",
            );
            (
                namespace,
                owner_kind
                    .map(namespace_kind_from_abi)
                    .unwrap_or(declared_kind),
            )
        }
        ExecutionContextTarget::ProjectGeneratedOwner {
            local_id,
            owner_kind,
        } => {
            let (namespace, declared_kind) = owners
                .get(&(GeneratedOwnerScope::Project, local_id.clone()))
                .cloned()
                .expect(
                    "INVARIANT VIOLATED: undeclared project-generated target reached context application. This is a bug because every context target must be validated before conversion. Fix: declare the project-scoped owner in the same execution context.",
                );
            (
                namespace,
                owner_kind
                    .map(namespace_kind_from_abi)
                    .unwrap_or(declared_kind),
            )
        }
    }
}

fn generated_owner_scope_identity<'a>(
    scope: GeneratedOwnerScope,
    source_identity: &'a str,
    project_identity: Option<&'a str>,
    label: &str,
) -> &'a str {
    match scope {
        GeneratedOwnerScope::Source => source_identity,
        GeneratedOwnerScope::Project => project_identity.unwrap_or_else(|| {
            panic!(
                "INVARIANT VIOLATED: {label} requested project-scoped identity without an owning project. This is a host validation bug because project-generated owners require ProjectContext. Fix: reject the patch before semantic application."
            )
        }),
    }
}

fn extension_ruby_constants(parts: &[String], label: &str) -> Vec<RubyConstant> {
    parts
        .iter()
        .map(|part| {
            RubyConstant::new(part).unwrap_or_else(|err| {
                panic!(
                    "INVARIANT VIOLATED: validated {label} component `{part}` failed fact conversion: {err}. This is a bug because validation and conversion use the same RubyConstant contract. Fix: keep extension context validation before application."
                )
            })
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum IndexPatchIdentity {
    Declaration {
        path: Vec<String>,
    },
    Reference {
        start_line: u32,
        start_character: u32,
        end_line: u32,
        end_character: u32,
    },
    Method {
        namespace: Vec<String>,
        owner_kind: String,
        name: String,
    },
    Superclass {
        namespace: Vec<String>,
    },
    Mixin {
        namespace: Vec<String>,
        target_kind: String,
        mixin: Vec<String>,
        kind: String,
    },
    ExecutionContextConnection {
        template: Vec<String>,
        application: Vec<String>,
        start_line: u32,
        start_character: u32,
        end_line: u32,
        end_character: u32,
    },
}

impl IndexPatchIdentity {
    fn display(&self) -> String {
        match self {
            Self::Declaration { path } => path.join("::"),
            Self::Reference {
                start_line,
                start_character,
                end_line,
                end_character,
            } => format!("reference at {start_line}:{start_character}-{end_line}:{end_character}"),
            Self::Method {
                namespace,
                owner_kind,
                name,
            } => {
                let separator = if owner_kind == "singleton" { "." } else { "#" };
                format!("{}{separator}{name}", namespace.join("::"))
            }
            Self::Superclass { namespace } => {
                format!("{} superclass", namespace.join("::"))
            }
            Self::Mixin {
                namespace,
                target_kind,
                mixin,
                kind,
            } => format!(
                "{} ({target_kind}) {kind} {}",
                namespace.join("::"),
                mixin.join("::")
            ),
            Self::ExecutionContextConnection {
                template,
                application,
                start_line,
                start_character,
                end_line,
                end_character,
            } => format!(
                "execution context {} -> {} at {start_line}:{start_character}-{end_line}:{end_character}",
                template.join("::"),
                application.join("::")
            ),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct IndexPatchConflict {
    extension_ids: Vec<String>,
    message: String,
}

fn resolve_index_patch_conflicts(
    mut patches: Vec<IndexPatch>,
) -> Result<Vec<IndexPatch>, IndexPatchConflict> {
    patches.sort_by(|left, right| {
        index_patch_identity(left)
            .cmp(&index_patch_identity(right))
            .then_with(|| index_patch_extension_id(left).cmp(index_patch_extension_id(right)))
    });
    let mut resolved = Vec::new();
    let mut index = 0;
    while index < patches.len() {
        let identity = index_patch_identity(&patches[index]);
        let mut end = index + 1;
        while end < patches.len() && index_patch_identity(&patches[end]) == identity {
            end += 1;
        }
        let group = &patches[index..end];
        if group
            .iter()
            .skip(1)
            .any(|patch| !index_patch_payload_eq(&group[0], patch))
        {
            let extension_ids = group
                .iter()
                .map(index_patch_extension_id)
                .map(str::to_string)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            return Err(IndexPatchConflict {
                message: format!(
                    "extensions {} emitted incompatible index patches for `{}`; conflicting semantic facts are rejected deterministically",
                    extension_ids.join(", "),
                    identity.display()
                ),
                extension_ids,
            });
        }
        resolved.push(group[0].clone());
        index = end;
    }
    Ok(resolved)
}

fn resolve_execution_context_conflicts(
    mut contexts: Vec<BlockExecutionContextPatch>,
) -> Result<Vec<BlockExecutionContextPatch>, IndexPatchConflict> {
    contexts.sort_by(|left, right| {
        execution_context_range_key(left)
            .cmp(&execution_context_range_key(right))
            .then_with(|| left.source.extension_id.cmp(&right.source.extension_id))
    });
    let mut resolved = Vec::new();
    let mut index = 0;
    while index < contexts.len() {
        let identity = execution_context_range_key(&contexts[index]);
        let mut end = index + 1;
        while end < contexts.len() && execution_context_range_key(&contexts[end]) == identity {
            end += 1;
        }
        let group = &contexts[index..end];
        if group
            .iter()
            .skip(1)
            .any(|context| !execution_context_payload_eq(&group[0], context))
        {
            let extension_ids = group
                .iter()
                .map(|context| context.source.extension_id.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            return Err(IndexPatchConflict {
                message: format!(
                    "extensions {} emitted incompatible block execution contexts for call at {}:{}-{}:{}; conflicting runtime ownership is rejected deterministically",
                    extension_ids.join(", "),
                    identity.0,
                    identity.1,
                    identity.2,
                    identity.3
                ),
                extension_ids,
            });
        }
        resolved.push(group[0].clone());
        index = end;
    }
    Ok(resolved)
}

fn execution_context_range_key(context: &BlockExecutionContextPatch) -> (u32, u32, u32, u32) {
    (
        context.call_range.start.line,
        context.call_range.start.character,
        context.call_range.end.line,
        context.call_range.end.character,
    )
}

fn execution_context_payload_eq(
    left: &BlockExecutionContextPatch,
    right: &BlockExecutionContextPatch,
) -> bool {
    left.call_range == right.call_range
        && left.block_range == right.block_range
        && left.generated_owners == right.generated_owners
        && left.implicit_receiver == right.implicit_receiver
        && left.method_definition_owner == right.method_definition_owner
        && left.lexical_scope == right.lexical_scope
        && left.local_scope == right.local_scope
}

fn index_patch_identity(patch: &IndexPatch) -> IndexPatchIdentity {
    match patch {
        IndexPatch::DefineNamespace(namespace) => IndexPatchIdentity::Declaration {
            path: namespace.namespace.clone(),
        },
        IndexPatch::DefineConstant(constant) => {
            let mut path = constant.namespace.clone();
            path.push(constant.name.clone());
            IndexPatchIdentity::Declaration { path }
        }
        IndexPatch::AddReference(reference) => IndexPatchIdentity::Reference {
            start_line: reference.location.start.line,
            start_character: reference.location.start.character,
            end_line: reference.location.end.line,
            end_character: reference.location.end.character,
        },
        IndexPatch::DefineMethod(method) => IndexPatchIdentity::Method {
            namespace: patch_owner_identity(&method.namespace, method.owner_target.as_ref()),
            owner_kind: namespace_kind_name(method.owner_kind).to_string(),
            name: method.name.clone(),
        },
        IndexPatch::SetSuperclass(superclass) => IndexPatchIdentity::Superclass {
            namespace: superclass.namespace.clone(),
        },
        IndexPatch::ApplyMixin(mixin) => IndexPatchIdentity::Mixin {
            namespace: patch_owner_identity(&mixin.namespace, mixin.owner_target.as_ref()),
            target_kind: namespace_kind_name(mixin.target_kind).to_string(),
            mixin: mixin
                .mixin_target
                .as_ref()
                .map(|target| patch_owner_identity(&[], Some(target)))
                .unwrap_or_else(|| mixin.mixin.clone()),
            kind: match mixin.kind {
                ruby_fast_lsp_extension_api::MixinKind::Include => "include",
                ruby_fast_lsp_extension_api::MixinKind::Prepend => "prepend",
                ruby_fast_lsp_extension_api::MixinKind::Extend => "extend",
            }
            .to_string(),
        },
        IndexPatch::ConnectExecutionContext(connection) => {
            IndexPatchIdentity::ExecutionContextConnection {
                template: patch_owner_identity(&[], Some(&connection.template)),
                application: patch_owner_identity(&[], Some(&connection.application)),
                start_line: connection.location.start.line,
                start_character: connection.location.start.character,
                end_line: connection.location.end.line,
                end_character: connection.location.end.character,
            }
        }
    }
}

fn patch_owner_identity(
    namespace: &[String],
    target: Option<&ExecutionContextTarget>,
) -> Vec<String> {
    match target {
        None => namespace.to_vec(),
        Some(ExecutionContextTarget::Namespace {
            namespace,
            owner_kind,
        }) => {
            let mut identity = vec![format!("@namespace:{}", namespace_kind_name(*owner_kind))];
            identity.extend(namespace.iter().cloned());
            identity
        }
        Some(ExecutionContextTarget::GeneratedOwner {
            local_id,
            owner_kind,
        }) => {
            vec![format!(
                "@generated:{local_id}:{}",
                owner_kind.map(namespace_kind_name).unwrap_or("fallback")
            )]
        }
        Some(ExecutionContextTarget::ProjectGeneratedOwner {
            local_id,
            owner_kind,
        }) => {
            vec![format!(
                "@project-generated:{local_id}:{}",
                owner_kind.map(namespace_kind_name).unwrap_or("fallback")
            )]
        }
    }
}

fn namespace_kind_name(kind: AbiNamespaceKind) -> &'static str {
    match kind {
        AbiNamespaceKind::Instance => "instance",
        AbiNamespaceKind::Singleton => "singleton",
    }
}

fn namespace_kind_from_abi(kind: AbiNamespaceKind) -> NamespaceKind {
    match kind {
        AbiNamespaceKind::Instance => NamespaceKind::Instance,
        AbiNamespaceKind::Singleton => NamespaceKind::Singleton,
    }
}

fn index_patch_extension_id(patch: &IndexPatch) -> &str {
    match patch {
        IndexPatch::DefineNamespace(namespace) => &namespace.source.extension_id,
        IndexPatch::DefineConstant(constant) => &constant.source.extension_id,
        IndexPatch::AddReference(reference) => &reference.source.extension_id,
        IndexPatch::DefineMethod(method) => &method.source.extension_id,
        IndexPatch::SetSuperclass(superclass) => &superclass.source.extension_id,
        IndexPatch::ApplyMixin(mixin) => &mixin.source.extension_id,
        IndexPatch::ConnectExecutionContext(connection) => &connection.source.extension_id,
    }
}

fn validate_index_patch_provenance(
    expected_extension_id: &str,
    patches: &[IndexPatch],
) -> Result<(), String> {
    if let Some(spoofed_id) = patches.iter().find_map(|patch| {
        let source_id = index_patch_extension_id(patch);
        (source_id != expected_extension_id).then(|| source_id.to_string())
    }) {
        return Err(spoofed_id);
    }
    Ok(())
}

fn response_patch_extension_id(patch: &ResponsePatch) -> &str {
    match patch {
        ResponsePatch::Diagnostic(diagnostic) => &diagnostic.source.extension_id,
        ResponsePatch::CodeLens(lens) => &lens.source.extension_id,
        ResponsePatch::DocumentSymbol(symbol) => &symbol.source.extension_id,
    }
}

fn validate_response_patch_provenance(
    expected_extension_id: &str,
    patches: &[ResponsePatch],
) -> Result<(), String> {
    if let Some(spoofed_id) = patches.iter().find_map(|patch| {
        let source_id = response_patch_extension_id(patch);
        (source_id != expected_extension_id).then(|| source_id.to_string())
    }) {
        return Err(spoofed_id);
    }
    Ok(())
}

fn validate_index_patch_payloads(patches: &[IndexPatch]) -> Result<(), String> {
    for patch in patches {
        match patch {
            IndexPatch::DefineNamespace(namespace) => {
                if namespace.namespace.is_empty() {
                    return Err("namespace declaration must not be empty".to_string());
                }
                validate_extension_namespace(&namespace.namespace, "namespace declaration")?;
                validate_source_range(namespace.location, "namespace location")?;
            }
            IndexPatch::DefineConstant(constant) => {
                validate_extension_namespace(&constant.namespace, "constant namespace")?;
                RubyConstant::new(&constant.name)
                    .map_err(|err| format!("invalid constant name `{}`: {err}", constant.name))?;
                validate_source_range(constant.location, "constant location")?;
                analysis_ruby_type_from_extension(constant.ruby_type.as_ref())?;
            }
            IndexPatch::AddReference(reference) => {
                validate_reference_target(&reference.target)?;
                validate_source_range(reference.location, "reference location")?;
            }
            IndexPatch::DefineMethod(method) => {
                RubyMethod::new(&method.name)
                    .map_err(|err| format!("invalid method name `{}`: {err}", method.name))?;
                validate_extension_namespace(&method.namespace, "method namespace")?;
                if let Some(target) = &method.owner_target {
                    validate_patch_owner_target(
                        target,
                        &method.source.extension_id,
                        "method owner",
                    )?;
                }
                validate_source_range(method.location, "method location")?;
                if method.params.iter().any(|param| param.name.is_empty()) {
                    return Err("method parameter names must not be empty".to_string());
                }
                if method.return_type.is_some() && method.return_type_source.is_some() {
                    return Err(
                        "method patch must use either `return_type` or `return_type_source`, not both"
                            .to_string(),
                    );
                }
                analysis_ruby_type_from_extension(method.return_type.as_ref())?;
            }
            IndexPatch::SetSuperclass(superclass) => {
                if superclass.namespace.is_empty() {
                    return Err("superclass namespace must not be empty".to_string());
                }
                if superclass.superclass.is_empty() {
                    return Err("superclass target must not be empty".to_string());
                }
                validate_extension_namespace(&superclass.namespace, "superclass namespace")?;
                validate_extension_namespace(&superclass.superclass, "superclass target")?;
                validate_source_range(superclass.location, "superclass location")?;
            }
            IndexPatch::ApplyMixin(mixin) => {
                validate_extension_namespace(&mixin.namespace, "mixin namespace")?;
                if let Some(target) = &mixin.owner_target {
                    validate_patch_owner_target(target, &mixin.source.extension_id, "mixin owner")?;
                }
                match &mixin.mixin_target {
                    Some(target) => {
                        if !mixin.mixin.is_empty() {
                            return Err(
                                "mixin patch must use either `mixin` or `mixin_target`, not both"
                                    .to_string(),
                            );
                        }
                        validate_patch_owner_target(
                            target,
                            &mixin.source.extension_id,
                            "semantic mixin target",
                        )?;
                    }
                    None => {
                        if mixin.mixin.is_empty() {
                            return Err(
                                "mixin patch must provide `mixin` or `mixin_target`".to_string()
                            );
                        }
                        validate_extension_namespace(&mixin.mixin, "mixin target")?;
                    }
                }
                validate_source_range(mixin.location, "mixin location")?;
            }
            IndexPatch::ConnectExecutionContext(connection) => {
                validate_patch_owner_target(
                    &connection.template,
                    &connection.source.extension_id,
                    "execution context template",
                )?;
                validate_patch_owner_target(
                    &connection.application,
                    &connection.source.extension_id,
                    "execution context application",
                )?;
                validate_source_range(
                    connection.location,
                    "execution context application location",
                )?;
            }
        }
    }
    for superclass in patches.iter().filter_map(|patch| match patch {
        IndexPatch::SetSuperclass(superclass) => Some(superclass),
        IndexPatch::DefineNamespace(_)
        | IndexPatch::DefineConstant(_)
        | IndexPatch::AddReference(_)
        | IndexPatch::DefineMethod(_)
        | IndexPatch::ApplyMixin(_)
        | IndexPatch::ConnectExecutionContext(_) => None,
    }) {
        let declares_class = patches.iter().any(|patch| match patch {
            IndexPatch::DefineNamespace(namespace) => {
                namespace.namespace == superclass.namespace
                    && namespace.kind
                        == ruby_fast_lsp_extension_api::NamespaceDeclarationKind::Class
                    && namespace.source.extension_id == superclass.source.extension_id
            }
            IndexPatch::DefineConstant(_)
            | IndexPatch::AddReference(_)
            | IndexPatch::DefineMethod(_)
            | IndexPatch::SetSuperclass(_)
            | IndexPatch::ApplyMixin(_)
            | IndexPatch::ConnectExecutionContext(_) => false,
        });
        if !declares_class {
            return Err(format!(
                "superclass patch for `{}` requires a matching generated class declaration from the same extension output",
                superclass.namespace.join("::")
            ));
        }
    }
    Ok(())
}

fn execution_target_requires_project(target: &ExecutionContextTarget) -> bool {
    matches!(target, ExecutionContextTarget::ProjectGeneratedOwner { .. })
}

fn index_patch_requires_project_context(patch: &IndexPatch) -> bool {
    match patch {
        IndexPatch::DefineMethod(method) => method
            .owner_target
            .as_ref()
            .is_some_and(execution_target_requires_project),
        IndexPatch::ApplyMixin(mixin) => {
            mixin
                .owner_target
                .as_ref()
                .is_some_and(execution_target_requires_project)
                || mixin
                    .mixin_target
                    .as_ref()
                    .is_some_and(execution_target_requires_project)
        }
        IndexPatch::ConnectExecutionContext(connection) => {
            execution_target_requires_project(&connection.template)
                || execution_target_requires_project(&connection.application)
        }
        IndexPatch::DefineNamespace(_)
        | IndexPatch::DefineConstant(_)
        | IndexPatch::AddReference(_)
        | IndexPatch::SetSuperclass(_) => false,
    }
}

fn validate_patch_owner_target(
    target: &ExecutionContextTarget,
    extension_id: &str,
    label: &str,
) -> Result<(), String> {
    match target {
        ExecutionContextTarget::Namespace { namespace, .. } => {
            if namespace.is_empty() {
                return Err(format!("{label} namespace must not be empty"));
            }
            validate_extension_namespace(namespace, label)
        }
        ExecutionContextTarget::GeneratedOwner { local_id, .. } => {
            GeneratedOwnerId::new(extension_id, "validation-source", local_id)
                .map(|_| ())
                .map_err(|err| format!("invalid {label} generated owner `{local_id}`: {err}"))
        }
        ExecutionContextTarget::ProjectGeneratedOwner { local_id, .. } => {
            GeneratedOwnerId::new(extension_id, "validation-project", local_id)
                .map(|_| ())
                .map_err(|err| {
                    format!("invalid {label} project-generated owner `{local_id}`: {err}")
                })
        }
    }
}

fn validate_execution_contexts(
    expected_extension_id: &str,
    call: &CallContext,
    contexts: &[BlockExecutionContextPatch],
) -> Result<(), String> {
    validate_execution_contexts_for_project(
        expected_extension_id,
        call,
        call.project.is_some(),
        contexts,
    )
}

fn validate_execution_contexts_for_project(
    expected_extension_id: &str,
    call: &CallContext,
    project_present: bool,
    contexts: &[BlockExecutionContextPatch],
) -> Result<(), String> {
    if contexts.len() > 1 {
        return Err("an extension may emit at most one execution context for one call".to_string());
    }
    for context in contexts {
        if context.source.extension_id != expected_extension_id {
            return Err(format!(
                "context provenance `{}` does not match loaded manifest id `{expected_extension_id}`",
                context.source.extension_id
            ));
        }
        if context.call_range != call.call_range {
            return Err("context call_range must exactly match the current call".to_string());
        }
        let expected_block = call.block_range.ok_or_else(|| {
            "execution context requires the current call to have a block".to_string()
        })?;
        if context.block_range != expected_block {
            return Err("context block_range must exactly match the current block".to_string());
        }
        validate_source_range(context.call_range, "execution context call range")?;
        validate_source_range(context.block_range, "execution context block range")?;
        let mut declared = BTreeSet::new();
        for owner in &context.generated_owners {
            if owner.scope == GeneratedOwnerScope::Project && !project_present {
                return Err(format!(
                    "project-generated owner `{}` requires an owning ProjectContext",
                    owner.local_id
                ));
            }
            GeneratedOwnerId::new(expected_extension_id, "validation-source", &owner.local_id)
                .map_err(|err| format!("invalid generated owner `{}`: {err}", owner.local_id))?;
            if !declared.insert((owner.scope, owner.local_id.clone())) {
                return Err(format!(
                    "{:?} generated owner `{}` is declared more than once",
                    owner.scope, owner.local_id
                ));
            }
        }
        for owner in &context.generated_owners {
            if let Some(parent) = &owner.parent {
                validate_execution_context_target(parent, &declared, "generated owner parent")?;
            }
        }
        validate_execution_context_target(
            &context.implicit_receiver,
            &declared,
            "implicit receiver",
        )?;
        validate_execution_context_target(
            &context.method_definition_owner,
            &declared,
            "method-definition owner",
        )?;
    }
    Ok(())
}

fn validate_execution_context_target(
    target: &ExecutionContextTarget,
    declared: &BTreeSet<(GeneratedOwnerScope, String)>,
    label: &str,
) -> Result<(), String> {
    match target {
        ExecutionContextTarget::Namespace { namespace, .. } => {
            if namespace.is_empty() {
                return Err(format!("{label} namespace must not be empty"));
            }
            validate_extension_namespace(namespace, label)
        }
        ExecutionContextTarget::GeneratedOwner { local_id, .. } => {
            if !declared.contains(&(GeneratedOwnerScope::Source, local_id.clone())) {
                return Err(format!(
                    "{label} references undeclared generated owner `{local_id}`"
                ));
            }
            Ok(())
        }
        ExecutionContextTarget::ProjectGeneratedOwner { local_id, .. } => {
            if !declared.contains(&(GeneratedOwnerScope::Project, local_id.clone())) {
                return Err(format!(
                    "{label} references undeclared project-generated owner `{local_id}`"
                ));
            }
            Ok(())
        }
    }
}

fn validate_extension_namespace(parts: &[String], label: &str) -> Result<(), String> {
    for part in parts {
        RubyConstant::new(part)
            .map_err(|err| format!("invalid {label} component `{part}`: {err}"))?;
    }
    Ok(())
}

fn validate_reference_target(
    target: &ruby_fast_lsp_extension_api::ReferenceTarget,
) -> Result<(), String> {
    match target {
        ruby_fast_lsp_extension_api::ReferenceTarget::Namespace(namespace) => {
            if namespace.is_empty() {
                return Err("reference namespace target must not be empty".to_string());
            }
            validate_extension_namespace(namespace, "reference namespace target")
        }
        ruby_fast_lsp_extension_api::ReferenceTarget::Constant { namespace, name } => {
            validate_extension_namespace(namespace, "reference constant namespace")?;
            RubyConstant::new(name)
                .map(|_| ())
                .map_err(|err| format!("invalid reference constant name `{name}`: {err}"))
        }
        ruby_fast_lsp_extension_api::ReferenceTarget::Method {
            namespace, name, ..
        } => {
            if namespace.is_empty() {
                return Err("reference method namespace must not be empty".to_string());
            }
            validate_extension_namespace(namespace, "reference method namespace")?;
            RubyMethod::new(name)
                .map(|_| ())
                .map_err(|err| format!("invalid reference method name `{name}`: {err}"))
        }
    }
}

fn validate_source_range(range: SourceRange, label: &str) -> Result<(), String> {
    let start = (range.start.line, range.start.character);
    let end = (range.end.line, range.end.character);
    if start > end {
        return Err(format!(
            "{label} start {}:{} is after end {}:{}",
            range.start.line, range.start.character, range.end.line, range.end.character
        ));
    }
    Ok(())
}

pub(crate) fn analysis_ruby_type_from_extension(
    ruby_type: Option<&ruby_fast_lsp_extension_api::RubyType>,
) -> Result<Option<AnalysisRubyType>, String> {
    let Some(ruby_type) = ruby_type else {
        return Ok(None);
    };
    if ruby_type == &ruby_fast_lsp_extension_api::RubyType::Unknown {
        return Ok(None);
    }
    let mut node_count = 0;
    let converted = analysis_ruby_type_from_extension_inner(ruby_type, 0, &mut node_count)?;
    if converted == AnalysisRubyType::Unknown {
        Ok(None)
    } else {
        Ok(Some(converted))
    }
}

fn analysis_ruby_type_from_extension_inner(
    ruby_type: &ruby_fast_lsp_extension_api::RubyType,
    depth: usize,
    node_count: &mut usize,
) -> Result<AnalysisRubyType, String> {
    const MAX_TYPE_DEPTH: usize = 8;
    const MAX_TYPE_NODES: usize = 64;
    if depth > MAX_TYPE_DEPTH {
        return Err(format!(
            "extension Ruby type nesting exceeds maximum depth {MAX_TYPE_DEPTH}"
        ));
    }
    *node_count += 1;
    if *node_count > MAX_TYPE_NODES {
        return Err(format!(
            "extension Ruby type exceeds maximum node count {MAX_TYPE_NODES}"
        ));
    }

    match ruby_type {
        ruby_fast_lsp_extension_api::RubyType::Named(name) => {
            let fqn = FullyQualifiedName::try_from(name.as_str())
                .map_err(|err| format!("invalid named Ruby type `{name}`: {err}"))?;
            Ok(AnalysisRubyType::Class(fqn))
        }
        ruby_fast_lsp_extension_api::RubyType::Array(element_types) => {
            let elements =
                convert_extension_type_list(element_types, depth + 1, node_count, "array element")?;
            Ok(AnalysisRubyType::Array(elements))
        }
        ruby_fast_lsp_extension_api::RubyType::Hash { keys, values } => {
            let keys = convert_extension_type_list(keys, depth + 1, node_count, "hash key")?;
            let values = convert_extension_type_list(values, depth + 1, node_count, "hash value")?;
            Ok(AnalysisRubyType::Hash(keys, values))
        }
        ruby_fast_lsp_extension_api::RubyType::Union(types) => {
            let types = convert_extension_type_list(types, depth + 1, node_count, "union")?;
            Ok(AnalysisRubyType::union(types))
        }
        ruby_fast_lsp_extension_api::RubyType::Unknown => Ok(AnalysisRubyType::Unknown),
    }
}

fn convert_extension_type_list(
    types: &[ruby_fast_lsp_extension_api::RubyType],
    depth: usize,
    node_count: &mut usize,
    label: &str,
) -> Result<Vec<AnalysisRubyType>, String> {
    if types.is_empty() {
        return Err(format!("extension {label} type list must not be empty"));
    }
    let mut converted = types
        .iter()
        .map(|ruby_type| analysis_ruby_type_from_extension_inner(ruby_type, depth, node_count))
        .collect::<Result<Vec<_>, _>>()?;
    converted.sort_by_key(|ruby_type| format!("{ruby_type:?}"));
    converted.dedup();
    Ok(converted)
}

fn extension_ruby_types_semantically_equal(
    left: Option<&ruby_fast_lsp_extension_api::RubyType>,
    right: Option<&ruby_fast_lsp_extension_api::RubyType>,
) -> bool {
    analysis_ruby_type_from_extension(left).expect(
        "INVARIANT VIOLATED: invalid left extension Ruby type reached conflict resolution. This is a bug because patch payloads must be validated before deterministic merging. Fix: keep validation before resolve_index_patch_conflicts.",
    ) == analysis_ruby_type_from_extension(right).expect(
        "INVARIANT VIOLATED: invalid right extension Ruby type reached conflict resolution. This is a bug because patch payloads must be validated before deterministic merging. Fix: keep validation before resolve_index_patch_conflicts.",
    )
}

fn index_patch_payload_eq(left: &IndexPatch, right: &IndexPatch) -> bool {
    match (left, right) {
        (IndexPatch::DefineNamespace(left), IndexPatch::DefineNamespace(right)) => {
            left.namespace == right.namespace
                && left.kind == right.kind
                && left.location == right.location
        }
        (IndexPatch::DefineConstant(left), IndexPatch::DefineConstant(right)) => {
            left.namespace == right.namespace
                && left.name == right.name
                && left.location == right.location
                && extension_ruby_types_semantically_equal(
                    left.ruby_type.as_ref(),
                    right.ruby_type.as_ref(),
                )
        }
        (IndexPatch::AddReference(left), IndexPatch::AddReference(right)) => {
            left.target == right.target && left.location == right.location
        }
        (IndexPatch::DefineMethod(left), IndexPatch::DefineMethod(right)) => {
            left.name == right.name
                && left.namespace == right.namespace
                && left.owner_target == right.owner_target
                && left.owner_kind == right.owner_kind
                && left.visibility == right.visibility
                && left.location == right.location
                && left.params == right.params
                && left.return_type_source == right.return_type_source
                && extension_ruby_types_semantically_equal(
                    left.return_type.as_ref(),
                    right.return_type.as_ref(),
                )
        }
        (IndexPatch::SetSuperclass(left), IndexPatch::SetSuperclass(right)) => {
            left.namespace == right.namespace
                && left.superclass == right.superclass
                && left.absolute == right.absolute
                && left.location == right.location
        }
        (IndexPatch::ApplyMixin(left), IndexPatch::ApplyMixin(right)) => {
            left.namespace == right.namespace
                && left.owner_target == right.owner_target
                && left.target_kind == right.target_kind
                && left.mixin == right.mixin
                && left.absolute == right.absolute
                && left.kind == right.kind
                && left.location == right.location
        }
        (IndexPatch::ConnectExecutionContext(left), IndexPatch::ConnectExecutionContext(right)) => {
            left.template == right.template
                && left.application == right.application
                && left.location == right.location
        }
        (IndexPatch::DefineNamespace(_), IndexPatch::DefineConstant(_))
        | (IndexPatch::DefineNamespace(_), IndexPatch::AddReference(_))
        | (IndexPatch::DefineNamespace(_), IndexPatch::DefineMethod(_))
        | (IndexPatch::DefineNamespace(_), IndexPatch::ApplyMixin(_))
        | (IndexPatch::DefineNamespace(_), IndexPatch::SetSuperclass(_))
        | (IndexPatch::DefineConstant(_), IndexPatch::DefineNamespace(_))
        | (IndexPatch::DefineConstant(_), IndexPatch::AddReference(_))
        | (IndexPatch::DefineConstant(_), IndexPatch::DefineMethod(_))
        | (IndexPatch::DefineConstant(_), IndexPatch::ApplyMixin(_))
        | (IndexPatch::DefineConstant(_), IndexPatch::SetSuperclass(_))
        | (IndexPatch::AddReference(_), IndexPatch::DefineNamespace(_))
        | (IndexPatch::AddReference(_), IndexPatch::DefineConstant(_))
        | (IndexPatch::AddReference(_), IndexPatch::DefineMethod(_))
        | (IndexPatch::AddReference(_), IndexPatch::ApplyMixin(_))
        | (IndexPatch::AddReference(_), IndexPatch::SetSuperclass(_))
        | (IndexPatch::DefineMethod(_), IndexPatch::DefineNamespace(_))
        | (IndexPatch::DefineMethod(_), IndexPatch::DefineConstant(_))
        | (IndexPatch::DefineMethod(_), IndexPatch::AddReference(_))
        | (IndexPatch::DefineMethod(_), IndexPatch::ApplyMixin(_))
        | (IndexPatch::DefineMethod(_), IndexPatch::SetSuperclass(_))
        | (IndexPatch::SetSuperclass(_), IndexPatch::DefineNamespace(_))
        | (IndexPatch::SetSuperclass(_), IndexPatch::DefineConstant(_))
        | (IndexPatch::SetSuperclass(_), IndexPatch::AddReference(_))
        | (IndexPatch::SetSuperclass(_), IndexPatch::DefineMethod(_))
        | (IndexPatch::SetSuperclass(_), IndexPatch::ApplyMixin(_))
        | (IndexPatch::ApplyMixin(_), IndexPatch::SetSuperclass(_))
        | (IndexPatch::ApplyMixin(_), IndexPatch::DefineNamespace(_))
        | (IndexPatch::ApplyMixin(_), IndexPatch::DefineConstant(_))
        | (IndexPatch::ApplyMixin(_), IndexPatch::AddReference(_))
        | (IndexPatch::ApplyMixin(_), IndexPatch::DefineMethod(_))
        | (IndexPatch::ConnectExecutionContext(_), IndexPatch::DefineNamespace(_))
        | (IndexPatch::ConnectExecutionContext(_), IndexPatch::DefineConstant(_))
        | (IndexPatch::ConnectExecutionContext(_), IndexPatch::AddReference(_))
        | (IndexPatch::ConnectExecutionContext(_), IndexPatch::DefineMethod(_))
        | (IndexPatch::ConnectExecutionContext(_), IndexPatch::SetSuperclass(_))
        | (IndexPatch::ConnectExecutionContext(_), IndexPatch::ApplyMixin(_))
        | (IndexPatch::DefineNamespace(_), IndexPatch::ConnectExecutionContext(_))
        | (IndexPatch::DefineConstant(_), IndexPatch::ConnectExecutionContext(_))
        | (IndexPatch::AddReference(_), IndexPatch::ConnectExecutionContext(_))
        | (IndexPatch::DefineMethod(_), IndexPatch::ConnectExecutionContext(_))
        | (IndexPatch::SetSuperclass(_), IndexPatch::ConnectExecutionContext(_))
        | (IndexPatch::ApplyMixin(_), IndexPatch::ConnectExecutionContext(_)) => false,
    }
}

fn discover_extension_packages(config: &ExtensionLoadConfig) -> Vec<ExtensionPackage> {
    let mut packages = Vec::new();
    for configured_path in &config.package_paths {
        if let Err(err) = collect_extension_package(configured_path, true, &mut packages) {
            warn!("Skipping Ruby Fast LSP extension package: {}", err);
        }
    }
    for configured_path in &config.directory_paths {
        if let Err(err) = collect_extension_directory(configured_path, &mut packages) {
            warn!("Skipping Ruby Fast LSP extension directory: {}", err);
        }
    }
    for configured_path in &config.project_package_paths {
        if let Err(err) = collect_extension_package(configured_path, false, &mut packages) {
            warn!(
                "Skipping project-local Ruby Fast LSP extension package: {}",
                err
            );
        }
    }
    packages.sort_by(|left, right| {
        extension_package_priority(left)
            .cmp(&extension_package_priority(right))
            .then_with(|| left.wasm_path.cmp(&right.wasm_path))
    });
    packages.dedup_by(|left, right| left.wasm_path == right.wasm_path);
    packages
}

fn extension_packages_fingerprint(packages: &[ExtensionPackage]) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(b"ruby-fast-lsp-extension-discovery-v1\0");
    for package in packages {
        digest.update([match package.source {
            ExtensionPathSource::Environment => 0,
            ExtensionPathSource::ProjectLocal => 1,
            ExtensionPathSource::InitializationOptions => 2,
        }]);
        digest.update([u8::from(package.explicit_package)]);
        let path = package.wasm_path.to_string_lossy();
        digest.update((path.len() as u64).to_le_bytes());
        digest.update(path.as_bytes());
        let manifest = format!("{:?}", package.manifest);
        digest.update((manifest.len() as u64).to_le_bytes());
        digest.update(manifest.as_bytes());
        digest.update((package.wasm_bytes.len() as u64).to_le_bytes());
        digest.update(package.wasm_bytes.as_ref());
    }
    digest.finalize().into()
}

#[cfg(test)]
fn load_wasm_extensions(config: &ExtensionLoadConfig) -> Vec<Arc<LoadedWasmExtension>> {
    load_wasm_extensions_from_packages(discover_extension_packages(config))
}

#[cfg(test)]
fn load_wasm_extensions_from_packages(
    packages: Vec<ExtensionPackage>,
) -> Vec<Arc<LoadedWasmExtension>> {
    load_wasm_extensions_from_packages_with_cache(packages, None)
}

fn load_wasm_extensions_from_packages_with_cache(
    packages: Vec<ExtensionPackage>,
    persistent_cache: Option<&PersistentDerivedProductCache>,
) -> Vec<Arc<LoadedWasmExtension>> {
    let mut extension_ids = BTreeSet::new();
    packages
        .into_iter()
        .filter_map(|package| match load_wasm_extension_with_cache(package, persistent_cache) {
            Ok(extension) if extension_ids.insert(extension.metadata.id.clone()) => Some(extension),
            Ok(extension) => {
                warn!(
                    "Skipping duplicate Ruby Fast LSP extension id `{}` from lower-priority package",
                    extension.metadata.id
                );
                None
            }
            Err(err) => {
                warn!("Skipping Ruby Fast LSP extension: {}", err);
                None
            }
        })
        .collect::<Vec<_>>()
}

#[derive(Debug)]
struct ExtensionPackage {
    wasm_path: PathBuf,
    wasm_bytes: Arc<[u8]>,
    manifest: Option<ExtensionManifest>,
    source: ExtensionPathSource,
    explicit_package: bool,
}

fn extension_package_priority(package: &ExtensionPackage) -> (u8, u8) {
    let source = match package.source {
        ExtensionPathSource::InitializationOptions => 0,
        ExtensionPathSource::ProjectLocal => 1,
        ExtensionPathSource::Environment => 2,
    };
    let discovery = if package.explicit_package { 0 } else { 1 };
    (source, discovery)
}

fn collect_extension_package(
    configured_path: &ConfiguredExtensionPath,
    explicit_package: bool,
    output: &mut Vec<ExtensionPackage>,
) -> Result<(), ExtensionLoadError> {
    let path = &configured_path.path;
    if path.is_file() {
        if path.extension().and_then(|ext| ext.to_str()) != Some("wasm") {
            return Err(ExtensionLoadError::new(format!(
                "extension path `{}` is not a .wasm file or package directory",
                path.display()
            )));
        }
        if configured_path.source == ExtensionPathSource::InitializationOptions {
            return Err(ExtensionLoadError::new(format!(
                "direct wasm path `{}` is not allowed from initialization options; use a package directory with extension.toml",
                path.display()
            )));
        }
        output.push(ExtensionPackage {
            wasm_path: path.to_path_buf(),
            wasm_bytes: read_extension_wasm(path)?,
            manifest: None,
            source: configured_path.source,
            explicit_package,
        });
        return Ok(());
    }

    if !path.is_dir() {
        return Err(ExtensionLoadError::new(format!(
            "extension path `{}` is neither a file nor directory",
            path.display()
        )));
    }

    let manifest_path = path.join("extension.toml");
    if manifest_path.exists() {
        let manifest = read_manifest(&manifest_path)?;
        let wasm_path = manifest_wasm_path(path, &manifest)?;
        let wasm_bytes = read_extension_wasm(&wasm_path)?;
        output.push(ExtensionPackage {
            wasm_path,
            wasm_bytes,
            manifest: Some(manifest),
            source: configured_path.source,
            explicit_package,
        });
        return Ok(());
    }

    if configured_path.source == ExtensionPathSource::InitializationOptions {
        return Err(ExtensionLoadError::new(format!(
            "extension package `{}` has no extension.toml",
            path.display()
        )));
    }

    collect_extension_directory(configured_path, output)
}

fn collect_extension_directory(
    configured_path: &ConfiguredExtensionPath,
    output: &mut Vec<ExtensionPackage>,
) -> Result<(), ExtensionLoadError> {
    let path = &configured_path.path;
    if !path.is_dir() {
        return Err(ExtensionLoadError::new(format!(
            "extension directory `{}` is not a directory",
            path.display()
        )));
    }

    for entry in fs::read_dir(path).map_err(|err| {
        ExtensionLoadError::new(format!(
            "failed to read extension directory `{}`: {}",
            path.display(),
            err
        ))
    })? {
        let entry = entry.map_err(|err| {
            ExtensionLoadError::new(format!(
                "failed to read extension directory entry in `{}`: {}",
                path.display(),
                err
            ))
        })?;
        let entry_path = entry.path();
        if entry_path.is_dir() && entry_path.join("extension.toml").exists() {
            let entry_path = ConfiguredExtensionPath {
                path: entry_path,
                source: configured_path.source,
            };
            if let Err(err) = collect_extension_package(&entry_path, false, output) {
                warn!("Skipping Ruby Fast LSP extension package: {}", err);
            }
        } else if entry_path.extension().and_then(|ext| ext.to_str()) == Some("wasm") {
            if configured_path.source == ExtensionPathSource::InitializationOptions {
                warn!(
                    "Skipping direct wasm extension `{}` from initialization options; use a package directory with extension.toml",
                    entry_path.display()
                );
                continue;
            }
            match read_extension_wasm(&entry_path) {
                Ok(wasm_bytes) => output.push(ExtensionPackage {
                    wasm_path: entry_path,
                    wasm_bytes,
                    manifest: None,
                    source: configured_path.source,
                    explicit_package: false,
                }),
                Err(error) => warn!("Skipping unreadable Wasm extension: {error}"),
            }
        }
    }
    Ok(())
}

fn read_manifest(path: &Path) -> Result<ExtensionManifest, ExtensionLoadError> {
    let contents = fs::read_to_string(path).map_err(|err| {
        ExtensionLoadError::new(format!(
            "failed to read extension manifest `{}`: {}",
            path.display(),
            err
        ))
    })?;
    toml::from_str(&contents).map_err(|err| {
        ExtensionLoadError::new(format!(
            "invalid extension manifest `{}`: {}",
            path.display(),
            err
        ))
    })
}

fn read_extension_wasm(path: &Path) -> Result<Arc<[u8]>, ExtensionLoadError> {
    let mut file = fs::File::open(path).map_err(|error| {
        ExtensionLoadError::new(format!(
            "failed to open Wasm extension `{}`: {error}",
            path.display()
        ))
    })?;
    let byte_length = file
        .metadata()
        .map_err(|error| {
            ExtensionLoadError::new(format!(
                "failed to inspect Wasm extension `{}`: {error}",
                path.display()
            ))
        })?
        .len();
    if byte_length > MAX_EXTENSION_WASM_BYTES {
        return Err(ExtensionLoadError::new(format!(
            "Wasm extension `{}` is {byte_length} bytes; maximum is {MAX_EXTENSION_WASM_BYTES}",
            path.display()
        )));
    }
    let capacity = usize::try_from(byte_length).map_err(|_| {
        ExtensionLoadError::new(format!(
            "Wasm extension `{}` length does not fit this platform",
            path.display()
        ))
    })?;
    let mut bytes = Vec::with_capacity(capacity);
    let read_limit = MAX_EXTENSION_WASM_BYTES
        .checked_add(1)
        .expect("INVARIANT VIOLATED: Wasm source read limit overflowed u64. This is a bug because the fixed 64 MiB limit must fit u64. Fix: keep the source limit below u64::MAX.");
    let mut bounded = std::io::Read::take(&mut file, read_limit);
    std::io::Read::read_to_end(&mut bounded, &mut bytes).map_err(|error| {
        ExtensionLoadError::new(format!(
            "failed to read Wasm extension `{}`: {error}",
            path.display()
        ))
    })?;
    let actual_length = u64::try_from(bytes.len()).map_err(|_| {
        ExtensionLoadError::new(format!(
            "Wasm extension `{}` read length does not fit u64",
            path.display()
        ))
    })?;
    if actual_length > MAX_EXTENSION_WASM_BYTES {
        return Err(ExtensionLoadError::new(format!(
            "Wasm extension `{}` grew to {actual_length} bytes while reading; maximum is {MAX_EXTENSION_WASM_BYTES}",
            path.display()
        )));
    }
    Ok(Arc::from(bytes))
}

fn manifest_wasm_path(
    package_dir: &Path,
    manifest: &ExtensionManifest,
) -> Result<PathBuf, ExtensionLoadError> {
    let relative_path = manifest
        .wasm
        .as_ref()
        .or_else(|| manifest.build.as_ref().map(|build| &build.output))
        .ok_or_else(|| {
            ExtensionLoadError::new(format!(
                "extension `{}` manifest has no `wasm` or `build.output` path",
                manifest.id
            ))
        })?;
    let wasm_path = package_dir.join(relative_path);
    if !wasm_path.is_file() {
        return Err(ExtensionLoadError::new(format!(
            "extension `{}` wasm path `{}` does not exist",
            manifest.id,
            wasm_path.display()
        )));
    }
    Ok(wasm_path)
}

fn load_wasm_extension(
    package: ExtensionPackage,
) -> Result<Arc<LoadedWasmExtension>, ExtensionLoadError> {
    load_wasm_extension_with_cache(package, None)
}

fn load_wasm_extension_with_cache(
    package: ExtensionPackage,
    persistent_cache: Option<&PersistentDerivedProductCache>,
) -> Result<Arc<LoadedWasmExtension>, ExtensionLoadError> {
    let id = package
        .manifest
        .as_ref()
        .map(|manifest| manifest.id.clone())
        .unwrap_or_else(|| wasm_file_stem(&package.wasm_path));

    let wasm_bytes = package.wasm_bytes.as_ref();

    if let Some(manifest) = &package.manifest {
        validate_manifest(manifest)?;
        validate_manifest_checksum(manifest, wasm_bytes)?;
    }
    let metadata = extension_metadata(&id, package.manifest.as_ref());

    let compiled_extension =
        load_compiled_wasm_extension(&id, &package.wasm_path, wasm_bytes, persistent_cache)?;
    let mut extension = ruby_fast_lsp_extension_wasm_host::WasmExtension::from_compiled(
        id.clone(),
        compiled_extension.clone(),
    )
    .map_err(|err| {
        ExtensionLoadError::new(format!(
            "failed to instantiate Wasm extension `{}` from `{}`: {}",
            id,
            package.wasm_path.display(),
            err
        ))
    })?;

    let abi_version = extension.abi_version().map_err(|err| {
        ExtensionLoadError::new(format!(
            "Wasm extension `{}` ABI check failed: {}",
            extension.id(),
            err
        ))
    })?;
    if abi_version != ruby_fast_lsp_extension_api::ABI_VERSION {
        return Err(ExtensionLoadError::new(format!(
            "Wasm extension `{}` ABI version {} != host ABI version {}",
            extension.id(),
            abi_version,
            ruby_fast_lsp_extension_api::ABI_VERSION
        )));
    }

    if let Some(manifest) = &package.manifest {
        validate_manifest_call_names(manifest, extension.indexed_call_names())?;
    }
    let semantic_targets = package
        .manifest
        .as_ref()
        .map(parse_manifest_method_targets)
        .transpose()?
        .unwrap_or_default();
    let frame_call_names = package
        .manifest
        .as_ref()
        .and_then(|manifest| manifest.indexing.as_ref())
        .map(|indexing| indexing.frame_call_names.iter().cloned().collect())
        .unwrap_or_default();
    let applicability = package
        .manifest
        .as_ref()
        .map(parse_manifest_applicability)
        .transpose()?
        .unwrap_or_default();
    let project_context_delivery = package
        .manifest
        .as_ref()
        .and_then(|manifest| manifest.indexing.as_ref())
        .map(|indexing| indexing.project_context)
        .unwrap_or_default();

    Ok(Arc::new(LoadedWasmExtension::new(
        metadata,
        extension,
        compiled_extension,
        semantic_targets,
        frame_call_names,
        applicability,
        project_context_delivery,
    )))
}

fn load_compiled_wasm_extension(
    id: &str,
    wasm_path: &Path,
    wasm_bytes: &[u8],
    persistent_cache: Option<&PersistentDerivedProductCache>,
) -> Result<ruby_fast_lsp_extension_wasm_host::CompiledWasmExtension, ExtensionLoadError> {
    let compiler =
        ruby_fast_lsp_extension_wasm_host::WasmExtensionCompiler::new().map_err(|error| {
            ExtensionLoadError::new(format!(
                "failed to prepare Wasm extension compiler for `{id}` from `{}`: {error}",
                wasm_path.display()
            ))
        })?;
    let key = CompiledWasmProductKey::new(wasm_bytes, compiler.cache_identity());
    if let Some(cache) = persistent_cache {
        for attempt in 0..2 {
            match cache.lookup_compiled_wasm_or_reserve(&key) {
                Ok(PersistentCompiledWasmLookup::Hit(serialized)) => {
                    // SAFETY: the persistent cache verifies its private
                    // envelope checksum, embedded source digest, compiler
                    // identity, artifact length, and artifact checksum before
                    // returning these bytes.
                    match unsafe { compiler.deserialize_verified(serialized.as_slice()) } {
                        Ok(compiled) => return Ok(compiled),
                        Err(error) if attempt == 0 => {
                            warn!(
                                "Rejecting incompatible compiled Wasm cache product for `{id}`: {error:#}"
                            );
                            if let Err(invalidation_error) = cache.invalidate_compiled_wasm(&key) {
                                warn!(
                                    "Failed to invalidate compiled Wasm cache product for `{id}`; compiling without persistence: {invalidation_error:#}"
                                );
                                break;
                            }
                        }
                        Err(error) => {
                            warn!(
                                "Replacement compiled Wasm cache product for `{id}` was still invalid; compiling without persistence: {error:#}"
                            );
                            break;
                        }
                    }
                }
                Ok(PersistentCompiledWasmLookup::Reservation(reservation)) => {
                    let compiled = compiler.compile(wasm_bytes).map_err(|error| {
                        ExtensionLoadError::new(format!(
                            "failed to compile Wasm extension `{id}` from `{}`: {error}",
                            wasm_path.display()
                        ))
                    })?;
                    match compiled.serialize() {
                        Ok(serialized) => {
                            if let Err(error) = reservation.publish(&key, &serialized) {
                                warn!(
                                    "Failed to publish compiled Wasm cache product for `{id}`; using the valid in-memory module: {error:#}"
                                );
                            }
                        }
                        Err(error) => warn!(
                            "Failed to serialize compiled Wasm extension `{id}`; using the valid in-memory module: {error:#}"
                        ),
                    }
                    return Ok(compiled);
                }
                Err(error) => {
                    warn!(
                        "Compiled Wasm cache lookup failed for `{id}`; compiling without persistence: {error:#}"
                    );
                    break;
                }
            }
        }
    }

    compiler.compile(wasm_bytes).map_err(|error| {
        ExtensionLoadError::new(format!(
            "failed to compile Wasm extension `{id}` from `{}`: {error}",
            wasm_path.display()
        ))
    })
}

fn require_empty_lifecycle_output(
    output: ruby_fast_lsp_extension_api::ExtensionOutput,
) -> anyhow::Result<()> {
    if output.index_patches.is_empty()
        && output.execution_contexts.is_empty()
        && output.response_patches.is_empty()
        && output.command_patches.is_empty()
        && output.process_requests.is_empty()
        && output.reindex_files.is_empty()
    {
        return Ok(());
    }
    Err(anyhow::anyhow!(
        "lifecycle callback returned patches or requests"
    ))
}

fn lifecycle_output_is_empty(
    result: &anyhow::Result<ruby_fast_lsp_extension_api::ExtensionOutput>,
) -> bool {
    result.as_ref().is_ok_and(|output| {
        output.index_patches.is_empty()
            && output.execution_contexts.is_empty()
            && output.response_patches.is_empty()
            && output.command_patches.is_empty()
            && output.process_requests.is_empty()
            && output.reindex_files.is_empty()
    })
}

fn parse_manifest_applicability(
    manifest: &ExtensionManifest,
) -> Result<Vec<ExtensionGemRequirement>, ExtensionLoadError> {
    let Some(applicability) = &manifest.applicability else {
        return Ok(Vec::new());
    };
    applicability
        .locked_gems
        .iter()
        .map(|gem| {
            let version = VersionReq::parse(&gem.version).map_err(|error| {
                ExtensionLoadError::new(format!(
                    "extension `{}` applicability for gem `{}` has invalid version requirement `{}`: {error}",
                    manifest.id, gem.name, gem.version
                ))
            })?;
            Ok(ExtensionGemRequirement {
                name: gem.name.clone(),
                version,
            })
        })
        .collect()
}

fn validate_manifest(manifest: &ExtensionManifest) -> Result<(), ExtensionLoadError> {
    validate_manifest_id(&manifest.id)?;
    validate_optional_non_empty("name", &manifest.id, manifest.name.as_deref())?;
    validate_optional_non_empty("version", &manifest.id, manifest.version.as_deref())?;
    validate_manifest_list("capability", &manifest.id, &manifest.capabilities)?;
    validate_manifest_list("permission", &manifest.id, &manifest.permissions)?;
    if manifest.abi_version != ruby_fast_lsp_extension_api::ABI_VERSION {
        return Err(ExtensionLoadError::new(format!(
            "extension manifest `{}` ABI version {} != host ABI version {}",
            manifest.id,
            manifest.abi_version,
            ruby_fast_lsp_extension_api::ABI_VERSION
        )));
    }
    if !matches!(manifest.runtime.as_str(), "wasm" | "mruby-wasm") {
        return Err(ExtensionLoadError::new(format!(
            "extension `{}` runtime `{}` is unsupported",
            manifest.id, manifest.runtime
        )));
    }
    if let Some(applicability) = &manifest.applicability {
        if applicability.locked_gems.is_empty() {
            return Err(ExtensionLoadError::new(format!(
                "extension `{}` applicability.locked_gems must not be empty",
                manifest.id
            )));
        }
        let mut names = BTreeSet::new();
        for gem in &applicability.locked_gems {
            if gem.name.is_empty()
                || gem.name.len() > 128
                || !gem
                    .name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
            {
                return Err(ExtensionLoadError::new(format!(
                    "extension `{}` applicability gem name `{}` is invalid",
                    manifest.id, gem.name
                )));
            }
            if !names.insert(gem.name.as_str()) {
                return Err(ExtensionLoadError::new(format!(
                    "extension `{}` applicability declares gem `{}` more than once",
                    manifest.id, gem.name
                )));
            }
            VersionReq::parse(&gem.version).map_err(|error| {
                ExtensionLoadError::new(format!(
                    "extension `{}` applicability for gem `{}` has invalid version requirement `{}`: {error}",
                    manifest.id, gem.name, gem.version
                ))
            })?;
        }
    }
    if let Some(server_version) = &manifest.server_version {
        validate_server_version(&manifest.id, server_version)?;
    }
    if manifest.settings_schema.is_some()
        && !manifest
            .capabilities
            .iter()
            .any(|capability| capability == "settings")
    {
        warn!(
            "Extension `{}` declares settings_schema without `settings` capability",
            manifest.id
        );
    }
    if let Some(indexing) = &manifest.indexing {
        validate_manifest_list("indexing call name", &manifest.id, &indexing.call_names)?;
        validate_manifest_list(
            "indexing frame call name",
            &manifest.id,
            &indexing.frame_call_names,
        )?;
        for name in &indexing.frame_call_names {
            RubyMethod::new(name).map_err(|err| {
                ExtensionLoadError::new(format!(
                    "extension `{}` indexing frame call name `{name}` is invalid: {err}",
                    manifest.id
                ))
            })?;
        }
    }
    if let Some(watching) = &manifest.watching {
        validate_manifest_list("watched file glob", &manifest.id, &watching.globs)?;
        if !manifest
            .capabilities
            .iter()
            .any(|capability| capability == "watching")
        {
            return Err(ExtensionLoadError::new(format!(
                "extension `{}` declares watched files without `watching` capability",
                manifest.id
            )));
        }
        build_watched_file_matcher(&manifest.id, &watching.globs)?;
    }
    if let Some(process) = &manifest.process {
        validate_manifest_list("process command", &manifest.id, &process.commands)?;
        if !manifest
            .capabilities
            .iter()
            .any(|capability| capability == "process")
        {
            return Err(ExtensionLoadError::new(format!(
                "extension `{}` declares process commands without `process` capability",
                manifest.id
            )));
        }
        if !manifest
            .permissions
            .iter()
            .any(|permission| permission == "process.exec")
        {
            return Err(ExtensionLoadError::new(format!(
                "extension `{}` declares process commands without `process.exec` permission",
                manifest.id
            )));
        }
        for command in &process.commands {
            let path = Path::new(command);
            if path.is_absolute()
                || path.components().any(|component| {
                    matches!(
                        component,
                        Component::ParentDir | Component::RootDir | Component::Prefix(_)
                    )
                })
            {
                return Err(ExtensionLoadError::new(format!(
                    "extension `{}` process command `{command}` must be a bare executable or workspace-relative path without traversal",
                    manifest.id
                )));
            }
        }
    }
    Ok(())
}

fn build_watched_file_matcher(
    extension_id: &str,
    globs: &[String],
) -> Result<GlobSet, ExtensionLoadError> {
    let mut builder = GlobSetBuilder::new();
    for pattern in globs {
        let path = Path::new(pattern);
        let has_windows_drive_prefix = pattern
            .as_bytes()
            .get(1)
            .is_some_and(|character| *character == b':');
        if path.is_absolute()
            || pattern.starts_with('/')
            || pattern.starts_with('\\')
            || pattern.contains('\\')
            || has_windows_drive_prefix
            || pattern.split('/').any(|component| component == "..")
            || path.components().any(|component| {
                matches!(
                    component,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
        {
            return Err(ExtensionLoadError::new(format!(
                "extension `{extension_id}` watched file glob `{pattern}` must be workspace-relative and cannot contain parent traversal"
            )));
        }
        let glob = Glob::new(pattern).map_err(|err| {
            ExtensionLoadError::new(format!(
                "extension `{extension_id}` has invalid watched file glob `{pattern}`: {err}"
            ))
        })?;
        builder.add(glob);
    }
    builder.build().map_err(|err| {
        ExtensionLoadError::new(format!(
            "extension `{extension_id}` failed to compile watched file globs: {err}"
        ))
    })
}

fn validate_manifest_checksum(
    manifest: &ExtensionManifest,
    wasm_bytes: &[u8],
) -> Result<(), ExtensionLoadError> {
    let Some(expected) = &manifest.checksum_sha256 else {
        return Ok(());
    };
    if expected.len() != 64
        || !expected
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return Err(ExtensionLoadError::new(format!(
            "extension `{}` checksum_sha256 must be 64 hex characters",
            manifest.id
        )));
    }
    let actual = format!("{:x}", Sha256::digest(wasm_bytes));
    if !expected.eq_ignore_ascii_case(&actual) {
        return Err(ExtensionLoadError::new(format!(
            "extension `{}` checksum mismatch: manifest {} != actual {}",
            manifest.id, expected, actual
        )));
    }
    Ok(())
}

fn validate_manifest_id(id: &str) -> Result<(), ExtensionLoadError> {
    if id.trim().is_empty() || id.chars().any(char::is_whitespace) {
        return Err(ExtensionLoadError::new(format!(
            "extension manifest id `{}` must be non-empty and contain no whitespace",
            id
        )));
    }
    Ok(())
}

fn validate_optional_non_empty(
    field: &str,
    id: &str,
    value: Option<&str>,
) -> Result<(), ExtensionLoadError> {
    if matches!(value, Some(value) if value.trim().is_empty()) {
        return Err(ExtensionLoadError::new(format!(
            "extension `{}` manifest field `{}` must not be empty",
            id, field
        )));
    }
    Ok(())
}

fn validate_manifest_list(
    label: &str,
    id: &str,
    values: &[String],
) -> Result<(), ExtensionLoadError> {
    let mut seen = BTreeSet::new();
    for value in values {
        if value.trim().is_empty() {
            return Err(ExtensionLoadError::new(format!(
                "extension `{}` manifest {} must not be empty",
                id, label
            )));
        }
        if !seen.insert(value) {
            return Err(ExtensionLoadError::new(format!(
                "extension `{}` manifest has duplicate {} `{}`",
                id, label, value
            )));
        }
    }
    Ok(())
}

fn validate_server_version(id: &str, requirement: &str) -> Result<(), ExtensionLoadError> {
    let requirement = VersionReq::parse(requirement).map_err(|err| {
        ExtensionLoadError::new(format!(
            "extension `{}` has invalid server_version `{}`: {}",
            id, requirement, err
        ))
    })?;
    let server_version = Version::parse(env!("CARGO_PKG_VERSION")).map_err(|err| {
        ExtensionLoadError::new(format!(
            "host server version `{}` is invalid semver: {}",
            env!("CARGO_PKG_VERSION"),
            err
        ))
    })?;
    if !requirement.matches(&server_version) {
        return Err(ExtensionLoadError::new(format!(
            "extension `{}` requires server_version `{}` but host is `{}`",
            id, requirement, server_version
        )));
    }
    Ok(())
}

fn extension_metadata(id: &str, manifest: Option<&ExtensionManifest>) -> ExtensionMetadata {
    match manifest {
        Some(manifest) => ExtensionMetadata {
            id: manifest.id.clone(),
            name: manifest.name.clone(),
            version: manifest.version.clone(),
            capabilities: manifest.capabilities.clone(),
            permissions: manifest.permissions.clone(),
            watched_files: manifest
                .watching
                .as_ref()
                .map(|watching| watching.globs.clone())
                .unwrap_or_default(),
            process_commands: manifest
                .process
                .as_ref()
                .map(|process| process.commands.clone())
                .unwrap_or_default(),
        },
        None => ExtensionMetadata {
            id: id.to_string(),
            name: None,
            version: None,
            capabilities: Vec::new(),
            permissions: Vec::new(),
            watched_files: Vec::new(),
            process_commands: Vec::new(),
        },
    }
}

fn validate_manifest_call_names(
    manifest: &ExtensionManifest,
    guest_call_names: &[String],
) -> Result<(), ExtensionLoadError> {
    let Some(indexing) = &manifest.indexing else {
        return Ok(());
    };
    let manifest_names: BTreeSet<&String> = indexing.call_names.iter().collect();
    let guest_names: BTreeSet<&String> = guest_call_names.iter().collect();
    if manifest_names != guest_names {
        return Err(ExtensionLoadError::new(format!(
            "extension `{}` manifest call names {:?} != guest call names {:?}",
            manifest.id, indexing.call_names, guest_call_names
        )));
    }
    Ok(())
}

fn parse_manifest_method_targets(
    manifest: &ExtensionManifest,
) -> Result<Vec<ExtensionMethodTarget>, ExtensionLoadError> {
    let Some(indexing) = &manifest.indexing else {
        return Ok(Vec::new());
    };
    indexing
        .targets
        .iter()
        .map(|target| parse_manifest_method_target(&manifest.id, target))
        .collect()
}

fn parse_manifest_method_target(
    extension_id: &str,
    target: &ExtensionMethodTargetManifest,
) -> Result<ExtensionMethodTarget, ExtensionLoadError> {
    let owner = target
        .owner
        .iter()
        .map(|part| {
            RubyConstant::new(part).map_err(|err| {
                ExtensionLoadError::new(format!(
                    "extension `{}` indexing target owner part `{}` is invalid: {}",
                    extension_id, part, err
                ))
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let owner_kind = match target.owner_kind.as_str() {
        "instance" => NamespaceKind::Instance,
        "singleton" => NamespaceKind::Singleton,
        other => {
            return Err(ExtensionLoadError::new(format!(
                "extension `{}` indexing target owner_kind `{}` is invalid; expected `instance` or `singleton`",
                extension_id, other
            )))
        }
    };
    let method = RubyMethod::new(&target.method).map_err(|err| {
        ExtensionLoadError::new(format!(
            "extension `{}` indexing target method `{}` is invalid: {}",
            extension_id, target.method, err
        ))
    })?;
    Ok(ExtensionMethodTarget {
        owner,
        owner_kind,
        method,
        frame: target.frame,
    })
}

fn wasm_file_stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_else(|| {
            panic!(
                "INVARIANT VIOLATED: Wasm extension path `{}` has no valid UTF-8 file stem. \
                 This is a bug because direct wasm loads default extension IDs to file stems. \
                 Fix: rename the wasm file or load it through an extension.toml manifest.",
                path.display()
            )
        })
        .to_string()
}

fn call_context(visitor: &FactCollector, node: &CallNode, include_project: bool) -> CallContext {
    let receiver = node
        .receiver()
        .map(|receiver| receiver_from_node(&receiver))
        .unwrap_or(Receiver::None);
    CallContext {
        project: include_project
            .then(|| visitor.extension_project_context.clone())
            .flatten(),
        method_name: utils::utf8_str(node.name().as_slice()).to_string(),
        receiver: receiver.clone(),
        arguments: node
            .arguments()
            .map(|args| {
                args.arguments()
                    .iter()
                    .flat_map(|arg| arguments_from_node(visitor, &arg))
                    .collect()
            })
            .unwrap_or_default(),
        current_namespace: visitor
            .scope_tracker
            .get_ns_stack()
            .iter()
            .map(ToString::to_string)
            .collect(),
        namespace_kind: namespace_kind_to_abi(visitor.scope_tracker.current_method_context()),
        call_range: source_range(visitor, &node.location()),
        block_range: node
            .block()
            .map(|block| source_range(visitor, &block.location())),
        message_range: node
            .message_loc()
            .map(|loc| source_range(visitor, &loc))
            .unwrap_or_else(|| source_range(visitor, &node.location())),
        resolved_callees: resolved_callees_for_call(visitor, node),
        enclosing_calls: visitor.extension_call_stack.clone(),
    }
}

pub fn resolved_call_for_stack(visitor: &FactCollector, node: &CallNode) -> ResolvedCall {
    let method_name = utils::utf8_str(node.name().as_slice()).to_string();
    let receiver = node
        .receiver()
        .map(|receiver| receiver_from_node(&receiver))
        .unwrap_or(Receiver::None);
    let resolved_callees = resolved_callees_for_call(visitor, node);
    ResolvedCall {
        method_name,
        receiver: receiver.clone(),
        arguments: node
            .arguments()
            .map(|args| {
                args.arguments()
                    .iter()
                    .flat_map(|arg| arguments_from_node(visitor, &arg))
                    .collect()
            })
            .unwrap_or_default(),
        resolved_callees,
        call_range: source_range(visitor, &node.location()),
        message_range: node
            .message_loc()
            .map(|loc| source_range(visitor, &loc))
            .unwrap_or_else(|| source_range(visitor, &node.location())),
        frame_extension_ids: Vec::new(),
    }
}

fn resolved_callees_for_call(visitor: &FactCollector, node: &CallNode) -> Vec<ResolvedCallee> {
    resolved_core_callees_for_call(visitor, node)
        .into_iter()
        .map(resolved_callee_to_abi)
        .collect()
}

fn resolved_core_callees_for_call(
    visitor: &FactCollector,
    node: &CallNode,
) -> Vec<ruby_analysis::core::ResolvedMethodCallee> {
    let method_name = utils::utf8_str(node.name().as_slice());
    let Ok(method) = RubyMethod::new(method_name) else {
        return Vec::new();
    };
    let core_receiver = node
        .receiver()
        .map(|receiver| core_method_receiver_from_node(visitor, &receiver))
        .unwrap_or(CoreMethodReceiver::None);

    resolved_core_callees_for_call_analysis(
        &visitor.analysis_engine,
        &core_receiver,
        &method,
        &visitor.scope_tracker.get_ns_stack(),
        visitor.scope_tracker.current_method_context(),
    )
}

fn resolved_core_callees_for_call_analysis(
    engine: &Arc<RwLock<ruby_analysis::engine::AnalysisEngine>>,
    receiver: &CoreMethodReceiver,
    method: &RubyMethod,
    current_namespace: &[RubyConstant],
    namespace_kind: NamespaceKind,
) -> Vec<ruby_analysis::core::ResolvedMethodCallee> {
    let engine = engine.read();
    let query = ruby_analysis::engine::AnalysisQuery::new(&engine);
    let namespace_fqn = match receiver {
        CoreMethodReceiver::Constant(path) => {
            query.resolve_constant_receiver(path, current_namespace)
        }
        CoreMethodReceiver::None | CoreMethodReceiver::SelfReceiver | CoreMethodReceiver::Super => {
            FullyQualifiedName::namespace_with_kind(current_namespace.to_vec(), namespace_kind)
        }
        CoreMethodReceiver::LocalVariable(_)
        | CoreMethodReceiver::InstanceVariable(_)
        | CoreMethodReceiver::ClassVariable(_)
        | CoreMethodReceiver::GlobalVariable(_)
        | CoreMethodReceiver::Expression
        | CoreMethodReceiver::MethodCall { .. }
        | CoreMethodReceiver::Literal(_) => return Vec::new(),
    };

    let Some(callees) = query.resolve_method_callees(&namespace_fqn, method) else {
        return Vec::new();
    };

    callees
}

fn resolved_callee_to_abi(callee: ruby_analysis::core::ResolvedMethodCallee) -> ResolvedCallee {
    let owner_kind = callee.owner.namespace_kind().unwrap_or_else(|| {
        panic!(
            "INVARIANT VIOLATED: analysis resolved extension callee owner `{}` is not a namespace. \
             This is a bug because extension callee owners must be namespaces. \
             Fix: keep AnalysisQuery::resolve_method_callees returning namespace owners.",
            callee.owner
        )
    });
    ResolvedCallee {
        owner: callee
            .owner
            .namespace_parts()
            .iter()
            .map(ToString::to_string)
            .collect(),
        owner_kind: namespace_kind_to_abi(owner_kind),
        method: callee.method.to_string(),
        resolution: callee_resolution_to_abi(callee.resolution),
    }
}

fn core_method_receiver_from_node(visitor: &FactCollector, node: &Node) -> CoreMethodReceiver {
    if node.as_self_node().is_some() {
        CoreMethodReceiver::SelfReceiver
    } else if let Some(constant) = node.as_constant_read_node() {
        CoreMethodReceiver::Constant(vec![RubyConstant::new(utils::utf8_str(
            constant.name().as_slice(),
        ))
        .expect(
            "INVARIANT VIOLATED: Prism returned an invalid constant-read name. \
             This is a bug because Prism constant names must be valid Ruby constants. \
             Fix: inspect constant receiver conversion.",
        )])
    } else if let Some(path) = node.as_constant_path_node() {
        let mut parts = Vec::new();
        utils::collect_namespaces(&path, &mut parts);
        CoreMethodReceiver::Constant(parts)
    } else if let Some(local) = node.as_local_variable_read_node() {
        CoreMethodReceiver::LocalVariable(utils::utf8_str(local.name().as_slice()).to_string())
    } else if let Some(ivar) = node.as_instance_variable_read_node() {
        CoreMethodReceiver::InstanceVariable(utils::utf8_str(ivar.name().as_slice()).to_string())
    } else if let Some(cvar) = node.as_class_variable_read_node() {
        CoreMethodReceiver::ClassVariable(utils::utf8_str(cvar.name().as_slice()).to_string())
    } else if let Some(gvar) = node.as_global_variable_read_node() {
        CoreMethodReceiver::GlobalVariable(utils::utf8_str(gvar.name().as_slice()).to_string())
    } else if let Some(call) = node.as_call_node() {
        CoreMethodReceiver::MethodCall {
            inner_receiver: Box::new(
                call.receiver()
                    .map(|receiver| core_method_receiver_from_node(visitor, &receiver))
                    .unwrap_or(CoreMethodReceiver::None),
            ),
            method_name: utils::utf8_str(call.name().as_slice()).to_string(),
        }
    } else if let Some(ruby_type) = visitor.literal_analyzer.analyze_literal(node) {
        CoreMethodReceiver::Literal(ruby_type)
    } else {
        CoreMethodReceiver::Expression
    }
}

fn callee_resolution_to_abi(
    resolution: MethodCalleeResolution,
) -> ruby_fast_lsp_extension_api::CalleeResolution {
    match resolution {
        MethodCalleeResolution::Exact | MethodCalleeResolution::MethodMissing => {
            ruby_fast_lsp_extension_api::CalleeResolution::Exact
        }
        MethodCalleeResolution::ReceiverOnly => {
            ruby_fast_lsp_extension_api::CalleeResolution::ReceiverOnly
        }
    }
}

fn receiver_from_node(node: &Node) -> Receiver {
    if node.as_self_node().is_some() {
        Receiver::SelfReceiver
    } else if let Some(constant) = node.as_constant_read_node() {
        Receiver::Constant(vec![utils::utf8_str(constant.name().as_slice()).to_string()])
    } else if let Some(path) = node.as_constant_path_node() {
        let mut parts = Vec::new();
        utils::collect_namespaces(&path, &mut parts);
        Receiver::Constant(parts.iter().map(ToString::to_string).collect())
    } else if let Some(local) = node.as_local_variable_read_node() {
        Receiver::LocalVariable(utils::utf8_str(local.name().as_slice()).to_string())
    } else if let Some(ivar) = node.as_instance_variable_read_node() {
        Receiver::InstanceVariable(utils::utf8_str(ivar.name().as_slice()).to_string())
    } else if let Some(cvar) = node.as_class_variable_read_node() {
        Receiver::ClassVariable(utils::utf8_str(cvar.name().as_slice()).to_string())
    } else if let Some(gvar) = node.as_global_variable_read_node() {
        Receiver::GlobalVariable(utils::utf8_str(gvar.name().as_slice()).to_string())
    } else if let Some(call) = node.as_call_node() {
        Receiver::MethodCall {
            method_name: utils::utf8_str(call.name().as_slice()).to_string(),
        }
    } else if is_literal(node) {
        Receiver::Literal
    } else {
        Receiver::Expression
    }
}

fn arguments_from_node(visitor: &FactCollector, node: &Node) -> Vec<Argument> {
    if let Some(keyword_hash) = node.as_keyword_hash_node() {
        return keyword_hash
            .elements()
            .iter()
            .filter_map(|element| {
                let assoc = element.as_assoc_node()?;
                let symbol = assoc.key().as_symbol_node()?;
                Some(Argument {
                    keyword: Some(Keyword {
                        name: String::from_utf8_lossy(symbol.unescaped()).to_string(),
                        range: source_range(visitor, &symbol.location()),
                    }),
                    value: argument_value_from_node(&assoc.value()),
                    range: argument_value_range(visitor, &assoc.value()),
                })
            })
            .collect();
    }

    vec![argument_from_node(visitor, node)]
}

fn argument_from_node(visitor: &FactCollector, node: &Node) -> Argument {
    Argument {
        keyword: None,
        value: argument_value_from_node(node),
        range: argument_value_range(visitor, node),
    }
}

fn argument_value_from_node(node: &Node) -> ArgumentValue {
    if let Some(symbol) = node.as_symbol_node() {
        return ArgumentValue::Symbol(String::from_utf8_lossy(symbol.unescaped()).to_string());
    }
    if let Some(string) = node.as_string_node() {
        return ArgumentValue::String(String::from_utf8_lossy(string.unescaped()).to_string());
    }
    if let Some(constant) = node.as_constant_read_node() {
        return ArgumentValue::Constant(vec![
            utils::utf8_str(constant.name().as_slice()).to_string()
        ]);
    }
    if let Some(path) = node.as_constant_path_node() {
        let mut parts = Vec::new();
        utils::collect_namespaces(&path, &mut parts);
        return ArgumentValue::Constant(parts.iter().map(ToString::to_string).collect());
    }
    if node.as_true_node().is_some() {
        ArgumentValue::Boolean(true)
    } else if node.as_false_node().is_some() {
        ArgumentValue::Boolean(false)
    } else if node.as_nil_node().is_some() {
        ArgumentValue::Nil
    } else {
        ArgumentValue::Unsupported
    }
}

fn argument_value_range(visitor: &FactCollector, node: &Node) -> SourceRange {
    if let Some(string) = node.as_string_node() {
        source_range(visitor, &string.content_loc())
    } else {
        source_range(visitor, &node.location())
    }
}

fn apply_patch(visitor: &mut FactCollector, call: &CallNode, patch: IndexPatch) {
    match &patch {
        IndexPatch::DefineNamespace(namespace) => {
            let parts = namespace
                .namespace
                .iter()
                .map(|part| RubyConstant::new(part).expect(
                    "INVARIANT VIOLATED: extension namespace reached application without validation. This is a bug because guest patches must be validated before conflict resolution. Fix: keep validate_index_patch_payloads before emitted patch collection.",
                ))
                .collect::<Vec<_>>();
            let range = visitor
                .document
                .lsp_range_to_text_range(range_from_abi(namespace.location));
            visitor.direct_push_namespace_facts(
                FullyQualifiedName::namespace(parts),
                match namespace.kind {
                    ruby_fast_lsp_extension_api::NamespaceDeclarationKind::Class => {
                        GraphNodeKind::Class
                    }
                    ruby_fast_lsp_extension_api::NamespaceDeclarationKind::Module => {
                        GraphNodeKind::Module
                    }
                },
                range,
                range,
            );
        }
        IndexPatch::DefineConstant(constant) => {
            let mut parts = constant
                .namespace
                .iter()
                .map(|part| RubyConstant::new(part).expect(
                    "INVARIANT VIOLATED: extension constant namespace reached application without validation. This is a bug because guest patches must be validated before conflict resolution. Fix: keep validate_index_patch_payloads before emitted patch collection.",
                ))
                .collect::<Vec<_>>();
            parts.push(RubyConstant::new(&constant.name).expect(
                "INVARIANT VIOLATED: extension constant name reached application without validation. This is a bug because guest patches must be validated before conflict resolution. Fix: keep validate_index_patch_payloads before emitted patch collection.",
            ));
            let fqn = FullyQualifiedName::constant(parts);
            let range = visitor
                .document
                .lsp_range_to_text_range(range_from_abi(constant.location));
            visitor.direct_facts.symbols.push(SymbolFact::new(
                fqn.clone(),
                AnalysisSymbolKind::Constant,
                range,
            ));
            if let Some(ruby_type) =
                analysis_ruby_type_from_extension(constant.ruby_type.as_ref()).expect(
                    "INVARIANT VIOLATED: extension constant type reached application without validation. This is a bug because guest patches must be validated before conflict resolution. Fix: keep validate_index_patch_payloads before emitted patch collection.",
                )
            {
                let fact = TypeFact::new(
                    TypeSubject::Constant(fqn),
                    ruby_type,
                    range,
                    TypeProvenance::Extension,
                );
                visitor.type_store.add(fact.clone());
                visitor.direct_facts.types.push(fact);
            }
        }
        IndexPatch::AddReference(reference) => {
            let range = visitor
                .document
                .lsp_range_to_text_range(range_from_abi(reference.location));
            let target = match &reference.target {
                ruby_fast_lsp_extension_api::ReferenceTarget::Namespace(namespace) => {
                    FullyQualifiedName::namespace(
                        namespace
                            .iter()
                            .map(|part| RubyConstant::new(part).expect(
                                "INVARIANT VIOLATED: extension reference namespace reached application without validation. This is a bug because guest patches must be validated before conflict resolution. Fix: keep validate_index_patch_payloads before emitted patch collection.",
                            ))
                            .collect::<Vec<_>>(),
                    )
                }
                ruby_fast_lsp_extension_api::ReferenceTarget::Constant { namespace, name } => {
                    let mut parts = namespace
                        .iter()
                        .map(|part| RubyConstant::new(part).expect(
                            "INVARIANT VIOLATED: extension reference constant namespace reached application without validation. This is a bug because guest patches must be validated before conflict resolution. Fix: keep validate_index_patch_payloads before emitted patch collection.",
                        ))
                        .collect::<Vec<_>>();
                    parts.push(RubyConstant::new(name).expect(
                        "INVARIANT VIOLATED: extension reference constant name reached application without validation. This is a bug because guest patches must be validated before conflict resolution. Fix: keep validate_index_patch_payloads before emitted patch collection.",
                    ));
                    FullyQualifiedName::constant(parts)
                }
                ruby_fast_lsp_extension_api::ReferenceTarget::Method {
                    namespace,
                    owner_kind,
                    name,
                } => {
                    let owner = namespace
                        .iter()
                        .map(|part| RubyConstant::new(part).expect(
                            "INVARIANT VIOLATED: extension method reference namespace reached application without validation. This is a bug because guest patches must be validated before conflict resolution. Fix: keep validate_index_patch_payloads before patch application.",
                        ))
                        .collect::<Vec<_>>();
                    let method = RubyMethod::new(name).expect(
                        "INVARIANT VIOLATED: extension method reference name reached application without validation. This is a bug because guest patches must be validated before conflict resolution. Fix: keep validate_index_patch_payloads before patch application.",
                    );
                    let owner_kind = match owner_kind {
                        AbiNamespaceKind::Instance => NamespaceKind::Instance,
                        AbiNamespaceKind::Singleton => NamespaceKind::Singleton,
                    };
                    visitor.reference_candidates.push(ReferenceCandidate::method_target(
                        range,
                        owner,
                        owner_kind,
                        method,
                        None,
                    ));
                    return;
                }
            };
            visitor
                .reference_candidates
                .push(ReferenceCandidate::resolved(range, target, None));
        }
        IndexPatch::DefineMethod(method) => {
            let declared_return_type = analysis_ruby_type_from_extension(method.return_type.as_ref())
                .expect("INVARIANT VIOLATED: extension return type reached application without validation. This is a bug because guest patches must be validated before conflict resolution. Fix: keep validate_index_patch_payloads before emitted patch collection.");
            let inferred_return_type = match method.return_type_source {
                Some(ruby_fast_lsp_extension_api::MethodReturnTypeSource::Block) => {
                    visitor.infer_call_block_return_type(call)
                }
                None => None,
            };
            let return_type = inferred_return_type.clone().or(declared_return_type);
            let (namespace, owner_kind) = resolved_patch_owner(
                method.owner_target.as_ref(),
                &method.namespace,
                method.owner_kind,
                &method.source.extension_id,
                visitor.document.uri.as_str(),
                visitor
                    .extension_project_context
                    .as_ref()
                    .map(|project| project.project_uri.as_str()),
                "method owner",
            );
            let ruby_method = RubyMethod::new(&method.name).expect(
                "INVARIANT VIOLATED: extension method name reached application without validation. This is a bug because guest patches must be validated before conflict resolution. Fix: keep validate_index_patch_payloads before emitted patch collection.",
            );
            let fqn = FullyQualifiedName::method(namespace, ruby_method);
            let range = visitor
                .document
                .lsp_range_to_text_range(range_from_abi(method.location));
            visitor.direct_push_method_fact_with_visibility(
                fqn.namespace_parts(),
                owner_kind,
                ruby_method,
                range,
                match method.visibility {
                    ruby_fast_lsp_extension_api::MethodVisibility::Public => {
                        AnalysisMethodVisibility::Public
                    }
                    ruby_fast_lsp_extension_api::MethodVisibility::Protected => {
                        AnalysisMethodVisibility::Protected
                    }
                    ruby_fast_lsp_extension_api::MethodVisibility::Private => {
                        AnalysisMethodVisibility::Private
                    }
                },
            );
            if let Some(return_type) = return_type {
                let type_fact = TypeFact::new(
                    TypeSubject::MethodReturn(fqn),
                    return_type,
                    range,
                    TypeProvenance::Extension,
                );
                visitor.type_store.add(type_fact.clone());
                if inferred_return_type.is_some()
                    && !visitor.direct_facts.types.contains(&type_fact)
                {
                    visitor.direct_facts.types.push(type_fact);
                }
            }
        }
        IndexPatch::SetSuperclass(_) => {}
        IndexPatch::ApplyMixin(_) => {}
        IndexPatch::ConnectExecutionContext(_) => {}
    }
    visitor.extension_index_patches.push(patch);
}

fn resolved_patch_owner(
    target: Option<&ExecutionContextTarget>,
    fallback_namespace: &[String],
    fallback_kind: AbiNamespaceKind,
    extension_id: &str,
    source_identity: &str,
    project_identity: Option<&str>,
    label: &str,
) -> (Vec<RubyConstant>, NamespaceKind) {
    match target {
        None => (
            extension_ruby_constants(fallback_namespace, label),
            namespace_kind_from_abi(fallback_kind),
        ),
        Some(ExecutionContextTarget::Namespace {
            namespace,
            owner_kind,
        }) => (
            extension_ruby_constants(namespace, label),
            namespace_kind_from_abi(*owner_kind),
        ),
        Some(ExecutionContextTarget::GeneratedOwner {
            local_id,
            owner_kind,
        }) => {
            let owner = GeneratedOwnerId::new(extension_id, source_identity, local_id).expect(
                "INVARIANT VIOLATED: invalid generated patch owner reached application. This is a bug because semantic patch owners must be validated before conversion. Fix: keep validate_patch_owner_target before apply_patch.",
            );
            (
                vec![RubyConstant::generated_owner(owner)],
                owner_kind
                    .map(namespace_kind_from_abi)
                    .unwrap_or_else(|| namespace_kind_from_abi(fallback_kind)),
            )
        }
        Some(ExecutionContextTarget::ProjectGeneratedOwner {
            local_id,
            owner_kind,
        }) => {
            let identity = generated_owner_scope_identity(
                GeneratedOwnerScope::Project,
                source_identity,
                project_identity,
                label,
            );
            let owner = GeneratedOwnerId::new(extension_id, identity, local_id).expect(
                "INVARIANT VIOLATED: invalid project-generated patch owner reached application. This is a bug because semantic patch owners must be validated before conversion. Fix: keep validation before apply_patch.",
            );
            (
                vec![RubyConstant::generated_owner(owner)],
                owner_kind
                    .map(namespace_kind_from_abi)
                    .unwrap_or_else(|| namespace_kind_from_abi(fallback_kind)),
            )
        }
    }
}

fn response_patch_to_document_symbol(
    patch: ResponsePatch,
) -> Result<Option<DocumentSymbol>, String> {
    let ResponsePatch::DocumentSymbol(symbol) = patch else {
        return Ok(None);
    };

    Ok(Some(DocumentSymbol {
        name: symbol.name,
        detail: symbol.detail,
        kind: symbol_kind_from_extension(&symbol.kind)?,
        tags: None,
        #[allow(deprecated)]
        deprecated: None,
        range: range_from_abi(symbol.range),
        selection_range: range_from_abi(symbol.selection_range),
        children: None,
    }))
}

fn response_patch_to_code_lens(patch: ResponsePatch) -> Result<Option<CodeLens>, String> {
    let ResponsePatch::CodeLens(lens) = patch else {
        return Ok(None);
    };

    Ok(Some(CodeLens {
        range: range_from_abi(lens.range),
        command: Some(Command {
            title: lens.title,
            command: lens.command,
            arguments: Some(
                lens.arguments
                    .into_iter()
                    .map(serde_json::Value::String)
                    .collect(),
            ),
        }),
        data: None,
    }))
}

fn range_from_abi(range: SourceRange) -> Range {
    Range::new(
        Position::new(range.start.line, range.start.character),
        Position::new(range.end.line, range.end.character),
    )
}

fn symbol_kind_from_extension(kind: &str) -> Result<SymbolKind, String> {
    let symbol_kind = match kind {
        "File" => SymbolKind::FILE,
        "Module" => SymbolKind::MODULE,
        "Namespace" => SymbolKind::NAMESPACE,
        "Package" => SymbolKind::PACKAGE,
        "Class" => SymbolKind::CLASS,
        "Method" => SymbolKind::METHOD,
        "Property" => SymbolKind::PROPERTY,
        "Field" => SymbolKind::FIELD,
        "Constructor" => SymbolKind::CONSTRUCTOR,
        "Enum" => SymbolKind::ENUM,
        "Interface" => SymbolKind::INTERFACE,
        "Function" => SymbolKind::FUNCTION,
        "Variable" => SymbolKind::VARIABLE,
        "Constant" => SymbolKind::CONSTANT,
        "String" => SymbolKind::STRING,
        "Number" => SymbolKind::NUMBER,
        "Boolean" => SymbolKind::BOOLEAN,
        "Array" => SymbolKind::ARRAY,
        "Object" => SymbolKind::OBJECT,
        "Key" => SymbolKind::KEY,
        "Null" => SymbolKind::NULL,
        "EnumMember" => SymbolKind::ENUM_MEMBER,
        "Struct" => SymbolKind::STRUCT,
        "Event" => SymbolKind::EVENT,
        "Operator" => SymbolKind::OPERATOR,
        "TypeParameter" => SymbolKind::TYPE_PARAMETER,
        other => return Err(format!("unsupported document symbol kind `{}`", other)),
    };
    Ok(symbol_kind)
}

fn source_range(visitor: &FactCollector, location: &ruby_prism::Location) -> SourceRange {
    let range = visitor.document.prism_location_to_lsp_range(location);
    SourceRange {
        start: source_position(range.start),
        end: source_position(range.end),
    }
}

fn source_position(position: Position) -> SourcePosition {
    SourcePosition {
        line: position.line,
        character: position.character,
    }
}

fn namespace_kind_to_abi(kind: NamespaceKind) -> AbiNamespaceKind {
    match kind {
        NamespaceKind::Instance => AbiNamespaceKind::Instance,
        NamespaceKind::Singleton => AbiNamespaceKind::Singleton,
    }
}

fn is_literal(node: &Node) -> bool {
    node.as_string_node().is_some()
        || node.as_interpolated_string_node().is_some()
        || node.as_integer_node().is_some()
        || node.as_float_node().is_some()
        || node.as_symbol_node().is_some()
        || node.as_array_node().is_some()
        || node.as_hash_node().is_some()
        || node.as_true_node().is_some()
        || node.as_false_node().is_some()
        || node.as_nil_node().is_some()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;
    use tower_lsp::lsp_types::{
        DidChangeWatchedFilesParams, DidChangeWorkspaceFoldersParams, FileChangeType, FileEvent,
        InitializeParams, Url, WorkspaceFolder, WorkspaceFoldersChangeEvent,
    };
    use tower_lsp::LanguageServer;

    use super::*;
    use crate::server::RubyLanguageServer;

    fn copy_rspec_package(destination: &Path, version: &str) {
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("extensions/rspec-ruby");
        let wasm_relative = Path::new("target/wasm32-wasip1/release/rspec-ruby.wasm");
        fs::create_dir_all(destination.join(wasm_relative).parent().unwrap())
            .expect("test package wasm directory must be created");
        let manifest = fs::read_to_string(source.join("extension.toml"))
            .expect("bundled RSpec manifest must be readable")
            .replace("version = \"0.1.0\"", &format!("version = \"{version}\""));
        fs::write(destination.join("extension.toml"), manifest)
            .expect("test manifest must be written");
        fs::copy(source.join(wasm_relative), destination.join(wasm_relative))
            .expect("bundled RSpec wasm must be copied");
    }

    fn write_cacheable_extension_package(destination: &Path) -> Vec<u8> {
        fs::create_dir_all(destination).expect("test package directory must be created");
        let wasm = wat::parse_str(
            r#"
            (module
              (memory (export "memory") 1)
              (data (i32.const 1024) "[]")
              (data (i32.const 2048) "{\22index_patches\22:[],\22response_patches\22:[],\22command_patches\22:[]}")
              (func (export "alloc") (param $len i32) (result i32)
                i32.const 4096)
              (func (export "dealloc") (param $ptr i32) (param $len i32))
              (func (export "abi_version") (result i32)
                i32.const 1)
              (func (export "indexed_call_names") (result i64)
                i64.const 4398046511106)
              (func (export "index_call") (param $ptr i32) (param $len i32) (result i64)
                i64.const 4398046511106)
              (func (export "handle_event") (param $ptr i32) (param $len i32) (result i64)
                i64.const 8796093022271)
            )
            "#,
        )
        .expect("test cacheable Wasm must compile");
        fs::write(destination.join("extension.wasm"), &wasm)
            .expect("test cacheable Wasm must be written");
        fs::write(
            destination.join("extension.toml"),
            r#"
id = "cacheable-extension"
name = "Cacheable Extension"
version = "0.1.0"
abi_version = 1
server_version = ">=0.2.0, <0.3.0"
runtime = "mruby-wasm"
wasm = "extension.wasm"
capabilities = []
permissions = []

[indexing]
call_names = []
"#,
        )
        .expect("test cacheable manifest must be written");
        wasm
    }

    fn write_activation_failure_package(destination: &Path) {
        fs::create_dir_all(destination).expect("test package directory must be created");
        let wasm = wat::parse_str(
            r#"
            (module
              (memory (export "memory") 1)
              (data (i32.const 1024) "[]")
              (func (export "alloc") (param $len i32) (result i32)
                i32.const 4096)
              (func (export "dealloc") (param $ptr i32) (param $len i32))
              (func (export "abi_version") (result i32)
                i32.const 1)
              (func (export "indexed_call_names") (result i64)
                i64.const 4398046511106)
              (func (export "index_call") (param $ptr i32) (param $len i32) (result i64)
                i64.const 4398046511106)
              (func (export "handle_event") (param $ptr i32) (param $len i32) (result i64)
                i64.const 0)
            )
            "#,
        )
        .expect("test lifecycle Wasm must compile");
        fs::write(destination.join("extension.wasm"), wasm)
            .expect("test lifecycle Wasm must be written");
        fs::write(
            destination.join("extension.toml"),
            r#"
id = "activation-failure"
name = "Activation Failure"
version = "0.1.0"
abi_version = 1
server_version = ">=0.2.0, <0.3.0"
runtime = "mruby-wasm"
wasm = "extension.wasm"
capabilities = []
permissions = []

[indexing]
call_names = []
"#,
        )
        .expect("test lifecycle manifest must be written");
    }

    fn write_resource_limit_failure_package(destination: &Path) {
        fs::create_dir_all(destination).expect("test package directory must be created");
        let wasm = wat::parse_str(
            r#"
            (module
              (memory (export "memory") 1)
              (data (i32.const 1024) "[]")
              (func (export "alloc") (param $len i32) (result i32)
                i32.const 4096)
              (func (export "dealloc") (param $ptr i32) (param $len i32))
              (func (export "abi_version") (result i32)
                i32.const 1)
              (func (export "indexed_call_names") (result i64)
                i64.const 4398046511106)
              (func (export "index_call") (param $ptr i32) (param $len i32) (result i64)
                i64.const 4398046511106)
              (func (export "handle_event") (param $ptr i32) (param $len i32) (result i64)
                i64.const 262145)
            )
            "#,
        )
        .expect("test resource-limit Wasm must compile");
        fs::write(destination.join("extension.wasm"), wasm)
            .expect("test resource-limit Wasm must be written");
        fs::write(
            destination.join("extension.toml"),
            r#"
id = "resource-limit-failure"
name = "Resource Limit Failure"
version = "0.1.0"
abi_version = 1
server_version = ">=0.2.0, <0.3.0"
runtime = "wasm"
wasm = "extension.wasm"
capabilities = []
permissions = []

[indexing]
call_names = []
"#,
        )
        .expect("test resource-limit manifest must be written");
    }

    fn write_trap_failure_package(destination: &Path) {
        fs::create_dir_all(destination).expect("test package directory must be created");
        let wasm = wat::parse_str(
            r#"
            (module
              (memory (export "memory") 1)
              (data (i32.const 1024) "[]")
              (func (export "alloc") (param $len i32) (result i32)
                i32.const 4096)
              (func (export "dealloc") (param $ptr i32) (param $len i32))
              (func (export "abi_version") (result i32)
                i32.const 1)
              (func (export "indexed_call_names") (result i64)
                i64.const 4398046511106)
              (func (export "index_call") (param $ptr i32) (param $len i32) (result i64)
                i64.const 4398046511106)
              (func (export "handle_event") (param $ptr i32) (param $len i32) (result i64)
                unreachable)
            )
            "#,
        )
        .expect("test trap Wasm must compile");
        fs::write(destination.join("extension.wasm"), wasm)
            .expect("test trap Wasm must be written");
        fs::write(
            destination.join("extension.toml"),
            r#"
id = "trap-failure"
name = "Trap Failure"
version = "0.1.0"
abi_version = 1
server_version = ">=0.2.0, <0.3.0"
runtime = "wasm"
wasm = "extension.wasm"
capabilities = []
permissions = []

[indexing]
call_names = []
"#,
        )
        .expect("test trap manifest must be written");
    }

    fn write_settings_failure_package(destination: &Path) {
        fs::create_dir_all(destination).expect("test package directory must be created");
        let wasm = wat::parse_str(
            r#"
            (module
              (memory (export "memory") 1)
              (data (i32.const 1024) "[]")
              (data (i32.const 2048) "{\"index_patches\":[],\"response_patches\":[],\"command_patches\":[]}")
              (func (export "alloc") (param $len i32) (result i32)
                i32.const 4096)
              (func (export "dealloc") (param $ptr i32) (param $len i32))
              (func (export "abi_version") (result i32)
                i32.const 1)
              (func (export "indexed_call_names") (result i64)
                i64.const 4398046511106)
              (func (export "index_call") (param $ptr i32) (param $len i32) (result i64)
                i64.const 4398046511106)
              (func (export "handle_event") (param $ptr i32) (param $len i32) (result i64)
                local.get $ptr
                i32.const 10
                i32.add
                i32.load8_u
                i32.const 115
                i32.eq
                if (result i64)
                  i64.const 0
                else
                  i64.const 8796093022271
                end)
            )
            "#,
        )
        .expect("test settings Wasm must compile");
        fs::write(destination.join("extension.wasm"), wasm)
            .expect("test settings Wasm must be written");
        fs::write(
            destination.join("extension.toml"),
            r#"
id = "settings-failure"
name = "Settings Failure"
version = "0.1.0"
abi_version = 1
server_version = ">=0.2.0, <0.3.0"
runtime = "mruby-wasm"
wasm = "extension.wasm"
capabilities = []
permissions = []

[indexing]
call_names = []
"#,
        )
        .expect("test settings manifest must be written");
    }

    fn write_watched_file_failure_package(destination: &Path) {
        fs::create_dir_all(destination).expect("test package directory must be created");
        let wasm = wat::parse_str(
            r#"
            (module
              (memory (export "memory") 1)
              (data (i32.const 1024) "[]")
              (data (i32.const 2048) "{\"index_patches\":[],\"response_patches\":[],\"command_patches\":[]}")
              (func (export "alloc") (param $len i32) (result i32)
                i32.const 4096)
              (func (export "dealloc") (param $ptr i32) (param $len i32))
              (func (export "abi_version") (result i32)
                i32.const 1)
              (func (export "indexed_call_names") (result i64)
                i64.const 4398046511106)
              (func (export "index_call") (param $ptr i32) (param $len i32) (result i64)
                i64.const 4398046511106)
              (func (export "handle_event") (param $ptr i32) (param $len i32) (result i64)
                local.get $ptr
                i32.const 10
                i32.add
                i32.load8_u
                i32.const 102
                i32.eq
                if (result i64)
                  i64.const 0
                else
                  i64.const 8796093022271
                end)
            )
            "#,
        )
        .expect("test watched-file Wasm must compile");
        fs::write(destination.join("extension.wasm"), wasm)
            .expect("test watched-file Wasm must be written");
        fs::write(
            destination.join("extension.toml"),
            r#"
id = "watched-file-failure"
name = "Watched File Failure"
version = "0.1.0"
abi_version = 1
server_version = ">=0.2.0, <0.3.0"
runtime = "mruby-wasm"
wasm = "extension.wasm"
capabilities = ["watching"]
permissions = []

[indexing]
call_names = []

[watching]
globs = ["config/routes.rb"]
"#,
        )
        .expect("test watched-file manifest must be written");
    }

    #[test]
    fn extension_discovery_rejects_oversized_wasm_before_allocating_its_payload() {
        let temp_dir = TempDir::new().expect("test temp dir must be created");
        let wasm_path = temp_dir.path().join("oversized.wasm");
        let file = fs::File::create(&wasm_path).expect("sparse Wasm fixture must be created");
        file.set_len(MAX_EXTENSION_WASM_BYTES + 1)
            .expect("sparse Wasm fixture length must be set");

        let error = read_extension_wasm(&wasm_path)
            .expect_err("oversized Wasm must be rejected from metadata before payload allocation");

        assert!(error.to_string().contains("maximum is 67108864"));
    }

    #[test]
    fn activation_failure_disables_extension_before_use() {
        let temp_dir = TempDir::new().expect("test temp dir must be created");
        let package = temp_dir.path().join("activation-failure");
        write_activation_failure_package(&package);

        let registry = ExtensionRegistry::load(&ExtensionLoadConfig {
            package_paths: vec![ConfiguredExtensionPath {
                path: package,
                source: ExtensionPathSource::InitializationOptions,
            }],
            directory_paths: Vec::new(),
            project_package_paths: Vec::new(),
            settings: BTreeMap::new(),
        });

        let reports = registry.status_reports();
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].id, "activation-failure");
        assert_eq!(reports[0].status, "failed");
        assert_eq!(reports[0].telemetry.guest_calls, 1);
        assert_eq!(reports[0].telemetry.lifecycle_calls, 1);
        assert_eq!(reports[0].telemetry.index_calls, 0);
        assert_eq!(reports[0].telemetry.event_calls, 0);
        assert_eq!(reports[0].telemetry.guest_failures, 1);
        assert_eq!(reports[0].telemetry.disablements, 1);
        assert!(reports[0].telemetry.max_guest_time_ns <= reports[0].telemetry.total_guest_time_ns);
        assert!(
            reports[0]
                .last_error
                .as_deref()
                .is_some_and(|error| error.contains("activation")),
            "INVARIANT VIOLATED: activation failure was not reported with lifecycle context. \
             This is a bug because users cannot diagnose why an extension was disabled. \
             Fix: retain the activation error in extension status."
        );
    }

    #[test]
    fn resource_limit_failure_is_visible_in_extension_telemetry() {
        let temp_dir = TempDir::new().expect("test temp dir must be created");
        let package = temp_dir.path().join("resource-limit-failure");
        write_resource_limit_failure_package(&package);

        let registry = ExtensionRegistry::load(&ExtensionLoadConfig {
            package_paths: vec![ConfiguredExtensionPath {
                path: package,
                source: ExtensionPathSource::InitializationOptions,
            }],
            ..ExtensionLoadConfig::default()
        });

        let report = &registry.status_reports()[0];
        assert_eq!(report.status, "failed");
        assert_eq!(report.telemetry.lifecycle_calls, 1);
        assert_eq!(report.telemetry.guest_failures, 1);
        assert_eq!(report.telemetry.resource_limit_failures, 1);
        assert_eq!(report.telemetry.guest_traps, 0);
        assert_eq!(report.telemetry.disablements, 1);
        assert!(
            report.last_error.as_deref().is_some_and(
                |error| error.contains("output payload") && error.contains("exceeds max")
            )
        );
    }

    #[test]
    fn guest_trap_is_visible_in_extension_telemetry() {
        let temp_dir = TempDir::new().expect("test temp dir must be created");
        let package = temp_dir.path().join("trap-failure");
        write_trap_failure_package(&package);

        let registry = ExtensionRegistry::load(&ExtensionLoadConfig {
            package_paths: vec![ConfiguredExtensionPath {
                path: package,
                source: ExtensionPathSource::InitializationOptions,
            }],
            ..ExtensionLoadConfig::default()
        });

        let report = &registry.status_reports()[0];
        assert_eq!(report.status, "failed");
        assert_eq!(report.telemetry.lifecycle_calls, 1);
        assert_eq!(report.telemetry.guest_failures, 1);
        assert_eq!(report.telemetry.guest_traps, 1);
        assert_eq!(report.telemetry.resource_limit_failures, 0);
        assert_eq!(report.telemetry.disablements, 1);
        assert!(report
            .last_error
            .as_deref()
            .is_some_and(|error| error.contains("wasm trap") || error.contains("unreachable")));
    }

    #[test]
    fn settings_only_reconfiguration_notifies_existing_extension() {
        let temp_dir = TempDir::new().expect("test temp dir must be created");
        let package = temp_dir.path().join("settings-failure");
        write_settings_failure_package(&package);
        let mut config = RubyFastLspConfig {
            extension_packages: vec![package.to_string_lossy().into_owned()],
            ..RubyFastLspConfig::default()
        };
        let registry = ExtensionRegistryHandle::from_config(&config);
        assert_eq!(registry.status_reports()[0].status, "loaded");

        config.extension_settings.insert(
            "settings-failure".to_string(),
            serde_json::json!({"mode": "strict"}),
        );
        registry.configure_from_config(&config);

        let reports = registry.status_reports();
        assert_eq!(reports[0].status, "failed");
        assert!(
            reports[0]
                .last_error
                .as_deref()
                .is_some_and(|error| error.contains("settings.changed")),
            "INVARIANT VIOLATED: settings event failure lacks event context. \
             This is a bug because settings-only reload failures must be diagnosable. \
             Fix: report the settings.changed event in extension status."
        );

        config.extension_settings.insert(
            "settings-failure".to_string(),
            serde_json::json!({"mode": "relaxed"}),
        );
        registry.configure_from_config(&config);
        assert_eq!(
            registry.status_reports()[0].status,
            "loaded",
            "a failed extension must be recreated so corrected settings can recover it"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn extension_reconfiguration_waits_for_weighted_admission_without_blocking_reactor() {
        let temp_dir = TempDir::new().expect("test temp dir must be created");
        let package = temp_dir.path().join("rspec-ruby");
        copy_rspec_package(&package, "0.1.0-governed");
        let config = RubyFastLspConfig {
            extension_packages: vec![package.to_string_lossy().into_owned()],
            ..RubyFastLspConfig::default()
        };
        let registry = ExtensionRegistryHandle::from_config(&RubyFastLspConfig::default());
        let governor = IndexingResourceGovernor::new(
            crate::indexing_resources::IndexingResourcePolicy::with_limits(
                1,
                1,
                256 * 1024 * 1024,
                1,
            ),
        );
        let holder_release = Arc::new(tokio::sync::Notify::new());
        let holder_release_task = holder_release.clone();
        let holder_governor = governor.clone();
        let holder = tokio::spawn(async move {
            holder_governor
                .run_async_with_resources(
                    "extension reload contention holder",
                    IndexingWorkSpec::new(
                        None,
                        IndexingResourcePriority::Background,
                        1,
                        256 * 1024 * 1024,
                        1,
                    ),
                    None,
                    async move {
                        holder_release_task.notified().await;
                    },
                )
                .await
                .unwrap();
        });
        tokio::time::timeout(Duration::from_secs(1), async {
            while governor.snapshot().active_tasks != 1 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("resource holder must be admitted before extension reload");

        let reload_registry = registry.clone();
        let reload_governor = governor.clone();
        let reload = tokio::spawn(async move {
            reload_registry
                .configure_from_config_and_workspace_roots_governed(&config, &[], reload_governor)
                .await
                .unwrap();
        });
        tokio::time::timeout(Duration::from_secs(1), async {
            while governor.snapshot().queued_tasks != 1 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("extension reload must queue behind the complete weighted claim");
        tokio::time::timeout(
            Duration::from_millis(50),
            tokio::time::sleep(Duration::from_millis(1)),
        )
        .await
        .expect("queued extension reload must not block the current-thread Tokio reactor");
        assert!(
            !reload.is_finished(),
            "extension reload must not bypass weighted admission"
        );

        holder_release.notify_one();
        holder.await.unwrap();
        reload.await.unwrap();
        let reports = registry.status_reports();
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].version.as_deref(), Some("0.1.0-governed"));
        assert_eq!(reports[0].status, "loaded");
        let complete = governor.snapshot();
        assert_eq!(complete.active_tasks, 0);
        assert_eq!(complete.queued_tasks, 0);
        assert_eq!(complete.completed_tasks, 2);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn response_requests_without_loaded_capability_bypass_resource_admission() {
        let registry = ExtensionRegistryHandle::empty();
        let governor = IndexingResourceGovernor::new(
            crate::indexing_resources::IndexingResourcePolicy::with_limits(1, 1, 1, 1),
        );

        assert!(registry
            .document_symbols_governed(
                governor.clone(),
                None,
                "file:///workspace/plain.rb".to_string(),
                "class Plain\nend\n".to_string(),
                None,
            )
            .await
            .unwrap()
            .is_empty());
        assert!(registry
            .code_lenses_governed(
                governor.clone(),
                None,
                "file:///workspace/plain.rb".to_string(),
                "class Plain\nend\n".to_string(),
                None,
            )
            .await
            .unwrap()
            .is_empty());

        let snapshot = governor.snapshot();
        assert_eq!(snapshot.active_tasks, 0);
        assert_eq!(snapshot.queued_tasks, 0);
        assert_eq!(snapshot.completed_tasks, 0);
    }

    #[test]
    fn in_place_package_change_reloads_same_configured_path() {
        let temp_dir = TempDir::new().expect("test temp dir must be created");
        let package = temp_dir.path().join("rspec-ruby");
        copy_rspec_package(&package, "0.1.0");
        let config = RubyFastLspConfig {
            extension_packages: vec![package.to_string_lossy().into_owned()],
            ..RubyFastLspConfig::default()
        };
        let registry = ExtensionRegistryHandle::from_config(&config);
        let previous = registry.extensions()[0].clone();
        assert_eq!(
            registry.status_reports()[0].version.as_deref(),
            Some("0.1.0")
        );

        copy_rspec_package(&package, "0.2.0");
        registry.configure_from_config(&config);

        let reports = registry.status_reports();
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].status, "loaded");
        assert_eq!(
            reports[0].version.as_deref(),
            Some("0.2.0"),
            "changing a package in place must reload it even when configured paths are unchanged"
        );
        assert_eq!(
            previous.status_report().status,
            "deactivated",
            "the replaced guest must receive lifecycle.deactivate after the replacement is active"
        );
    }

    #[test]
    fn fresh_registry_process_reuses_exact_persistent_compiled_wasm() {
        let temp_dir = TempDir::new().expect("test temp dir must be created");
        let package = temp_dir.path().join("cacheable-extension");
        write_cacheable_extension_package(&package);
        let config = RubyFastLspConfig {
            extension_packages: vec![package.to_string_lossy().into_owned()],
            ..RubyFastLspConfig::default()
        };
        let cache_root = temp_dir.path().join("cache");
        let first_cache =
            PersistentDerivedProductCache::with_limits(cache_root.clone(), 8, 16 * 1024 * 1024);
        let first_registry = ExtensionRegistryHandle::empty_with_cache(first_cache.clone());
        first_registry.configure_from_config(&config);
        assert_eq!(first_registry.status_reports()[0].status, "loaded");
        assert_eq!(first_cache.compiled_wasm_snapshot().producers, 1);
        assert_eq!(first_cache.compiled_wasm_snapshot().publications, 1);
        first_registry.shutdown();
        drop(first_registry);
        drop(first_cache);

        let second_cache =
            PersistentDerivedProductCache::with_limits(cache_root, 8, 16 * 1024 * 1024);
        let second_registry = ExtensionRegistryHandle::empty_with_cache(second_cache.clone());
        second_registry.configure_from_config(&config);
        assert_eq!(second_registry.status_reports()[0].status, "loaded");
        assert_eq!(second_cache.compiled_wasm_snapshot().hits, 1);
        assert_eq!(second_cache.compiled_wasm_snapshot().producers, 0);
    }

    #[test]
    fn valid_envelope_with_invalid_native_wasm_artifact_is_rebuilt() {
        let temp_dir = TempDir::new().expect("test temp dir must be created");
        let package = temp_dir.path().join("cacheable-extension");
        let wasm = write_cacheable_extension_package(&package);
        let compiler = ruby_fast_lsp_extension_wasm_host::WasmExtensionCompiler::new().unwrap();
        let key = CompiledWasmProductKey::new(&wasm, compiler.cache_identity());
        let cache_root = temp_dir.path().join("cache");
        let seeding_cache =
            PersistentDerivedProductCache::with_limits(cache_root.clone(), 8, 16 * 1024 * 1024);
        let PersistentCompiledWasmLookup::Reservation(reservation) =
            seeding_cache.lookup_compiled_wasm_or_reserve(&key).unwrap()
        else {
            panic!("test compiled Wasm cache must begin empty");
        };
        reservation
            .publish(&key, b"not a Wasmtime serialized module")
            .unwrap();
        drop(seeding_cache);

        let recovering_cache =
            PersistentDerivedProductCache::with_limits(cache_root, 8, 16 * 1024 * 1024);
        let registry = ExtensionRegistryHandle::empty_with_cache(recovering_cache.clone());
        registry.configure_from_config(&RubyFastLspConfig {
            extension_packages: vec![package.to_string_lossy().into_owned()],
            ..RubyFastLspConfig::default()
        });
        assert_eq!(registry.status_reports()[0].status, "loaded");
        let snapshot = recovering_cache.compiled_wasm_snapshot();
        assert_eq!(snapshot.hits, 1);
        assert_eq!(snapshot.corruptions, 1);
        assert_eq!(snapshot.producers, 1);
        assert_eq!(snapshot.publications, 1);
    }

    #[test]
    fn shutdown_deactivates_loaded_extensions() {
        let temp_dir = TempDir::new().expect("test temp dir must be created");
        let package = temp_dir.path().join("rspec-ruby");
        copy_rspec_package(&package, "0.1.0");
        let config = RubyFastLspConfig {
            extension_packages: vec![package.to_string_lossy().into_owned()],
            ..RubyFastLspConfig::default()
        };
        let registry = ExtensionRegistryHandle::from_config(&config);
        assert_eq!(registry.status_reports()[0].status, "loaded");

        registry.shutdown();

        assert_eq!(registry.status_reports()[0].status, "deactivated");
    }

    #[tokio::test]
    async fn trusted_workspace_discovers_project_local_extension_package() {
        let temp_dir = TempDir::new().expect("test temp dir must be created");
        let package = temp_dir.path().join(".ruby-fast-lsp/extensions/rspec-ruby");
        copy_rspec_package(&package, "0.1.0-project");
        let root_uri = Url::from_directory_path(temp_dir.path())
            .expect("test workspace path must convert to a file URI");
        let server = RubyLanguageServer::default();

        server
            .initialize(InitializeParams {
                root_uri: Some(root_uri),
                initialization_options: Some(serde_json::json!({
                    "workspaceTrusted": true,
                    "projectExtensionsEnabled": true
                })),
                ..InitializeParams::default()
            })
            .await
            .expect("test server initialization must succeed");

        let reports = server.extension_registry.status_reports();
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].id, "rspec-ruby");
        assert_eq!(reports[0].version.as_deref(), Some("0.1.0-project"));
        assert_eq!(reports[0].status, "loaded");
    }

    #[tokio::test]
    async fn dynamic_workspace_change_reconfigures_project_extensions() {
        let temp_dir = TempDir::new().expect("test temp dir must be created");
        copy_rspec_package(
            &temp_dir.path().join(".ruby-fast-lsp/extensions/rspec-ruby"),
            "0.1.0-dynamic-lsp",
        );
        let root_uri = Url::from_directory_path(temp_dir.path())
            .expect("test workspace path must convert to a file URI");
        let folder = WorkspaceFolder {
            uri: root_uri,
            name: "dynamic".to_string(),
        };
        let server = RubyLanguageServer::default();
        server
            .initialize(InitializeParams {
                initialization_options: Some(serde_json::json!({
                    "workspaceTrusted": true,
                    "projectExtensionsEnabled": true
                })),
                ..InitializeParams::default()
            })
            .await
            .expect("test server initialization must succeed");

        server
            .did_change_workspace_folders(DidChangeWorkspaceFoldersParams {
                event: WorkspaceFoldersChangeEvent {
                    added: vec![folder.clone()],
                    removed: Vec::new(),
                },
            })
            .await;
        assert_eq!(
            server.extension_registry.status_reports()[0]
                .version
                .as_deref(),
            Some("0.1.0-dynamic-lsp")
        );

        server
            .did_change_workspace_folders(DidChangeWorkspaceFoldersParams {
                event: WorkspaceFoldersChangeEvent {
                    added: Vec::new(),
                    removed: vec![folder],
                },
            })
            .await;
        assert!(server.extension_registry.status_reports().is_empty());
    }

    #[test]
    fn untrusted_or_disabled_workspace_does_not_discover_project_extensions() {
        let temp_dir = TempDir::new().expect("test temp dir must be created");
        let package = temp_dir.path().join(".ruby-fast-lsp/extensions/rspec-ruby");
        copy_rspec_package(&package, "0.1.0-project");

        let untrusted = RubyFastLspConfig {
            workspace_trusted: false,
            project_extensions_enabled: true,
            ..RubyFastLspConfig::default()
        };
        let disabled = RubyFastLspConfig {
            workspace_trusted: true,
            project_extensions_enabled: false,
            ..RubyFastLspConfig::default()
        };

        for config in [&untrusted, &disabled] {
            let load_config = ExtensionLoadConfig::from_config_and_workspace_roots(
                config,
                &[temp_dir.path().to_path_buf()],
            );
            assert!(
                load_config.project_package_paths.is_empty(),
                "project-local Wasm must require both explicit trust and enablement"
            );
            assert!(ExtensionRegistry::load(&load_config)
                .status_reports()
                .is_empty());
        }
    }

    #[test]
    fn conventional_project_tree_is_discovered_recursively() {
        let temp_dir = TempDir::new().expect("test temp dir must be created");
        let package = temp_dir
            .path()
            .join("ruby_fast_lsp/frameworks/testing/rspec-ruby");
        copy_rspec_package(&package, "0.1.0-conventional");
        let config = RubyFastLspConfig {
            workspace_trusted: true,
            project_extensions_enabled: true,
            ..RubyFastLspConfig::default()
        };

        let registry =
            ExtensionRegistry::load(&ExtensionLoadConfig::from_config_and_workspace_roots(
                &config,
                &[temp_dir.path().to_path_buf()],
            ));

        assert_eq!(
            registry.status_reports()[0].version.as_deref(),
            Some("0.1.0-conventional")
        );
    }

    #[test]
    fn explicit_package_wins_over_project_local_duplicate() {
        let temp_dir = TempDir::new().expect("test temp dir must be created");
        let explicit = temp_dir.path().join("explicit-rspec");
        let project = temp_dir
            .path()
            .join(".ruby-fast-lsp/extensions/project-rspec");
        copy_rspec_package(&explicit, "0.1.0-explicit");
        copy_rspec_package(&project, "0.1.0-project");
        let config = RubyFastLspConfig {
            extension_packages: vec![explicit.to_string_lossy().into_owned()],
            workspace_trusted: true,
            project_extensions_enabled: true,
            ..RubyFastLspConfig::default()
        };

        let registry =
            ExtensionRegistry::load(&ExtensionLoadConfig::from_config_and_workspace_roots(
                &config,
                &[temp_dir.path().to_path_buf()],
            ));

        assert_eq!(registry.status_reports().len(), 1);
        assert_eq!(
            registry.status_reports()[0].version.as_deref(),
            Some("0.1.0-explicit")
        );
    }

    #[test]
    fn project_duplicate_tie_break_is_filesystem_path_not_root_order() {
        let temp_dir = TempDir::new().expect("test temp dir must be created");
        let root_a = temp_dir.path().join("a-root");
        let root_z = temp_dir.path().join("z-root");
        copy_rspec_package(
            &root_a.join(".ruby-fast-lsp/extensions/rspec-ruby"),
            "0.1.0-a",
        );
        copy_rspec_package(
            &root_z.join(".ruby-fast-lsp/extensions/rspec-ruby"),
            "0.1.0-z",
        );
        let config = RubyFastLspConfig {
            workspace_trusted: true,
            project_extensions_enabled: true,
            ..RubyFastLspConfig::default()
        };

        let registry = ExtensionRegistry::load(
            &ExtensionLoadConfig::from_config_and_workspace_roots(&config, &[root_z, root_a]),
        );

        assert_eq!(registry.status_reports().len(), 1);
        assert_eq!(
            registry.status_reports()[0].version.as_deref(),
            Some("0.1.0-a")
        );
    }

    #[test]
    fn workspace_root_changes_add_and_remove_project_extensions() {
        let temp_dir = TempDir::new().expect("test temp dir must be created");
        copy_rspec_package(
            &temp_dir.path().join(".ruby-fast-lsp/extensions/rspec-ruby"),
            "0.1.0-dynamic",
        );
        let config = RubyFastLspConfig {
            workspace_trusted: true,
            project_extensions_enabled: true,
            ..RubyFastLspConfig::default()
        };
        let registry = ExtensionRegistryHandle::from_config(&config);
        assert!(registry.status_reports().is_empty());

        registry
            .configure_from_config_and_workspace_roots(&config, &[temp_dir.path().to_path_buf()]);
        assert_eq!(registry.status_reports()[0].status, "loaded");

        registry.configure_from_config_and_workspace_roots(&config, &[]);
        assert!(registry.status_reports().is_empty());
    }

    #[tokio::test]
    async fn matching_watched_file_change_is_routed_to_manifest_extension() {
        let temp_dir = TempDir::new().expect("test temp dir must be created");
        let package = temp_dir.path().join("watched-file-failure");
        write_watched_file_failure_package(&package);
        let root_uri = Url::from_directory_path(temp_dir.path())
            .expect("test workspace path must convert to a file URI");
        let config = RubyFastLspConfig {
            extension_packages: vec![package.to_string_lossy().into_owned()],
            ..RubyFastLspConfig::default()
        };
        let server = RubyLanguageServer::default();
        server.add_workspace(root_uri);
        server.extension_registry.configure_from_config(&config);
        assert_eq!(
            server.extension_registry.status_reports()[0].status,
            "loaded"
        );

        crate::handlers::notification::handle_did_change_watched_files(
            &server,
            DidChangeWatchedFilesParams {
                changes: vec![FileEvent::new(
                    Url::from_file_path(temp_dir.path().join("README.md"))
                        .expect("test nonmatching path must convert to URI"),
                    FileChangeType::CHANGED,
                )],
            },
        )
        .await;
        assert_eq!(
            server.extension_registry.status_reports()[0].status,
            "loaded"
        );

        crate::handlers::notification::handle_did_change_watched_files(
            &server,
            DidChangeWatchedFilesParams {
                changes: vec![FileEvent::new(
                    Url::from_file_path(temp_dir.path().join("config/routes.rb"))
                        .expect("test matching path must convert to URI"),
                    FileChangeType::CHANGED,
                )],
            },
        )
        .await;

        let report = &server.extension_registry.status_reports()[0];
        assert_eq!(report.status, "failed");
        assert!(
            report
                .last_error
                .as_deref()
                .is_some_and(|error| error.contains("files.changed")),
            "INVARIANT VIOLATED: watched-file failure lacks event context. This is a bug because extension watcher failures must be diagnosable. Fix: retain files.changed in extension status."
        );
    }

    #[test]
    fn watched_file_candidates_use_deepest_root_and_deduplicate() {
        let root = PathBuf::from("/workspace");
        let nested = root.join("engines/payments");
        let uri = Url::from_file_path(nested.join("config/routes.rb"))
            .expect("test watched path must convert to URI");
        let event = FileEvent::new(uri.clone(), FileChangeType::CHANGED);

        let candidates = watched_file_candidates(&[root, nested.clone()], &[event.clone(), event]);

        assert_eq!(candidates.len(), 1);
        assert_eq!(
            candidates[0].workspace_root,
            nested.to_string_lossy().replace('\\', "/")
        );
        assert_eq!(candidates[0].path, "config/routes.rb");
        assert_eq!(candidates[0].uri, uri.to_string());
        assert_eq!(candidates[0].kind, WatchedFileChangeKind::Changed);
    }

    #[test]
    fn watched_file_globs_must_be_valid_workspace_relative_patterns() {
        let matcher = build_watched_file_matcher(
            "watch-test",
            &["config/**/*.rb".to_string(), ".rubocop.yml".to_string()],
        )
        .expect("valid watcher globs must compile");
        assert!(matcher.is_match("config/routes.rb"));
        assert!(matcher.is_match("config/environments/test.rb"));
        assert!(!matcher.is_match("app/models/user.rb"));

        for invalid in [
            "../outside.yml",
            "/absolute.yml",
            "C:/absolute.yml",
            "config\\routes.rb",
            "[",
        ] {
            let err = build_watched_file_matcher("watch-test", &[invalid.to_string()])
                .expect_err("invalid or escaping watcher glob must be rejected");
            assert!(
                err.to_string().contains("watched file glob"),
                "watcher validation error must identify the manifest field"
            );
        }

        let manifest: ExtensionManifest = toml::from_str(
            r#"
id = "missing-capability"
abi_version = 1
runtime = "mruby-wasm"
wasm = "extension.wasm"

[watching]
globs = ["config/routes.rb"]
"#,
        )
        .expect("test watcher manifest must parse");
        let err = validate_manifest(&manifest)
            .expect_err("watching declaration without capability must fail");
        assert!(err.to_string().contains("without `watching` capability"));
    }

    #[test]
    fn initialization_package_wins_duplicate_id_independent_of_path_order() {
        let temp_dir = TempDir::new().expect("test temp dir must be created");
        let environment_package = temp_dir.path().join("a-environment");
        let initialization_package = temp_dir.path().join("z-initialization");
        copy_rspec_package(&environment_package, "0.1.0-environment");
        copy_rspec_package(&initialization_package, "0.1.0-initialization");
        let registry = ExtensionRegistry::load(&ExtensionLoadConfig {
            package_paths: vec![
                ConfiguredExtensionPath {
                    path: environment_package,
                    source: ExtensionPathSource::Environment,
                },
                ConfiguredExtensionPath {
                    path: initialization_package,
                    source: ExtensionPathSource::InitializationOptions,
                },
            ],
            directory_paths: Vec::new(),
            project_package_paths: Vec::new(),
            settings: BTreeMap::new(),
        });

        let reports = registry.status_reports();

        assert_eq!(
            reports.len(),
            1,
            "duplicate extension IDs must not both execute"
        );
        assert_eq!(reports[0].id, "rspec-ruby");
        assert_eq!(reports[0].version.as_deref(), Some("0.1.0-initialization"));
    }

    #[test]
    fn wall_clock_deadline_is_reported_as_slow_status() {
        let status = ExtensionStatus::from_failure(
            "failed to call extension: extension wall-clock deadline exceeded",
        );

        assert!(matches!(status, ExtensionStatus::Slow { .. }));
    }

    #[test]
    fn telemetry_classifies_calls_failures_rejections_and_conflicts_without_dimensions() {
        let telemetry = ExtensionTelemetry::default();
        telemetry.record_call(
            GuestCallKind::Lifecycle,
            Duration::from_nanos(3),
            None,
            None,
        );
        telemetry.record_call(
            GuestCallKind::Index,
            Duration::from_nanos(5),
            None,
            Some("failed to call extension: wasm trap: all fuel consumed"),
        );
        telemetry.record_call(
            GuestCallKind::Event,
            Duration::from_nanos(7),
            None,
            Some("extension output payload 9 bytes exceeds max 8 bytes"),
        );
        telemetry.record_rejected_output();
        telemetry.record_patch_conflict();
        telemetry.record_disablement();

        let report = telemetry.report(2);
        assert_eq!(report.guest_calls, 3);
        assert_eq!(report.lifecycle_calls, 1);
        assert_eq!(report.index_calls, 1);
        assert_eq!(report.event_calls, 1);
        assert_eq!(report.guest_failures, 2);
        assert_eq!(report.guest_traps, 1);
        assert_eq!(report.resource_limit_failures, 2);
        assert_eq!(report.rejected_outputs, 1);
        assert_eq!(report.patch_conflicts, 1);
        assert_eq!(report.disablements, 1);
        assert_eq!(report.total_guest_time_ns, 15);
        assert_eq!(report.max_guest_time_ns, 7);
        assert_eq!(report.project_instances, 2);

        let serialized = serde_json::to_value(&report)
            .expect("extension telemetry must remain serializable through the status contract");
        assert!(serialized.get("guest_calls").is_some());
        assert!(serialized.get("resource_limit_failures").is_some());
        assert!(serialized.get("patch_conflicts").is_some());
    }

    #[test]
    fn initialization_options_do_not_load_direct_wasm_files() {
        let temp_dir = TempDir::new().expect("test temp dir must be created");
        let wasm_path = temp_dir.path().join("extension.wasm");
        fs::write(&wasm_path, b"not real wasm").expect("test wasm marker must be written");

        let config = ExtensionLoadConfig {
            package_paths: vec![ConfiguredExtensionPath {
                path: wasm_path,
                source: ExtensionPathSource::InitializationOptions,
            }],
            directory_paths: Vec::new(),
            project_package_paths: Vec::new(),
            settings: BTreeMap::new(),
        };

        let extensions = load_wasm_extensions(&config);
        assert!(
            extensions.is_empty(),
            "INVARIANT VIOLATED: initialization options loaded a direct wasm file. \
             This is a bug because editor-installed extensions must be manifest packages. \
             Fix: require extension.toml for initialization option extension paths."
        );
    }

    #[test]
    fn manifest_method_targets_parse_semantic_owner_kind_and_method() {
        let manifest: ExtensionManifest = toml::from_str(
            r#"
id = "semantic"
abi_version = 1
runtime = "mruby-wasm"
wasm = "extension.wasm"

[indexing]
call_names = ["describe"]

[[indexing.targets]]
owner = ["RSpec"]
owner_kind = "singleton"
method = "describe"
frame = true
"#,
        )
        .expect("test manifest must parse");

        let targets =
            parse_manifest_method_targets(&manifest).expect("test semantic targets must parse");
        assert_eq!(targets.len(), 1);
        assert_eq!(
            targets[0],
            ExtensionMethodTarget {
                owner: vec![RubyConstant::new("RSpec").expect("test constant is valid")],
                owner_kind: NamespaceKind::Singleton,
                method: RubyMethod::new("describe").expect("test method is valid"),
                frame: true,
            },
            "INVARIANT VIOLATED: extension semantic target parsing changed. \
             This is a bug because extension dispatch must be gated by resolved method target. \
             Fix: preserve owner, owner_kind, method, and frame fields."
        );
    }

    #[test]
    fn semantic_seed_facts_are_installed_only_in_every_applicable_isolated_project_engine() {
        let package = Path::new(env!("CARGO_MANIFEST_DIR")).join("extensions/rspec-ruby");
        let config = RubyFastLspConfig {
            extension_packages: vec![package.to_string_lossy().into_owned()],
            ..RubyFastLspConfig::default()
        };
        let registry = ExtensionRegistryHandle::from_config(&config);
        let first = Arc::new(RwLock::new(ruby_analysis::engine::AnalysisEngine::new()));
        let second = Arc::new(RwLock::new(ruby_analysis::engine::AnalysisEngine::new()));
        let ineligible = Arc::new(RwLock::new(ruby_analysis::engine::AnalysisEngine::new()));
        let project =
            |project_uri: &str, version: &str| ruby_fast_lsp_extension_api::ProjectContext {
                project_uri: project_uri.to_string(),
                source_uri: format!("{project_uri}/spec/example_spec.rb"),
                source_kind: ruby_fast_lsp_extension_api::ProjectSourceKind::Project,
                workspace_trusted: true,
                ruby_version: Some("3.3.0".to_string()),
                lockfile_present: true,
                locked_gems_complete: true,
                locked_gems: vec![ruby_fast_lsp_extension_api::LockedGem {
                    name: "rspec-core".to_string(),
                    version: version.to_string(),
                    source: ruby_fast_lsp_extension_api::LockedGemSource::Registry,
                }],
            };
        let first_project = project("file:///umbrella/first", "3.12.0");
        let second_project = project("file:///umbrella/second", "3.13.5");
        let ineligible_project = project("file:///umbrella/future", "4.0.0");

        registry.ensure_semantic_seed_facts(&first, Some(&first_project));
        registry.ensure_semantic_seed_facts(&second, Some(&second_project));
        registry.ensure_semantic_seed_facts(&ineligible, Some(&ineligible_project));

        for engine in [first, second] {
            let engine = engine.read();
            assert!(
                engine.all_method_facts().iter().any(|fact| {
                    matches!(
                        &fact.fqn,
                        FullyQualifiedName::Method(namespace, method)
                            if namespace.as_slice() == [RubyConstant::new("RSpec").expect("RSpec is a valid constant")]
                                && method.as_str() == "describe"
                    )
                }),
                "every applicable isolated project engine must receive the RSpec.describe semantic target"
            );
        }
        assert!(
            ineligible.read().all_method_facts().is_empty(),
            "an isolated project with an unsupported RSpec version must not receive semantic targets"
        );
    }

    #[test]
    fn semantic_seed_applicability_fingerprint_ignores_per_document_context() {
        let context = |source_uri: &str, source_kind| ruby_fast_lsp_extension_api::ProjectContext {
            project_uri: "file:///workspace/app".to_string(),
            source_uri: source_uri.to_string(),
            source_kind,
            workspace_trusted: true,
            ruby_version: Some("3.3.0".to_string()),
            lockfile_present: true,
            locked_gems_complete: true,
            locked_gems: vec![ruby_fast_lsp_extension_api::LockedGem {
                name: "rspec-core".to_string(),
                version: "3.13.6".to_string(),
                source: ruby_fast_lsp_extension_api::LockedGemSource::Registry,
            }],
        };
        let project_source = context(
            "file:///workspace/app/spec/example_spec.rb",
            ruby_fast_lsp_extension_api::ProjectSourceKind::Project,
        );
        let dependency_source = context(
            "file:///gems/rspec-core-3.13.6/lib/rspec/core.rb",
            ruby_fast_lsp_extension_api::ProjectSourceKind::Gem,
        );

        assert_eq!(
            extension_applicability_fingerprint(Some(&project_source)),
            extension_applicability_fingerprint(Some(&dependency_source)),
            "semantic seed identity must depend on project applicability, not the current file URI or source kind"
        );
    }

    #[test]
    fn cached_project_snapshot_replaces_semantic_seed_after_dependency_refresh() {
        let package = Path::new(env!("CARGO_MANIFEST_DIR")).join("extensions/rspec-ruby");
        let registry = ExtensionRegistryHandle::from_config(&RubyFastLspConfig {
            extension_packages: vec![package.to_string_lossy().into_owned()],
            ..RubyFastLspConfig::default()
        });
        let project = TempDir::new().expect("project temp directory must be created");
        fs::write(
            project.path().join("Gemfile.lock"),
            "GEM\n  specs:\n    rspec-core (3.13.1)\n",
        )
        .expect("eligible lockfile must be written");
        let mut seed = ProjectContextSeed::detect(
            "file:///umbrella/app".to_string(),
            project.path(),
            true,
            Some("3.3.0".to_string()),
        );
        let engine = Arc::new(RwLock::new(ruby_analysis::engine::AnalysisEngine::new()));

        let eligible = seed.context_snapshot(
            "file:///umbrella/app/spec/example_spec.rb".to_string(),
            SourceKind::Project,
        );
        registry.ensure_semantic_seed_facts_for_snapshot(&engine, &eligible);
        assert!(
            engine.read().all_method_facts().iter().any(|fact| {
                matches!(
                    &fact.fqn,
                    FullyQualifiedName::Method(namespace, method)
                        if namespace.as_slice() == [RubyConstant::new("RSpec").expect("RSpec is a valid constant")]
                            && method.as_str() == "describe"
                )
            }),
            "the eligible cached snapshot must seed RSpec.describe"
        );

        fs::write(
            project.path().join("Gemfile.lock"),
            "GEM\n  specs:\n    rspec-core (4.0.0)\n",
        )
        .expect("ineligible lockfile must be written");
        seed.refresh_dependencies(project.path());
        let ineligible = seed.context_snapshot(
            "file:///umbrella/app/spec/example_spec.rb".to_string(),
            SourceKind::Project,
        );
        registry.ensure_semantic_seed_facts_for_snapshot(&engine, &ineligible);

        assert!(
            engine.read().all_method_facts().is_empty(),
            "dependency refresh must replace stale extension semantic targets"
        );
    }

    #[test]
    fn extension_dispatch_skips_dependency_and_signature_sources_without_losing_applicability() {
        let package = Path::new(env!("CARGO_MANIFEST_DIR")).join("extensions/rspec-ruby");
        let registry = ExtensionRegistryHandle::from_config(&RubyFastLspConfig {
            extension_packages: vec![package.to_string_lossy().into_owned()],
            ..RubyFastLspConfig::default()
        });
        let extension = registry
            .extensions()
            .into_iter()
            .find(|extension| extension.metadata.id == "rspec-ruby")
            .expect("bundled RSpec extension must load for source policy testing");
        let context = |source_kind| ruby_fast_lsp_extension_api::ProjectContext {
            project_uri: "file:///workspace/app".to_string(),
            source_uri: "file:///workspace/app/source.rb".to_string(),
            source_kind,
            workspace_trusted: true,
            ruby_version: Some("3.3.0".to_string()),
            lockfile_present: true,
            locked_gems_complete: true,
            locked_gems: vec![ruby_fast_lsp_extension_api::LockedGem {
                name: "rspec-core".to_string(),
                version: "3.13.6".to_string(),
                source: ruby_fast_lsp_extension_api::LockedGemSource::Registry,
            }],
        };

        for kind in [
            ruby_fast_lsp_extension_api::ProjectSourceKind::Project,
            ruby_fast_lsp_extension_api::ProjectSourceKind::Excluded,
        ] {
            let project = context(kind);
            assert!(extension.applies_to(Some(&project)));
            assert!(extension.applies_to_source(Some(&project)));
        }
        for kind in [
            ruby_fast_lsp_extension_api::ProjectSourceKind::Gem,
            ruby_fast_lsp_extension_api::ProjectSourceKind::Stdlib,
            ruby_fast_lsp_extension_api::ProjectSourceKind::Stub,
            ruby_fast_lsp_extension_api::ProjectSourceKind::Signature,
        ] {
            let project = context(kind);
            assert!(
                extension.applies_to(Some(&project)),
                "locked-gem applicability must remain a project-level decision"
            );
            assert!(
                !extension.applies_to_source(Some(&project)),
                "extensions must not execute DSL hooks while indexing {kind:?} inputs"
            );
        }
    }

    #[test]
    fn collector_applicability_snapshot_reuses_exact_project_decisions() {
        let package = Path::new(env!("CARGO_MANIFEST_DIR")).join("extensions/rspec-ruby");
        let registry = ExtensionRegistryHandle::from_config(&RubyFastLspConfig {
            extension_packages: vec![package.to_string_lossy().into_owned()],
            ..RubyFastLspConfig::default()
        });
        let project = |version: &str| ruby_fast_lsp_extension_api::ProjectContext {
            project_uri: "file:///workspace/app".to_string(),
            source_uri: "file:///workspace/app/spec/example_spec.rb".to_string(),
            source_kind: ruby_fast_lsp_extension_api::ProjectSourceKind::Project,
            workspace_trusted: true,
            ruby_version: Some("3.3.0".to_string()),
            lockfile_present: true,
            locked_gems_complete: true,
            locked_gems: vec![ruby_fast_lsp_extension_api::LockedGem {
                name: "rspec-core".to_string(),
                version: version.to_string(),
                source: ruby_fast_lsp_extension_api::LockedGemSource::Registry,
            }],
        };
        let eligible = project("3.13.6");
        let ineligible = project("4.0.0");
        let extension = registry
            .extensions()
            .into_iter()
            .find(|extension| extension.metadata.id == "rspec-ruby")
            .expect("bundled RSpec extension must load for applicability snapshot testing");

        let evaluations_before = extension.test_applicability_evaluations();
        let eligible_snapshot = registry.applicability_snapshot(Some(&eligible));
        assert!(eligible_snapshot.applies_to_extension(&registry, "rspec-ruby", Some(&eligible)));
        for _ in 0..100 {
            assert!(
                eligible_snapshot.applies_to_extension(&registry, "rspec-ruby", Some(&eligible)),
                "the exact eligible project decision must remain reusable throughout one file traversal"
            );
        }
        assert_eq!(
            extension.test_applicability_evaluations(),
            evaluations_before + 1,
            "one file snapshot must evaluate each extension's locked-gem requirement exactly once"
        );

        let ineligible_snapshot = registry.applicability_snapshot(Some(&ineligible));
        assert!(
            !ineligible_snapshot.applies_to_extension(&registry, "rspec-ruby", Some(&ineligible)),
            "a changed exact locked version must produce a new fail-closed applicability decision"
        );
        assert_eq!(
            extension.test_applicability_evaluations(),
            evaluations_before + 2,
            "a changed project dependency snapshot must be evaluated independently"
        );
    }

    #[test]
    fn manifest_frame_call_names_are_validated_separately_from_guest_handlers() {
        let manifest: ExtensionManifest = toml::from_str(
            r#"
id = "frames"
abi_version = 1
runtime = "mruby-wasm"
wasm = "extension.wasm"

[indexing]
call_names = ["resources"]
frame_call_names = ["draw", "namespace"]
"#,
        )
        .expect("frame manifest must parse");

        validate_manifest(&manifest).expect("valid Ruby frame names must be accepted");
        assert_eq!(
            manifest.indexing.unwrap().frame_call_names,
            ["draw", "namespace"],
            "frame call names must not need fake guest handlers"
        );

        let invalid: ExtensionManifest = toml::from_str(
            r#"
id = "frames"
abi_version = 1
runtime = "mruby-wasm"
wasm = "extension.wasm"

[indexing]
call_names = ["resources"]
frame_call_names = ["not a call"]
"#,
        )
        .expect("invalid frame name must remain syntactically valid TOML");
        let error = validate_manifest(&invalid).expect_err("invalid Ruby frame names must fail");
        assert!(
            error.to_string().contains("frame call name"),
            "got: {error}"
        );
    }

    #[test]
    fn invalid_manifest_package_is_skipped_without_panicking() {
        let temp_dir = TempDir::new().expect("test temp dir must be created");
        let package_dir = temp_dir.path().join("broken");
        fs::create_dir(&package_dir).expect("test package dir must be created");
        fs::write(
            package_dir.join("extension.toml"),
            r#"
id = "broken"
abi_version = 999
runtime = "mruby-wasm"
wasm = "missing.wasm"
"#,
        )
        .expect("test manifest must be written");

        let config = ExtensionLoadConfig {
            package_paths: vec![ConfiguredExtensionPath {
                path: package_dir,
                source: ExtensionPathSource::InitializationOptions,
            }],
            directory_paths: Vec::new(),
            project_package_paths: Vec::new(),
            settings: BTreeMap::new(),
        };

        let extensions = load_wasm_extensions(&config);
        assert!(
            extensions.is_empty(),
            "INVARIANT VIOLATED: invalid extension manifest loaded successfully. \
             This is a bug because package validation must reject mismatched ABI or missing wasm. \
             Fix: keep manifest validation in the recoverable load path."
        );
    }

    #[test]
    fn initialization_option_package_without_manifest_is_skipped() {
        let temp_dir = TempDir::new().expect("test temp dir must be created");
        let package_dir = temp_dir.path().join("not-a-package");
        fs::create_dir(&package_dir).expect("test package dir must be created");

        let config = ExtensionLoadConfig {
            package_paths: vec![ConfiguredExtensionPath {
                path: package_dir,
                source: ExtensionPathSource::InitializationOptions,
            }],
            directory_paths: Vec::new(),
            project_package_paths: Vec::new(),
            settings: BTreeMap::new(),
        };

        let extensions = load_wasm_extensions(&config);
        assert!(
            extensions.is_empty(),
            "INVARIANT VIOLATED: initialization option package without manifest loaded. \
             This is a bug because editor-installed extension packages must have extension.toml. \
             Fix: keep extensionPackages stricter than extensionDirs."
        );
    }

    #[test]
    fn incompatible_server_version_manifest_is_skipped() {
        let temp_dir = TempDir::new().expect("test temp dir must be created");
        let package_dir = temp_dir.path().join("incompatible");
        fs::create_dir(&package_dir).expect("test package dir must be created");
        fs::write(package_dir.join("extension.wasm"), b"not real wasm")
            .expect("test wasm marker must be written");
        fs::write(
            package_dir.join("extension.toml"),
            r#"
id = "incompatible"
name = "Incompatible"
version = "0.1.0"
abi_version = 1
server_version = ">=999.0.0"
runtime = "mruby-wasm"
wasm = "extension.wasm"
capabilities = ["index.call"]
permissions = []
"#,
        )
        .expect("test manifest must be written");

        let config = ExtensionLoadConfig {
            package_paths: vec![ConfiguredExtensionPath {
                path: package_dir,
                source: ExtensionPathSource::InitializationOptions,
            }],
            directory_paths: Vec::new(),
            project_package_paths: Vec::new(),
            settings: BTreeMap::new(),
        };

        let extensions = load_wasm_extensions(&config);
        assert!(
            extensions.is_empty(),
            "INVARIANT VIOLATED: incompatible server_version manifest loaded. \
             This is a bug because extension packages must be gated by host compatibility. \
             Fix: validate manifest server_version before wasm instantiation."
        );
    }

    #[test]
    fn checksum_mismatch_manifest_is_skipped() {
        let temp_dir = TempDir::new().expect("test temp dir must be created");
        let package_dir = temp_dir.path().join("checksum");
        fs::create_dir(&package_dir).expect("test package dir must be created");
        fs::write(package_dir.join("extension.wasm"), b"not real wasm")
            .expect("test wasm marker must be written");
        fs::write(
            package_dir.join("extension.toml"),
            r#"
id = "checksum"
name = "Checksum"
version = "0.1.0"
abi_version = 1
server_version = ">=0.2.3, <0.3.0"
runtime = "mruby-wasm"
wasm = "extension.wasm"
checksum_sha256 = "0000000000000000000000000000000000000000000000000000000000000000"
capabilities = ["index.call"]
permissions = []
"#,
        )
        .expect("test manifest must be written");

        let config = ExtensionLoadConfig {
            package_paths: vec![ConfiguredExtensionPath {
                path: package_dir,
                source: ExtensionPathSource::InitializationOptions,
            }],
            directory_paths: Vec::new(),
            project_package_paths: Vec::new(),
            settings: BTreeMap::new(),
        };

        let extensions = load_wasm_extensions(&config);
        assert!(
            extensions.is_empty(),
            "INVARIANT VIOLATED: checksum mismatch manifest loaded. \
             This is a bug because extension packages must bind manifest metadata to wasm bytes. \
             Fix: validate checksum_sha256 before wasm instantiation."
        );
    }

    #[test]
    fn process_manifest_requires_process_exec_permission() {
        let temp_dir = TempDir::new().expect("test temp dir must be created");
        let package_dir = temp_dir.path().join("process");
        fs::create_dir(&package_dir).expect("test package dir must be created");
        fs::write(package_dir.join("extension.wasm"), b"not real wasm")
            .expect("test wasm marker must be written");
        fs::write(
            package_dir.join("extension.toml"),
            r#"
id = "process"
name = "Process"
version = "0.1.0"
abi_version = 1
server_version = ">=0.2.3, <0.3.0"
runtime = "mruby-wasm"
wasm = "extension.wasm"
capabilities = ["process"]
permissions = []

[process]
commands = ["standardrb"]
"#,
        )
        .expect("test manifest must be written");

        let config = ExtensionLoadConfig {
            package_paths: vec![ConfiguredExtensionPath {
                path: package_dir,
                source: ExtensionPathSource::InitializationOptions,
            }],
            directory_paths: Vec::new(),
            project_package_paths: Vec::new(),
            settings: BTreeMap::new(),
        };

        let extensions = load_wasm_extensions(&config);
        assert!(
            extensions.is_empty(),
            "INVARIANT VIOLATED: process command manifest loaded without process.exec. \
             This is a bug because external process permissions must be explicit. \
             Fix: require process.exec when [process].commands is present."
        );
    }

    #[test]
    fn extension_process_request_requires_trust_permission_and_allowlist() {
        let root = PathBuf::from("/workspace");
        let request = ruby_fast_lsp_extension_api::ProcessRequest {
            request_id: "routes".to_string(),
            command: "bundle".to_string(),
            arguments: vec![
                "exec".to_string(),
                "rails".to_string(),
                "routes".to_string(),
            ],
            stdin: None,
            workspace_root: None,
            timeout_ms: None,
        };

        for (trusted, permissions, commands, expected) in [
            (
                false,
                vec!["process.exec"],
                vec!["bundle"],
                "trusted workspace",
            ),
            (true, Vec::new(), vec!["bundle"], "process.exec"),
            (true, vec!["process.exec"], vec!["ruby"], "allowlisted"),
        ] {
            let err = validate_extension_process_request(
                "process-test",
                trusted,
                &permissions
                    .into_iter()
                    .map(str::to_string)
                    .collect::<Vec<_>>(),
                &commands.into_iter().map(str::to_string).collect::<Vec<_>>(),
                &[root.clone()],
                &[root.clone()],
                &request,
            )
            .expect_err("unsafe extension process request must be denied");
            assert!(
                err.to_string().contains(expected),
                "process denial must explain the violated policy: {err}"
            );
        }
    }

    #[tokio::test]
    async fn extension_process_host_captures_bounded_result() {
        let root = TempDir::new().expect("test workspace must be created");
        let request = ProcessRequest {
            request_id: "version".to_string(),
            command: "rustc".to_string(),
            arguments: vec!["--version".to_string()],
            stdin: None,
            workspace_root: None,
            timeout_ms: Some(5_000),
        };
        let validated = validate_extension_process_request(
            "process-test",
            true,
            &["process.exec".to_string()],
            &["rustc".to_string()],
            &[root.path().to_path_buf()],
            &[root.path().to_path_buf()],
            &request,
        )
        .expect("trusted allowlisted process request must validate");

        let result = run_extension_process(validated, IndexingResourceGovernor::default()).await;

        assert_eq!(result.request_id, "version");
        assert_eq!(result.status, ProcessResultStatus::Exited);
        assert_eq!(result.exit_code, Some(0));
        assert!(result.stdout.starts_with("rustc "));
        assert!(!result.stdout_truncated);
        assert!(!result.stderr_truncated);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn extension_process_waits_for_weighted_admission_and_releases_exact_lease() {
        let governor = crate::indexing_resources::IndexingResourceGovernor::new(
            crate::indexing_resources::IndexingResourcePolicy::with_limits(
                1,
                1,
                128 * 1024 * 1024,
                1,
            ),
        );
        let (holder_started_tx, holder_started_rx) = tokio::sync::oneshot::channel();
        let holder_release = Arc::new(tokio::sync::Notify::new());
        let holder_governor = governor.clone();
        let holder_release_task = holder_release.clone();
        let holder = tokio::spawn(async move {
            holder_governor
                .run_async_with_resources(
                    "extension process contention holder",
                    crate::indexing_resources::IndexingWorkSpec::new(
                        Some(PathBuf::from("/workspace/background")),
                        crate::indexing_resources::IndexingResourcePriority::Background,
                        1,
                        128 * 1024 * 1024,
                        1,
                    ),
                    None,
                    async move {
                        holder_started_tx.send(()).unwrap();
                        holder_release_task.notified().await;
                    },
                )
                .await
                .unwrap();
        });
        holder_started_rx.await.unwrap();

        let root = TempDir::new().expect("test workspace must be created");
        let request = ProcessRequest {
            request_id: "version-after-admission".to_string(),
            command: "rustc".to_string(),
            arguments: vec!["--version".to_string()],
            stdin: None,
            workspace_root: None,
            timeout_ms: Some(5_000),
        };
        let validated = validate_extension_process_request(
            "process-test",
            true,
            &["process.exec".to_string()],
            &["rustc".to_string()],
            &[root.path().to_path_buf()],
            &[root.path().to_path_buf()],
            &request,
        )
        .expect("trusted allowlisted process request must validate");
        let process_governor = governor.clone();
        let process =
            tokio::spawn(async move { run_extension_process(validated, process_governor).await });
        tokio::time::timeout(Duration::from_secs(1), async {
            while governor.snapshot().queued_tasks != 1 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("extension process must queue behind the complete weighted claim");
        assert_eq!(governor.snapshot().active_tasks, 1);

        holder_release.notify_one();
        holder.await.unwrap();
        let result = process.await.unwrap();
        assert_eq!(result.status, ProcessResultStatus::Exited);
        assert!(result.stdout.starts_with("rustc "));
        let complete = governor.snapshot();
        assert_eq!(complete.active_tasks, 0);
        assert_eq!(complete.queued_tasks, 0);
        assert_eq!(complete.completed_tasks, 2);
        assert_eq!(complete.peak_active_cpu_lanes, 1);
        assert_eq!(
            complete.peak_active_transient_memory_bytes,
            128 * 1024 * 1024
        );
        assert_eq!(complete.peak_active_io_slots, 1);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn extension_process_host_kills_timed_out_process() {
        let root = TempDir::new().expect("test workspace must be created");
        let request = ProcessRequest {
            request_id: "timeout".to_string(),
            command: "sh".to_string(),
            arguments: vec!["-c".to_string(), "sleep 5".to_string()],
            stdin: None,
            workspace_root: None,
            timeout_ms: Some(10),
        };
        let validated = validate_extension_process_request(
            "process-test",
            true,
            &["process.exec".to_string()],
            &["sh".to_string()],
            &[root.path().to_path_buf()],
            &[root.path().to_path_buf()],
            &request,
        )
        .expect("explicitly allowlisted shell process must validate");

        let result = run_extension_process(validated, IndexingResourceGovernor::default()).await;

        assert_eq!(result.status, ProcessResultStatus::TimedOut);
        assert_eq!(result.exit_code, None);
    }

    #[test]
    fn incompatible_extension_index_patches_are_rejected_deterministically() {
        let patch = |extension_id: &str, return_type| {
            IndexPatch::DefineMethod(ruby_fast_lsp_extension_api::DefineMethodPatch {
                name: "factory".to_string(),
                namespace: vec!["Widget".to_string()],
                owner_target: None,
                owner_kind: AbiNamespaceKind::Singleton,
                visibility: ruby_fast_lsp_extension_api::MethodVisibility::Public,
                location: SourceRange {
                    start: SourcePosition {
                        line: 1,
                        character: 2,
                    },
                    end: SourcePosition {
                        line: 1,
                        character: 9,
                    },
                },
                params: Vec::new(),
                return_type,
                return_type_source: None,
                source: ruby_fast_lsp_extension_api::PatchSource {
                    extension_id: extension_id.to_string(),
                    macro_name: "factory".to_string(),
                },
            })
        };
        let left = patch(
            "z-extension",
            Some(ruby_fast_lsp_extension_api::RubyType::Named(
                "String".to_string(),
            )),
        );
        let right = patch(
            "a-extension",
            Some(ruby_fast_lsp_extension_api::RubyType::Named(
                "Integer".to_string(),
            )),
        );

        let err = resolve_index_patch_conflicts(vec![left, right])
            .expect_err("incompatible patches for one semantic identity must be rejected");

        assert_eq!(err.extension_ids, vec!["a-extension", "z-extension"]);
        assert!(err.message.contains("Widget.factory"));
    }

    #[test]
    fn equivalent_extension_index_patches_are_deduplicated() {
        let patch = |extension_id: &str| {
            IndexPatch::ApplyMixin(ruby_fast_lsp_extension_api::ApplyMixinPatch {
                namespace: vec!["Widget".to_string()],
                owner_target: None,
                target_kind: AbiNamespaceKind::Instance,
                mixin_target: None,
                mixin: vec!["Shared".to_string()],
                absolute: true,
                kind: ruby_fast_lsp_extension_api::MixinKind::Include,
                location: SourceRange {
                    start: SourcePosition {
                        line: 2,
                        character: 0,
                    },
                    end: SourcePosition {
                        line: 2,
                        character: 7,
                    },
                },
                source: ruby_fast_lsp_extension_api::PatchSource {
                    extension_id: extension_id.to_string(),
                    macro_name: "shared".to_string(),
                },
            })
        };

        let resolved =
            resolve_index_patch_conflicts(vec![patch("z-extension"), patch("a-extension")])
                .expect("equivalent semantic patches must merge without ambiguity");

        assert_eq!(resolved.len(), 1);
        assert_eq!(index_patch_extension_id(&resolved[0]), "a-extension");
    }

    fn execution_context_fixture(extension_id: &str) -> BlockExecutionContextPatch {
        let call_range = SourceRange {
            start: SourcePosition {
                line: 2,
                character: 2,
            },
            end: SourcePosition {
                line: 6,
                character: 5,
            },
        };
        let block_range = SourceRange {
            start: SourcePosition {
                line: 2,
                character: 20,
            },
            end: SourcePosition {
                line: 6,
                character: 5,
            },
        };
        BlockExecutionContextPatch {
            call_range,
            block_range,
            generated_owners: vec![ruby_fast_lsp_extension_api::GeneratedOwnerPatch {
                local_id: "group:2:2".to_string(),
                scope: ruby_fast_lsp_extension_api::GeneratedOwnerScope::Source,
                declaration_kind: ruby_fast_lsp_extension_api::NamespaceDeclarationKind::Class,
                owner_kind: AbiNamespaceKind::Instance,
                parent: None,
            }],
            implicit_receiver: ExecutionContextTarget::GeneratedOwner {
                local_id: "group:2:2".to_string(),
                owner_kind: None,
            },
            method_definition_owner: ExecutionContextTarget::GeneratedOwner {
                local_id: "group:2:2".to_string(),
                owner_kind: None,
            },
            lexical_scope: ruby_fast_lsp_extension_api::LexicalScopeMode::Preserve,
            local_scope: ruby_fast_lsp_extension_api::LocalScopeMode::Preserve,
            source: ruby_fast_lsp_extension_api::PatchSource {
                extension_id: extension_id.to_string(),
                macro_name: "describe".to_string(),
            },
        }
    }

    fn execution_call_fixture() -> CallContext {
        let context = execution_context_fixture("rspec-ruby");
        CallContext {
            project: None,
            method_name: "describe".to_string(),
            receiver: Receiver::Constant(vec!["RSpec".to_string()]),
            arguments: Vec::new(),
            current_namespace: vec!["Lexical".to_string()],
            namespace_kind: AbiNamespaceKind::Instance,
            call_range: context.call_range,
            block_range: Some(context.block_range),
            message_range: context.call_range,
            resolved_callees: Vec::new(),
            enclosing_calls: Vec::new(),
        }
    }

    fn rust_isolation_probe_context(project_uri: &str) -> CallContext {
        let mut context = execution_call_fixture();
        context.project = Some(ruby_fast_lsp_extension_api::ProjectContext {
            project_uri: project_uri.to_string(),
            source_uri: format!("{project_uri}/probe.rb"),
            source_kind: ruby_fast_lsp_extension_api::ProjectSourceKind::Project,
            workspace_trusted: true,
            ruby_version: Some("3.3".to_string()),
            lockfile_present: true,
            locked_gems_complete: true,
            locked_gems: vec![ruby_fast_lsp_extension_api::LockedGem {
                name: "example-framework".to_string(),
                version: "1.0.0".to_string(),
                source: ruby_fast_lsp_extension_api::LockedGemSource::Registry,
            }],
        });
        context.method_name = "isolation_probe".to_string();
        context.receiver = Receiver::None;
        context.block_range = None;
        context.resolved_callees.clear();
        context.enclosing_calls.clear();
        context
    }

    #[test]
    fn guest_call_context_delivers_project_only_for_legacy_per_call_guests() {
        let complete = rust_isolation_probe_context("file:///workspace/a");
        let project = complete
            .project
            .as_ref()
            .expect("fixture must carry an owning project");
        let mut compact = complete.clone();
        compact.project = None;

        let activation = guest_call_context(
            ExtensionProjectContextDelivery::Activation,
            project,
            &compact,
        );
        assert!(matches!(&activation, Cow::Borrowed(_)));
        assert!(activation.project.is_none());

        let per_call =
            guest_call_context(ExtensionProjectContextDelivery::PerCall, project, &compact);
        assert!(matches!(&per_call, Cow::Owned(_)));
        assert_eq!(
            per_call
                .project
                .as_ref()
                .map(|project| project.project_uri.as_str()),
            Some("file:///workspace/a")
        );

        let already_complete =
            guest_call_context(ExtensionProjectContextDelivery::PerCall, project, &complete);
        assert!(matches!(&already_complete, Cow::Borrowed(_)));
    }

    #[test]
    fn wasm_private_state_is_isolated_per_project_uri() {
        let package_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("extensions/example-rust");
        let artifact = package_path
            .join("target/wasm32-wasip1/release/ruby_fast_lsp_example_rust_extension.wasm");
        if !artifact.is_file() {
            eprintln!(
                "skipping project-state Wasm isolation test; run extensions/example-rust/build-and-test.sh"
            );
            return;
        }
        let registry = ExtensionRegistry::load(&ExtensionLoadConfig {
            package_paths: vec![ConfiguredExtensionPath {
                path: package_path,
                source: ExtensionPathSource::InitializationOptions,
            }],
            ..ExtensionLoadConfig::default()
        });
        let registry = ExtensionRegistryHandle {
            inner: Arc::new(RwLock::new(registry)),
            reconfiguration: Arc::new(tokio::sync::Mutex::new(())),
            persistent_cache: None,
        };
        let extension = registry
            .extensions()
            .iter()
            .find(|extension| extension.metadata.id == "example-rust")
            .expect("typed Rust acceptance extension must load")
            .clone();

        let project_a = rust_isolation_probe_context("file:///workspace/a");
        let project_b = rust_isolation_probe_context("file:///workspace/b");
        assert_eq!(
            extension
                .index_call_output(&project_a)
                .expect("first project A probe must run")
                .index_patches
                .len(),
            1
        );
        let project_a_context = project_a
            .project
            .clone()
            .expect("project A probe must carry project context");
        let project_b_context = project_b
            .project
            .clone()
            .expect("project B probe must carry project context");
        let symbols_a = registry.document_symbols(
            "file:///workspace/a/probe.rb",
            "isolation_probe\n",
            Some(project_a_context.clone()),
        );
        assert_eq!(
            symbols_a
                .iter()
                .filter(|symbol| symbol.name == "project-isolated-symbol")
                .count(),
            1,
            "document responses must observe the same private project A Wasm instance as call hooks"
        );
        assert!(
            registry
                .document_symbols(
                    "file:///workspace/b/probe.rb",
                    "isolation_probe\n",
                    Some(project_b_context.clone()),
                )
                .iter()
                .all(|symbol| symbol.name != "project-isolated-symbol"),
            "an untouched project B response must not observe project A guest state"
        );
        assert!(
            extension
                .index_call_output(&project_a)
                .expect("second project A probe must run")
                .index_patches
                .is_empty(),
            "the same project must observe its own guest state"
        );
        assert_eq!(
            extension
                .index_call_output(&project_b)
                .expect("first project B probe must run")
                .index_patches
                .len(),
            1,
            "project B must receive a fresh Wasm heap instead of project A state"
        );
        assert!(
            registry
                .document_symbols(
                    "file:///workspace/b/probe.rb",
                    "isolation_probe\n",
                    Some(project_b_context.clone()),
                )
                .iter()
                .any(|symbol| symbol.name == "project-isolated-symbol"),
            "project B document responses must use project B's now-initialized Wasm instance"
        );
        assert!(
            registry
                .code_lenses(
                    "file:///workspace/b/probe.rb",
                    "isolation_probe\n",
                    Some(project_b_context),
                )
                .iter()
                .any(|lens| lens
                    .command
                    .as_ref()
                    .is_some_and(|command| command.title == "Project-isolated lens")),
            "code lenses must use the same project-aware response dispatch as document symbols"
        );
        let telemetry = extension.status_report().telemetry;
        assert_eq!(telemetry.project_instances, 2);
        assert_eq!(telemetry.project_instance_creations, 2);
        assert_eq!(telemetry.project_instance_failures, 0);
        assert!(telemetry.max_project_instance_time_ns <= telemetry.total_project_instance_time_ns);
        assert_eq!(
            telemetry.guest_calls, 8,
            "activation, three call hooks, three symbol requests, and one lens request must all be observed: {telemetry:?}"
        );
        assert_eq!(telemetry.lifecycle_calls, 1);
        assert_eq!(telemetry.index_calls, 3);
        assert_eq!(telemetry.event_calls, 4);
        assert!(telemetry.emitted_index_patches >= 2);
        assert!(telemetry.emitted_response_patches >= 3);
        assert_eq!(telemetry.guest_failures, 0);
        assert_eq!(telemetry.guest_traps, 0);
        assert_eq!(telemetry.resource_limit_failures, 0);
        assert_eq!(telemetry.disablements, 0);
        assert_eq!(telemetry.rejected_outputs, 0);
        assert_eq!(telemetry.patch_conflicts, 0);
        assert!(telemetry.max_guest_time_ns <= telemetry.total_guest_time_ns);
    }

    #[test]
    fn execution_context_validation_rejects_spoofed_ranges_and_undeclared_targets() {
        let call = execution_call_fixture();
        let valid = execution_context_fixture("rspec-ruby");
        validate_execution_contexts("rspec-ruby", &call, std::slice::from_ref(&valid))
            .expect("valid execution context must pass the guest boundary");

        let mut spoofed = valid.clone();
        spoofed.source.extension_id = "other-extension".to_string();
        assert!(validate_execution_contexts("rspec-ruby", &call, &[spoofed])
            .expect_err("spoofed provenance must be rejected")
            .contains("provenance"));

        let mut wrong_block = valid.clone();
        wrong_block.block_range.start.character += 1;
        assert!(
            validate_execution_contexts("rspec-ruby", &call, &[wrong_block])
                .expect_err("a guest must not redirect semantics to another block")
                .contains("block_range")
        );

        let mut undeclared = valid;
        undeclared.method_definition_owner = ExecutionContextTarget::GeneratedOwner {
            local_id: "missing".to_string(),
            owner_kind: None,
        };
        assert!(
            validate_execution_contexts("rspec-ruby", &call, &[undeclared])
                .expect_err("undeclared generated targets must be rejected")
                .contains("undeclared")
        );
    }

    #[test]
    fn execution_context_validation_accepts_exact_namespace_targets_without_generated_owners() {
        let call = execution_call_fixture();
        let mut context = execution_context_fixture("sinatra-rust");
        context.generated_owners.clear();
        context.implicit_receiver = ExecutionContextTarget::Namespace {
            namespace: vec!["Sinatra".to_string(), "Application".to_string()],
            owner_kind: AbiNamespaceKind::Instance,
        };
        context.method_definition_owner = ExecutionContextTarget::Namespace {
            namespace: vec!["Object".to_string()],
            owner_kind: AbiNamespaceKind::Instance,
        };
        context.source.extension_id = "sinatra-rust".to_string();

        validate_execution_contexts("sinatra-rust", &call, &[context]).expect(
            "exact existing namespaces must not require an unrelated hidden-owner declaration",
        );
    }

    #[test]
    fn project_generated_execution_context_requires_project_and_validates_scope() {
        let mut call = execution_call_fixture();
        let mut context = execution_context_fixture("rspec-ruby");
        context.generated_owners[0].scope =
            ruby_fast_lsp_extension_api::GeneratedOwnerScope::Project;
        context.implicit_receiver = ExecutionContextTarget::ProjectGeneratedOwner {
            local_id: "group:2:2".to_string(),
            owner_kind: Some(AbiNamespaceKind::Singleton),
        };
        context.method_definition_owner = ExecutionContextTarget::ProjectGeneratedOwner {
            local_id: "group:2:2".to_string(),
            owner_kind: Some(AbiNamespaceKind::Instance),
        };

        let error = validate_execution_contexts("rspec-ruby", &call, &[context.clone()])
            .expect_err("project-generated context without project metadata must fail closed");
        assert!(error.contains("ProjectContext"), "got: {error}");

        call.project = rust_isolation_probe_context("file:///workspace/project").project;
        validate_execution_contexts("rspec-ruby", &call, &[context])
            .expect("project-generated context with matching declaration must validate");
    }

    #[test]
    fn semantic_mixin_target_is_exclusive_with_ruby_namespace_target() {
        let zero = SourcePosition {
            line: 0,
            character: 0,
        };
        let patch = |mixin: Vec<String>, mixin_target: Option<ExecutionContextTarget>| {
            IndexPatch::ApplyMixin(ruby_fast_lsp_extension_api::ApplyMixinPatch {
                namespace: Vec::new(),
                owner_target: Some(ExecutionContextTarget::ProjectGeneratedOwner {
                    local_id: "consumer".to_string(),
                    owner_kind: Some(AbiNamespaceKind::Instance),
                }),
                target_kind: AbiNamespaceKind::Instance,
                mixin_target,
                mixin,
                absolute: false,
                kind: ruby_fast_lsp_extension_api::MixinKind::Include,
                location: SourceRange {
                    start: zero,
                    end: zero,
                },
                source: ruby_fast_lsp_extension_api::PatchSource {
                    extension_id: "test".to_string(),
                    macro_name: "include_context".to_string(),
                },
            })
        };
        let semantic_target = Some(ExecutionContextTarget::ProjectGeneratedOwner {
            local_id: "shared".to_string(),
            owner_kind: Some(AbiNamespaceKind::Instance),
        });

        validate_index_patch_payloads(&[patch(Vec::new(), semantic_target.clone())])
            .expect("an exact generated mixin target must validate");
        let both =
            validate_index_patch_payloads(&[patch(vec!["Shared".to_string()], semantic_target)])
                .expect_err("mixin target representations must be mutually exclusive");
        assert!(both.contains("either `mixin` or `mixin_target`"));
        let neither = validate_index_patch_payloads(&[patch(Vec::new(), None)])
            .expect_err("a mixin patch without a target must be rejected");
        assert!(neither.contains("must provide"));
    }

    #[test]
    fn execution_context_connection_validates_exact_targets_and_project_requirement() {
        let location = SourceRange {
            start: SourcePosition {
                line: 4,
                character: 2,
            },
            end: SourcePosition {
                line: 4,
                character: 29,
            },
        };
        let patch = IndexPatch::ConnectExecutionContext(
            ruby_fast_lsp_extension_api::ConnectExecutionContextPatch {
                template: ExecutionContextTarget::ProjectGeneratedOwner {
                    local_id: "shared-examples-runtime:auditable".to_string(),
                    owner_kind: Some(AbiNamespaceKind::Singleton),
                },
                application: ExecutionContextTarget::GeneratedOwner {
                    local_id: "example-group:1:0-8:3".to_string(),
                    owner_kind: Some(AbiNamespaceKind::Instance),
                },
                location,
                source: ruby_fast_lsp_extension_api::PatchSource {
                    extension_id: "rspec-ruby".to_string(),
                    macro_name: "it_behaves_like".to_string(),
                },
            },
        );

        validate_index_patch_payloads(std::slice::from_ref(&patch))
            .expect("valid exact execution-context targets must pass the guest boundary");
        assert!(
            index_patch_requires_project_context(&patch),
            "a project-generated execution template must fail closed without ProjectContext"
        );

        let mut invalid = patch;
        let IndexPatch::ConnectExecutionContext(connection) = &mut invalid else {
            panic!(
                "INVARIANT VIOLATED: connection fixture changed variant. This is a test bug because target mutation requires ConnectExecutionContext. Fix: preserve the fixture variant."
            );
        };
        connection.application = ExecutionContextTarget::GeneratedOwner {
            local_id: "".to_string(),
            owner_kind: Some(AbiNamespaceKind::Instance),
        };
        assert!(validate_index_patch_payloads(&[invalid])
            .expect_err("empty generated application identity must be rejected")
            .contains("invalid execution context application generated owner"));
    }

    #[test]
    fn incompatible_execution_contexts_are_rejected_deterministically() {
        let left = execution_context_fixture("z-extension");
        let mut right = execution_context_fixture("a-extension");
        right.generated_owners[0].owner_kind = AbiNamespaceKind::Singleton;

        let err = resolve_execution_context_conflicts(vec![left, right])
            .expect_err("one block cannot have competing runtime owners");

        assert_eq!(err.extension_ids, vec!["a-extension", "z-extension"]);
        assert!(err.message.contains("block execution contexts"));
    }

    #[test]
    fn incompatible_generated_namespace_kinds_are_rejected_deterministically() {
        let patch = |extension_id: &str, kind| {
            IndexPatch::DefineNamespace(ruby_fast_lsp_extension_api::DefineNamespacePatch {
                namespace: vec!["GeneratedRecord".to_string()],
                kind,
                location: SourceRange {
                    start: SourcePosition {
                        line: 1,
                        character: 8,
                    },
                    end: SourcePosition {
                        line: 1,
                        character: 13,
                    },
                },
                source: ruby_fast_lsp_extension_api::PatchSource {
                    extension_id: extension_id.to_string(),
                    macro_name: "field".to_string(),
                },
            })
        };

        let err = resolve_index_patch_conflicts(vec![
            patch(
                "z-extension",
                ruby_fast_lsp_extension_api::NamespaceDeclarationKind::Class,
            ),
            patch(
                "a-extension",
                ruby_fast_lsp_extension_api::NamespaceDeclarationKind::Module,
            ),
        ])
        .expect_err("class and module declarations for one namespace must conflict");

        assert_eq!(err.extension_ids, vec!["a-extension", "z-extension"]);
        assert!(err.message.contains("GeneratedRecord"));
    }

    #[test]
    fn incompatible_generated_reference_targets_are_rejected_deterministically() {
        let patch = |extension_id: &str, target| {
            IndexPatch::AddReference(ruby_fast_lsp_extension_api::ReferencePatch {
                target,
                location: SourceRange {
                    start: SourcePosition {
                        line: 1,
                        character: 8,
                    },
                    end: SourcePosition {
                        line: 1,
                        character: 13,
                    },
                },
                source: ruby_fast_lsp_extension_api::PatchSource {
                    extension_id: extension_id.to_string(),
                    macro_name: "association".to_string(),
                },
            })
        };

        let err = resolve_index_patch_conflicts(vec![
            patch(
                "z-extension",
                ruby_fast_lsp_extension_api::ReferenceTarget::Namespace(vec!["User".to_string()]),
            ),
            patch(
                "a-extension",
                ruby_fast_lsp_extension_api::ReferenceTarget::Namespace(
                    vec!["Account".to_string()],
                ),
            ),
        ])
        .expect_err("one source range must not resolve to incompatible generated targets");

        assert_eq!(err.extension_ids, vec!["a-extension", "z-extension"]);
        assert!(err.message.contains("reference at 1:8-1:13"));
    }

    #[test]
    fn incompatible_generated_superclasses_are_rejected_deterministically() {
        let patch = |extension_id: &str, superclass: &str| {
            IndexPatch::SetSuperclass(ruby_fast_lsp_extension_api::SetSuperclassPatch {
                namespace: vec!["GeneratedRecord".to_string()],
                superclass: vec![superclass.to_string()],
                absolute: true,
                location: SourceRange {
                    start: SourcePosition {
                        line: 1,
                        character: 8,
                    },
                    end: SourcePosition {
                        line: 1,
                        character: 13,
                    },
                },
                source: ruby_fast_lsp_extension_api::PatchSource {
                    extension_id: extension_id.to_string(),
                    macro_name: "model".to_string(),
                },
            })
        };

        let err = resolve_index_patch_conflicts(vec![
            patch("z-extension", "ApplicationRecord"),
            patch("a-extension", "ActiveRecordBase"),
        ])
        .expect_err("one generated class must not acquire competing superclasses");

        assert_eq!(err.extension_ids, vec!["a-extension", "z-extension"]);
        assert!(err.message.contains("GeneratedRecord superclass"));
    }

    #[test]
    fn invalid_generated_superclass_is_rejected_before_fact_conversion() {
        let patch = IndexPatch::SetSuperclass(ruby_fast_lsp_extension_api::SetSuperclassPatch {
            namespace: vec!["GeneratedRecord".to_string()],
            superclass: Vec::new(),
            absolute: true,
            location: SourceRange {
                start: SourcePosition {
                    line: 1,
                    character: 8,
                },
                end: SourcePosition {
                    line: 1,
                    character: 13,
                },
            },
            source: ruby_fast_lsp_extension_api::PatchSource {
                extension_id: "superclass-test".to_string(),
                macro_name: "model".to_string(),
            },
        });

        let err = validate_index_patch_payloads(&[patch])
            .expect_err("empty superclass targets must be rejected at the guest boundary");
        assert!(
            err.contains("superclass target must not be empty"),
            "got: {err}"
        );
    }

    #[test]
    fn runtime_reindex_requests_are_scoped_to_related_workspace_roots() {
        let temp = TempDir::new().expect("runtime reindex temp workspace must be created");
        let root = temp.path().join("workspace");
        let model = root.join("app/models/user.rb");
        fs::create_dir_all(model.parent().expect("model path must have parent"))
            .expect("runtime reindex model directory must be created");
        fs::write(&model, "class User\nend\n")
            .expect("runtime reindex model fixture must be written");
        let root_label = normalized_path(&root);
        let unrelated = temp.path().join("other");
        let requests = vec![ruby_fast_lsp_extension_api::ReindexFile {
            workspace_root: root_label.clone(),
            path: "app/models/user.rb".to_string(),
        }];

        let uris = validate_extension_reindex_files(
            "rails-ruby",
            &[root.clone(), unrelated],
            std::slice::from_ref(&root),
            &requests,
        )
        .expect("a related workspace-relative runtime reindex request must be accepted");
        assert_eq!(uris.len(), 1);
        assert_eq!(
            uris[0].to_file_path().expect("file URI"),
            fs::canonicalize(model).expect("model fixture must canonicalize")
        );

        let traversal = vec![ruby_fast_lsp_extension_api::ReindexFile {
            workspace_root: root_label,
            path: "../secret.rb".to_string(),
        }];
        let err = validate_extension_reindex_files(
            "rails-ruby",
            std::slice::from_ref(&root),
            std::slice::from_ref(&root),
            &traversal,
        )
        .expect_err("runtime reindex requests must reject parent traversal");
        assert!(err.to_string().contains("workspace-relative"), "got: {err}");
    }

    #[test]
    fn generated_superclass_requires_class_declaration_from_same_guest_output() {
        let patch = IndexPatch::SetSuperclass(ruby_fast_lsp_extension_api::SetSuperclassPatch {
            namespace: vec!["GeneratedRecord".to_string()],
            superclass: vec!["BaseRecord".to_string()],
            absolute: true,
            location: SourceRange {
                start: SourcePosition {
                    line: 1,
                    character: 8,
                },
                end: SourcePosition {
                    line: 1,
                    character: 13,
                },
            },
            source: ruby_fast_lsp_extension_api::PatchSource {
                extension_id: "superclass-test".to_string(),
                macro_name: "model".to_string(),
            },
        });

        let err = validate_index_patch_payloads(&[patch]).expect_err(
            "a superclass patch must not override a parser-owned or separately generated class",
        );
        assert!(
            err.contains("requires a matching generated class declaration"),
            "got: {err}"
        );
    }

    #[test]
    fn invalid_generated_reference_target_is_rejected_before_fact_conversion() {
        let patch = IndexPatch::AddReference(ruby_fast_lsp_extension_api::ReferencePatch {
            target: ruby_fast_lsp_extension_api::ReferenceTarget::Namespace(Vec::new()),
            location: SourceRange {
                start: SourcePosition {
                    line: 1,
                    character: 8,
                },
                end: SourcePosition {
                    line: 1,
                    character: 13,
                },
            },
            source: ruby_fast_lsp_extension_api::PatchSource {
                extension_id: "reference-test".to_string(),
                macro_name: "association".to_string(),
            },
        });

        let err = validate_index_patch_payloads(&[patch])
            .expect_err("empty generated reference targets must be rejected");
        assert!(err.contains("must not be empty"), "got: {err}");

        let method_patch = IndexPatch::AddReference(ruby_fast_lsp_extension_api::ReferencePatch {
            target: ruby_fast_lsp_extension_api::ReferenceTarget::Method {
                namespace: vec!["User".to_string()],
                owner_kind: AbiNamespaceKind::Instance,
                name: "not a method".to_string(),
            },
            location: SourceRange {
                start: SourcePosition {
                    line: 2,
                    character: 14,
                },
                end: SourcePosition {
                    line: 2,
                    character: 26,
                },
            },
            source: ruby_fast_lsp_extension_api::PatchSource {
                extension_id: "reference-test".to_string(),
                macro_name: "before_save".to_string(),
            },
        });
        let err = validate_index_patch_payloads(&[method_patch])
            .expect_err("invalid extension method targets must be rejected");
        assert!(err.contains("invalid reference method name"), "got: {err}");
    }

    #[test]
    fn invalid_generated_constant_metadata_is_rejected_before_fact_conversion() {
        let patch = IndexPatch::DefineConstant(ruby_fast_lsp_extension_api::DefineConstantPatch {
            namespace: vec!["GeneratedRecord".to_string()],
            name: "not-a-constant".to_string(),
            location: SourceRange {
                start: SourcePosition {
                    line: 1,
                    character: 8,
                },
                end: SourcePosition {
                    line: 1,
                    character: 13,
                },
            },
            ruby_type: Some(ruby_fast_lsp_extension_api::RubyType::Named(
                "String".to_string(),
            )),
            source: ruby_fast_lsp_extension_api::PatchSource {
                extension_id: "constant-test".to_string(),
                macro_name: "field".to_string(),
            },
        });

        let err = validate_index_patch_payloads(&[patch])
            .expect_err("invalid generated constant names must be rejected at guest boundary");

        assert!(err.contains("invalid constant name"), "got: {err}");
    }

    #[test]
    fn structured_extension_types_are_canonical_and_order_independent() {
        use ruby_fast_lsp_extension_api::RubyType as ExtensionRubyType;

        let left = ExtensionRubyType::Union(vec![
            ExtensionRubyType::Array(vec![ExtensionRubyType::Named("String".to_string())]),
            ExtensionRubyType::Named("NilClass".to_string()),
        ]);
        let right = ExtensionRubyType::Union(vec![
            ExtensionRubyType::Named("NilClass".to_string()),
            ExtensionRubyType::Array(vec![ExtensionRubyType::Named("String".to_string())]),
        ]);

        assert!(extension_ruby_types_semantically_equal(
            Some(&left),
            Some(&right)
        ));
        assert_eq!(
            analysis_ruby_type_from_extension(Some(&left))
                .expect("valid structured type must convert")
                .expect("structured type must produce an analysis type")
                .to_string(),
            "(NilClass | Array<String>)"
        );
    }

    #[test]
    fn malformed_or_excessively_nested_extension_types_are_rejected() {
        use ruby_fast_lsp_extension_api::RubyType as ExtensionRubyType;

        let empty_array = ExtensionRubyType::Array(Vec::new());
        let empty_err = analysis_ruby_type_from_extension(Some(&empty_array))
            .expect_err("empty collection type payloads must be rejected");
        assert!(empty_err.contains("must not be empty"), "got: {empty_err}");

        let mut nested = ExtensionRubyType::Named("String".to_string());
        for _ in 0..10 {
            nested = ExtensionRubyType::Array(vec![nested]);
        }
        let depth_err = analysis_ruby_type_from_extension(Some(&nested))
            .expect_err("deeply nested guest types must be bounded");
        assert!(depth_err.contains("maximum depth"), "got: {depth_err}");
    }

    #[test]
    fn extension_index_patch_provenance_must_match_manifest_identity() {
        let patch = IndexPatch::ApplyMixin(ruby_fast_lsp_extension_api::ApplyMixinPatch {
            namespace: vec!["Widget".to_string()],
            owner_target: None,
            target_kind: AbiNamespaceKind::Instance,
            mixin_target: None,
            mixin: vec!["Shared".to_string()],
            absolute: true,
            kind: ruby_fast_lsp_extension_api::MixinKind::Include,
            location: SourceRange {
                start: SourcePosition {
                    line: 0,
                    character: 0,
                },
                end: SourcePosition {
                    line: 0,
                    character: 1,
                },
            },
            source: ruby_fast_lsp_extension_api::PatchSource {
                extension_id: "spoofed-extension".to_string(),
                macro_name: "shared".to_string(),
            },
        });

        let spoofed = validate_index_patch_provenance("loaded-extension", &[patch])
            .expect_err("guest patch provenance must not impersonate another extension");

        assert_eq!(spoofed, "spoofed-extension");
    }

    #[test]
    fn extension_response_patch_provenance_must_match_manifest_identity() {
        let zero = SourcePosition {
            line: 0,
            character: 0,
        };
        let patch =
            ResponsePatch::DocumentSymbol(ruby_fast_lsp_extension_api::DocumentSymbolPatch {
                name: "Example".to_string(),
                detail: None,
                kind: "Method".to_string(),
                range: SourceRange {
                    start: zero,
                    end: zero,
                },
                selection_range: SourceRange {
                    start: zero,
                    end: zero,
                },
                source: ruby_fast_lsp_extension_api::PatchSource {
                    extension_id: "spoofed-extension".to_string(),
                    macro_name: "symbol".to_string(),
                },
            });

        let spoofed = validate_response_patch_provenance("loaded-extension", &[patch])
            .expect_err("guest response provenance must not impersonate another extension");

        assert_eq!(spoofed, "spoofed-extension");
    }

    #[test]
    fn invalid_extension_method_metadata_is_rejected_before_fact_conversion() {
        let patch = IndexPatch::DefineMethod(ruby_fast_lsp_extension_api::DefineMethodPatch {
            name: "generated".to_string(),
            namespace: vec!["Widget".to_string()],
            owner_target: None,
            owner_kind: AbiNamespaceKind::Instance,
            visibility: ruby_fast_lsp_extension_api::MethodVisibility::Public,
            location: SourceRange {
                start: SourcePosition {
                    line: 1,
                    character: 0,
                },
                end: SourcePosition {
                    line: 1,
                    character: 9,
                },
            },
            params: Vec::new(),
            return_type: Some(ruby_fast_lsp_extension_api::RubyType::Named(
                "not a Ruby type".to_string(),
            )),
            return_type_source: None,
            source: ruby_fast_lsp_extension_api::PatchSource {
                extension_id: "metadata-test".to_string(),
                macro_name: "generated".to_string(),
            },
        });

        let err = validate_index_patch_payloads(&[patch.clone()])
            .expect_err("invalid extension return type must be rejected at guest boundary");

        assert!(err.contains("invalid named Ruby type"), "got: {err}");

        let IndexPatch::DefineMethod(mut conflicting_return_source) = patch else {
            panic!("INVARIANT VIOLATED: the method validation fixture changed patch variants. This is a test bug because the return-source invariant applies only to DefineMethod. Fix: keep this fixture as DefineMethod.");
        };
        conflicting_return_source.return_type = Some(ruby_fast_lsp_extension_api::RubyType::Named(
            "Widget".to_string(),
        ));
        conflicting_return_source.return_type_source =
            Some(ruby_fast_lsp_extension_api::MethodReturnTypeSource::Block);
        let err =
            validate_index_patch_payloads(&[IndexPatch::DefineMethod(conflicting_return_source)])
                .expect_err("a method return must not have both explicit and inferred sources");
        assert!(
            err.contains("either `return_type` or `return_type_source`"),
            "got: {err}"
        );
    }

    #[test]
    fn initialization_option_direct_wasm_in_directory_is_skipped() {
        let temp_dir = TempDir::new().expect("test temp dir must be created");
        fs::write(temp_dir.path().join("extension.wasm"), b"not real wasm")
            .expect("test wasm marker must be written");

        let config = ExtensionLoadConfig {
            package_paths: Vec::new(),
            directory_paths: vec![ConfiguredExtensionPath {
                path: temp_dir.path().to_path_buf(),
                source: ExtensionPathSource::InitializationOptions,
            }],
            project_package_paths: Vec::new(),
            settings: BTreeMap::new(),
        };

        let extensions = load_wasm_extensions(&config);
        assert!(
            extensions.is_empty(),
            "INVARIANT VIOLATED: initialization option directory loaded a raw wasm file. \
             This is a bug because editor extension directories must contain manifest packages. \
            Fix: keep raw wasm loading scoped to environment/dev paths."
        );
    }

    #[test]
    fn invalid_document_symbol_kind_is_recoverable_error() {
        let zero = SourcePosition {
            line: 0,
            character: 0,
        };
        let patch =
            ResponsePatch::DocumentSymbol(ruby_fast_lsp_extension_api::DocumentSymbolPatch {
                name: "Example".to_string(),
                detail: None,
                kind: "NotASymbolKind".to_string(),
                range: SourceRange {
                    start: zero,
                    end: zero,
                },
                selection_range: SourceRange {
                    start: zero,
                    end: zero,
                },
                source: ruby_fast_lsp_extension_api::PatchSource {
                    extension_id: "test".to_string(),
                    macro_name: "symbol".to_string(),
                },
            });

        let err = response_patch_to_document_symbol(patch)
            .expect_err("invalid symbol kind must be a recoverable extension error");
        assert!(
            err.contains("unsupported document symbol kind"),
            "INVARIANT VIOLATED: invalid extension document symbol kind did not produce a clear error. \
             This is a bug because extension response patches must disable the extension instead of panicking. \
             Fix: keep symbol kind conversion on the recoverable error path."
        );
    }
}
