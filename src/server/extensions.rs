use super::RubyLanguageServer;
use crate::config::RubyFastLspConfig;
use crate::extensions::ExtensionRegistryHandle;
use crate::extensions::ExtensionStatusReport;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Clone)]
pub(crate) struct ExtensionServices {
    registry: ExtensionRegistryHandle,
    dynamic_registration: Arc<AtomicBool>,
    registration: Arc<tokio::sync::Mutex<Vec<String>>>,
}
impl ExtensionServices {
    pub(super) fn new(registry: ExtensionRegistryHandle) -> Self {
        Self {
            registry,
            dynamic_registration: Arc::new(AtomicBool::new(false)),
            registration: Arc::default(),
        }
    }
    pub(crate) fn registry(&self) -> &ExtensionRegistryHandle {
        &self.registry
    }
    pub(crate) fn dynamic_registration(&self) -> &AtomicBool {
        &self.dynamic_registration
    }
    pub(crate) fn registration(&self) -> &tokio::sync::Mutex<Vec<String>> {
        &self.registration
    }
}

impl RubyLanguageServer {
    /// Configure an embedded server before indexing starts. Client-backed
    /// servers receive configuration through their LSP initialization lifecycle.
    /// Returns extension load time, excluding configuration publication.
    pub fn configure_embedded(&mut self, config: RubyFastLspConfig) -> Duration {
        let started = Instant::now();
        self.extensions.registry().configure_from_config(&config);
        let elapsed = started.elapsed();
        *self.config.lock() = config;
        elapsed
    }

    /// Bind extension discovery to the embedded server's registered projects.
    pub fn refresh_embedded_extensions(&self) {
        self.extensions
            .registry()
            .configure_from_config_and_workspace_roots(
                &self.configuration_snapshot(),
                &self.workspace_root_paths(),
            );
    }

    pub fn extension_status_reports(&self) -> Vec<ExtensionStatusReport> {
        self.extensions.registry().status_reports()
    }
}
