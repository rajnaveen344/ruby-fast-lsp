mod diagnostics;
mod documents;
mod extensions;
mod indexing;
mod namespace_tree;
mod products;
mod projects;
mod watched_files;

use crate::invariant::ExpectInvariant;
pub use diagnostics::engine_diagnostics;
use diagnostics::DiagnosticPublisher;
pub(crate) use documents::OpenDocuments;
pub(crate) use extensions::ExtensionServices;
pub(crate) use indexing::IndexingServices;
use namespace_tree::NamespaceTreeCache;
pub(crate) use products::RuntimeProducts;
pub use products::{
    ProjectRuntimeStatus, RuntimeProductSnapshot, RuntimeStatus, RuntimeStatusParams,
};
use projects::ProjectRegistry;
pub use projects::{ProjectHandle, Workspace};
use watched_files::WatchedFileChanges;

use crate::environment::config::RubyFastLspConfig;
use crate::environment::extensions::ExtensionRegistryHandle;

use anyhow::Result;
use log::{info, warn};
use parking_lot::Mutex;

use ruby_analysis::indexer::RubyDocument;

use std::path::{Path, PathBuf};
use std::process::exit;

use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tower_lsp::lsp_types::Url;
use tower_lsp::Client;

pub(crate) const WATCHED_FILE_DEBOUNCE_INTERVAL: Duration = Duration::from_millis(100);

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

/// Server state: each owner preserves its own locks and shared clone identity.
/// The protocol facade lives in `src/lsp/service.rs`.
#[derive(Clone)]
pub struct Server {
    pub(self) client: Option<Client>,
    pub(crate) config: Arc<Mutex<RubyFastLspConfig>>,
    pub(self) documents: OpenDocuments,
    pub(self) projects: ProjectRegistry,
    pub(crate) indexing: IndexingServices,
    pub(self) products: RuntimeProducts,
    pub(crate) extensions: ExtensionServices,
    pub(self) diagnostics: DiagnosticPublisher,
    pub(self) file_changes: WatchedFileChanges,
    pub(self) namespace_tree: NamespaceTreeCache,
}

impl Server {
    /// Copy the accepted configuration without exposing its shared lock.
    pub fn configuration_snapshot(&self) -> RubyFastLspConfig {
        self.config.lock().clone()
    }

    /// The editor connection; an embedded server has none.
    pub(crate) fn client(&self) -> Option<&Client> {
        self.client.as_ref()
    }

    /// Shared runtime products, for tests that observe reuse across projects.
    #[cfg(test)]
    pub(crate) fn shared_products(&self) -> crate::loader::context::SharedProducts {
        self.products.shared()
    }

    pub(crate) fn document_semantic_lock(&self, uri: &Url) -> Arc<tokio::sync::Mutex<()>> {
        self.documents.semantic_lock(uri)
    }

    pub fn get_doc(&self, uri: &Url) -> Option<RubyDocument> {
        self.documents
            .read()
            .get(uri)
            .map(|doc_arc| doc_arc.read().clone())
    }

    pub fn new(client: Client) -> Result<Self> {
        Self::with_cache_root(
            Some(client),
            crate::utils::cache::ruby_fast_lsp_user_cache_root()?,
        )
    }

    /// Construct an embedded server with an explicit ordinary cache location.
    pub fn with_user_cache_root(root: PathBuf) -> Result<Self> {
        Self::with_cache_root(None, root)
    }

    pub(crate) fn with_cache_root(client: Option<Client>, root: PathBuf) -> Result<Self> {
        let products = RuntimeProducts::new(root);
        // A client server discovers extensions during initialize, under the
        // resource governor. Embedded use retains eager environment loading.
        let registry = if client.is_some() {
            ExtensionRegistryHandle::empty_with_cache(products.persistent().clone())
        } else {
            ExtensionRegistryHandle::from_environment_with_cache(products.persistent().clone())
        };
        Ok(Self {
            client,
            config: Arc::new(Mutex::new(RubyFastLspConfig::default())),
            documents: OpenDocuments::default(),
            projects: ProjectRegistry::default(),
            indexing: IndexingServices::default(),
            products,
            extensions: ExtensionServices::new(registry),
            diagnostics: DiagnosticPublisher::default(),
            file_changes: WatchedFileChanges::default(),
            namespace_tree: NamespaceTreeCache::default(),
        })
    }

    /// Set the parent process ID and start monitoring it.
    /// If the parent process dies, the LSP server will exit.
    pub fn set_parent_process_id(&self, pid: Option<u32>) {
        if let Some(pid) = pid {
            self.start_parent_process_monitor(pid);
        }
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

    /// Request the client to refresh inlay hints
    pub async fn refresh_inlay_hints(&self) {
        if let Some(client) = &self.client {
            // Send workspace/inlayHint/refresh request to client
            let _ = client
                .send_request::<tower_lsp::lsp_types::request::InlayHintRefreshRequest>(())
                .await;
        }
    }

    /// Refresh inlay hints only when this isolated project currently owns an
    /// open document. The LSP refresh request is global, so closed projects in
    /// an umbrella workspace must not each create redundant client work when
    /// their cold indexing generation converges.
    pub async fn refresh_inlay_hints_for_workspace(&self, workspace_root: &Path) {
        let open_uris = self.documents.read().keys().cloned().collect::<Vec<_>>();
        let owns_open_document = open_uris.iter().any(|uri| {
            self.workspace_for_uri(uri)
                .is_some_and(|workspace| workspace.root_path == workspace_root)
        });
        if owns_open_document {
            self.refresh_inlay_hints().await;
        }
    }
}

impl Default for Server {
    fn default() -> Self {
        let root = crate::utils::cache::ruby_fast_lsp_user_cache_root().expect_invariant(
            "the default server could not resolve an absolute user cache root",
            "embedded construction requires deterministic derived-product ownership",
            "configure an absolute user cache root",
        );
        Self::with_user_cache_root(root).expect_invariant(
            "embedded server construction failed",
            "the resolved cache root must support ordinary server construction",
            "inspect the cache root and extension initialization",
        )
    }
}

#[cfg(test)]
mod tests;
