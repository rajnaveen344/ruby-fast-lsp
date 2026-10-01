//! Effective Ruby runtime, version, and library-path discovery for a project.

use super::resources::{run_cpu_indexing_task, IndexingWorkClass};
use super::IndexingCoordinator;
use crate::config::runtime::{EffectiveRuntimeSelection, SelectedRuntimeDescriptor};
use crate::indexer::sources::stdlib::{RuntimeStdlibPathKey, RuntimeStdlibPaths};
use crate::indexer::version::ruby_version::{RubyImplementation, RubyVersion};
#[cfg(test)]
use crate::indexer::version::version_detector::RubyVersionDetector;
use crate::runtime::catalog::RuntimeImplementation;
use crate::server::RubyLanguageServer;
use anyhow::Result;
use log::{debug, info, warn};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

pub(super) async fn runtime_stdlib_paths_for_project(
    server: &RubyLanguageServer,
    runtime: &SelectedRuntimeDescriptor,
) -> Result<RuntimeStdlibPaths> {
    let key = RuntimeStdlibPathKey::new(&runtime.executable, runtime.java_home.as_deref())?;
    let producer_key = key.clone();
    let producer_server = server.clone();
    let started = Instant::now();
    let product = server
        .products
        .stdlib_paths()
        .get_or_try_init(key, move || async move {
            run_cpu_indexing_task(
                &producer_server,
                None,
                None,
                IndexingWorkClass::Io,
                "exact runtime stdlib path discovery",
                move || producer_key.discover(),
            )
            .await
            .map_err(|error| error.to_string())?
            .map_err(|error| error.to_string())
        })
        .await
        .map_err(anyhow::Error::msg)?;
    info!(
        "[PERF][runtime stdlib path product] executable={} paths={} wait={:?}",
        runtime.executable.display(),
        product.paths().len(),
        started.elapsed()
    );
    Ok(product.as_ref().clone())
}

fn ruby_version_for_runtime(runtime: &SelectedRuntimeDescriptor) -> Option<RubyVersion> {
    let mut components = runtime.compatibility_version.split('.');
    let major = components.next()?.parse::<u8>().ok()?;
    let minor = components.next()?.parse::<u8>().ok()?;
    let implementation = match runtime.implementation {
        RuntimeImplementation::Mri => RubyImplementation::Mri,
        RuntimeImplementation::Jruby => RubyImplementation::JRuby,
        RuntimeImplementation::Truffleruby => RubyImplementation::TruffleRuby,
    };
    Some(RubyVersion::new_with_implementation(
        major,
        minor,
        implementation,
    ))
}

impl IndexingCoordinator {
    /// Step 1: Detect which Ruby version we're working with
    #[cfg(test)]
    pub(super) fn detect_ruby_version(&mut self) -> Option<RubyVersion> {
        if let Some(runtime) = &self.effective_runtime {
            let version = ruby_version_for_runtime(runtime);
            self.detected_ruby_version = version;
            return version;
        }
        let root = self.workspace_root.to_string_lossy();
        let selected = self
            .config
            .runtime
            .selection_for_project(&root, &self.config.ruby_version);
        let version = match selected {
            EffectiveRuntimeSelection::Explicit(runtime) => {
                self.effective_runtime = Some(runtime.clone());
                let mut components = runtime.compatibility_version.split('.');
                let major = components.next()?.parse::<u8>().ok()?;
                let minor = components.next()?.parse::<u8>().ok()?;
                let implementation = match runtime.implementation {
                    RuntimeImplementation::Mri => RubyImplementation::Mri,
                    RuntimeImplementation::Jruby => RubyImplementation::JRuby,
                    RuntimeImplementation::Truffleruby => RubyImplementation::TruffleRuby,
                };
                Some(RubyVersion::new_with_implementation(
                    major,
                    minor,
                    implementation,
                ))
            }
            EffectiveRuntimeSelection::Auto
            | EffectiveRuntimeSelection::LegacyMriCompatibility { .. } => self
                .config
                .get_ruby_version()
                .map(RubyVersion::from_tuple)
                .or_else(|| {
                    RubyVersionDetector::from_path(self.workspace_root.clone()).detect_version()
                }),
        };
        self.detected_ruby_version = version;
        version
    }

    pub(super) async fn detect_ruby_version_off_reactor(
        &mut self,
        _server: &RubyLanguageServer,
    ) -> Result<Option<RubyVersion>> {
        if let Some(runtime) = &self.effective_runtime {
            let version = ruby_version_for_runtime(runtime);
            self.detected_ruby_version = version;
            return Ok(version);
        }
        if let Some(version) = self.config.get_ruby_version().map(RubyVersion::from_tuple) {
            self.detected_ruby_version = Some(version);
            return Ok(Some(version));
        }
        // Auto was already resolved against the exact runtime catalog. A
        // marker with no installed match is an unfulfilled runtime request,
        // not permission to reconstruct compatibility through another probe.
        // None selects the bundled Ruby 3.0 core fallback; explicit legacy
        // compatibility above remains authoritative without an executable.
        self.detected_ruby_version = None;
        Ok(None)
    }

    pub(super) async fn resolve_effective_runtime(
        &mut self,
        server: &RubyLanguageServer,
    ) -> Result<()> {
        let root = self.workspace_root.to_string_lossy();
        self.effective_runtime = match self
            .config
            .runtime
            .selection_for_project(&root, &self.config.ruby_version)
        {
            EffectiveRuntimeSelection::Explicit(runtime) => Some(runtime),
            EffectiveRuntimeSelection::Auto => {
                server.resolve_auto_runtime(&self.workspace_root).await?
            }
            EffectiveRuntimeSelection::LegacyMriCompatibility { .. } => None,
        };
        server.set_effective_runtime(&self.workspace_root, self.effective_runtime.clone());
        Ok(())
    }

    /// Step 2: Find where Ruby libraries are installed on this system
    ///
    /// This looks for Ruby's standard library and gem directories so we know
    /// where to find external code that the project might be using.
    pub fn discover_ruby_library_paths(&mut self) {
        self.ruby_library_paths.clear();

        // Use ruby -e to get the actual load path from the Ruby installation
        if let Ok(output) = Command::new("ruby")
            .args(["-e", "puts $LOAD_PATH"])
            .output()
        {
            if output.status.success() {
                let load_paths = String::from_utf8_lossy(&output.stdout);
                for path_str in load_paths.lines() {
                    let path = PathBuf::from(path_str.trim());
                    if path.exists() && path.is_dir() {
                        self.ruby_library_paths.push(path);
                        debug!("Found Ruby lib directory: {:?}", path_str.trim());
                    }
                }
            } else {
                debug!(
                    "Failed to get Ruby load path: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        } else {
            debug!("Failed to execute ruby command to get load path");
        }

        // Also try to get gem paths
        if let Ok(output) = Command::new("ruby")
            .args(["-e", "require 'rubygems'; puts Gem.path"])
            .output()
        {
            if output.status.success() {
                let gem_paths = String::from_utf8_lossy(&output.stdout);
                for path_str in gem_paths.lines() {
                    let path = PathBuf::from(path_str.trim());
                    if path.exists() && path.is_dir() {
                        // Add the gems subdirectory which contains actual gem sources
                        let gems_dir = path.join("gems");
                        if gems_dir.exists() {
                            self.ruby_library_paths.push(gems_dir.clone());
                            debug!("Found gem directory: {:?}", gems_dir);
                        }
                    }
                }
            }
        }
    }

    /// Find all Ruby files in a directory and its subdirectories
    ///
    /// This walks through a directory tree and collects all Ruby files,
    /// but skips common directories that usually don't contain Ruby source code
    /// (like node_modules, .git, tmp, etc.)
    pub fn find_all_ruby_files_in_directory(&self, dir: &Path, files: &mut Vec<PathBuf>) {
        let collected_files = crate::utils::collect_ruby_files(dir);
        files.extend(collected_files);
    }

    /// Check if a file is a Ruby file
    ///
    /// This looks at the file extension (.rb, .ruby, .rake) and also checks
    /// for common Ruby files that don't have extensions (like Rakefile, Gemfile)
    pub fn is_ruby_file(&self, path: &Path) -> bool {
        crate::utils::should_index_file(path)
    }

    /// Find the Ruby core stubs for a specific Ruby version
    ///
    /// Ruby core stubs are pre-written definitions of Ruby's built-in classes and methods.
    /// This helps the language server understand Ruby's core functionality.
    ///
    /// We try to find stubs in this order:
    /// 1. Use the configured stub path
    /// 2. Look in the workspace's editors/vscode/vsix/stubs directory
    /// 3. Fall back to Ruby 3.0 stubs if available
    pub fn find_core_stubs_for_version(&self, version: (u8, u8)) -> Option<PathBuf> {
        // First, try the configured stub path
        if let Some(stubs_path_str) = self.config.get_core_stubs_path_internal(version) {
            return Some(PathBuf::from(stubs_path_str));
        }

        // Look for stubs in the workspace
        let stubs_dir = self
            .workspace_root
            .join("editors")
            .join("vscode")
            .join("vsix")
            .join("stubs");
        let version_dir = format!("rubystubs{}{}", version.0, version.1);
        let stubs_path = stubs_dir.join(version_dir);

        if stubs_path.exists() {
            debug!("Found core stubs in workspace at: {:?}", stubs_path);
            return Some(stubs_path);
        }

        // Fall back to Ruby 3.0 stubs if the specific version isn't available
        let default_stubs = stubs_dir.join("rubystubs30");
        if default_stubs.exists() {
            info!("Using default Ruby 3.0 stubs at: {:?}", default_stubs);
            Some(default_stubs)
        } else {
            warn!("No core stubs found for Ruby version {:?}", version);
            None
        }
    }

    /// Get the Ruby library paths we discovered
    ///
    /// This returns the list of directories where Ruby libraries are installed.
    pub fn get_ruby_library_paths(&self) -> &[PathBuf] {
        &self.ruby_library_paths
    }
}
