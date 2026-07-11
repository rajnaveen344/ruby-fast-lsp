use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use globset::{Glob, GlobSet, GlobSetBuilder};
use log::warn;
use once_cell::sync::Lazy;
use parking_lot::{Mutex, RwLock};
use ruby_fast_lsp_extension_api::{
    Argument, ArgumentValue, CallContext, DocumentContext, Extension, ExtensionEvent, IndexPatch,
    Keyword, NamespaceKind as AbiNamespaceKind, ProcessRequest, ProcessResult, ProcessResultStatus,
    Receiver, ResolvedCall, ResolvedCallee, ResponsePatch, SourcePosition, SourceRange,
    WatchedFileChange, WatchedFileChangeKind,
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
use ruby_analysis::core::{
    FullyQualifiedName, GraphNodeKind, MethodCalleeResolution, MethodFact, NamespaceKind,
    ReferenceCandidate, RubyConstant, RubyMethod, RubyType as AnalysisRubyType, SourceKind,
    SymbolFact, SymbolKind as AnalysisSymbolKind, TextRange, TypeFact, TypeProvenance, TypeSubject,
};
use ruby_analysis::engine::{FileFacts, ResolveMode, SourceFileInput};
use ruby_analysis::indexer as utils;
use ruby_analysis::indexer::fact_collector::{FactCollector, FactCollectorExtensionHost};
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

#[derive(Clone)]
pub struct ExtensionRegistryHandle {
    inner: Arc<RwLock<ExtensionRegistry>>,
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
    semantic_seeded: Mutex<bool>,
    load_config: ExtensionLoadConfig,
    discovery_fingerprint: [u8; 32],
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
    status: Mutex<ExtensionStatus>,
    indexed_call_names: BTreeSet<String>,
    semantic_targets: Vec<ExtensionMethodTarget>,
    watched_file_matcher: GlobSet,
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
}

#[derive(Clone, Debug, Deserialize)]
struct ExtensionBuildManifest {
    output: String,
}

#[derive(Clone, Debug, Deserialize)]
struct ExtensionIndexingManifest {
    call_names: Vec<String>,
    #[serde(default)]
    targets: Vec<ExtensionMethodTargetManifest>,
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
    pub fn from_environment() -> Self {
        Self {
            inner: Arc::new(RwLock::new(ExtensionRegistry::load(
                &ExtensionLoadConfig::from_environment(),
            ))),
        }
    }

    pub fn from_config(config: &RubyFastLspConfig) -> Self {
        Self {
            inner: Arc::new(RwLock::new(ExtensionRegistry::load(
                &ExtensionLoadConfig::from_config(config),
            ))),
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
        let mut registry = self.inner.write();
        if registry.same_discovery(&load_config) && registry.all_extensions_loaded() {
            registry.update_settings(load_config.settings.clone());
            registry.load_config.settings = load_config.settings;
            return;
        }

        let replacement = ExtensionRegistry::load(&load_config);
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
    ) {
        self.inner.read().ensure_semantic_seed_facts(engine);
    }

    pub fn process_call_node(&self, visitor: &mut FactCollector, node: &CallNode) {
        process_call_node_with_registry(self, visitor, node);
    }

    pub fn document_symbols(&self, uri: &str, text: &str) -> Vec<DocumentSymbol> {
        document_symbols_with_registry(self, uri, text)
    }

    pub fn code_lenses(&self, uri: &str, text: &str) -> Vec<CodeLens> {
        code_lenses_with_registry(self, uri, text)
    }

    pub fn watcher_globs(&self) -> Vec<String> {
        self.inner.read().watcher_globs()
    }

    pub async fn handle_watched_file_changes(
        &self,
        workspace_trusted: bool,
        workspace_roots: &[PathBuf],
        changes: &[FileEvent],
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
                    pending.loaded.fail(err.to_string());
                    continue;
                }
            };
            let result = run_extension_process(validated).await;
            let event = ExtensionEvent {
                event: "process.completed".to_string(),
                call: None,
                document: None,
                settings: None,
                files: None,
                process_results: Some(vec![result]),
            };
            let mut extension = pending.loaded.extension.lock();
            match extension.handle_event(&event) {
                Ok(output)
                    if output.index_patches.is_empty()
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
                            drop(extension);
                            pending.loaded.fail(err.to_string());
                        }
                    }
                }
                Ok(_) => {
                    drop(extension);
                    pending.loaded.fail(format!(
                        "extension `{}` returned output from `process.completed`; process completion callbacks may update private extension state only",
                        pending.loaded.metadata.id
                    ));
                }
                Err(err) => {
                    drop(extension);
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
}

impl FactCollectorExtensionHost for ExtensionRegistryHandle {
    fn process_call_node(&self, visitor: &mut FactCollector, node: &CallNode) {
        ExtensionRegistryHandle::process_call_node(self, visitor, node);
    }

    fn should_track_enclosing_call(&self, visitor: &FactCollector, node: &CallNode) -> bool {
        self.inner.read().should_track_enclosing_call(visitor, node)
    }

    fn resolved_call_for_stack(&self, visitor: &FactCollector, node: &CallNode) -> ResolvedCall {
        resolved_call_for_stack(visitor, node)
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
    fn load(config: &ExtensionLoadConfig) -> Self {
        let packages = discover_extension_packages(config);
        let discovery_fingerprint = extension_packages_fingerprint(&packages);
        let extensions = load_wasm_extensions_from_packages(packages);
        let tracked_call_names = tracked_call_names(&extensions);
        let registry = Self {
            extensions,
            tracked_call_names,
            semantic_seeded: Mutex::new(false),
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

    fn should_track_enclosing_call(&self, visitor: &FactCollector, node: &CallNode) -> bool {
        let method_name = utils::utf8_str(node.name().as_slice());
        if !self.tracked_call_names.contains(method_name) {
            return false;
        }

        if self.extensions.iter().any(|extension| {
            extension.is_loaded()
                && extension
                    .semantic_targets
                    .iter()
                    .any(|target| target.frame && target.method.as_str() == method_name)
                && extension.semantically_matches_call(visitor, node)
        }) {
            return true;
        }

        if !visitor.extension_call_stack.is_empty()
            && self.extensions.iter().any(|extension| {
                extension.is_loaded() && extension.can_run_inside_extension_frame(visitor, node)
            })
        {
            return true;
        }

        !self.has_loaded_wasm_for_call(method_name)
            && ruby_fast_lsp_extension_rspec::extension()
                .indexed_call_names()
                .contains(&method_name)
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
    ) {
        let mut seeded = self.semantic_seeded.lock();
        if *seeded {
            return;
        }

        let mut engine = engine.write();
        let file_id = engine.register_file(SourceFileInput {
            path: PathBuf::from("/__ruby_fast_lsp_extension__/semantic_targets.rb"),
            content: String::new(),
            kind: SourceKind::Stub,
        });
        let range = TextRange::new(file_id, 0, 0);
        let mut facts = FileFacts::default();
        for extension in &self.extensions {
            if !extension.is_loaded() {
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
        engine.replace_facts(file_id, facts, ResolveMode::Deferred);
        *seeded = true;
    }
}

impl LoadedWasmExtension {
    fn new(
        metadata: ExtensionMetadata,
        extension: ruby_fast_lsp_extension_wasm_host::WasmExtension,
        semantic_targets: Vec<ExtensionMethodTarget>,
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
            status: Mutex::new(ExtensionStatus::Discovered),
            indexed_call_names,
            semantic_targets,
            watched_file_matcher,
        }
    }

    fn is_loaded(&self) -> bool {
        *self.status.lock() == ExtensionStatus::Loaded
    }

    fn handle_lifecycle_event(&self, event_name: &str, settings: Option<serde_json::Value>) {
        let event = ExtensionEvent {
            event: event_name.to_string(),
            call: None,
            document: None,
            settings,
            files: None,
            process_results: None,
        };
        let mut extension = self.extension.lock();
        match extension.handle_event(&event) {
            Ok(output)
                if output.index_patches.is_empty()
                    && output.response_patches.is_empty()
                    && output.command_patches.is_empty() =>
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
            Ok(_) => self.fail(format!(
                "extension `{}` returned patches from `{event_name}`; lifecycle events must not mutate semantic or editor state",
                self.metadata.id
            )),
            Err(err) => self.fail(format!(
                "extension `{}` {event_name} failed: {err}",
                self.metadata.id
            )),
        }
    }

    fn fail(&self, reason: impl Into<String>) {
        *self.status.lock() = ExtensionStatus::from_failure(reason);
    }

    fn status_report(&self) -> ExtensionStatusReport {
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
        }
    }

    fn handles_call(&self, method_name: &str) -> bool {
        self.indexed_call_names.contains(method_name)
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

    fn can_run_inside_extension_frame(&self, visitor: &FactCollector, node: &CallNode) -> bool {
        !visitor.extension_call_stack.is_empty()
            && self.handles_call(utils::utf8_str(node.name().as_slice()))
    }
}

fn tracked_call_names(extensions: &[Arc<LoadedWasmExtension>]) -> BTreeSet<String> {
    let mut names = ruby_fast_lsp_extension_rspec::extension()
        .indexed_call_names()
        .iter()
        .map(|name| (*name).to_string())
        .collect::<BTreeSet<_>>();
    for extension in extensions {
        if !extension.is_loaded() {
            continue;
        }
        names.extend(extension.indexed_call_names.iter().cloned());
    }
    names
}

fn extension_target_owner_exists(visitor: &FactCollector, target: &ExtensionMethodTarget) -> bool {
    let required_owner = FullyQualifiedName::namespace(target.owner.clone());
    let engine = visitor.analysis_engine.read();
    ruby_analysis::engine::AnalysisQuery::new(&engine)
        .known_namespace_fqns()
        .contains(&required_owner)
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
    process_call_node_with_registry(&EXTENSION_REGISTRY, visitor, node);
}

fn process_call_node_with_registry(
    registry: &ExtensionRegistryHandle,
    visitor: &mut FactCollector,
    node: &CallNode,
) {
    if process_wasm_call_node(registry, visitor, node) {
        return;
    }
    let method_name = utils::utf8_str(node.name().as_slice());
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

    let ctx = call_context(visitor, node);
    for patch in rspec.index_call(&ctx) {
        apply_patch(visitor, patch);
    }
}

pub fn document_symbols(uri: &str, text: &str) -> Vec<DocumentSymbol> {
    document_symbols_with_registry(&EXTENSION_REGISTRY, uri, text)
}

fn document_symbols_with_registry(
    registry: &ExtensionRegistryHandle,
    uri: &str,
    text: &str,
) -> Vec<DocumentSymbol> {
    let mut symbols = Vec::new();
    handle_response_event(registry, "request.document_symbol", uri, text, |patch| {
        match response_patch_to_document_symbol(patch) {
            Ok(Some(symbol)) => {
                symbols.push(symbol);
                Ok(())
            }
            Ok(None) => Ok(()),
            Err(err) => Err(err),
        }
    });
    symbols
}

pub fn code_lenses(uri: &str, text: &str) -> Vec<CodeLens> {
    code_lenses_with_registry(&EXTENSION_REGISTRY, uri, text)
}

fn code_lenses_with_registry(
    registry: &ExtensionRegistryHandle,
    uri: &str,
    text: &str,
) -> Vec<CodeLens> {
    let mut lenses = Vec::new();
    handle_response_event(registry, "request.code_lens", uri, text, |patch| {
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
    mut handle_patch: impl FnMut(ResponsePatch) -> Result<(), String>,
) {
    let event = ExtensionEvent {
        event: event_name.to_string(),
        call: None,
        document: Some(DocumentContext {
            uri: uri.to_string(),
            text: text.to_string(),
        }),
        settings: None,
        files: None,
        process_results: None,
    };
    let extensions = registry.extensions();

    for loaded in extensions {
        if !loaded.is_loaded() {
            continue;
        }

        let mut extension = loaded.extension.lock();
        let extension_output = match extension.handle_event(&event) {
            Ok(extension_output) => extension_output,
            Err(err) => {
                let extension_id = extension.id().to_string();
                warn!(
                    "Disabling Ruby Fast LSP extension `{}` after event `{}` failure: {}",
                    extension_id, event_name, err
                );
                let reason = err.to_string();
                drop(extension);
                loaded.fail(reason);
                continue;
            }
        };
        for patch in extension_output.response_patches {
            if let Err(err) = handle_patch(patch) {
                let extension_id = extension.id().to_string();
                warn!(
                    "Disabling Ruby Fast LSP extension `{}` after invalid response patch for `{}`: {}",
                    extension_id, event_name, err
                );
                drop(extension);
                loaded.fail(err);
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
            settings: None,
            files: Some(matched),
            process_results: None,
        };
        let mut extension = loaded.extension.lock();
        match extension.handle_event(&event) {
            Ok(output)
                if output.index_patches.is_empty()
                    && output.response_patches.is_empty()
                    && output.command_patches.is_empty() =>
            {
                if output.process_requests.len() > MAX_PROCESS_REQUESTS_PER_EVENT {
                    drop(extension);
                    loaded.fail(format!(
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
                    drop(extension);
                    loaded.fail(format!(
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
                drop(extension);
                loaded.fail(format!(
                    "extension `{}` returned patches from `files.changed`; watched-file events may update private extension state only",
                    loaded.metadata.id
                ));
            }
            Err(err) => {
                drop(extension);
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

async fn run_extension_process(request: ValidatedExtensionProcessRequest) -> ProcessResult {
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
) -> bool {
    let method_name = utils::utf8_str(node.name().as_slice());
    let extensions = registry.extensions();
    let mut emitted = Vec::new();
    let mut emitters = BTreeMap::new();

    for loaded in extensions {
        if !loaded.is_loaded() {
            continue;
        }
        if !loaded.handles_call(method_name) {
            continue;
        }
        if loaded.has_semantic_targets()
            && !loaded.semantically_matches_call(visitor, node)
            && !loaded.can_run_inside_extension_frame(visitor, node)
        {
            continue;
        }
        let mut extension = loaded.extension.lock();

        let ctx = call_context(visitor, node);
        let patches = match extension.index_call(&ctx) {
            Ok(patches) => patches,
            Err(err) => {
                let extension_id = extension.id().to_string();
                warn!(
                    "Disabling Ruby Fast LSP extension `{}` after indexing failure on `{}`: {}",
                    extension_id, method_name, err
                );
                let reason = err.to_string();
                drop(extension);
                loaded.fail(reason);
                continue;
            }
        };
        if patches.is_empty() {
            continue;
        }
        if let Err(spoofed_id) = validate_index_patch_provenance(&loaded.metadata.id, &patches) {
            drop(extension);
            loaded.fail(format!(
                "extension `{}` emitted an index patch attributed to `{spoofed_id}`; patch provenance must match the loaded manifest id",
                loaded.metadata.id
            ));
            continue;
        }
        if let Err(err) = validate_index_patch_payloads(&patches) {
            drop(extension);
            loaded.fail(format!(
                "extension `{}` emitted an invalid index patch: {err}",
                loaded.metadata.id
            ));
            continue;
        }
        emitters.insert(loaded.metadata.id.clone(), Arc::clone(&loaded));
        emitted.extend(patches);
    }

    if emitted.is_empty() {
        return false;
    }
    let mut pending = emitted;
    let patches = loop {
        match resolve_index_patch_conflicts(pending.clone()) {
            Ok(patches) => break patches,
            Err(conflict) => {
                let rejected_ids = conflict
                    .extension_ids
                    .iter()
                    .cloned()
                    .collect::<BTreeSet<_>>();
                for extension_id in &conflict.extension_ids {
                    let loaded = emitters.get(extension_id).expect(
                        "INVARIANT VIOLATED: conflicting patch source has no emitting extension. This is a bug because provenance is validated before conflict resolution. Fix: keep emitter registration adjacent to accepted patch collection.",
                    );
                    loaded.fail(conflict.message.clone());
                }
                warn!(
                    "Rejecting conflicting extension index patches: {}",
                    conflict.message
                );
                pending.retain(|patch| !rejected_ids.contains(index_patch_extension_id(patch)));
                if pending.is_empty() {
                    return false;
                }
            }
        }
    };
    for patch in patches {
        apply_patch(visitor, patch);
    }
    true
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
            namespace: method.namespace.clone(),
            owner_kind: namespace_kind_name(method.owner_kind).to_string(),
            name: method.name.clone(),
        },
        IndexPatch::SetSuperclass(superclass) => IndexPatchIdentity::Superclass {
            namespace: superclass.namespace.clone(),
        },
        IndexPatch::ApplyMixin(mixin) => IndexPatchIdentity::Mixin {
            namespace: mixin.namespace.clone(),
            target_kind: namespace_kind_name(mixin.target_kind).to_string(),
            mixin: mixin.mixin.clone(),
            kind: match mixin.kind {
                ruby_fast_lsp_extension_api::MixinKind::Include => "include",
                ruby_fast_lsp_extension_api::MixinKind::Prepend => "prepend",
                ruby_fast_lsp_extension_api::MixinKind::Extend => "extend",
            }
            .to_string(),
        },
    }
}

fn namespace_kind_name(kind: AbiNamespaceKind) -> &'static str {
    match kind {
        AbiNamespaceKind::Instance => "instance",
        AbiNamespaceKind::Singleton => "singleton",
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
                validate_source_range(method.location, "method location")?;
                if method.params.iter().any(|param| param.name.is_empty()) {
                    return Err("method parameter names must not be empty".to_string());
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
                validate_extension_namespace(&mixin.mixin, "mixin target")?;
                validate_source_range(mixin.location, "mixin location")?;
            }
        }
    }
    for superclass in patches.iter().filter_map(|patch| match patch {
        IndexPatch::SetSuperclass(superclass) => Some(superclass),
        IndexPatch::DefineNamespace(_)
        | IndexPatch::DefineConstant(_)
        | IndexPatch::AddReference(_)
        | IndexPatch::DefineMethod(_)
        | IndexPatch::ApplyMixin(_) => None,
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
            | IndexPatch::ApplyMixin(_) => false,
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
                && left.owner_kind == right.owner_kind
                && left.visibility == right.visibility
                && left.location == right.location
                && left.params == right.params
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
                && left.target_kind == right.target_kind
                && left.mixin == right.mixin
                && left.absolute == right.absolute
                && left.kind == right.kind
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
        | (IndexPatch::ApplyMixin(_), IndexPatch::DefineMethod(_)) => false,
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
        match fs::read(&package.wasm_path) {
            Ok(bytes) => {
                digest.update([1]);
                digest.update((bytes.len() as u64).to_le_bytes());
                digest.update(bytes);
            }
            Err(err) => {
                digest.update([0]);
                let error = err.to_string();
                digest.update((error.len() as u64).to_le_bytes());
                digest.update(error.as_bytes());
            }
        }
    }
    digest.finalize().into()
}

#[cfg(test)]
fn load_wasm_extensions(config: &ExtensionLoadConfig) -> Vec<Arc<LoadedWasmExtension>> {
    load_wasm_extensions_from_packages(discover_extension_packages(config))
}

fn load_wasm_extensions_from_packages(
    packages: Vec<ExtensionPackage>,
) -> Vec<Arc<LoadedWasmExtension>> {
    let mut extension_ids = BTreeSet::new();
    packages
        .into_iter()
        .filter_map(|package| match load_wasm_extension(package) {
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
        output.push(ExtensionPackage {
            wasm_path,
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
            output.push(ExtensionPackage {
                wasm_path: entry_path,
                manifest: None,
                source: configured_path.source,
                explicit_package: false,
            });
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
    let id = package
        .manifest
        .as_ref()
        .map(|manifest| manifest.id.clone())
        .unwrap_or_else(|| wasm_file_stem(&package.wasm_path));

    if let Some(manifest) = &package.manifest {
        validate_manifest(manifest)?;
        validate_manifest_checksum(manifest, &package.wasm_path)?;
    }
    let metadata = extension_metadata(&id, package.manifest.as_ref());

    let mut extension =
        ruby_fast_lsp_extension_wasm_host::WasmExtension::from_file(id.clone(), &package.wasm_path)
            .map_err(|err| {
                ExtensionLoadError::new(format!(
                    "failed to load Wasm extension `{}` from `{}`: {}",
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

    Ok(Arc::new(LoadedWasmExtension::new(
        metadata,
        extension,
        semantic_targets,
    )))
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
    if manifest.runtime != "mruby-wasm" {
        return Err(ExtensionLoadError::new(format!(
            "extension `{}` runtime `{}` is unsupported",
            manifest.id, manifest.runtime
        )));
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
    wasm_path: &Path,
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
    let wasm_bytes = fs::read(wasm_path).map_err(|err| {
        ExtensionLoadError::new(format!(
            "failed to read extension `{}` wasm for checksum `{}`: {}",
            manifest.id,
            wasm_path.display(),
            err
        ))
    })?;
    let actual = format!("{:x}", Sha256::digest(&wasm_bytes));
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

fn call_context(visitor: &FactCollector, node: &CallNode) -> CallContext {
    let receiver = node
        .receiver()
        .map(|receiver| receiver_from_node(&receiver))
        .unwrap_or(Receiver::None);
    CallContext {
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
        resolved_callees,
        call_range: source_range(visitor, &node.location()),
        message_range: node
            .message_loc()
            .map(|loc| source_range(visitor, &loc))
            .unwrap_or_else(|| source_range(visitor, &node.location())),
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

fn apply_patch(visitor: &mut FactCollector, patch: IndexPatch) {
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
            };
            let range = visitor
                .document
                .lsp_range_to_text_range(range_from_abi(reference.location));
            visitor
                .reference_candidates
                .push(ReferenceCandidate::resolved(range, target, None));
        }
        IndexPatch::DefineMethod(method) => {
            let return_type = analysis_ruby_type_from_extension(method.return_type.as_ref())
                .expect("INVARIANT VIOLATED: extension return type reached application without validation. This is a bug because guest patches must be validated before conflict resolution. Fix: keep validate_index_patch_payloads before emitted patch collection.");
            let namespace = method
                .namespace
                .iter()
                .map(|part| RubyConstant::new(part).expect(
                    "INVARIANT VIOLATED: extension method namespace reached application without validation. This is a bug because guest patches must be validated before conflict resolution. Fix: keep validate_index_patch_payloads before emitted patch collection.",
                ))
                .collect::<Vec<_>>();
            let ruby_method = RubyMethod::new(&method.name).expect(
                "INVARIANT VIOLATED: extension method name reached application without validation. This is a bug because guest patches must be validated before conflict resolution. Fix: keep validate_index_patch_payloads before emitted patch collection.",
            );
            let fqn = FullyQualifiedName::method(namespace, ruby_method);
            let range = visitor
                .document
                .lsp_range_to_text_range(range_from_abi(method.location));
            visitor.direct_push_method_fact_with_visibility(
                fqn.namespace_parts(),
                match method.owner_kind {
                    AbiNamespaceKind::Instance => NamespaceKind::Instance,
                    AbiNamespaceKind::Singleton => NamespaceKind::Singleton,
                },
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
                visitor.type_store.add(TypeFact::new(
                    TypeSubject::MethodReturn(fqn),
                    return_type,
                    range,
                    TypeProvenance::Extension,
                ));
            }
        }
        IndexPatch::SetSuperclass(_) => {}
        IndexPatch::ApplyMixin(_) => {}
    }
    visitor.extension_index_patches.push(patch);
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

        let result = run_extension_process(validated).await;

        assert_eq!(result.request_id, "version");
        assert_eq!(result.status, ProcessResultStatus::Exited);
        assert_eq!(result.exit_code, Some(0));
        assert!(result.stdout.starts_with("rustc "));
        assert!(!result.stdout_truncated);
        assert!(!result.stderr_truncated);
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

        let result = run_extension_process(validated).await;

        assert_eq!(result.status, ProcessResultStatus::TimedOut);
        assert_eq!(result.exit_code, None);
    }

    #[test]
    fn incompatible_extension_index_patches_are_rejected_deterministically() {
        let patch = |extension_id: &str, return_type| {
            IndexPatch::DefineMethod(ruby_fast_lsp_extension_api::DefineMethodPatch {
                name: "factory".to_string(),
                namespace: vec!["Widget".to_string()],
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
                target_kind: AbiNamespaceKind::Instance,
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
            "(Array<String> | NilClass)"
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
            target_kind: AbiNamespaceKind::Instance,
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
    fn invalid_extension_method_metadata_is_rejected_before_fact_conversion() {
        let patch = IndexPatch::DefineMethod(ruby_fast_lsp_extension_api::DefineMethodPatch {
            name: "generated".to_string(),
            namespace: vec!["Widget".to_string()],
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
            source: ruby_fast_lsp_extension_api::PatchSource {
                extension_id: "metadata-test".to_string(),
                macro_name: "generated".to_string(),
            },
        });

        let err = validate_index_patch_payloads(&[patch])
            .expect_err("invalid extension return type must be rejected at guest boundary");

        assert!(err.contains("invalid named Ruby type"), "got: {err}");
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
