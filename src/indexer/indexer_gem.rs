//! Gem Indexing
//!
//! This module handles gem discovery and indexing for the Ruby Language Server.
//! It supports both Bundler-based (Gemfile) and global gem discovery.

use crate::indexer::coordinator::IndexingCoordinator;
use crate::indexer::indexer_core::IndexerCore;
use crate::server::RubyLanguageServer;
use anyhow::{anyhow, Context, Result};
use log::{debug, info};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use tower_lsp::lsp_types::Url;

// ============================================================================
// Types
// ============================================================================

/// Information about a discovered gem
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GemInfo {
    pub name: String,
    pub version: String,
    pub path: PathBuf,
    pub lib_paths: Vec<PathBuf>,
    pub dependencies: Vec<String>,
    pub is_default: bool,
}

// ============================================================================
// IndexerGem
// ============================================================================

/// Handles gem indexing for the Ruby Language Server.
/// Manages gem discovery, prioritization, and selective indexing.
#[derive(Debug)]
pub struct IndexerGem {
    core: Arc<Mutex<IndexerCore>>,
    workspace_root: Option<PathBuf>,
    required_gems: HashSet<String>,
    discovered_gems: HashMap<String, Vec<GemInfo>>,
    gem_paths: Vec<PathBuf>,
}

impl IndexerGem {
    pub fn new(core: Arc<Mutex<IndexerCore>>, workspace_root: Option<PathBuf>) -> Self {
        Self {
            core,
            workspace_root,
            required_gems: HashSet::new(),
            discovered_gems: HashMap::new(),
            gem_paths: Vec::new(),
        }
    }

    // ========================================================================
    // Configuration
    // ========================================================================

    /// Set the required gems for the project
    pub fn set_required_gems(&mut self, gems: HashSet<String>) {
        self.required_gems = gems;
        debug!(
            "Set {} required gems for indexing",
            self.required_gems.len()
        );
    }

    /// Add a required gem to the project
    pub fn add_required_gem(&mut self, gem_name: String) {
        if self.required_gems.insert(gem_name.clone()) {
            debug!("Added required gem: {}", gem_name);
        }
    }

    // ========================================================================
    // Indexing
    // ========================================================================

    /// Index gems based on project requirements.
    /// If `selective` is true, only index required gems.
    /// If `selective` is false, index all discovered gems.
    pub async fn index_gems(
        &mut self,
        selective: bool,
        server: &RubyLanguageServer,
    ) -> Result<Vec<Url>> {
        info!("Starting gem indexing (selective: {})", selective);

        self.discover_gems().await?;
        info!("Discovered {} gems", self.discovered_gems.len());

        let indexed_files = if selective && !self.required_gems.is_empty() {
            self.index_required_gems(server).await?
        } else {
            self.index_all_gems(server).await?
        };

        info!("Indexed {} files from gems", indexed_files.len());
        Ok(indexed_files)
    }

    /// Index only the gems required by the project
    async fn index_required_gems(&self, server: &RubyLanguageServer) -> Result<Vec<Url>> {
        let total = self.required_gems.len();
        let mut indexed_files = Vec::new();

        for (current, gem_name) in self.required_gems.iter().enumerate() {
            IndexingCoordinator::send_progress_report(
                server,
                "Indexing Gems".to_string(),
                current + 1,
                total,
            )
            .await;

            if let Some(gem_versions) = self.discovered_gems.get(gem_name) {
                if let Some(gem_info) = self.select_preferred_version(gem_versions) {
                    debug!(
                        "Indexing required gem: {} v{}",
                        gem_info.name, gem_info.version
                    );
                    indexed_files.extend(self.collect_gem_files(gem_info));
                }
            } else {
                debug!("Required gem not found: {}", gem_name);
            }
        }

        Ok(indexed_files)
    }

    /// Index all discovered gems
    async fn index_all_gems(&self, server: &RubyLanguageServer) -> Result<Vec<Url>> {
        let total = self.discovered_gems.len();
        let mut indexed_files = Vec::new();

        for (current, gem_versions) in self.discovered_gems.values().enumerate() {
            IndexingCoordinator::send_progress_report(
                server,
                "Indexing Gems".to_string(),
                current + 1,
                total,
            )
            .await;

            if let Some(gem_info) = self.select_preferred_version(gem_versions) {
                debug!("Indexing gem: {} v{}", gem_info.name, gem_info.version);
                indexed_files.extend(self.collect_gem_files(gem_info));
            }
        }

        Ok(indexed_files)
    }

    /// Collect all Ruby file URIs from a gem's lib paths
    fn collect_gem_files(&self, gem_info: &GemInfo) -> Vec<Url> {
        let mut files = Vec::new();

        for lib_path in &gem_info.lib_paths {
            if lib_path.exists() && lib_path.is_dir() {
                debug!("Collecting files from gem lib path: {:?}", lib_path);

                let core = self.core.lock();
                let ruby_files = core.collect_ruby_files(lib_path);
                drop(core);

                for file_path in ruby_files {
                    if let Ok(uri) = Url::from_file_path(&file_path) {
                        files.push(uri);
                    }
                }
            }
        }

        files
    }

    // ========================================================================
    // Discovery
    // ========================================================================

    /// Discover available gems in the system
    pub async fn discover_gems(&mut self) -> Result<usize> {
        debug!("Starting gem discovery process");

        self.discovered_gems.clear();
        self.gem_paths.clear();

        self.discover_gem_paths()?;
        self.discover_installed_gems()?;
        self.resolve_gem_lib_paths();

        info!("Discovered {} unique gems", self.discovered_gems.len());
        Ok(self.discovered_gems.len())
    }

    /// Get gem paths from Ruby's gem environment
    fn discover_gem_paths(&mut self) -> Result<()> {
        let output = Command::new("ruby")
            .args(["-e", "require 'rubygems'; puts Gem.path.join('\n')"])
            .output()
            .map_err(|e| anyhow!("Failed to execute ruby command: {}", e))?;

        if !output.status.success() {
            return Err(anyhow!(
                "Ruby command failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        for line in String::from_utf8_lossy(&output.stdout).lines() {
            let path = PathBuf::from(line.trim());
            if path.exists() && path.is_dir() {
                self.gem_paths.push(path.clone());
                debug!("Found gem path: {:?}", path);
            }
        }

        Ok(())
    }

    /// Discover all installed gems using the configured scope
    fn discover_installed_gems(&mut self) -> Result<()> {
        let scope = std::env::var("RUBY_LSP_GEM_SCOPE")
            .unwrap_or_else(|_| "auto".to_string())
            .to_lowercase();

        match scope.as_str() {
            "bundler" | "gemfile" => {
                info!("Gem indexing scope: Bundler/Gemfile only");
                self.discover_bundler_gems()
            }
            "global" => {
                info!("Gem indexing scope: Global gems only");
                self.discover_global_gems()
            }
            _ => {
                debug!("Gem indexing scope: Auto (Bundler with global fallback)");
                if self.discover_bundler_gems().is_ok() {
                    debug!("Using Bundler gems from Gemfile");
                    Ok(())
                } else {
                    debug!("Falling back to global gem discovery");
                    self.discover_global_gems()
                }
            }
        }
    }

    /// Discover gems using Bundler (Gemfile-based)
    fn discover_bundler_gems(&mut self) -> Result<()> {
        let gemfile_path = self.find_gemfile()?;

        let script = format!(
            r#"
            require 'bundler'
            require 'json'
            begin
              Dir.chdir('{}')
              Bundler.root
              gems = Bundler.load.specs.map do |spec|
                next if spec.name.nil? || spec.version.nil?
                {{
                  name: spec.name,
                  version: spec.version.to_s,
                  gem_dir: spec.gem_dir,
                  lib_dirs: spec.require_paths.map {{ |p| File.join(spec.gem_dir, p) }},
                  dependencies: spec.dependencies.map(&:name),
                  default_gem: spec.default_gem?
                }}
              end.compact
              puts JSON.generate(gems)
            rescue Bundler::GemfileNotFound
              exit 1
            end
            "#,
            gemfile_path.parent().unwrap().display()
        );

        let output = Command::new("ruby")
            .args(["-e", &script])
            .output()
            .map_err(|e| anyhow!("Failed to execute bundler gem discovery: {}", e))?;

        if !output.status.success() {
            return Err(anyhow!("No Gemfile found or bundler failed"));
        }

        self.process_gem_json(&output.stdout, "Bundler")
    }

    /// Discover all global gems
    fn discover_global_gems(&mut self) -> Result<()> {
        let script = r#"
            require 'rubygems'
            require 'json'
            gems = Gem::Specification.map do |spec|
              next if spec.name.nil? || spec.version.nil?
              {
                name: spec.name,
                version: spec.version.to_s,
                gem_dir: spec.gem_dir,
                lib_dirs: spec.require_paths.map { |p| File.join(spec.gem_dir, p) },
                dependencies: spec.dependencies.map(&:name),
                default_gem: spec.default_gem?
              }
            end.compact
            puts JSON.generate(gems)
        "#;

        let output = Command::new("ruby")
            .args(["-e", script])
            .output()
            .map_err(|e| anyhow!("Failed to execute ruby gem discovery: {}", e))?;

        if !output.status.success() {
            return Err(anyhow!(
                "Ruby gem discovery failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        self.process_gem_json(&output.stdout, "Global")
    }

    /// Find Gemfile in workspace hierarchy
    fn find_gemfile(&self) -> Result<PathBuf> {
        if let Some(root) = &self.workspace_root {
            // Check workspace root
            let gemfile = root.join("Gemfile");
            if gemfile.exists() {
                return Ok(gemfile);
            }

            // Check subdirectories
            if let Ok(entries) = std::fs::read_dir(root) {
                for entry in entries.flatten() {
                    if entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
                        let subdir_gemfile = entry.path().join("Gemfile");
                        if subdir_gemfile.exists() {
                            return Ok(subdir_gemfile);
                        }
                    }
                }
            }
        }

        // Fallback to current directory
        let current = std::env::current_dir()?.join("Gemfile");
        if current.exists() {
            return Ok(current);
        }

        Err(anyhow!("No Gemfile found in workspace hierarchy"))
    }

    /// Process gem data from JSON output
    fn process_gem_json(&mut self, data: &[u8], source: &str) -> Result<()> {
        use serde_json::Value;

        let json_str = String::from_utf8_lossy(data);
        let gems: Vec<Value> =
            serde_json::from_str(&json_str).context("Failed to parse gem JSON data")?;

        for gem in gems {
            let Some(obj) = gem.as_object() else { continue };

            let name = obj.get("name").and_then(|v| v.as_str()).unwrap_or_default();
            let version = obj
                .get("version")
                .and_then(|v| v.as_str())
                .unwrap_or_default();

            if name.is_empty() || version.is_empty() {
                continue;
            }

            let gem_info = GemInfo {
                name: name.to_string(),
                version: version.to_string(),
                path: obj
                    .get("gem_dir")
                    .and_then(|v| v.as_str())
                    .map(PathBuf::from)
                    .unwrap_or_default(),
                lib_paths: obj
                    .get("lib_dirs")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str())
                            .map(PathBuf::from)
                            .collect()
                    })
                    .unwrap_or_default(),
                dependencies: obj
                    .get("dependencies")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str())
                            .map(String::from)
                            .collect()
                    })
                    .unwrap_or_default(),
                is_default: obj
                    .get("default_gem")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
            };

            self.discovered_gems
                .entry(name.to_string())
                .or_default()
                .push(gem_info);
        }

        debug!(
            "Processed {} gems from {} source",
            self.discovered_gems.len(),
            source
        );
        Ok(())
    }

    /// Resolve and validate gem library paths
    fn resolve_gem_lib_paths(&mut self) {
        for versions in self.discovered_gems.values_mut() {
            for gem in versions.iter_mut() {
                // Filter out non-existent lib paths
                gem.lib_paths.retain(|p| p.exists() && p.is_dir());

                // Try default lib path if none exist
                if gem.lib_paths.is_empty() {
                    let default_lib = gem.path.join("lib");
                    if default_lib.exists() && default_lib.is_dir() {
                        gem.lib_paths.push(default_lib);
                    }
                }
            }
        }
    }

    // ========================================================================
    // Version Selection
    // ========================================================================

    /// Select the preferred version of a gem from multiple available versions
    fn select_preferred_version<'a>(&self, versions: &'a [GemInfo]) -> Option<&'a GemInfo> {
        if versions.is_empty() {
            return None;
        }

        // Prefer bundler-managed gems
        if let Some(bundler_gem) = versions.iter().find(|g| {
            g.path.to_string_lossy().contains("bundler/gems")
                || g.path.to_string_lossy().contains(".bundle")
        }) {
            return Some(bundler_gem);
        }

        // Otherwise select highest version
        versions
            .iter()
            .max_by(|a, b| compare_versions(&a.version, &b.version))
    }

    // ========================================================================
    // Accessors
    // ========================================================================

    pub fn get_gem(&self, name: &str) -> Option<&GemInfo> {
        self.discovered_gems
            .get(name)
            .and_then(|v| self.select_preferred_version(v))
    }

    pub fn has_gem(&self, name: &str) -> bool {
        self.discovered_gems.contains_key(name)
    }

    pub fn gem_count(&self) -> usize {
        self.discovered_gems.len()
    }

    pub fn get_required_gems(&self) -> &HashSet<String> {
        &self.required_gems
    }

    pub fn get_all_gems(&self) -> Vec<&GemInfo> {
        self.discovered_gems.values().flatten().collect()
    }

    pub fn get_gem_lib_paths(&self) -> Vec<PathBuf> {
        self.discovered_gems
            .values()
            .filter_map(|v| self.select_preferred_version(v))
            .flat_map(|g| g.lib_paths.iter().cloned())
            .collect()
    }

    pub fn get_gem_paths(&self, name: &str) -> Vec<PathBuf> {
        self.discovered_gems
            .get(name)
            .and_then(|v| self.select_preferred_version(v))
            .map(|g| g.lib_paths.clone())
            .unwrap_or_default()
    }

    pub fn get_gem_lib_paths_for_gems(&self, names: &[String]) -> Vec<PathBuf> {
        let mut paths: Vec<PathBuf> = names.iter().flat_map(|n| self.get_gem_paths(n)).collect();

        // Deduplicate while preserving order
        let mut seen = HashSet::new();
        paths.retain(|p| seen.insert(p.clone()));
        paths
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Compare two gem version strings
fn compare_versions(a: &str, b: &str) -> Ordering {
    let parse = |v: &str| -> Vec<u32> {
        v.split('.')
            .filter_map(|part| {
                part.chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect::<String>()
                    .parse()
                    .ok()
            })
            .collect()
    };

    let parts_a = parse(a);
    let parts_b = parse(b);

    for (x, y) in parts_a.iter().zip(parts_b.iter()) {
        match x.cmp(y) {
            Ordering::Equal => continue,
            other => return other,
        }
    }

    parts_a.len().cmp(&parts_b.len())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexer::index::RubyIndex;
    use tempfile::TempDir;

    fn create_test_indexer() -> IndexerGem {
        let temp_dir = TempDir::new().unwrap();
        let index = Arc::new(Mutex::new(RubyIndex::new()));
        let core = Arc::new(Mutex::new(IndexerCore::new(index)));
        IndexerGem::new(core, Some(temp_dir.path().to_path_buf()))
    }

    #[test]
    fn test_gem_indexer_creation() {
        let indexer = create_test_indexer();
        assert_eq!(indexer.gem_count(), 0);
        assert!(indexer.get_required_gems().is_empty());
    }

    #[test]
    fn test_version_comparison() {
        assert_eq!(compare_versions("1.0.0", "1.0.0"), Ordering::Equal);
        assert_eq!(compare_versions("1.0.1", "1.0.0"), Ordering::Greater);
        assert_eq!(compare_versions("1.0.0", "1.0.1"), Ordering::Less);
        assert_eq!(compare_versions("2.0.0", "1.9.9"), Ordering::Greater);
    }
}
