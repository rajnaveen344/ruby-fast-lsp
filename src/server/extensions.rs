use super::Server;
use crate::environment::config::RubyFastLspConfig;
use crate::environment::extensions::ExtensionRegistryHandle;
use crate::environment::extensions::ExtensionStatusReport;
use crate::invariant::ExpectInvariant;
use log::{info, warn};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tower_lsp::lsp_types::{
    DidChangeWatchedFilesRegistrationOptions, FileSystemWatcher, GlobPattern, Registration,
    Unregistration,
};

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
    pub(super) fn registry(&self) -> &ExtensionRegistryHandle {
        &self.registry
    }
}

impl Server {
    /// The extension registry; its handle is shared by every load of the server.
    pub(crate) fn extension_registry(&self) -> &ExtensionRegistryHandle {
        self.extensions.registry()
    }

    /// Record whether the client accepts dynamic watched-file registration.
    pub(crate) fn set_extension_watch_dynamic_registration(&self, supported: bool) {
        self.extensions
            .dynamic_registration
            .store(supported, Ordering::Release);
    }

    /// Reload extensions for `config` and the current workspace roots under the
    /// resource governor.
    pub(crate) async fn reconfigure_extensions(
        &self,
        config: &RubyFastLspConfig,
    ) -> anyhow::Result<()> {
        self.extensions
            .registry()
            .configure_from_config_and_workspace_roots_governed(
                config,
                &self.workspace_root_paths(),
                self.indexing.resources().clone(),
            )
            .await
    }

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

    /// Register the extension watcher globs with a client that supports
    /// dynamic registration, replacing any earlier registration.
    pub(crate) async fn refresh_extension_watch_registration(&self) {
        if !self.extensions.dynamic_registration.load(Ordering::Acquire) {
            return;
        }
        let Some(client) = self.client() else {
            return;
        };

        let desired = self.extensions.registry.watcher_globs();
        let mut current = self.extensions.registration.lock().await;
        if *current == desired {
            return;
        }

        if !current.is_empty() {
            let unregistration = Unregistration {
                id: "ruby-fast-lsp-extension-watchers".to_string(),
                method: "workspace/didChangeWatchedFiles".to_string(),
            };
            if let Err(err) = client.unregister_capability(vec![unregistration]).await {
                warn!("Failed to unregister extension file watchers: {:?}", err);
                return;
            }
            current.clear();
        }

        if desired.is_empty() {
            return;
        }
        let registration = extension_watch_registration(&desired);
        match client.register_capability(vec![registration]).await {
            Ok(()) => {
                info!(
                    "Registered {} extension watched-file glob(s)",
                    desired.len()
                );
                *current = desired;
            }
            Err(err) => warn!("Failed to register extension file watchers: {:?}", err),
        }
    }
}

pub(crate) fn extension_watch_registration(globs: &[String]) -> Registration {
    let options = DidChangeWatchedFilesRegistrationOptions {
        watchers: globs
            .iter()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .map(|pattern| FileSystemWatcher {
                glob_pattern: GlobPattern::String(pattern),
                kind: None,
            })
            .collect(),
    };
    Registration {
        id: "ruby-fast-lsp-extension-watchers".to_string(),
        method: "workspace/didChangeWatchedFiles".to_string(),
        register_options: Some(serde_json::to_value(options).expect_invariant(
            "typed watched-file registration options failed to serialize",
            "lsp-types registration values must serialize",
            "preserve serializable watcher option fields",
        )),
    }
}
