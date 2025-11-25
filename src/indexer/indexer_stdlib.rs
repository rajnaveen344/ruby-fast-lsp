//! Standard Library Indexing
//!
//! This module handles indexing of Ruby's standard library based on the detected
//! Ruby version and required modules from project dependencies.

use crate::indexer::coordinator::IndexingCoordinator;
use crate::indexer::indexer_core::IndexerCore;
use crate::server::RubyLanguageServer;
use crate::types::ruby_version::RubyVersion;
use anyhow::Result;
use log::{debug, info, warn};
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Instant;

// ============================================================================
// IndexerStdlib
// ============================================================================

/// Handles standard library indexing
pub struct IndexerStdlib {
    core: IndexerCore,
    ruby_version: Option<RubyVersion>,
    stdlib_paths: Vec<PathBuf>,
    required_modules: HashSet<String>,
}

impl IndexerStdlib {
    pub fn new(core: IndexerCore, ruby_version: Option<RubyVersion>) -> Self {
        Self {
            core,
            ruby_version,
            stdlib_paths: Vec::new(),
            required_modules: HashSet::new(),
        }
    }

    // ========================================================================
    // Configuration
    // ========================================================================

    /// Set the list of required stdlib modules to index
    pub fn set_required_modules(&mut self, modules: Vec<String>) {
        self.required_modules = modules.into_iter().collect();
        info!(
            "Set {} required stdlib modules",
            self.required_modules.len()
        );
    }

    /// Add a required stdlib module
    pub fn add_required_module(&mut self, module: String) {
        self.required_modules.insert(module);
    }

    // ========================================================================
    // Indexing
    // ========================================================================

    /// Index standard library based on Ruby version and required modules
    pub async fn index_stdlib(&mut self, server: &RubyLanguageServer) -> Result<()> {
        let start = Instant::now();
        info!("Starting stdlib indexing");

        self.discover_stdlib_paths();

        if self.stdlib_paths.is_empty() {
            warn!("No stdlib paths found, skipping stdlib indexing");
            return Ok(());
        }

        // Index core stubs first (if available)
        self.index_core_stubs(server).await?;

        // Index required stdlib modules
        self.index_required_modules(server).await?;

        // Resolve all mixin references
        info!("Resolving stdlib mixin references");
        server.index().lock().resolve_all_mixins();

        info!("Stdlib indexing completed in {:?}", start.elapsed());
        Ok(())
    }

    /// Index core stubs if available
    async fn index_core_stubs(&self, server: &RubyLanguageServer) -> Result<()> {
        let Some(version) = &self.ruby_version else {
            return Ok(());
        };

        let Some(stubs_path) = self.find_core_stubs_path(version.to_tuple()) else {
            return Ok(());
        };

        info!("Indexing core stubs from: {:?}", stubs_path);

        let stub_files = self.core.collect_ruby_files(&stubs_path);
        if stub_files.is_empty() {
            warn!("No stub files found in: {:?}", stubs_path);
            return Ok(());
        }

        self.core
            .index_definitions_parallel(&stub_files, server)
            .await?;
        info!("Indexed {} core stub files", stub_files.len());

        Ok(())
    }

    /// Index only the required stdlib modules
    async fn index_required_modules(&self, server: &RubyLanguageServer) -> Result<()> {
        if self.required_modules.is_empty() {
            debug!("No required stdlib modules to index");
            return Ok(());
        }

        let total = self.required_modules.len();
        info!("Indexing {} required stdlib modules", total);

        let mut indexed_count = 0;

        for (current, module_name) in self.required_modules.iter().enumerate() {
            IndexingCoordinator::send_progress_report(
                server,
                "Indexing Stdlib".to_string(),
                current + 1,
                total,
            )
            .await;

            let Some(files) = self.find_module_files(module_name) else {
                debug!("Stdlib module '{}' not found", module_name);
                continue;
            };

            debug!(
                "Indexing stdlib module '{}' ({} files)",
                module_name,
                files.len()
            );

            for file_path in files {
                if let Err(e) = self.core.index_file_definitions(&file_path, server).await {
                    warn!("Failed to index stdlib file {:?}: {}", file_path, e);
                } else {
                    indexed_count += 1;
                }
            }
        }

        info!(
            "Indexed {} stdlib files for required modules",
            indexed_count
        );
        Ok(())
    }

    // ========================================================================
    // Path Discovery
    // ========================================================================

    /// Discover standard library paths based on Ruby version
    fn discover_stdlib_paths(&mut self) {
        self.stdlib_paths.clear();

        if let Some(version) = self.ruby_version.clone() {
            self.discover_version_specific_paths(&version);
        }

        self.discover_system_stdlib_paths();
        self.discover_bundled_stubs();

        info!("Discovered {} stdlib paths", self.stdlib_paths.len());
    }

    /// Discover version-specific stdlib paths
    fn discover_version_specific_paths(&mut self, version: &RubyVersion) {
        let version_str = version.to_string();
        let home = std::env::var("HOME").unwrap_or_default();

        let potential_paths = [
            format!("/usr/lib/ruby/{}", version_str),
            format!("/usr/local/lib/ruby/{}", version_str),
            format!("/opt/ruby/{}/lib/ruby/{}", version_str, version_str),
            format!(
                "{}/.rbenv/versions/{}/lib/ruby/{}",
                home, version_str, version_str
            ),
            format!(
                "{}/.rvm/rubies/ruby-{}/lib/ruby/{}",
                home, version_str, version_str
            ),
        ];

        for path_str in potential_paths {
            let path = PathBuf::from(path_str);
            if path.exists() && path.is_dir() {
                debug!("Found version-specific stdlib path: {:?}", path);
                self.stdlib_paths.push(path);
            }
        }
    }

    /// Discover system Ruby stdlib paths
    fn discover_system_stdlib_paths(&mut self) {
        let Ok(output) = std::process::Command::new("ruby")
            .args([
                "-e",
                "puts $LOAD_PATH.select { |p| p.include?('ruby') && !p.include?('gems') }",
            ])
            .output()
        else {
            return;
        };

        if !output.status.success() {
            return;
        }

        for line in String::from_utf8_lossy(&output.stdout).lines() {
            let path = PathBuf::from(line.trim());
            if path.exists() && path.is_dir() {
                debug!("Found system stdlib path: {:?}", path);
                self.stdlib_paths.push(path);
            }
        }
    }

    /// Discover bundled stub files
    fn discover_bundled_stubs(&mut self) {
        let Some(version) = &self.ruby_version else {
            return;
        };

        if let Some(path) = self.find_core_stubs_path(version.to_tuple()) {
            if path.exists() {
                debug!("Found bundled stubs: {:?}", path);
                self.stdlib_paths.push(path);
            }
        }
    }

    /// Get the path to core stubs for a specific Ruby version
    fn find_core_stubs_path(&self, version: (u8, u8)) -> Option<PathBuf> {
        let stub_dir = format!("rubystubs{}{}", version.0, version.1);

        let Ok(exe_path) = std::env::current_exe() else {
            return None;
        };

        let exe_dir = exe_path.parent()?;

        // Try various relative paths
        let candidates = [
            exe_dir.join("stubs").join(&stub_dir),
            exe_dir.parent()?.join("stubs").join(&stub_dir),
            exe_dir.parent()?.parent()?.join("stubs").join(&stub_dir),
            exe_dir.parent()?.join("vsix").join("stubs").join(&stub_dir),
        ];

        candidates.into_iter().find(|p| p.exists())
    }

    /// Find files for a specific stdlib module
    fn find_module_files(&self, module_name: &str) -> Option<Vec<PathBuf>> {
        let mut files = Vec::new();

        for stdlib_path in &self.stdlib_paths {
            // Try direct file match (e.g., json.rb)
            let direct_file = stdlib_path.join(format!("{}.rb", module_name));
            if direct_file.exists() {
                files.push(direct_file);
            }

            // Try directory match for nested modules (e.g., net/http)
            if module_name.contains('/') {
                let dir_file = stdlib_path.join(format!("{}.rb", module_name));
                if dir_file.exists() {
                    files.push(dir_file);
                }

                let module_dir = stdlib_path.join(module_name);
                if module_dir.exists() && module_dir.is_dir() {
                    files.extend(self.core.collect_ruby_files(&module_dir));
                }
            }
        }

        if files.is_empty() {
            None
        } else {
            Some(files)
        }
    }

    // ========================================================================
    // Accessors
    // ========================================================================

    pub fn get_stdlib_paths(&self) -> &[PathBuf] {
        &self.stdlib_paths
    }

    pub fn get_required_modules(&self) -> Vec<String> {
        self.required_modules.iter().cloned().collect()
    }

    pub fn is_module_required(&self, module_name: &str) -> bool {
        self.required_modules.contains(module_name)
    }

    pub fn core(&self) -> &IndexerCore {
        &self.core
    }
}
