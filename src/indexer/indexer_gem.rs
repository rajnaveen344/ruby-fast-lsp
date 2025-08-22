use crate::indexer::indexer_core::IndexerCore;
use anyhow::{anyhow, Context, Result};
use log::{debug, info};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};
use tower_lsp::lsp_types::Url;

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

/// Handles gem indexing for the Ruby Language Server
/// Manages gem discovery, prioritization, and selective indexing
#[derive(Debug)]
pub struct IndexerGem {
    /// Core indexing functionality
    indexer_core: Arc<Mutex<IndexerCore>>,
    /// Set of gems required by the project
    required_gems: HashSet<String>,
    /// Workspace root for context
    workspace_root: Option<PathBuf>,
    /// Discovered gems cache (supports multiple versions per gem)
    discovered_gems: HashMap<String, Vec<GemInfo>>,
    /// Gem paths from Ruby environment
    gem_paths: Vec<PathBuf>,
}

impl IndexerGem {
    /// Create a new gem indexer
    pub fn new(indexer_core: Arc<Mutex<IndexerCore>>, workspace_root: Option<PathBuf>) -> Self {
        Self {
            indexer_core,
            required_gems: HashSet::new(),
            workspace_root,
            discovered_gems: HashMap::new(),
            gem_paths: Vec::new(),
        }
    }

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

    /// Index gems based on project requirements
    /// If selective is true, only index required gems
    /// If selective is false, index all discovered gems
    pub async fn index_gems(&mut self, selective: bool) -> Result<Vec<Url>> {
        info!("Starting gem indexing (selective: {})", selective);

        // Discover available gems
        self.discover_gems().await?;
        let discovered_count = self.discovered_gems.len();
        info!("Discovered {} gems", discovered_count);

        let indexed_files;

        if selective && !self.required_gems.is_empty() {
            // Index only required gems
            indexed_files = self.index_required_gems().await?;
        } else {
            // Index all discovered gems
            indexed_files = self.index_all_gems().await?;
        }

        info!("Indexed {} files from gems", indexed_files.len());
        Ok(indexed_files)
    }

    /// Index only the gems required by the project
    async fn index_required_gems(&self) -> Result<Vec<Url>> {
        let mut indexed_files = Vec::new();

        for gem_name in &self.required_gems {
            if let Some(gem_versions) = self.discovered_gems.get(gem_name) {
                if let Some(gem_info) = self.select_preferred_gem_version(gem_versions) {
                    debug!(
                        "Indexing required gem: {} v{}",
                        gem_info.name, gem_info.version
                    );
                    let gem_files = self.index_gem(gem_info).await?;
                    indexed_files.extend(gem_files);
                }
            } else {
                debug!("Required gem not found: {}", gem_name);
            }
        }

        Ok(indexed_files)
    }

    /// Index all discovered gems
    async fn index_all_gems(&self) -> Result<Vec<Url>> {
        let mut indexed_files = Vec::new();

        for gem_versions in self.discovered_gems.values() {
            if let Some(gem_info) = self.select_preferred_gem_version(gem_versions) {
                debug!("Indexing gem: {} v{}", gem_info.name, gem_info.version);
                let gem_files = self.index_gem(gem_info).await?;
                indexed_files.extend(gem_files);
            }
        }

        Ok(indexed_files)
    }

    /// Index a single gem
    async fn index_gem(&self, gem_info: &GemInfo) -> Result<Vec<Url>> {
        let mut indexed_files = Vec::new();

        for lib_path in &gem_info.lib_paths {
            if lib_path.exists() && lib_path.is_dir() {
                debug!("Indexing gem lib path: {:?}", lib_path);

                if let Ok(core) = self.indexer_core.lock() {
                    let ruby_files = core.collect_ruby_files(lib_path);
                    drop(core); // Release lock before async operations

                    for file_path in ruby_files {
                        if let Ok(file_url) = Url::from_file_path(&file_path) {
                            indexed_files.push(file_url);
                        }
                    }
                }
            }
        }

        Ok(indexed_files)
    }

    /// Discover available gems in the system
    /// Returns the number of discovered gems
    pub async fn discover_gems(&mut self) -> Result<usize> {
        debug!("Starting gem discovery process");

        // Clear previous discoveries
        self.discovered_gems.clear();
        self.gem_paths.clear();

        // Get gem environment information
        self.discover_gem_paths()?;
        self.discover_installed_gems()?;
        self.resolve_gem_lib_paths()?;

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

        let gem_paths = String::from_utf8_lossy(&output.stdout);
        for path_str in gem_paths.lines() {
            let path = PathBuf::from(path_str.trim());
            if path.exists() && path.is_dir() {
                self.gem_paths.push(path.clone());
                debug!("Found gem path: {:?}", path);
            }
        }

        Ok(())
    }

    /// Discover all installed gems using efficient bulk approach
    fn discover_installed_gems(&mut self) -> Result<()> {
        // Check configuration for gem indexing scope
        let gem_scope = std::env::var("RUBY_LSP_GEM_SCOPE")
            .unwrap_or_else(|_| "auto".to_string())
            .to_lowercase();

        match gem_scope.as_str() {
            "bundler" | "gemfile" => {
                // Only use Bundler gems, fail if no Gemfile
                info!("Gem indexing scope: Bundler/Gemfile only");
                self.discover_bundler_gems()
            }
            "global" => {
                // Only use global gems, skip Bundler
                info!("Gem indexing scope: Global gems only");
                self.discover_global_gems()
            }
            "auto" | _ => {
                // Auto mode: try Bundler first, fallback to global
                debug!("Gem indexing scope: Auto (Bundler with global fallback)");
                if let Ok(_bundler_gems) = self.discover_bundler_gems() {
                    debug!("Using Bundler gems from Gemfile");
                    return Ok(());
                }

                debug!("Falling back to global gem discovery");
                self.discover_global_gems()
            }
        }
    }

    /// Discover gems using Bundler (Gemfile-based)
    fn discover_bundler_gems(&mut self) -> Result<()> {
        // Find Gemfile in workspace hierarchy
        let gemfile_path = self.find_gemfile_in_workspace()?;

        let ruby_script = format!(
            r#"
            require 'bundler'
            require 'json'
            
            begin
              # Change to the directory containing the Gemfile
              Dir.chdir('{}')
              
              # Check if we're in a bundler project
              Bundler.root
              
              gems = []
              Bundler.load.specs.each do |spec|
                next if spec.name.nil? || spec.version.nil?
                
                gem_info = {{
                  name: spec.name,
                  version: spec.version.to_s,
                  gem_dir: spec.gem_dir,
                  lib_dirs: spec.require_paths.map {{ |p| File.join(spec.gem_dir, p) }},
                  dependencies: spec.dependencies.map(&:name),
                  default_gem: spec.default_gem?,
                  bundler_gem: true
                }}
                gems << gem_info
              end
              
              puts JSON.generate(gems)
            rescue Bundler::GemfileNotFound
              exit 1
            end
        "#,
            gemfile_path.parent().unwrap().display()
        );

        let output = Command::new("ruby")
            .args(["-e", &ruby_script])
            .output()
            .map_err(|e| anyhow!("Failed to execute bundler gem discovery script: {}", e))?;

        if !output.status.success() {
            return Err(anyhow!("No Gemfile found or bundler failed"));
        }

        self.process_gem_data(&output.stdout, "Bundler")
    }

    /// Find Gemfile in workspace hierarchy
    fn find_gemfile_in_workspace(&self) -> Result<PathBuf> {
        if let Some(workspace_root) = &self.workspace_root {
            // First check the workspace root
            let gemfile_path = workspace_root.join("Gemfile");
            if gemfile_path.exists() {
                return Ok(gemfile_path);
            }

            // Then check subdirectories
            if let Ok(entries) = std::fs::read_dir(workspace_root) {
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
        let current_dir_gemfile = std::env::current_dir()?.join("Gemfile");
        if current_dir_gemfile.exists() {
            return Ok(current_dir_gemfile);
        }

        Err(anyhow!("No Gemfile found in workspace hierarchy"))
    }

    /// Discover all global gems
    fn discover_global_gems(&mut self) -> Result<()> {
        let ruby_script = r#"
            require 'rubygems'
            require 'json'
            
            gems = []
            Gem::Specification.each do |spec|
              next if spec.name.nil? || spec.version.nil?
              
              gem_info = {
                name: spec.name,
                version: spec.version.to_s,
                gem_dir: spec.gem_dir,
                lib_dirs: spec.require_paths.map { |p| File.join(spec.gem_dir, p) },
                dependencies: spec.dependencies.map(&:name),
                default_gem: spec.default_gem?,
                bundler_gem: false
              }
              gems << gem_info
            end
            
            puts JSON.generate(gems)
        "#;

        let output = Command::new("ruby")
            .args(["-e", ruby_script])
            .output()
            .map_err(|e| anyhow!("Failed to execute ruby gem discovery script: {}", e))?;

        if !output.status.success() {
            return Err(anyhow!(
                "Ruby gem discovery script failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        self.process_gem_data(&output.stdout, "Global")
    }

    /// Process gem data from JSON output
    fn process_gem_data(&mut self, json_data: &[u8], source: &str) -> Result<()> {
        use serde_json::Value;

        let json_str = String::from_utf8_lossy(json_data);
        let gems: Vec<Value> =
            serde_json::from_str(&json_str).context("Failed to parse gem JSON data")?;

        for gem_value in gems {
            if let Some(gem_obj) = gem_value.as_object() {
                let name = gem_obj
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();

                let version = gem_obj
                    .get("version")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();

                let gem_dir = gem_obj
                    .get("gem_dir")
                    .and_then(|v| v.as_str())
                    .map(PathBuf::from)
                    .unwrap_or_default();

                let lib_dirs: Vec<PathBuf> = gem_obj
                    .get("lib_dirs")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str())
                            .map(PathBuf::from)
                            .collect()
                    })
                    .unwrap_or_default();

                let dependencies: Vec<String> = gem_obj
                    .get("dependencies")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str())
                            .map(String::from)
                            .collect()
                    })
                    .unwrap_or_default();

                let is_default = gem_obj
                    .get("default_gem")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                if !name.is_empty() && !version.is_empty() {
                    let gem_info = GemInfo {
                        name: name.clone(),
                        version,
                        path: gem_dir,
                        lib_paths: lib_dirs,
                        dependencies,
                        is_default,
                    };

                    self.discovered_gems
                        .entry(name)
                        .or_insert_with(Vec::new)
                        .push(gem_info);
                }
            }
        }

        debug!(
            "Processed {} gems from {} source",
            self.discovered_gems.len(),
            source
        );
        Ok(())
    }

    /// Resolve and validate gem library paths
    fn resolve_gem_lib_paths(&mut self) -> Result<()> {
        for gem_versions in self.discovered_gems.values_mut() {
            for gem_info in gem_versions.iter_mut() {
                // Filter out non-existent lib paths
                gem_info
                    .lib_paths
                    .retain(|path| path.exists() && path.is_dir());

                // If no lib paths exist, try to find them
                if gem_info.lib_paths.is_empty() {
                    let default_lib = gem_info.path.join("lib");
                    if default_lib.exists() && default_lib.is_dir() {
                        gem_info.lib_paths.push(default_lib);
                    }
                }
            }
        }

        Ok(())
    }

    /// Select the preferred version of a gem from multiple available versions
    fn select_preferred_gem_version<'a>(&self, gem_versions: &'a [GemInfo]) -> Option<&'a GemInfo> {
        if gem_versions.is_empty() {
            return None;
        }

        // First, try to find an active version (from Bundler)
        if let Some(active_version) = self.find_active_gem_version(gem_versions) {
            return Some(active_version);
        }

        // Otherwise, select the highest version
        gem_versions
            .iter()
            .max_by(|a, b| self.compare_gem_versions(&a.version, &b.version))
    }

    /// Find the active version of a gem (typically from Bundler)
    fn find_active_gem_version<'a>(&self, gem_versions: &'a [GemInfo]) -> Option<&'a GemInfo> {
        // Look for bundler gems first (they are "active")
        gem_versions.iter().find(|gem| {
            // Check if this gem is in a bundler-managed location
            gem.path.to_string_lossy().contains("bundler/gems")
                || gem.path.to_string_lossy().contains(".bundle")
        })
    }

    /// Compare two gem version strings
    fn compare_gem_versions(&self, version_a: &str, version_b: &str) -> std::cmp::Ordering {
        use std::cmp::Ordering;

        let parse_version = |v: &str| -> Vec<u32> {
            v.split('.')
                .filter_map(|part| {
                    // Remove non-numeric suffixes like "rc1", "beta", etc.
                    let numeric_part = part
                        .chars()
                        .take_while(|c| c.is_ascii_digit())
                        .collect::<String>();
                    numeric_part.parse().ok()
                })
                .collect()
        };

        let parts_a = parse_version(version_a);
        let parts_b = parse_version(version_b);

        // Compare version parts
        for (a, b) in parts_a.iter().zip(parts_b.iter()) {
            match a.cmp(b) {
                Ordering::Equal => continue,
                other => return other,
            }
        }

        // If all compared parts are equal, the longer version is considered higher
        parts_a.len().cmp(&parts_b.len())
    }

    /// Get gem information
    pub fn get_gem(&self, name: &str) -> Option<&GemInfo> {
        self.discovered_gems
            .get(name)
            .and_then(|versions| self.select_preferred_gem_version(versions))
    }

    /// Check if a gem is available
    pub fn has_gem(&self, name: &str) -> bool {
        self.discovered_gems.contains_key(name)
    }

    /// Get count of discovered gems
    pub fn gem_count(&self) -> usize {
        self.discovered_gems.len()
    }

    /// Get all gem library paths
    pub fn get_gem_lib_paths(&self) -> Vec<PathBuf> {
        self.discovered_gems
            .values()
            .filter_map(|versions| self.select_preferred_gem_version(versions))
            .flat_map(|gem| gem.lib_paths.iter().cloned())
            .collect()
    }

    /// Get required gems list
    pub fn get_required_gems(&self) -> &HashSet<String> {
        &self.required_gems
    }

    /// Get all discovered gems as a flat list
    pub fn get_all_gems(&self) -> Vec<&GemInfo> {
        self.discovered_gems
            .values()
            .flat_map(|versions| versions.iter())
            .collect()
    }

    /// Get gem paths for a specific gem
    pub fn get_gem_paths(&self, gem_name: &str) -> Vec<PathBuf> {
        if let Some(gem_versions) = self.discovered_gems.get(gem_name) {
            if let Some(preferred_gem) = self.select_preferred_gem_version(gem_versions) {
                return preferred_gem.lib_paths.clone();
            }
        }
        Vec::new()
    }

    /// Get gem library paths for specific gems by name
    pub fn get_gem_lib_paths_for_gems(&self, gem_names: &[String]) -> Vec<PathBuf> {
        let mut all_paths = Vec::new();

        for gem_name in gem_names {
            let gem_paths = self.get_gem_paths(gem_name);
            all_paths.extend(gem_paths);
        }

        // Remove duplicates while preserving order
        let mut unique_paths = Vec::new();
        for path in all_paths {
            if !unique_paths.contains(&path) {
                unique_paths.push(path);
            }
        }

        unique_paths
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_gem_indexer_creation() {
        use crate::indexer::index::RubyIndex;
        use parking_lot::Mutex as ParkingLotMutex;
        use std::sync::{Arc, Mutex};

        let temp_dir = TempDir::new().unwrap();
        let index = Arc::new(ParkingLotMutex::new(RubyIndex::new()));
        let indexer_core = Arc::new(Mutex::new(IndexerCore::new(index)));
        let indexer = IndexerGem::new(indexer_core, Some(temp_dir.path().to_path_buf()));

        assert_eq!(indexer.gem_count(), 0);
        assert!(indexer.get_required_gems().is_empty());
    }

    #[test]
    fn test_version_comparison() {
        use crate::indexer::index::RubyIndex;
        use parking_lot::Mutex as ParkingLotMutex;
        use std::sync::{Arc, Mutex};

        let temp_dir = TempDir::new().unwrap();
        let index = Arc::new(ParkingLotMutex::new(RubyIndex::new()));
        let indexer_core = Arc::new(Mutex::new(IndexerCore::new(index)));
        let indexer = IndexerGem::new(indexer_core, Some(temp_dir.path().to_path_buf()));

        use std::cmp::Ordering;

        assert_eq!(
            indexer.compare_gem_versions("1.0.0", "1.0.0"),
            Ordering::Equal
        );
        assert_eq!(
            indexer.compare_gem_versions("1.0.1", "1.0.0"),
            Ordering::Greater
        );
        assert_eq!(
            indexer.compare_gem_versions("1.0.0", "1.0.1"),
            Ordering::Less
        );
        assert_eq!(
            indexer.compare_gem_versions("2.0.0", "1.9.9"),
            Ordering::Greater
        );
    }
}
