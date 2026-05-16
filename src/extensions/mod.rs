use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use log::warn;
use once_cell::sync::Lazy;
use parking_lot::{Mutex, RwLock};
use ruby_fast_lsp_extension_api::{
    ApplyMixinPatch, Argument, ArgumentValue, CallContext, DefineMethodPatch, DocumentContext,
    Extension, ExtensionEvent, IndexPatch, MethodVisibility, NamespaceKind as AbiNamespaceKind,
    Receiver, ResolvedCall, ResolvedCallee, ResponsePatch, RubyType as AbiRubyType, SourcePosition,
    SourceRange,
};
use ruby_prism::{CallNode, Node};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tower_lsp::lsp_types::{CodeLens, Command, DocumentSymbol, Position, Range, SymbolKind};

use crate::analyzer_prism::utils;
use crate::analyzer_prism::visitors::index_visitor::IndexVisitor;
use crate::analyzer_prism::MethodReceiver as CoreMethodReceiver;
use crate::config::RubyFastLspConfig;
use crate::indexer::entry::NamespaceKind;
use crate::indexer::entry::{
    entry_kind::{MethodParamInfo, ParamKind},
    EntryBuilder, EntryKind, MethodOrigin, MethodVisibility as CoreVisibility, MixinRef,
};
use crate::inferrer::r#type::ruby::RubyType;
use crate::query::{IndexQuery, MethodCalleeResolution};
use crate::types::compact_location::CompactLocation;
use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::types::ruby_method::RubyMethod;
use crate::types::ruby_namespace::RubyConstant;

static EXTENSION_REGISTRY: Lazy<ExtensionRegistryHandle> =
    Lazy::new(ExtensionRegistryHandle::from_environment);

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
    Loaded,
    Failed { reason: String },
}

#[derive(Default)]
struct ExtensionLoadConfig {
    package_paths: Vec<ConfiguredExtensionPath>,
    directory_paths: Vec<ConfiguredExtensionPath>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExtensionPathSource {
    Environment,
    InitializationOptions,
}

#[derive(Debug)]
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
        let load_config = ExtensionLoadConfig::from_config(config);
        *self.inner.write() = ExtensionRegistry::load(&load_config);
    }

    pub fn status_reports(&self) -> Vec<ExtensionStatusReport> {
        self.inner.read().status_reports()
    }

    pub fn process_call_node(&self, visitor: &mut IndexVisitor, node: &CallNode) {
        process_call_node_with_registry(self, visitor, node);
    }

    pub fn document_symbols(&self, uri: &str, text: &str) -> Vec<DocumentSymbol> {
        document_symbols_with_registry(self, uri, text)
    }

    pub fn code_lenses(&self, uri: &str, text: &str) -> Vec<CodeLens> {
        code_lenses_with_registry(self, uri, text)
    }

    fn extensions(&self) -> Vec<Arc<LoadedWasmExtension>> {
        self.inner.read().extensions()
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
        Self {
            extensions: load_wasm_extensions(config),
        }
    }

    fn extensions(&self) -> Vec<Arc<LoadedWasmExtension>> {
        self.extensions.clone()
    }

    fn status_reports(&self) -> Vec<ExtensionStatusReport> {
        self.extensions
            .iter()
            .map(|extension| extension.status_report())
            .collect()
    }
}

impl LoadedWasmExtension {
    fn new(
        metadata: ExtensionMetadata,
        extension: ruby_fast_lsp_extension_wasm_host::WasmExtension,
    ) -> Self {
        Self {
            metadata,
            extension: Mutex::new(extension),
            status: Mutex::new(ExtensionStatus::Loaded),
        }
    }

    fn is_loaded(&self) -> bool {
        *self.status.lock() == ExtensionStatus::Loaded
    }

    fn fail(&self, reason: impl Into<String>) {
        *self.status.lock() = ExtensionStatus::Failed {
            reason: reason.into(),
        };
    }

    fn status_report(&self) -> ExtensionStatusReport {
        let status_guard = self.status.lock();
        let (status, last_error) = match &*status_guard {
            ExtensionStatus::Loaded => ("loaded", None),
            ExtensionStatus::Failed { reason } => ("failed", Some(reason.clone())),
        };
        let indexed_call_names = self.extension.lock().indexed_call_names().to_vec();
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
            indexed_call_names,
        }
    }
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

pub fn process_call_node(visitor: &mut IndexVisitor, node: &CallNode) {
    process_call_node_with_registry(&EXTENSION_REGISTRY, visitor, node);
}

fn process_call_node_with_registry(
    registry: &ExtensionRegistryHandle,
    visitor: &mut IndexVisitor,
    node: &CallNode,
) {
    if process_wasm_call_node(registry, visitor, node) {
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

    let method_name = utils::utf8_str(node.name().as_slice());
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

fn process_wasm_call_node(
    registry: &ExtensionRegistryHandle,
    visitor: &mut IndexVisitor,
    node: &CallNode,
) -> bool {
    let method_name = utils::utf8_str(node.name().as_slice());
    let mut handled = false;
    let extensions = registry.extensions();

    for loaded in extensions {
        if !loaded.is_loaded() {
            continue;
        }
        let mut extension = loaded.extension.lock();
        if !extension
            .indexed_call_names()
            .iter()
            .any(|name| name == method_name)
        {
            continue;
        }

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
        for patch in patches {
            apply_patch(visitor, patch);
        }
        handled = true;
    }

    handled
}

fn load_wasm_extensions(config: &ExtensionLoadConfig) -> Vec<Arc<LoadedWasmExtension>> {
    let mut packages = Vec::new();
    for configured_path in &config.package_paths {
        if let Err(err) = collect_extension_package(configured_path, &mut packages) {
            warn!("Skipping Ruby Fast LSP extension package: {}", err);
        }
    }
    for configured_path in &config.directory_paths {
        if let Err(err) = collect_extension_directory(configured_path, &mut packages) {
            warn!("Skipping Ruby Fast LSP extension directory: {}", err);
        }
    }
    packages.sort_by(|left, right| left.wasm_path.cmp(&right.wasm_path));
    packages.dedup_by(|left, right| left.wasm_path == right.wasm_path);

    packages
        .into_iter()
        .filter_map(|package| match load_wasm_extension(package) {
            Ok(extension) => Some(extension),
            Err(err) => {
                warn!("Skipping Ruby Fast LSP extension: {}", err);
                None
            }
        })
        .collect::<Vec<_>>()
}

struct ExtensionPackage {
    wasm_path: PathBuf,
    manifest: Option<ExtensionManifest>,
}

fn collect_extension_package(
    configured_path: &ConfiguredExtensionPath,
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
            if let Err(err) = collect_extension_package(&entry_path, output) {
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

    Ok(Arc::new(LoadedWasmExtension::new(metadata, extension)))
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
    }
    if let Some(process) = &manifest.process {
        validate_manifest_list("process command", &manifest.id, &process.commands)?;
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
    }
    Ok(())
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

fn call_context(visitor: &IndexVisitor, node: &CallNode) -> CallContext {
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
                    .map(|arg| argument_from_node(visitor, &arg))
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

pub fn resolved_call_for_stack(visitor: &IndexVisitor, node: &CallNode) -> ResolvedCall {
    let receiver = node
        .receiver()
        .map(|receiver| receiver_from_node(&receiver))
        .unwrap_or(Receiver::None);
    ResolvedCall {
        method_name: utils::utf8_str(node.name().as_slice()).to_string(),
        receiver: receiver.clone(),
        resolved_callees: resolved_callees_for_call(visitor, node),
        call_range: source_range(visitor, &node.location()),
        message_range: node
            .message_loc()
            .map(|loc| source_range(visitor, &loc))
            .unwrap_or_else(|| source_range(visitor, &node.location())),
    }
}

fn resolved_callees_for_call(visitor: &IndexVisitor, node: &CallNode) -> Vec<ResolvedCallee> {
    let method_name = utils::utf8_str(node.name().as_slice());
    let Ok(method) = RubyMethod::new(method_name) else {
        return Vec::new();
    };
    let core_receiver = node
        .receiver()
        .map(|receiver| core_method_receiver_from_node(visitor, &receiver))
        .unwrap_or(CoreMethodReceiver::None);
    let document = Arc::new(RwLock::new(visitor.document.clone()));
    let query = IndexQuery::with_doc(visitor.index.clone(), document);

    query
        .resolve_method_callees(
            &core_receiver,
            &method,
            &visitor.scope_tracker.get_ns_stack(),
            visitor.scope_tracker.current_method_context(),
            position_from_source(source_range(visitor, &node.location()).start),
        )
        .into_iter()
        .map(|callee| {
            let owner_kind = callee.owner.namespace_kind().unwrap_or_else(|| {
                panic!(
                    "INVARIANT VIOLATED: resolved method callee owner `{}` is not a namespace. \
                     This is a bug because methods must resolve to namespace owners. \
                     Fix: ensure IndexQuery::resolve_method_callees only returns namespace FQNs.",
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
        })
        .collect()
}

fn core_method_receiver_from_node(visitor: &IndexVisitor, node: &Node) -> CoreMethodReceiver {
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
        MethodCalleeResolution::Exact => ruby_fast_lsp_extension_api::CalleeResolution::Exact,
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

fn argument_from_node(visitor: &IndexVisitor, node: &Node) -> Argument {
    if let Some(symbol) = node.as_symbol_node() {
        return Argument {
            value: ArgumentValue::Symbol(String::from_utf8_lossy(symbol.unescaped()).to_string()),
            range: source_range(visitor, &symbol.location()),
        };
    }

    if let Some(string) = node.as_string_node() {
        return Argument {
            value: ArgumentValue::String(String::from_utf8_lossy(string.unescaped()).to_string()),
            range: source_range(visitor, &string.content_loc()),
        };
    }

    if let Some(constant) = node.as_constant_read_node() {
        return Argument {
            value: ArgumentValue::Constant(vec![
                utils::utf8_str(constant.name().as_slice()).to_string()
            ]),
            range: source_range(visitor, &constant.location()),
        };
    }

    if let Some(path) = node.as_constant_path_node() {
        let mut parts = Vec::new();
        utils::collect_namespaces(&path, &mut parts);
        return Argument {
            value: ArgumentValue::Constant(parts.iter().map(ToString::to_string).collect()),
            range: source_range(visitor, &path.location()),
        };
    }

    let value = if node.as_true_node().is_some() {
        ArgumentValue::Boolean(true)
    } else if node.as_false_node().is_some() {
        ArgumentValue::Boolean(false)
    } else if node.as_nil_node().is_some() {
        ArgumentValue::Nil
    } else {
        ArgumentValue::Unsupported
    };

    Argument {
        value,
        range: source_range(visitor, &node.location()),
    }
}

fn apply_patch(visitor: &mut IndexVisitor, patch: IndexPatch) {
    match patch {
        IndexPatch::DefineMethod(method) => apply_define_method(visitor, method),
        IndexPatch::ApplyMixin(mixin) => apply_mixin(visitor, mixin),
    }
}

fn apply_define_method(visitor: &mut IndexVisitor, patch: DefineMethodPatch) {
    assert!(
        RubyMethod::is_valid_ruby_method_name(&patch.name),
        "INVARIANT VIOLATED: extension emitted invalid method name `{}`. \
         This is a bug because RubyIndex only accepts valid Ruby method identifiers. \
         Fix: validate method names inside extension before emitting DefineMethod.",
        patch.name
    );

    let method = RubyMethod::new(&patch.name).expect(
        "INVARIANT VIOLATED: RubyMethod validation diverged. \
         This is a bug because is_valid_ruby_method_name accepted a name that RubyMethod::new rejected. \
         Fix: keep RubyMethod validators consistent.",
    );
    let namespace = ruby_constants(&patch.namespace);
    let owner_kind = namespace_kind_from_abi(patch.owner_kind);
    let fqn = FullyQualifiedName::method(namespace.clone(), method.clone());
    let owner = FullyQualifiedName::namespace_with_kind(namespace, owner_kind);
    let location = compact_location(visitor, patch.location);
    let params = method_params_from_abi(&patch.params, patch.location.end);
    let return_type = patch.return_type.map(ruby_type_from_abi);
    let visibility = visibility_from_abi(patch.visibility);

    let entry = {
        let mut index = visitor.index.lock();
        EntryBuilder::new()
            .fqn(fqn)
            .compact_location(location)
            .kind(EntryKind::new_method(
                method,
                params,
                owner,
                visibility,
                MethodOrigin::Generated {
                    extension_id: patch.source.extension_id,
                    macro_name: patch.source.macro_name,
                },
                None,
                None,
                None,
                return_type,
                Vec::new(),
            ))
            .build(&mut index)
            .expect(
                "INVARIANT VIOLATED: extension DefineMethod patch could not build index entry. \
                 This is a bug because validated extension patches must map to EntryBuilder inputs. \
                 Fix: inspect DefineMethod patch conversion.",
            )
    };

    visitor.add_entry(entry);
}

fn method_params_from_abi(
    params: &[ruby_fast_lsp_extension_api::MethodParamPatch],
    end_position: SourcePosition,
) -> Vec<MethodParamInfo> {
    params
        .iter()
        .map(|param| {
            assert!(
                !param.name.is_empty(),
                "INVARIANT VIOLATED: extension emitted method parameter with empty name. \
                 This is a bug because index MethodParamInfo requires a stable parameter identifier. \
                 Fix: validate extension DefineMethod params before emitting patches."
            );
            MethodParamInfo::new(
                param.name.clone(),
                Position {
                    line: end_position.line,
                    character: end_position.character,
                },
                param_kind_from_abi(param.kind),
            )
        })
        .collect()
}

fn param_kind_from_abi(kind: ruby_fast_lsp_extension_api::MethodParamKind) -> ParamKind {
    match kind {
        ruby_fast_lsp_extension_api::MethodParamKind::Required => ParamKind::Required,
        ruby_fast_lsp_extension_api::MethodParamKind::Optional => ParamKind::Optional,
        ruby_fast_lsp_extension_api::MethodParamKind::Rest => ParamKind::Rest,
        ruby_fast_lsp_extension_api::MethodParamKind::RequiredKeyword => ParamKind::RequiredKeyword,
        ruby_fast_lsp_extension_api::MethodParamKind::OptionalKeyword => ParamKind::OptionalKeyword,
        ruby_fast_lsp_extension_api::MethodParamKind::KeywordRest => ParamKind::KeywordRest,
        ruby_fast_lsp_extension_api::MethodParamKind::Block => ParamKind::Block,
    }
}

fn apply_mixin(visitor: &mut IndexVisitor, patch: ApplyMixinPatch) {
    let namespace = ruby_constants(&patch.namespace);
    let target_kind = namespace_kind_from_abi(patch.target_kind);
    let target_fqn = FullyQualifiedName::namespace_with_kind(namespace, target_kind);
    let location = compact_location(visitor, patch.location);
    let mixin_ref = MixinRef {
        parts: ruby_constants(&patch.mixin),
        absolute: patch.absolute,
        location,
    };

    let mut index = visitor.index.lock();
    ensure_root_mixin_target_exists(&mut index, visitor, &target_fqn);
    let Some(entry) = index.get_last_definition_mut(&target_fqn) else {
        return;
    };
    if !matches!(entry.kind, EntryKind::Class(_) | EntryKind::Module(_)) {
        return;
    }

    match patch.kind {
        ruby_fast_lsp_extension_api::MixinKind::Include => entry.add_includes(vec![mixin_ref]),
        ruby_fast_lsp_extension_api::MixinKind::Prepend => entry.add_prepends(vec![mixin_ref]),
        ruby_fast_lsp_extension_api::MixinKind::Extend => entry.add_extends(vec![mixin_ref]),
    }
}

fn ensure_root_mixin_target_exists(
    index: &mut crate::indexer::index::RubyIndex,
    visitor: &IndexVisitor,
    target_fqn: &FullyQualifiedName,
) {
    if index.get(target_fqn).is_some() {
        return;
    }
    if !target_fqn.namespace_parts().is_empty() {
        return;
    }

    let file_id = index.get_or_insert_file(&visitor.document.uri);
    let location = CompactLocation::new(
        file_id,
        tower_lsp::lsp_types::Range::new(Position::new(0, 0), Position::new(0, 0)),
    );
    let entry = EntryBuilder::new()
        .fqn(target_fqn.clone())
        .compact_location(location)
        .kind(EntryKind::Class(Box::new(
            crate::indexer::entry::entry_kind::ClassData {
                superclass: None,
                includes: Vec::new(),
                prepends: Vec::new(),
                extends: Vec::new(),
            },
        )))
        .build(index)
        .expect(
            "INVARIANT VIOLATED: failed to create Object entry for extension mixin patch. \
             This is a bug because Object target data is validated locally. \
             Fix: keep EntryBuilder requirements in sync with extension mixin target creation.",
        );
    index.add_entry(entry);
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

fn ruby_constants(parts: &[String]) -> Vec<RubyConstant> {
    parts
        .iter()
        .map(|part| {
            RubyConstant::new(part).unwrap_or_else(|err| {
                panic!(
                    "INVARIANT VIOLATED: extension emitted invalid namespace part `{}`: {}. \
                     This is a bug because namespace parts must be Ruby constants. \
                     Fix: validate namespace parts inside extension before emitting patches.",
                    part, err
                )
            })
        })
        .collect()
}

fn compact_location(visitor: &IndexVisitor, range: SourceRange) -> CompactLocation {
    let file_id = visitor
        .index
        .lock()
        .get_or_insert_file(&visitor.document.uri);
    CompactLocation::new(
        file_id,
        Range::new(
            position_from_source(range.start),
            position_from_source(range.end),
        ),
    )
}

fn source_range(visitor: &IndexVisitor, location: &ruby_prism::Location) -> SourceRange {
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

fn position_from_source(position: SourcePosition) -> Position {
    Position {
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

fn namespace_kind_from_abi(kind: AbiNamespaceKind) -> NamespaceKind {
    match kind {
        AbiNamespaceKind::Instance => NamespaceKind::Instance,
        AbiNamespaceKind::Singleton => NamespaceKind::Singleton,
    }
}

fn visibility_from_abi(visibility: MethodVisibility) -> CoreVisibility {
    match visibility {
        MethodVisibility::Public => CoreVisibility::Public,
        MethodVisibility::Protected => CoreVisibility::Protected,
        MethodVisibility::Private => CoreVisibility::Private,
    }
}

fn ruby_type_from_abi(ruby_type: AbiRubyType) -> RubyType {
    match ruby_type {
        AbiRubyType::Named(name) => FullyQualifiedName::try_from(name.as_str())
            .map(RubyType::Class)
            .unwrap_or(RubyType::Unknown),
        AbiRubyType::Unknown => RubyType::Unknown,
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

    use super::*;

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
capabilities = ["diagnostics"]
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
