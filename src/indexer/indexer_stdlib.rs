use crate::indexer::indexer_core::IndexerCore;
use crate::server::RubyLanguageServer;
use crate::types::ruby_version::RubyVersion;
use anyhow::Result;
use log::{debug, info, warn};
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Instant;

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

    /// Index standard library based on Ruby version and required modules
    pub async fn index_stdlib(&mut self, server: &RubyLanguageServer) -> Result<()> {
        let start_time = Instant::now();
        info!("Starting stdlib indexing");

        // Discover stdlib paths
        self.discover_stdlib_paths();

        if self.stdlib_paths.is_empty() {
            warn!("No stdlib paths found, skipping stdlib indexing");
            return Ok(());
        }

        // Index core stubs first (if available)
        self.index_core_stubs(server).await?;

        // Index required stdlib modules
        let total_modules = self.required_modules.len();
        self.index_required_modules(server, total_modules).await?;

        // Resolve all mixin references now that all stdlib definitions are indexed
        info!("Resolving stdlib mixin references");
        server.index().lock().resolve_all_mixins();

        info!("Stdlib indexing completed in {:?}", start_time.elapsed());
        Ok(())
    }

    /// Discover standard library paths based on Ruby version
    fn discover_stdlib_paths(&mut self) {
        self.stdlib_paths.clear();

        // Try to find stdlib paths from various sources
        if let Some(version) = self.ruby_version.clone() {
            // Try version-specific paths first
            self.discover_version_specific_paths(&version);
        }

        // Try system Ruby stdlib paths
        self.discover_system_stdlib_paths();

        // Try bundled stubs
        self.discover_bundled_stubs();

        info!("Discovered {} stdlib paths", self.stdlib_paths.len());
    }

    /// Discover version-specific stdlib paths
    fn discover_version_specific_paths(&mut self, version: &RubyVersion) {
        let version_str = version.to_string();

        // Common version-specific paths
        let potential_paths = vec![
            format!("/usr/lib/ruby/{}", version_str),
            format!("/usr/local/lib/ruby/{}", version_str),
            format!("/opt/ruby/{}/lib/ruby/{}", version_str, version_str),
            format!(
                "{}/.rbenv/versions/{}/lib/ruby/{}",
                std::env::var("HOME").unwrap_or_default(),
                version_str,
                version_str
            ),
            format!(
                "{}/.rvm/rubies/ruby-{}/lib/ruby/{}",
                std::env::var("HOME").unwrap_or_default(),
                version_str,
                version_str
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
        // Try to get stdlib path from ruby command
        if let Ok(output) = std::process::Command::new("ruby")
            .args([
                "-e",
                "puts $LOAD_PATH.select { |p| p.include?('ruby') && !p.include?('gems') }",
            ])
            .output()
        {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    let path = PathBuf::from(line.trim());
                    if path.exists() && path.is_dir() {
                        debug!("Found system stdlib path: {:?}", path);
                        self.stdlib_paths.push(path);
                    }
                }
            }
        }
    }

    /// Discover bundled stub files
    fn discover_bundled_stubs(&mut self) {
        // Look for bundled stubs in the extension directory
        if let Some(version) = &self.ruby_version {
            let (major, minor) = version.to_tuple();
            let stub_path = self.get_core_stubs_path((major, minor));

            if let Some(path) = stub_path {
                if path.exists() {
                    debug!("Found bundled stubs: {:?}", path);
                    self.stdlib_paths.push(path);
                }
            }
        }
    }

    /// Get the path to core stubs for a specific Ruby version
    fn get_core_stubs_path(&self, version: (u8, u8)) -> Option<PathBuf> {
        // This should match the logic in coordinator.rs
        let stub_dir = format!("rubystubs{}{}", version.0, version.1);

        // Try to find stubs relative to the current executable
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(exe_dir) = exe_path.parent() {
                // Try: exe_dir/stubs/rubystubsXX (e.g., bin/macos-arm64/stubs/rubystubs30)
                let stubs_path = exe_dir.join("stubs").join(&stub_dir);
                if stubs_path.exists() {
                    return Some(stubs_path);
                }

                // Try: exe_dir/../stubs/rubystubsXX (e.g., bin/stubs/rubystubs30)
                if let Some(parent) = exe_dir.parent() {
                    let stubs_path = parent.join("stubs").join(&stub_dir);
                    if stubs_path.exists() {
                        return Some(stubs_path);
                    }

                    // Try: exe_dir/../../stubs/rubystubsXX (for VS Code extension: extension_root/stubs/rubystubs30)
                    if let Some(grandparent) = parent.parent() {
                        let stubs_path = grandparent.join("stubs").join(&stub_dir);
                        if stubs_path.exists() {
                            return Some(stubs_path);
                        }
                    }
                }

                // Try one level up (for development in workspace)
                let stubs_path = exe_dir.parent()?.join("vsix").join("stubs").join(&stub_dir);
                if stubs_path.exists() {
                    return Some(stubs_path);
                }
            }
        }

        None
    }

    /// Index core stubs if available
    async fn index_core_stubs(&self, server: &RubyLanguageServer) -> Result<()> {
        if let Some(version) = &self.ruby_version {
            let (major, minor) = version.to_tuple();
            if let Some(stubs_path) = self.get_core_stubs_path((major, minor)) {
                info!("Indexing core stubs from: {:?}", stubs_path);

                let stub_files = self.core.collect_ruby_files(&stubs_path);
                if !stub_files.is_empty() {
                    self.core
                        .index_definitions_parallel(&stub_files, server)
                        .await?;
                    info!("Indexed {} core stub files", stub_files.len());
                } else {
                    warn!("No stub files found in: {:?}", stubs_path);
                }
            }
        }
        Ok(())
    }

    /// Index only the required stdlib modules
    async fn index_required_modules(&self, server: &RubyLanguageServer, total_modules: usize) -> Result<()> {
        use crate::indexer::coordinator::IndexingCoordinator;

        if self.required_modules.is_empty() {
            debug!("No required stdlib modules to index");
            return Ok(());
        }

        info!(
            "Indexing {} required stdlib modules",
            self.required_modules.len()
        );

        let mut indexed_count = 0;
        let mut current = 0;

        for module_name in &self.required_modules {
            current += 1;
            IndexingCoordinator::send_progress_report(
                server,
                "Indexing Stdlib".to_string(),
                current,
                total_modules,
            ).await;

            if let Some(module_files) = self.find_stdlib_module_files(module_name) {
                debug!(
                    "Indexing stdlib module '{}' with {} files",
                    module_name,
                    module_files.len()
                );

                for file_path in module_files {
                    if let Err(e) = self.core.index_file_definitions(&file_path, server).await {
                        warn!("Failed to index stdlib file {:?}: {}", file_path, e);
                    } else {
                        indexed_count += 1;
                    }
                }
            } else {
                debug!(
                    "Stdlib module '{}' not found in any stdlib path",
                    module_name
                );
            }
        }

        info!(
            "Indexed {} stdlib files for required modules",
            indexed_count
        );
        Ok(())
    }

    /// Find files for a specific stdlib module
    fn find_stdlib_module_files(&self, module_name: &str) -> Option<Vec<PathBuf>> {
        let mut files = Vec::new();

        for stdlib_path in &self.stdlib_paths {
            // Try direct file match (e.g., json.rb)
            let direct_file = stdlib_path.join(format!("{}.rb", module_name));
            if direct_file.exists() {
                files.push(direct_file);
            }

            // Try directory match (e.g., net/http.rb for net/http)
            if module_name.contains('/') {
                let dir_file = stdlib_path.join(format!("{}.rb", module_name));
                if dir_file.exists() {
                    files.push(dir_file);
                }

                // Also try the directory itself
                let module_dir = stdlib_path.join(module_name);
                if module_dir.exists() && module_dir.is_dir() {
                    let dir_files = self.core.collect_ruby_files(&module_dir);
                    files.extend(dir_files);
                }
            }
        }

        if files.is_empty() {
            None
        } else {
            Some(files)
        }
    }

    /// Get the list of discovered stdlib paths
    pub fn get_stdlib_paths(&self) -> &[PathBuf] {
        &self.stdlib_paths
    }

    /// Get the list of required modules
    pub fn get_required_modules(&self) -> Vec<String> {
        self.required_modules.iter().cloned().collect()
    }

    /// Check if a module is required
    pub fn is_module_required(&self, module_name: &str) -> bool {
        self.required_modules.contains(module_name)
    }

    /// Get a reference to the core indexer
    pub fn core(&self) -> &IndexerCore {
        &self.core
    }
}
