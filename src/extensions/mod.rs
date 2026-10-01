mod dispatch;
mod loading;
mod patches;
mod processes;
mod project_context;
mod registry;
mod responses;
#[cfg(test)]
mod tests;

pub use dispatch::call_context::resolved_call_for_stack;
pub(crate) use patches::types::analysis_ruby_type_from_extension;
pub(crate) use project_context::{
    ExtensionApplicabilityFingerprint, ProjectContextSeed, ProjectContextSnapshot,
};
pub use registry::handle::ExtensionRegistryHandle;
pub(crate) use registry::state::ExtensionApplicabilitySnapshot;
pub use registry::status::{
    ExtensionStatusParams, ExtensionStatusReport, ExtensionStatusResponse, ExtensionTelemetryReport,
};

use std::path::Path;
use std::time::Duration;

use once_cell::sync::Lazy;
use ruby_analysis::indexer::fact_collector::FactCollector;
use ruby_prism::CallNode;
use tower_lsp::lsp_types::{CodeLens, DocumentSymbol};

use crate::config::RubyFastLspConfig;
use crate::extensions::dispatch::calls::process_call_node_with_registry;
use crate::extensions::loading::config::{ConfiguredExtensionPath, ExtensionPathSource};
use crate::extensions::loading::packages::collect_extension_package;
use crate::extensions::loading::wasm::load_wasm_extension;
use crate::extensions::responses::{code_lenses_with_registry, document_symbols_with_registry};

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

pub fn configure_from_config(config: &RubyFastLspConfig) {
    EXTENSION_REGISTRY.configure_from_config(config);
}

pub fn extension_status_reports() -> Vec<ExtensionStatusReport> {
    EXTENSION_REGISTRY.status_reports()
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
    let _ = process_call_node_with_registry(&EXTENSION_REGISTRY, visitor, node, None, false);
}

pub fn document_symbols(uri: &str, text: &str) -> Vec<DocumentSymbol> {
    document_symbols_with_registry(&EXTENSION_REGISTRY, uri, text, None)
}

pub fn code_lenses(uri: &str, text: &str) -> Vec<CodeLens> {
    code_lenses_with_registry(&EXTENSION_REGISTRY, uri, text, None)
}
